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
affects type layouts. There are some exceptions however. Treat the following as a non-exhaustive list, since I bet
there are some I don't know about:
- `-Z randomize-layout / -Z layout-seed`: This randomizes type layouts on builds, breaking all of our assumptions.
  While this doesn't come up in regular builds, there are some that like to run `miri` checks with this option enabled.
- `-C target-feature=-crt-static`: forces static linking.
- `-C soft-float`: changing the calling conventions of all things touching floating point numbers to use software instead of hardware implementations.
  This comes up often when building for targets such as embedded where there may be no hardware floating point support.
  If you want to support plugins for such targets, you will need to also ship `api`/`host` with `-C soft-float` enabled.
- `-C panic`: changes the panic strategy. For the standard library, the default is `unwind`, but you can set it to `abort` to make panics non-recoverable.
  If you build for `no_std`, you will need to specify your own panic handler.
  Regardless, `api`/`host` and `plugin` need to agree on the panic strategy used.

## Some words on plugin systems in general

Now that we know how the `rmeta` trick works and what to watch out for, let's dive into some more general
considerations when building plugin systems. Most of this is not specific to the `rmeta` trick, but important to
know when you want to actually make use of it.




TODO:
- panic unwind in plugin
  - don't panic in api
- API version check
- entrypoint needs to be `repr(C)`
- `#[unsafe(no_mangle)]`
- this works when distributing release
- this works when using different rustflags (with a few rare exceptions)
- `rust-toolchain.toml`

## Caveats

TODO:
- maybe r-a overrides stuff
- get libstd.so into the right dir
- leak libloading instead of dropping
- allocator
- platform
- target-feature / simd
