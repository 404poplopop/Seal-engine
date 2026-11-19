//! UTF-8 and UTF-16 conversion for graphs and UI helpers.

pub fn utf8_to_utf16_units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

pub fn utf16_units_to_utf8(units: &[u16]) -> Result<String, String> {
    String::from_utf16(units).map_err(|e| format!("invalid UTF-16: {e}"))
}

/// Comma-separated decimal UTF-16 code units (no spaces).
pub fn utf8_to_utf16_text(text: &str) -> String {
    utf8_to_utf16_units(text).into_iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",")
}

pub fn utf16_text_to_utf8(text: &str) -> Result<String, String> {
    if text.trim().is_empty() {
        return Ok(String::new());
    }
    let units: Result<Vec<u16>, String> = text
        .split(',')
        .map(|part| {
            part.trim().parse::<u32>().map_err(|_| format!("{part} is not a code unit")).and_then(
                |v| u16::try_from(v).map_err(|_| format!("{v} is out of range for UTF-16")),
            )
        })
        .collect();
    utf16_units_to_utf8(&units?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_round_trip_ascii() {
        let text = "Hello";
        let enc = utf8_to_utf16_text(text);
        assert_eq!(utf16_text_to_utf8(&enc).unwrap(), text);
    }

    #[test]
    fn utf16_round_trip_unicode() {
        let text = "café";
        let enc = utf8_to_utf16_text(text);
        assert_eq!(utf16_text_to_utf8(&enc).unwrap(), text);
    }

    #[test]
    fn damaged_utf16_is_refused() {
        assert!(utf16_text_to_utf8("72,0xFFFF").is_err());
    }
}
