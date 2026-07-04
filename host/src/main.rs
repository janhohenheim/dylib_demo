use common::*;
use libloading::{Library, Symbol};

fn main() {
    let mut person = Person {
        name: "hello".into(),
        age: 42,
    };

    unsafe {
        let lib = Library::new("target/debug/libplugin.so").expect("failed to find libplugin.so");
        let entrypoint: Symbol<EntrypointFn> = lib
            .get(b"ENTRYPOINT")
            .expect("No ENTRYPOINT symbol exported");
        entrypoint(&mut person);
    }
}
