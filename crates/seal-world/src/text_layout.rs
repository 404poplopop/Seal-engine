//! Simple word-wrapped line layout with glyph positions (monospace estimate).

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutGlyph {
    pub ch: char,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutLine {
    pub text: String,
    pub glyphs: Vec<LayoutGlyph>,
}

/// Wrap `text` into lines that fit `width` using `font_size` as the em height.
/// Returns one entry per line with glyph positions in local line coordinates.
pub fn layout_lines(text: &str, width: f32, font_size: f32) -> Vec<LayoutLine> {
    let width = width.max(font_size);
    let advance = font_size * 0.55;
    let line_height = font_size * 1.2;
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut glyphs = Vec::new();
    let mut x = 0.0_f32;
    let mut y = 0.0_f32;

    let flush = |lines: &mut Vec<LayoutLine>,
                 current: &mut String,
                 glyphs: &mut Vec<LayoutGlyph>| {
        if current.is_empty() && glyphs.is_empty() {
            return;
        }
        lines.push(LayoutLine { text: std::mem::take(current), glyphs: std::mem::take(glyphs) });
    };

    for word in text.split_whitespace() {
        let word_width = word.chars().count() as f32 * advance;
        let need_space = !current.is_empty();
        let extra = if need_space { advance } else { 0.0 };
        if x + extra + word_width > width && !current.is_empty() {
            flush(&mut lines, &mut current, &mut glyphs);
            x = 0.0;
            y += line_height;
        }
        if need_space {
            current.push(' ');
            glyphs.push(LayoutGlyph { ch: ' ', x, y });
            x += advance;
        }
        for ch in word.chars() {
            current.push(ch);
            glyphs.push(LayoutGlyph { ch, x, y });
            x += advance;
        }
    }
    flush(&mut lines, &mut current, &mut glyphs);
    if lines.is_empty() && !text.is_empty() {
        lines.push(LayoutLine { text: text.to_string(), glyphs: Vec::new() });
    }
    lines
}

/// SealText-friendly dump: lines joined with newlines, positions as `x,y` per glyph comma-separated.
pub fn layout_lines_text(text: &str, width: f32, font_size: f32) -> String {
    let lines = layout_lines(text, width, font_size);
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&line.text);
        out.push('|');
        for (j, g) in line.glyphs.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            out.push_str(&format!("{:.3},{:.3}", g.x, g.y));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_width_breaks_lines() {
        let lines = layout_lines("one two three", 40.0, 16.0);
        assert!(lines.len() >= 2);
        assert!(lines[0].glyphs.len() >= 3);
    }

    #[test]
    fn layout_text_round_trip_shape() {
        let packed = layout_lines_text("ab cd", 80.0, 10.0);
        assert!(packed.contains('|'));
        assert!(packed.contains(','));
    }
}
