fn main() {
    #[cfg(not(all(windows, target_env = "gnu")))]
    {
        if std::env::var("CARGO_CFG_TARGET_FAMILY").as_deref() == Ok("unix") {
            println!("cargo:rustc-link-lib=dylib=stdc++");
        }
    }
}