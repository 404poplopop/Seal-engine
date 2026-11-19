#![no_main]

use libfuzzer_sys::fuzz_target;
use seal_editor_core::Layout;
use seal_sealtext::parse_with_limits;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        if let Ok(value) = parse_with_limits(text, &seal_sealtext::Limits::default()) {
            let _ = Layout::from_value(&value);
        }
    }
});
