#![no_main]

use libfuzzer_sys::fuzz_target;
use seal_editor_core::journal::complete_records;
use seal_editor_core::journal_crc::decode_complete;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = complete_records(text);
    }
    let _ = decode_complete(data);
});
