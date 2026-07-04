use common::*;

#[unsafe(no_mangle)]
pub static ENTRYPOINT: EntrypointFn = entrypoint;

pub unsafe extern "C" fn entrypoint(person: &mut Person) {
    person.birthday();
    println!("{}!", person);
}
