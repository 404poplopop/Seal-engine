#![no_main]

use libfuzzer_sys::fuzz_target;
use seal_editor_core::Keymap;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = Keymap::from_text(text);
    }
});
