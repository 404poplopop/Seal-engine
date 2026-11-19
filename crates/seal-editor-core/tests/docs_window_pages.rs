//! System requirements and coordinate pages are part of the manual.

use std::path::PathBuf;

fn book(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/book/src").join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

#[test]
fn requirements_and_coordinates_are_linked() {
    let summary = book("SUMMARY.md");
    assert!(summary.contains("user/system-requirements.md"));
    assert!(summary.contains("user/coordinate-system.md"));
    let packaging = book("user/packaging.md");
    assert!(packaging.contains("system-requirements.md"));
    let intro = book("intro.md");
    assert!(intro.contains("system-requirements.md"));
    let viewport = book("user/viewport.md");
    assert!(viewport.contains("coordinate-system.md"));
    let requirements = book("user/system-requirements.md");
    assert!(requirements.contains("Vulkan"));
    assert!(requirements.contains("Direct3D 12"));
    assert!(requirements.contains("Metal"));
    assert!(requirements.contains("HDR output"));
    let axes = book("user/coordinate-system.md");
    assert!(axes.contains("left-handed"));
    assert!(axes.contains("metre"));
    assert!(axes.contains("degrees"));
}
