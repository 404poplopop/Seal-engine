//! Project settings for raw mouse look and the gamepad light bar.

use seal_editor_core::{SettingsStore, MOUSE_LOOK_RAW, PAD_LIGHT};

#[test]
fn mouse_look_and_light_settings_reject_damaged_values() {
    let mut settings = SettingsStore::builtin();
    assert_eq!(settings.get(MOUSE_LOOK_RAW), Some("false"));
    assert_eq!(settings.get(PAD_LIGHT), Some(""));
    settings.set_project(MOUSE_LOOK_RAW, "true").unwrap();
    assert_eq!(settings.get(MOUSE_LOOK_RAW), Some("true"));
    assert!(settings.set_user(MOUSE_LOOK_RAW, "yes").is_err());
    settings.set_project(PAD_LIGHT, "10,20,30").unwrap();
    assert_eq!(settings.get(PAD_LIGHT), Some("10,20,30"));
    assert!(settings.set_project(PAD_LIGHT, "").is_ok());
    assert!(settings.set_project(PAD_LIGHT, "red").is_err());
    assert!(settings.set_project(PAD_LIGHT, "0,0,256").is_err());
    assert!(settings.set_project(PAD_LIGHT, "1,2").is_err());
}
