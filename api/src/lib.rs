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
pub type EntrypointFn = extern "C" fn(*mut App) -> Result<()>;

/// The symbol that the plugin exposes and the host reads.
#[repr(C)]
pub struct Plugin {
    /// Forbid the user from accidentally modifying the API version by making it private.
    api_version: u32,
    pub entrypoint: EntrypointFn,
}

impl Plugin {
    pub const fn new(entrypoint: EntrypointFn) -> Self {
        Self {
            api_version: API_VERSION,
            entrypoint,
        }
    }

    pub const fn api_version(&self) -> u32 {
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
