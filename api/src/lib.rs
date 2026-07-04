use std::fmt::Display;

#[repr(C)]
pub struct Plugin {
    pub entrypoint: EntrypointFn,
}

// rmeta fixes our ABI stability issues for structs, but not for calling conventions,
// so functions going across the ABI boundary must be marked as unsafe extern "C".
pub type EntrypointFn = unsafe extern "C" fn(&mut Person);

pub struct Person {
    pub name: String,
    pub age: u32,
}

impl Person {
    pub extern "C" fn birthday(&mut self) {
        self.age += 1;
    }
}

impl Display for Person {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Hello, my name is {} and I am {} years old!",
            self.name, self.age
        )
    }
}
