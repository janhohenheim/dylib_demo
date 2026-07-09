use std::env::current_exe;

use api::{
    rootcause::{option_ext::OptionExt as _, prelude::ResultExt},
    *,
};
use libloading::{Library, Symbol};

fn main() -> Result {
    let mut app = App::new();

    // Here we read the single plugin from the same dir as the host executable,
    // but we could also load multiple plugins from a dedicated `plugins` directory.
    let exe_dir = current_exe()
        .context("failed to get current exe path")?
        .parent()
        .context("exe has no parent dir")?
        .to_path_buf();
    let plugin_path = exe_dir.join("libplugin.so");

    let lib = unsafe { Library::new(&plugin_path) }.context("failed to find libplugin.so")?;
    // First: validate that the plugin is API-compatible
    {
        // Only load the `PluginHeader`, which is guaranteed to be
        // the first field inside `Plugin`s across all versions of `api`.
        let header: Symbol<&PluginHeader> =
            unsafe { lib.get(PLUGIN_SYMBOL) }.context("No PLUGIN symbol exported")?;
        let api_version = header.api_version();
        if api_version != api::API_VERSION {
            rootcause::bail!(
                "API version mismatch: expected {}, got {}",
                api::API_VERSION,
                api_version
            );
        }
    }
    // Okay, it's compatile! Now let's leak it and then load the plugin from the leaked ref.
    // Why leak? well TL;DR unloading a dylib is really really hard to do right, and honestly not really worth it.
    let lib: &'static mut Library = Box::leak(Box::new(lib));

    // *Now* we can properly use the plugin.
    let plugin: Symbol<&Plugin> = unsafe { lib.get(PLUGIN_SYMBOL) }.unwrap();
    (plugin.entrypoint)(&mut app)?;
    Ok(())
}
