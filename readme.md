# `rmeta`-Based Dynamic Library Plugin System

AKA the `rmeta` trick.

This is a showcase of a somewhat obscure way to do plugin systems in Rust using dynamic libraries.
It consists of 
- `host`: The main application that loads and uses the plugins. Assume that this is distributed to users.
- `plugin`: A `dylib` plugin that is loaded and run by the host. Assume that this is made by users or plugin developers.
- `api`: A `dylib` containing the `repr(Rust)` API used by the host and all plugins. Assume that this is distributed to plugin authors.

TL;DR for the technique: we distribute `libapi.so` (including the `.rmeta` inside) and its build-time deps to plugin authors. 
Plugins are linked against `api`, but don't build it themselves. Instead, they use the build artifacts provided by the host to ensure ABI compatibility.

Now for the long story, strap in.

## Dynamic Linking Basics

This is just a very very brief and imprecise introduction to dynamic linking, since it's a big topic.
But basically, when create a rust app, all your dependencies are bundled into the final executable by default.
That's cool because you can usually just send that single file around and it will work. 
No need to make sure the user has any specific things preinstalled.

There's a lot of pros and cons to this approach, but for the discussion at hand, the important part is that this generally doesn't jive with plugin systems. If you want to e.g. have a mod in a game, it's cool
if you can just drop it into a folder and let the game pick it up, without you having to recompile the game,
since you as a player usually don't even have the source code.

Dynamic linking is the alternative: code is built into a dynamic library with the file extension `.dll`, `.so`, or `.dylib`, depending on your operating system. Then the executable loads that dynamic library at runtime. In a plugin setting, the host process (e.g. the game) can try loading all dynamic libraries in a folder (e.g. `mods/`) and run them all.
Again, this is a deep rabbit hole, so forgive me for being very handwavy and ignoring all the other use-cases for dynamic linking.

## Why can't we just do it the easy way?

So in C++, when you want to setup a plugin system, you just build your executable, build your dynamic library, and load it at runtime.
C++ being C++, this process is usually followed by a long period of debugging undefined behavior, but the
basic concept is as simple as that. This is only possible due to one very important property: when you
build a type in C++, that type has a guaranteed size and layout. In other words, all compilers are supposed to agree about how a type like this looks like in memory:

```cpp
struct Person {
    std::string name;
    uint8_t age;
};
```

This matters because the host and the plugin will agree about how to talk about a `Person`. If the plugin
has a function that takes a `Person` as an argument, and the host builds a `Person` and passes it to the plugin, that will work out fine and dandy.
We call this nebulous "how a `Person` is represented in memory" the *ABI*, or Application Binary Interface. Here, we say that the host and the plugin *agree on the ABI*. 
Again, there's some nuance here, and we left out functions entirely, but bear with me.

Now this is super duper cool for devs, but it comes at a crucial cost: all the internals of the standard library are frozen forever.

If `std::string` wanted to switch up its internal workings and needs a new `bool` flag for that internally, tough luck. 
Doing so would make anyone using this new version of the standard library compile `Person` with a different ABI than the one the plugin expects, and things would break.
Since standard libraries have a very strong incentive to remain backwards-compatible, this means that old mistakes cannot be fixed. Well, or you create an `std::string2` that is fancy, but then you fragment
the ecosystem unless you do a ton of type conversions and bleh.

Rust does not wish to run into this problem. The compiler and standard library people want to have the
liberty to change internals at will, without breaking existing code. For this reason, almost all Rust code
is by default *not ABI stable*. That means that even simple structs like our `Person` from before:
```rust
struct Person {
    name: String,
    age: u8,
}
```
can change their memory layout overnight. The compiler is allowed to reshuffle the fields as it sees fit,
and can freely break the ABI in the process.

That means that Rust has excellent freedom in how to improve their internals, but comes with the cost of dynamic linking no longer being straightforward. 

> Foreshadowing: There is one situation where this is fine: dynamic linking is guaranteed to have compatible ABI
> across all crates that were compiled within the same compiler invocation, 
> e.g. by a single `cargo build` command.

The go-to solution for the problem of an unstable ABI is to force a stable once by using `repr(C)`.

## `repr(C)` and why it sucks

The unstable ABI convention is called `repr(Rust)`, and you probably have never seen that written out anywhere because it's the implicit default.
As an alternative, `#[repr(C)]` is an annotation you can use on data types to say "dear Rust compiler, please keep this type's ABI stable"
It's called like that because it geeeenerally uses the C ABI, which is a very widely used standard supported by most modern programming languages.
If you want your code to be able to be compatible with other languages, `repr(C)` is your friend.
Rust has a couple of concepts like Zero-Sized Types (ZSTs) that don't map cleanly to the C ABI, so `repr(C)` is not exactly the
same as C ABI compatibility in all cases. We will gloss over that for now, but it's something to keep in the back of your head.

Anyways, let's see a `repr(C)` in action!
```rust
#[repr(C)]
struct Person {
    name_ptr: *mut u8,
    name_len: usize,
    name_capacity: usize,
    age: u8,
}
```
uuuh hold up, why does this look so low-level all of a sudden? 
Well, `String` comes from the standard library, and as we just discussed before, the standard library intentionally reserves the right to change its ABI at will. 
In other words, the overwhelming majority of types in the standard library are `repr(Rust)`. And if we
use `repr(C)`, that is not transitive. Just because `Person` is `repr(C)`, and `Person` contains a `String`, doesn't mean that `String` is `repr(C)`.
You do get some limited guarantees when using `repr(C)` on something with a `String`, like guaranteeing that the fields of `Person` won't get reordered. 
But it still means that you cannot know how big a `String` is or how it looks like. So, to get what we actually want, we need to unpack a `String` into constituent parts that themselves *are* `repr(C)`.
In this case, we split it into a pointer, a length, and a capacity. We can do this because `String`
exposes API for turning it into these parts and back again:
```rust
let string = String::from("hello");
// let's unpack the string into repr(C) parts
let (ptr, len, capacity) = string.into_raw_parts();
// whenever we need a String again, we can reconstruct it again
let string = unsafe { String::from_raw_parts(ptr, len, capacity) };
```

This works great when you want to interface with actual C libraries, where you sling pointers around everywhere anyways, but is suuuuuuuper annoying when working with Rust to Rust communication.
If you want to create a plugin system based on this, you need to 
- take your nice Rusty types
- carefully repack them into such `repr(C)` types as above
- pass those from the host to the plugin
- them repack them back into Rusty types again in the plugin

If this sounds manageable, remember that you need to do this for *all* types that you want to pass the boundary, including all standard library types.
If you use a third-party dependency, and their types don't expose any such `into_raw_parts` and `from_raw_parts` methods, you must go ahead and fork them if you need to pass them around.
If your API uses a ZST, you must translate that into a magic `u8` or such to know which ZST type it is.
If your API uses a trait object like `Box<dyn Error>`, you enter the realm of hand-rolling your own vtable to pass around.
There's a ton of these papercuts everywhere waiting to slice your application into tons of `*mut c_void` until you find yourself wondering if you accidentally became a C developer.
Which is silly, considering we are still talking only about Rust <-> Rust communication.

## But surely the ABI is not *that* unstable in practice, right?

Clever readers may at this point wondered what would happen if we just ignored this nuance and just...
used dynamic libraries with `repr(Rust)` anyways.
I will skip any discussion of how stable that is in practice, since there is human component to all of this, and software is about people.
Whether this works or doesn't is irrelevant, since the Rust maintainers explicitly wish to have the freedom of working on the internals without fear of breaking existing users.
If you want to empathize with their position, look up language discussions in C++ across the years. 
Many language features and libraries turned out to have critical flaws, or are incompatible with new ways of doing things, or suck to implement in compilers, 
or are just plain broken, yet it's often impossible to fix them due to forced ABI stability everywhere.

If your only way to fix gaps is to bolt on new features or APIs, you end up with things like
```cpp
template <std::invocable<I> F>
auto map(F&& f) const {
    Box<std::invoke_result_t<F, I>> out(n);
    for (std::size_t i = 0; i < n; ++i)
        out[i] = std::invoke(std::forward<F>(f), data[i]);
    return out;
}
```
The above is an unfair example since it wouldn't actually be fixed by ABI instability, but you get the idea.
Not to be too mean to C++, the designers and maintainers are talented people doing the best with what they have. (also, hurray at this point to Rust's `edition` feature!)
It's just that their hands are tied in how much they can actually improve things.

Long story short: the compiler people want the best for Rust and trust you to not depend on any internal ABI so that they have the freedom to create great things.
Be kind to the Rust folk, don't depend on unstable ABI.

## Okay, but what if I ensured that...

no.

## A Proposed Solution: The `rmeta` trick

First up: I didn't invent this. [bjorn3](https://github.com/bjorn3) and [jyn](https://github.com/jyn514) taught me this trick. 
But I couldn't find any good public writeups about it (please send them my way!), so I decided to vomit out this stream of consciousness at 2 am.
Anyways, let's get to the heart of it.
The crate structure we want is:
- `host`: something that loads and runs plugins
- `plugin`: a dynamic library
- `api`: a crate that defines the API and is used by both the host and the plugin

Since `api` should be the exact same for both the host and the plugin, it makes sense to also make it a shared library.

Our core issue is that we *know* that we only care about Rust <-> Rust interop and don't care about what C may or may not like. 
All we need is a way for the host and the plugin to share on how the API looks, even if it unstable.
The solution is to share build artifacts.

While a dev is not allowed to peek inside the ABI definition, `rustc` (the Rust compiler used by `cargo`) certainly creates and in turn uses that information.
Good news: that data is not thrown away, but actually stored on disk as part of the build artifacts under `target/debug/deps`. 
It usually comes in the form of a `libmy_cool_crate.rmeta` file, or ([rmeta](https://rustc-dev-guide.rust-lang.org/backend/libs-and-metadata.html#metadata)) for short.

If we create and compile a `dylib` for `api`, we will not find any `rmeta` for it. Don't worry though, it's there, embedded in the dynamic library itself!

Per the schema above, `host` depends on `api`. As mentioned a while ago, dynamic linking is guaranteed to have compatible ABI across all crates that were compiled within the same compiler invocation. 
So, if `host` depends on `api`, and we build `host` with a `cargo build`, that will in turn build `api` in the same compiler invocation, making `host` and `api` ABI compatible.

Now if a dev wants to create a plugin for a `host` they downloaded, using the `api` crate, the first instinct
would be to add `api` to their `Cargo.toml` under `[dependencies]`:

```toml
[dependencies]
my_cool_api = "1.0.0"
```

That is precisely where the trouble begins, since that would no longer be the same compiler invocation we used for `host` and `api`. 
BUT! Remember when we said before that our dynamic library contains within it an `rmeta` file with all the ABI information needed? 
The trick now is to not let a plugin dev build their own `api`, but give them our pre-built `libapi.so` (along with some extra build information we'll get to later), 
and let them link against that not at runtime, but at build time.

Since that will read the exact same `rmeta` that was used to build `host`, `plugin` and `host` will both agree on how all types in `api` look like.

There's a few details to keep in mind when doing that, and we'll look at them in the next section. But this is the core idea:
By distributing `api`'s `rmeta`, which is embedded in the `dylib` itself, `plugin` can use that information
at build time to ensure ABI compatibility with `host`, even if all types involved are `repr(Rust)`.

## The nitty-gritty

Let's start with the build instructions needed for all of this, then chew through them.

This is the build command for the `host` + `api`:

```sh
RUSTFLAGS="\
-C prefer-dynamic \
-C rpath" \
cargo build -p host
```

Remember that `host` depends on `api`, so the above builds both, implicitly. The only thing of note here are the rustflags.
These let all crates touched by this compiler invocation link against the standard library dynamically.
The reason we do this is because the standard library uses internal global state, so if you have multiple statically linked
copies of the standard library, they will disagree about the state of the world, since they all have their own
view of the world, which may be ABI incompatible between them. [That will blow up](https://github.com/rust-lang/rust/issues/131468#issuecomment-2405285332), so we dynamically link them all against the same standard library dynamic library.

Now let's get to the true magicc.
This here is where `plugin` is built by referencing the existing `libapi.so`:

```sh
RUSTFLAGS="\
-C prefer-dynamic \
-C rpath" \
rustc \
    --crate-name=plugin \
    --edition 2024 \
    --crate-type=dylib \
    --extern api="lib/libapi.so" \
    -L dependency="lib/deps" \
    -o lib/libplugin.so \
    plugin/src/lib.rs
```

unfortunately, this functionality is not (yet) exposed in `cargo`, so we need to go one level deeper and manually build the `plugin` with `rustc`.
Let's take a look at some select lines of the above command:
```sh
RUSTFLAGS="\
-C prefer-dynamic \
-C rpath" \
```
same as before, we link against the standard library dynamically.

```sh
--crate-type=dylib
```
we compile our `plugin` as a `dylib`. Rust can build two flavors of dynamic libraries: `dylib` and `cdylib`.
`cdylib` is where we use C conventions everywhere (roughly, there's again a ton of nuance), 
but since we only care about Rust <-> Rust interop and have already found a way to get ABI compatibility, we use `dylib`, which is a regular Rust dynamic library.

```sh
--extern api="lib/libapi.so" \
```
we fetch the `libapi.so` from the `lib` directory to use it in our build. We assume that this is run on the machine
of a plugin author that has received this `lib` directory from us in advance.
```sh
-L dependency="lib/deps" \
 ```
`api` itself can also bring in its own dependencies that it compiled statically into itself. If you want to be 
ABI compatible with `api`, you also need to be `ABI` compatible with its dependencies. By also distributing the 
build artifacts of the dependencies, we can ensure that.

```sh
-o lib/libplugin.so \
```

emit the `plugin` as a `libplugin.so` file in the `lib` directory.

```sh
plugin/src/lib.rs
```
use `plugin/src/lib.rs` as the entry point for our `plugin`'s code. If you've ever used a C or C++ compiler, you may think that 
you need to list every `.rs` file you have in your project, but this is actually smarter. `rustc` will
automatically follow any `mod` declarations recursively to find all necessary `.rs` files to compile your `plugin`.

If you think this all sounds like quite a bit of work for a regular user, I've got good news. 
You can tell cargo to replace all calls to `rustc` with a custom command by setting the `RUSTC_WRAPPER` env var.
This way, you can change internal calls to `--extern api=...` to point to our `.so`, as well as add all other magic incantations needed.
The user's call then becomes a trivial `RUSTC_WRAPPER="my_cool_wrapper" cargo build`. 
The code for doing this is left as an exercise to the reader ;)

Anyways, if you *don't* have a `RUSTC_WRAPPER`, you should be aware that calling `rustc` like shown above completely
bypasses the `Cargo.toml` you are used to. You probably still want to keep a `Cargo.toml` around so rust-analyzer
doesn't get too sad. That will however also lead to r-a running `cargo check` without our custom linking step,
potentially producing invalid artifacts along the way. I never ran into problems with this, but YMMV.

## Determinism as a consequence

As hinted at before, there's one more thing to dynamic linking interop than types: calling conventions.
These are the way that parameters and return values are laid out in memory: most notably which registers they are placed in and how the stack is managed. 
In this aspect too, Rust is unstable by design.

Just like all structs in Rust are `repr(C)` by default, all functions have a similar implicit annotation: `extern "Rust"`. 
If we want to nail down the C calling convention, we can use `extern "C"` instead:
```rust
extern "C" fn some_function(arg1: i32, arg2: i32) -> i32 {
    arg1 + arg2
}
```

Using `extern "C"` requires only using `repr(C)` types everywhere, or the compiler will yell at you:

```rust
extern "C" fn append(string: String, to_append: &str) -> String {
    string + &to_append
}
```
triggers:
```
extern fn uses type String, which is not FFI-safe
consider adding a #[repr(C)] or #[repr(transparent)] attribute to this struct
this struct has unspecified layout
#[warn(improper_ctypes_definitions)] on by default (rustc improper_ctypes_definitions)
```
This is a lovely lint that is enabled by default, and is much more helpful than the laissez-faire attitude of `repr(C)` in many cases.

So, does that mean that we still need to annotate all of our types with `extern "C"`? Nope!
Remember, we don't actually care about any C calling conventions since we are doing Rust <-> Rust communication.
But what about the fact that `extern "Rust"` is unstable? Well, turns out that is simply a direct consequence of the fact that `repr(Rust)` is unstable.
As soon as we use the `rmeta` trick to make our types stable, `extern "Rust"` functions operating on them are
stable as well. In other words: no annotations needed for our functions!

For very similar reasons, generics are not a problem either. 
Generic monomorphization (aka what happens when you fill in a concrete type for a generic parameter) is 
only unstable as a consequence of `repr(Rust)` being unstable, so using this trick makes all generics stable too,
as long as the specific types used were re-exported by the `api` crate.

It's worth pointing out that this does not mean that we are limited to only using `api`-exported types at all.
If `api` exports a `Query<T>` struct, and `plugin` has an internal struct `Player` so that it can use `Query<&Player>`, that is fine, since `Player` never crosses any FFI boundaries.

## Caution when setting RUSTFLAGS

There's a very annoying fact you should be aware of when setting `RUSTFLAGS`.
Devs can specify which `RUSTFLAGS` to use in many different places. 
The environment variable we already discussed, can be set on the user's environment in general, e.g. in their shell config. 
If you then go ahead and use `RUSTFLAGS="..." cargo build`, you will override the environment variable.
Users can also use a `myproject/.cargo/config.toml` file to set `RUSTFLAGS` for their project. Or,
they can use a global `~/.cargo/config.toml` file to set `RUSTFLAGS` for all their projects.
All of these sources of `RUSTFLAGS` will NOT be merged. They will happily completely override each other.
So, if you want to be friendly to users that already have `RUSTFLAGS` in place, e.g. to speed up their builds,
you should never just override them anywhere. Instead, the correct (and very annoying approach) is to
write some CLI tool that goes and reads all `RUSTFLAGS` already in place by checking the sources I mentioned above,
and then adds our own `RUSTFLAGS` on top before passing it along to `cargo build`.
In fact, it gets even more annoying. There's more sources than what I mentionned, and depending on where they are
defined, they will actually be merged with specific others. 

There's some hacks that can be used to make working around this easier. See
[rust-lang/cargo#5376](https://github.com/rust-lang/cargo/issues/5376) for more details.

## Mixing RUSTFLAGS

Assume you have done the work and nicely merged user's `RUSTFLAGS` with our own. Is that dangerous?
After all, `host` + `api` and `plugin` are now compiled with different `RUSTFLAGS`.

Well, there's not clear cut answer here. Fortunately, this will almost always be fine, since almost no `RUSTFLAG`
affects type layouts. 
The same is true for `dev` vs `release` builds. These are compatible, so you can ship `release` builds of `api` and `host` to your users, 
compiled with the most expensive, most optimized settings possible.
That means that users downloading your dynamic libraries get full performance for no compilation cost at all. Yay!

Back to RUSTFLAGS, there are some exceptions we should mention as being incompatible with `api`/`host` builds that are not aware of them.
Treat the following as a non-exhaustive list, since I bet there are some I don't know about:
- `-Z randomize-layout / -Z layout-seed`: This randomizes type layouts on builds, breaking all of our assumptions.
  While this doesn't come up in regular builds, there are some that like to run `miri` checks with this option enabled.
- `-C target-feature=-crt-static`: forces static linking.
- `-C soft-float`: changing the calling conventions of all things touching floating point numbers to use software instead of hardware implementations.
  This comes up often when building for targets such as embedded where there may be no hardware floating point support.
  If you want to support plugins for such targets, you will need to also ship `api`/`host` with `-C soft-float` enabled.
- `-C panic`: changes the panic strategy. For the standard library, the default is `unwind`, but you can set it to `abort` to make panics non-recoverable.
  If you build for `no_std`, you will need to specify your own panic handler.
  Regardless, `api`/`host` and `plugin` need to agree on the panic strategy used.

## Other gotchas

`api` / `host` and `plugin` also need to agree on the compiler version and target triple used. 
The target triple is solved by the fact that you need to distribute individual dynamic libraries for each target platform anyways.
For the compiler version, a good way to ensure agreement is to distribute a `rust-toolchain.toml` pointing to a compiler version you like.
I personally always build on `nightly` because it's much faster than on `stable` (see [my config](https://gist.github.com/janhohenheim/5731c11e91736bab5e9ef58c2a982c36)), 
so my `rust-toolchain.toml` looks like this:
```toml
[toolchain]
channel = "nightly-2026-06-21"
```

All packages must also agree on the allocator used, so `plugin`s are not allowed to use `#[global_allocator]` to change it away from the default.
If you want to use a custom allocator across `api`/`host` and `plugin`, I'm sure there's a way to do it, but
I confess I'm not very into allocators, so I don't have any advice here. All I know is that the `rmeta` trick
works with the default allocator.

A very similar story with `#[target_feature]`-based SIMD support. Be aware that `plugin`s shouldn't just enable
these without agreeing with the `api`/`host` beforehand, but I don't have any great insights here either.

## Some words on plugin systems in general

Now that we know how the `rmeta` trick works and what to watch out for, let's dive into some more general
considerations when building plugin systems. Most of this is not specific to the `rmeta` trick, but important to
know when you want to actually make use of it.

### Versioning

First up, let's show the minimal stuff that `api` needs to include. Let's say that all of our plugins have an entrypoint function that the host can call:
```rust
pub fn entrypoint() {
    // this method is called when the plugin is loaded
}
```
While a real-life plugin would probably want some parameter and / or return value, let's keep it simple for now.
How does the host call this function? Basically, it loads the dynamic library and searches for a well-known
*symbol* that the plugin exports. This is basically looking through the dynamic library for a magic string.
We could search for the string `"entrypoint"`, but we have to think about forwards compatibility for a second.
In the future, we might want to add new ways for the host and plugin to interact with each other, which will call for more API.
While the rmeta trick wouldn't allow an outdated plugin to be used with a new host anyways, we must still ensure
that the host can at least verify that this situation is happening, so it can gracefully reject the plugin instead of just crashing.

So, let's wrap the entrypoint function in a little extendable `struct`:
```rust
pub type EntrypointFn = fn();

pub struct Plugin {
    pub entrypoint: EntrypointFn,
}
```

Now a host can search for an entire `Plugin` with whatever it may contain. Let's add one very crucial bit of information to it: the *API version*.

```rust
pub type EntrypointFn = fn();

#[repr(C)]
pub struct Plugin {
    pub version: u32,
    pub entrypoint: EntrypointFn,
}
```

Oh, where did that `repr(C)` come from, I thought we didn't need it? Don't worry, this is the only place where it shows up.
The rationale is this: the host should be able to check a plugin's API version to check compatibility even if the plugin comes from a totally divergent API version.
We can do this by ensuring that the version number is always the very first field of the struct, so that a host can cast the `Plugin` to a `u32`. 
That way, only the version number is extracted, and nothing else about the plugin layout is assumed.
That's what `repr(C)` is for here: ensuring the ordering of the fields stays exactly as written.

The example code in this repo has some extra bells and whistles, but you should be able to understand it easily if the above made sense to you.

### Exporting a plugin

Now let's see how a plugin should look like. It's clear that it should export a `Plugin` struct. 
We can do that with a `static` variable:

```rust
use api::*;

#[unsafe(no_mangle)]
pub static PLUGIN: Plugin = Plugin {
    version: 1,
    entrypoint,
};

fn entrypoint() {
    // we'll get here soon
    // ...
}
```

The only scary bit about this is the `unsafe(no_mangle)` attribute. This essentially tells the compiler 
to please write down the symbol name exactly as written in the code. In our case, it will be exported as `PLUGIN`.
That's the magic string I mentioned earlier, and we will see it again in a bit when we look at how to set up the host.

Now let's have a little look at how `entrypoint` has to look, because it's got a twist:

```rust
fn entrypoint() {
    std::panic::catch_unwind(|| {
        // your code goes here
        println!("Hello World!");
    })
    .map_err(|payload| {
        let reason = if let Some(s) =
            payload.downcast_ref::<&'static str>()
        {
            (*s).to_owned()
        } else if let Some(s) =
            payload.downcast_ref::<String>()
        {
            s.clone()
        } else {
            "<non-string panic payload>".to_owned()
        };
        eprintln!("Plugin panicked: {}", reason)
    })
}
```

The reason why this looks a bit spooky is that when a Rust plugin panics, the host process is guaranteed to just terminate instantly.
Usually it's nicer to print a big error message, maybe do some additional logging, some cleanup, and then
just ignore the failing plugin and try to go on with the rest of the program. 
And even if you *do* want to panic the host too, you still want to control that shutdown in the host itself and not just brutally tear it down from a plugin.

This is why we use `catch_unwind` to not abort the plugin on panics, but to instead just print an error message and exit the entrypoint. 
If you want to propagate this error to the host and handle it there, you can let `entrypoint` return a `Result<(), PluginError>` or similar, that part is just regular old Rust error handling.

Note that unwinding requires the panic handler to be set to `unwind` in the first place, which it is by default.
If you however distribute your api and host with `panic = "abort"`, you can disregard this advice, since there's by definition no way to stop a plugin from panicking there.

A little note on the string handling: `catch_unwind` doesn't get a typed panic, but a general `Box<dyn Any + Send>`, which cannot be printed directly.
Almost all panic payloads are some kind of string value, so we check if we can cast it to a `&str` or `String` and print that if possible.

### The host process

Now let's combine all that we built up. We will create a binary that 
- loads the plugin dynamic library
- finds the `PLUGIN` symbol
- treats it as a `Plugin`
- checks if the API version is compatible
- if so: calls the plugin entrypoint

The star of the show here will be the [`libloading`](https://docs.rs/libloading/latest/libloading/) crate,
which handles all the attrocities of loading dynamic libraries across different platforms for us.

```rust
use api::*
use libloading::{Library, Symbol};

fn main() {
    let lib = unsafe { Library::new("path/to/your/libplugin.so").context("failed to find libplugin.so")? };
    // We loaded the library, now let's leak it. Why?
    // Well TL;DR unloading a dylib is really really hard to do right, and honestly not really worth it.
    let lib: &'static mut Library = Box::leak(Box::new(lib));
    // First: validate that the plugin is API-compatible
    {
        // Only load the `api_version`, which is guaranteed to be
        // the first field inside `Plugin`s across all versions of `api`.
        let api_version: Symbol<&u32> =
            unsafe { lib.get("PLUGIN") }.context("No PLUGIN symbol exported")?;
        if api_version != 1 {
            panic!(
                "API version mismatch: expected {}, got {}",
                1,
                api_version
            );
        }
    }
    // Okay, it's compatile! Now let's leak it and then load the plugin from the leaked ref.
    // Why leak? well TL;DR unloading a dylib is really really hard to do right, and honestly not really worth it.
    let lib: &'static mut Library = Box::leak(Box::new(lib));

    // *Now* we can properly use the plugin.
    let plugin: Symbol<&Plugin> = unsafe { lib.get("PLUGIN").unwrap() };
    (plugin.entrypoint)();
}
```

Hopefully the comments should be more or less self-explanatory, but let's look at some select bits of code to make sure.

```rust
let lib = unsafe { Library::new("path/to/your/libplugin.so").context("failed to find libplugin.so")? };
```
Here we load the `plugin` library. `libplugin.so` is the naming convention on Linux, 
so you will want to adapt this name for the platform you are targeting. E.g. on Windows, you would be looking for `plugin.dll` instead.
Instead of searching for a single specific plugin, you would probably also want to loop through all plugins in a 
well-known directory such as `plugins/` or `mods/`, so that users can drag new plugins in that directory on their own.
And finally, you'd want to not panic if the plugin is not found of course, but that goes for all instances of
`.expect` and `panic!` in this example. Error handling is important! But let's not get bogged down in that for this showcase.


```rust
let api_version: Symbol<&u32> =
            unsafe { lib.get("PLUGIN") }.context("No PLUGIN symbol exported")?;
```
This line searches for the plugin we exported earlier under the name `PLUGIN`, and loads it as a `u32`.
We can do that because we have guaranteed that the first field in the `Plugin` will always be a version number.
In a real host, you'd probably use some kind of semver scheme instead of just a single `u32`, 
but you get the idea. Just make sure that bit, which is usually called the *plugin header*, 
is `repr(C)` so that incompatible hosts can at least inspect it.

Assuming that the `api_version` is correct, we can now actually use the plugin.
But before, let's commit a memory leak on purpose!

```rust
let lib: &'static mut Library = Box::leak(Box::new(lib));
```

Eek, why would we do that?! Well, `libloading` has one mechanism that is a bit controversial: dropping the
`Library` variable will automatically unload the dynamic library. That *sounds* great, but remember that
the `plugin` is allowed to allocate and store memory freely. This memory will be freed, but not zeroed on an unload.
That means that if the `plugin` allocates a `Bunny` and passes it onto `api` for storage, then gets unloaded,
the `api` will now be holding an undead zombie `Bunny` that is pointing to invalid memory.
There's a ton of rather obscure ways in which similar things happen when unloading, leading to a zoo of
undefined behavior. As such, unloading a dynamic library is really really hard in practice, so it's common
to just leak it to avoid the hassle. Yes, that is a memory leak, but in most plugin systems, you will never 
load, unload, load, unload, load, unload a ton of plugins at runtime anyways. 
Usually it's just a check at startup, and once a plugin is loaded, there's little reason to unload it other than it crashing.

```rust
let plugin: Symbol<&Plugin> = unsafe { lib.get("PLUGIN") }.unwrap();
(plugin.entrypoint)();
```

We load the full `Plugin` now and finally call its entrypoint. 
At this point, you could also pass parameters to `entrypoint`, or check for a return value. As mentioned before,
this is a great time to check if any internal errors made the plugin panic and return a `Result` that communicates this fact.


Now that we have the full code, there is one annoying last thing we need to do. See, when we told Rust to use the dynamically linked standard library,
it will not automatically put that standard library into the right place to actually load it. Silly, I know.
You can manually find it on your machine and place it next to the `host` executable, 
but [`prefer-dynamic`](https://github.com/WilliamVenner/prefer-dynamic/) does that job for you.
Simply add it to your dependencies and it will move things where they need to be.
