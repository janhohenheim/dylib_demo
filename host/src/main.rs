use api::*;
use libloading::{Library, Symbol};

fn main() {
    let mut person = Person {
        name: "hello".into(),
        age: 42,
    };

    unsafe {
        let lib = Library::new("target/debug/libplugin.so").expect("failed to find libplugin.so");
        let plugin: Symbol<&Plugin> = lib.get(b"PLUGIN").expect("No PLUGIN symbol exported");
        (plugin.entrypoint)(&mut person);
    }
}
