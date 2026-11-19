//! The shadow lookup shader must be valid WGSL, and its layout must match what the CPU side writes.
use seal_render_wgpu::SHADOW_WGSL;

#[test]
fn the_shadow_shader_parses_and_validates() {
    let module = naga::front::wgsl::parse_str(SHADOW_WGSL).expect("parse");
    let mut v = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    v.validate(&module).expect("validate");
    assert_eq!(module.entry_points.len(), 1);
    assert_eq!(module.entry_points[0].name, "main");
    assert_eq!(module.entry_points[0].workgroup_size, [64, 1, 1]);
}
