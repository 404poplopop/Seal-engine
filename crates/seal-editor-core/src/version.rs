/// Product version of Seal. Cargo packages use the semver form in `PACKAGE_VERSION`.
pub const ENGINE_VERSION: &str = "0.2";

/// Short commit hash baked in at build time.
pub const ENGINE_HASH: &str = env!("SEAL_GIT_HASH");

/// Product version plus the commit hash, for the window title and diagnostics.
pub const ENGINE_VERSION_FULL: &str = concat!("0.2+", env!("SEAL_GIT_HASH"));

/// Semver package version. `0.2.0` is Seal 0.2.
pub const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_version_is_0_2() {
        assert_eq!(ENGINE_VERSION, "0.2");
        assert_eq!(PACKAGE_VERSION, "0.2.0");
        assert!(ENGINE_VERSION_FULL.starts_with("0.2+"));
        assert!(!ENGINE_HASH.is_empty());
    }
}
