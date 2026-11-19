//! Imported TTF/OTF fonts: what a file holds, and whether Seal can draw text with it.

use skrifa::instance::{LocationRef, Size};
use skrifa::raw::TableProvider;
use skrifa::{string::StringId, MetadataProvider};

/// What the Asset Editor shows about a font file.
#[derive(Clone, Debug, PartialEq)]
pub struct FontInfo {
    /// Family name from the file, e.g. "Ubuntu".
    pub family: String,
    /// Style name from the file, e.g. "Light" or "Bold Italic".
    pub style: String,
    pub glyphs: u32,
    /// Whether the file has italic or oblique letters.
    pub italic: bool,
    /// 100 (thin) to 900 (black); 400 is regular.
    pub weight: f32,
}

/// Read a font file. On failure, returns a message id from the English catalog
/// (`font-error-*`) for the Asset Editor and the Console.
pub fn font_info(bytes: &[u8]) -> Result<FontInfo, &'static str> {
    if bytes.is_empty() {
        return Err("font-error-empty");
    }
    let font = skrifa::FontRef::new(bytes).map_err(|_| "font-error-unreadable")?;
    // The table list reads lazily; a cut-off file fails on its core tables.
    if font.head().is_err() || font.maxp().is_err() || font.cmap().is_err() || font.hhea().is_err()
    {
        return Err("font-error-unreadable");
    }
    let charmap = font.charmap();
    let outlines = font.outline_glyphs();
    // The text drawer needs outlines for ordinary letters.
    let letter = charmap.map('A').or_else(|| charmap.map('a')).or_else(|| charmap.map('0'));
    match letter {
        Some(g) if outlines.get(g).is_some() => {}
        _ => return Err("font-error-no-letters"),
    }
    let name = |id| {
        font.localized_strings(id).english_or_first().map(|s| s.to_string()).unwrap_or_default()
    };
    let family = {
        let typo = name(StringId::TYPOGRAPHIC_FAMILY_NAME);
        if typo.is_empty() {
            name(StringId::FAMILY_NAME)
        } else {
            typo
        }
    };
    let style = {
        let typo = name(StringId::TYPOGRAPHIC_SUBFAMILY_NAME);
        if typo.is_empty() {
            name(StringId::SUBFAMILY_NAME)
        } else {
            typo
        }
    };
    let attrs = font.attributes();
    Ok(FontInfo {
        family: if family.is_empty() { "Unnamed font".into() } else { family },
        style: if style.is_empty() { "Regular".into() } else { style },
        glyphs: font.metrics(Size::unscaled(), LocationRef::default()).glyph_count as u32,
        italic: attrs.style != skrifa::attribute::Style::Normal,
        weight: attrs.weight.value(),
    })
}

/// Whether a file name is a font Seal imports.
pub fn is_font_file(name: &str) -> bool {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    name.contains('.') && crate::gameui::FONT_EXTS.contains(&ext.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn junk_and_empty_files_are_refused_with_a_reason() {
        assert_eq!(font_info(b""), Err("font-error-empty"));
        assert_eq!(font_info(b"not a font at all"), Err("font-error-unreadable"));
    }

    #[test]
    fn font_files_are_known_by_extension() {
        assert!(is_font_file("fonts/Title.TTF"));
        assert!(is_font_file("a.otf"));
        assert!(!is_font_file("ttf"));
        assert!(!is_font_file("a.woff"));
    }
}
