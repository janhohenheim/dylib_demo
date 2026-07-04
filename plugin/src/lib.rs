use api::*;

#[unsafe(no_mangle)]
pub static PLUGIN: Plugin = Plugin { entrypoint };

pub unsafe extern "C" fn entrypoint(person: &mut Person) {
    person.birthday();
    println!("{}!", person);
}
