//! Builds a real `.msix` with MakeAppx downloaded the way Export Game does it.
//! Windows only, and it downloads about 22 MB: `cargo test -p seal-editor-core --test msix_build -- --ignored`.

#[test]
#[ignore = "downloads the Windows SDK Build Tools and runs MakeAppx"]
fn a_store_package_builds_with_the_downloaded_makeappx() {
    if !cfg!(windows) {
        return;
    }
    use seal_editor_core::msix_pack;
    let tool = msix_pack::download_makeappx().expect("MakeAppx downloads and unpacks");
    assert!(tool.is_file());
    assert_eq!(msix_pack::which_makeappx().ok().map(std::path::PathBuf::from), Some(tool));
    let dir = std::env::temp_dir().join(format!("seal-msix-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let layout = dir.join("layout");
    msix_pack::write_layout(&layout, "Seal Test", "CN=Seal", "1.0.0.0", "Game.exe").unwrap();
    std::fs::copy(std::env::current_exe().unwrap(), layout.join("Game.exe")).unwrap();
    let msix = dir.join("Seal Test.msix");
    msix_pack::try_build_msix(&layout, &msix).expect("MakeAppx builds the package");
    let size = std::fs::metadata(&msix).map(|m| m.len()).unwrap_or(0);
    println!("built {} ({size} bytes)", msix.display());
    assert!(size > 1000);
    let _ = std::fs::remove_dir_all(&dir);
}
