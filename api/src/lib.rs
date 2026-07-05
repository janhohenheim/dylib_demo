// Third-party dependencies must be re-exported so they end up only once in the final build tree.
// Here we show how to do that for the `rootcause` crate, but the same is true for anything that either
// both `api` + `plugin` need, or both `api` + `host` need.
// safest is to re-export *all* dependencies of `api`.
pub use rootcause;

/// Safeguard so that the plugin and the host agree on which API version to use.
/// this could be a full semver identifier.
pub const API_VERSION: u32 = 1;
/// Document the name of the magic symbol we need to look for in the host process.
pub const PLUGIN_SYMBOL: &[u8] = b"PLUGIN";

// idiomatic use of `rootcause`, not directly relevant to dylibbing.
pub type Result<T = ()> = rootcause::Result<T>;

/// rmeta fixes our ABI stability issues for structs, but not for calling conventions,
/// so functions going across the ABI boundary must be marked as unsafe extern "C".
/// `App` is a pointer to signify that this is not really an actual Rust reference,
/// but a magic pointer coming from a host process.
///
/// This is allowed to be `extern "Rust"` because the calling convention is stable for a given ABI.
/// Since the ABI is compatible between all `api` consumers, this calling convention is also compatible.
pub type EntrypointFn = fn(*mut App) -> Result<()>;

/// The symbol that the plugin exposes and the host reads.
/// This is the only bit we keep at `repr(C)` so that `PluginHeader`
/// can be read even if the host and plugin use different versions of the `api` crate.
/// Since `PluginHeader` is `repr(C)` and guaranteed to be the first field of `Plugin`,
/// the host can always safely do a compatibility check before reading the entire `Plugin` struct.
#[repr(C)]
pub struct Plugin {
    /// Forbid the user from accidentally modifying the header by making it private.
    /// Always keep this as the first field.
    header: PluginHeader,
    pub entrypoint: EntrypointFn,
}

/// This struct is fixed in stone and must never change across API versions.
#[repr(C)]
pub struct PluginHeader {
    api_version: u32,
}

impl Plugin {
    pub const fn new(entrypoint: EntrypointFn) -> Self {
        Self {
            header: PluginHeader {
                api_version: API_VERSION,
            },
            entrypoint,
        }
    }

    pub const fn header(&self) -> &PluginHeader {
        &self.header
    }
}

impl PluginHeader {
    /// The `extern "C"` is not strictly required here I think,
    /// but let's be safe
    pub const extern "C" fn api_version(&self) -> u32 {
        self.api_version
    }
}

/// Whatever useful API we want to share between the plugin and the host.
pub struct App {
    internal_state: String,
}

impl App {
    pub fn new() -> Self {
        Self {
            internal_state: "some state".to_owned(),
        }
    }

    pub fn do_a_thing(&mut self) -> Result {
        println!("Running an app with state {}", self.internal_state);
        Ok(())
    }
}
