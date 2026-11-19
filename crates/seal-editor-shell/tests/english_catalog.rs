#[test]
fn the_english_catalog_matches_editor_sources() {
    seal_editor_core::verify_english_catalog().expect("English catalog");
}
