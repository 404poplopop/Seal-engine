//! Inline pictures in label text, string tables, and per-character text motion.
//!
//! A label or button may contain `[img=path]` tags. A path ending in a picture
//! extension or `.frames` is a sprite (a sprite animation plays its clip). A
//! tag that parses but does not name one of those files is an unknown asset.
//! A tag missing its closing bracket is ordinary text.
//!
//! A `.strings` file is a SealText map of keys to text. `{name}` in an entry
//! is replaced when a value for `name` is supplied. A label whose text is
//! `@key` (optionally followed by `name=value` pairs) shows the entry.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

use seal_sealtext::SealValue;

pub const STRINGS_EXT: &str = "strings";

const PICTURE_EXTS: &[&str] = &["png", "jpg", "jpeg", "bmp", "gif", "frames"];

/// One run of a label after tags are split out.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Text(String),
    /// `known` is false when `path` is not a picture or a sprite animation.
    Sprite {
        path: String,
        clip: String,
        known: bool,
    },
}

/// One character or inline sprite, placed on a line that starts at x = 0.
#[derive(Clone, Debug, PartialEq)]
pub enum Glyph {
    Char { ch: char, x: f32, w: f32 },
    Sprite { path: String, clip: String, known: bool, x: f32, w: f32 },
}

/// Split `text` into words and inline picture tags.
pub fn parse_inline(text: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("[img=") {
        buf.push_str(&rest[..at]);
        let after = &rest[at + 5..];
        match after.find(']') {
            Some(end) => {
                flush_text(&mut buf, &mut out);
                let body = &after[..end];
                let (path, clip) = split_clip(body.trim());
                let known = known_asset(&path);
                out.push(Piece::Sprite { path, clip, known });
                rest = &after[end + 1..];
            }
            None => {
                buf.push_str(&rest[at..]);
                rest = "";
            }
        }
    }
    buf.push_str(rest);
    flush_text(&mut buf, &mut out);
    out
}

fn flush_text(buf: &mut String, out: &mut Vec<Piece>) {
    if !buf.is_empty() {
        out.push(Piece::Text(std::mem::take(buf)));
    }
}

fn split_clip(body: &str) -> (String, String) {
    match body.split_once('#') {
        Some((path, clip)) => (path.trim().replace('\\', "/"), clip.trim().to_string()),
        None => (body.replace('\\', "/"), String::new()),
    }
}

fn known_asset(path: &str) -> bool {
    if path.is_empty() || path.split('/').any(|p| p == "..") {
        return false;
    }
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    PICTURE_EXTS.contains(&ext.as_str())
}

/// Glyphs of `text` at font `size`. A picture is a square of that size.
pub fn layout_glyphs(text: &str, size: f32) -> Vec<Glyph> {
    let advance = size * 0.55;
    let mut x = 0.0;
    let mut glyphs = Vec::new();
    for piece in parse_inline(text) {
        match piece {
            Piece::Text(s) => {
                for ch in s.chars() {
                    glyphs.push(Glyph::Char { ch, x, w: advance });
                    x += advance;
                }
            }
            Piece::Sprite { path, clip, known } => {
                glyphs.push(Glyph::Sprite { path, clip, known, x, w: size });
                x += size;
            }
        }
    }
    glyphs
}

/// Width of `text` at font `size`, counting inline pictures as one square.
pub fn measure_width(text: &str, size: f32) -> f32 {
    if !text.contains("[img=") {
        return text.chars().count() as f32 * size * 0.55;
    }
    layout_glyphs(text, size).iter().map(glyph_width).sum()
}

fn glyph_width(g: &Glyph) -> f32 {
    match g {
        Glyph::Char { w, .. } | Glyph::Sprite { w, .. } => *w,
    }
}

/// How many glyphs a typewriter has revealed. `shown` is a count, not pixels.
pub fn visible_glyphs(glyphs: &[Glyph], shown: f32) -> usize {
    let n = glyphs.len();
    if shown <= 0.0 {
        0
    } else if shown >= n as f32 {
        n
    } else {
        shown.floor() as usize
    }
}

/// Advance a typewriter. Returns the new count and whether it just finished.
pub fn step_typewriter(shown: f32, chars_per_second: f32, dt: f32, total: usize) -> (f32, bool) {
    let total = total as f32;
    if chars_per_second <= 0.0 || total <= 0.0 {
        return (total, false);
    }
    if shown >= total {
        return (total, false);
    }
    let dt = if dt.is_finite() && dt > 0.0 { dt } else { 0.0 };
    let next = (shown + chars_per_second * dt).min(total);
    (next, next >= total)
}

/// Pixel shift of glyph `index` at `time` seconds.
/// Wave moves up and down. Shake moves on both axes. Speeds are cycles per second.
pub fn glyph_shift(
    index: usize,
    time: f32,
    wave_amp: f32,
    wave_speed: f32,
    shake_amp: f32,
    shake_speed: f32,
) -> [f32; 2] {
    let time = if time.is_finite() { time } else { 0.0 };
    let wave = if wave_amp == 0.0 || wave_speed == 0.0 {
        0.0
    } else {
        let phase = (time * wave_speed + index as f32 * 0.35) * std::f32::consts::TAU;
        wave_amp * phase.sin()
    };
    if shake_amp == 0.0 || shake_speed == 0.0 {
        return [0.0, wave];
    }
    let x = shake_amp * ((time * shake_speed + index as f32 * 0.5) * std::f32::consts::TAU).sin();
    let y = shake_amp
        * ((time * shake_speed * 1.3 + index as f32 * 0.2) * std::f32::consts::TAU + 1.2).sin();
    [x, wave + y]
}

/// A label that starts with `@key`, plus optional `name=value` pairs.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelKey {
    pub key: String,
    pub values: Vec<(String, String)>,
}

pub fn label_key(text: &str) -> Option<LabelKey> {
    let text = text.trim();
    let rest = text.strip_prefix('@')?;
    let (key, mut rest) = take_word(rest)?;
    if !is_key(&key) {
        return None;
    }
    let mut values = Vec::new();
    rest = rest.trim_start();
    while !rest.is_empty() {
        let (pair, next) = take_assignment(rest)?;
        values.push(pair);
        rest = next.trim_start();
    }
    Some(LabelKey { key, values })
}

fn take_word(text: &str) -> Option<(String, &str)> {
    let text = text.trim_start();
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    if end == 0 {
        return None;
    }
    Some((text[..end].to_string(), &text[end..]))
}

fn take_assignment(text: &str) -> Option<((String, String), &str)> {
    let eq = text.find('=')?;
    if eq == 0 || text[..eq].chars().any(char::is_whitespace) {
        return None;
    }
    let name = &text[..eq];
    if !is_key(name) {
        return None;
    }
    let rest = &text[eq + 1..];
    if rest.starts_with('"') {
        let (value, next) = take_quoted(rest)?;
        Some(((name.to_string(), value), next))
    } else {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if end == 0 {
            return None;
        }
        Some(((name.to_string(), rest[..end].to_string()), &rest[end..]))
    }
}

fn take_quoted(text: &str) -> Option<(String, &str)> {
    let rest = text.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.char_indices();
    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            if let Some((_, n)) = chars.next() {
                out.push(n);
            }
        } else if c == '"' {
            return Some((out, &rest[i + c.len_utf8()..]));
        } else {
            out.push(c);
        }
    }
    None
}

pub fn is_table_key(name: &str) -> bool {
    is_key(name)
}

fn is_key(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '/' || c == '-')
}

/// `name=value` pairs separated by spaces. A value may be quoted.
/// A broken list yields nothing, so a placeholder is left as written.
pub fn parse_assignments(text: &str) -> Vec<(String, String)> {
    let mut rest = text.trim();
    if rest.is_empty() {
        return Vec::new();
    }
    let mut values = Vec::new();
    while !rest.is_empty() {
        let Some((pair, next)) = take_assignment(rest) else {
            return Vec::new();
        };
        values.push(pair);
        rest = next.trim_start();
    }
    values
}

/// One string table: keys in file order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StringTable {
    pub rows: Vec<(String, String)>,
}

impl StringTable {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.rows.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn parse(text: &str) -> Result<StringTable, String> {
        let value =
            seal_sealtext::parse(text).map_err(|e| format!("string table is damaged: {e}"))?;
        let SealValue::Map(map) = value else {
            return Err("string table is damaged: the file must be a map of keys".into());
        };
        let mut rows = Vec::new();
        for (key, value) in &map.entries {
            if key.is_empty() || !is_key(key) {
                return Err(format!("string table is damaged: \"{key}\" is not a key"));
            }
            let SealValue::String(text) = value else {
                return Err(format!("string table is damaged: \"{key}\" must be text"));
            };
            if rows.iter().any(|(k, _)| k == key) {
                return Err(format!("string table is damaged: \"{key}\" is listed twice"));
            }
            rows.push((key.clone(), text.clone()));
        }
        Ok(StringTable { rows })
    }

    pub fn to_text(&self) -> String {
        let entries =
            self.rows.iter().map(|(k, v)| (k.clone(), SealValue::String(v.clone()))).collect();
        format!(
            "# Seal string table\n{}",
            SealValue::Map(seal_sealtext::SealMap::new(entries)).to_text()
        )
    }

    pub fn load(path: &Path) -> Result<StringTable, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        StringTable::parse(&text)
    }
}

/// The text for `key`, with `{name}` filled from `values`.
/// A missing key yields the key itself and `missing` set.
#[derive(Clone, Debug, PartialEq)]
pub struct Lookup {
    pub text: String,
    pub missing: bool,
}

pub fn lookup(table: &StringTable, key: &str, values: &[(String, String)]) -> Lookup {
    let Some(template) = table.get(key) else {
        return Lookup { text: key.to_string(), missing: true };
    };
    Lookup { text: fill_placeholders(template, values), missing: false }
}

pub fn missing_key_warning(key: &str) -> String {
    format!("warning: string table has no \"{key}\"")
}

fn fill_placeholders(template: &str, values: &[(String, String)]) -> String {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' {
            if let Some(end) = chars[i + 1..].iter().position(|c| *c == '}') {
                let name: String = chars[i + 1..i + 1 + end].iter().collect();
                if is_key(&name) {
                    if let Some((_, value)) = values.iter().find(|(k, _)| k == &name) {
                        out.push_str(value);
                    } else {
                        out.push('{');
                        out.push_str(&name);
                        out.push('}');
                    }
                    i += end + 2;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

type TableCache = Mutex<Vec<(PathBuf, Option<SystemTime>, Result<Arc<StringTable>, String>)>>;

fn table_cache() -> &'static TableCache {
    static CACHE: OnceLock<TableCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Vec::new()))
}

/// The table at `path`, read again when the file changes.
pub fn load_table_cached(path: &Path) -> Result<Arc<StringTable>, String> {
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let mut map = table_cache().lock().map_err(|_| "string table cache is busy".to_string())?;
    if let Some((_, seen, hit)) = map.iter().find(|(p, _, _)| p == path) {
        if *seen == mtime && mtime.is_some() {
            return hit.clone();
        }
    }
    map.retain(|(p, _, _)| p != path);
    let loaded = StringTable::load(path).map(Arc::new);
    map.push((path.to_path_buf(), mtime, loaded.clone()));
    loaded
}

pub fn invalidate_table(path: &Path) {
    if let Ok(mut map) = table_cache().lock() {
        map.retain(|(p, _, _)| p != path);
    }
}

/// Picture file and texture rectangle for an inline tag at `seconds`.
/// A sprite animation uses its named clip, or the first clip.
pub fn inline_image(
    project: Option<&Path>,
    path: &str,
    clip: &str,
    seconds: f32,
) -> Option<(PathBuf, [f32; 4])> {
    let rel = Path::new(path);
    let full = match project {
        Some(root) if rel.is_relative() => root.join(rel),
        _ => rel.to_path_buf(),
    };
    if !crate::frames::is_frames_path(path) {
        return Some((full, [0.0, 0.0, 1.0, 1.0]));
    }
    let lib = crate::frames::load_cached(&full)?;
    let clip = lib.clip_or_first(clip)?;
    let frame = clip.frames.get(clip.frame_at(seconds.max(0.0)))?;
    let dir = full.parent().unwrap_or(Path::new(""));
    Some((dir.join(&frame.image), frame.uv))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_parse_as_sprites_unknown_assets_or_plain_text() {
        let pieces = parse_inline("Pay [img=sprites/coin.png] now");
        assert_eq!(
            pieces,
            vec![
                Piece::Text("Pay ".into()),
                Piece::Sprite { path: "sprites/coin.png".into(), clip: String::new(), known: true },
                Piece::Text(" now".into()),
            ]
        );
        let anim = parse_inline("[img=anims/coin.frames#spin]");
        assert_eq!(
            anim,
            vec![Piece::Sprite {
                path: "anims/coin.frames".into(),
                clip: "spin".into(),
                known: true
            }]
        );
        let unknown = parse_inline("[img=notes.txt]");
        assert!(matches!(
            &unknown[..],
            [Piece::Sprite { path, known: false, .. }] if path == "notes.txt"
        ));
        let empty = parse_inline("[img=]");
        assert!(
            matches!(&empty[..], [Piece::Sprite { path, known: false, .. }] if path.is_empty())
        );
        let broken = parse_inline("see [img=sprites/coin.png");
        assert_eq!(broken, vec![Piece::Text("see [img=sprites/coin.png".into())]);
        assert_eq!(parse_inline("[img]"), vec![Piece::Text("[img]".into())]);
    }

    #[test]
    fn placeholders_fill_and_a_missing_key_is_the_key() {
        let table =
            StringTable::parse("coin = \"{count} coins\"\nhello = \"Hi {name}\"\n").unwrap();
        let found = lookup(&table, "coin", &[("count".into(), "3".into())]);
        assert_eq!(found, Lookup { text: "3 coins".into(), missing: false });
        let kept = lookup(&table, "hello", &[]);
        assert_eq!(kept.text, "Hi {name}");
        let missing = lookup(&table, "gem", &[]);
        assert_eq!(missing, Lookup { text: "gem".into(), missing: true });
        assert!(missing_key_warning("gem").starts_with("warning:"));
    }

    #[test]
    fn string_table_round_trip_and_damaged_text() {
        let text = StringTable {
            rows: vec![("title".into(), "Hello {name}".into()), ("coin".into(), "x".into())],
        }
        .to_text();
        let back = StringTable::parse(&text).unwrap();
        assert_eq!(back.to_text(), text);
        assert!(StringTable::parse("coin = {").is_err());
        assert!(StringTable::parse("coin = 1\n").is_err());
    }

    #[test]
    fn typewriter_reaches_the_end_once() {
        let (n, done) = step_typewriter(0.0, 10.0, 0.1, 5);
        assert!((n - 1.0).abs() < 1e-4 && !done);
        let (n, done) = step_typewriter(4.5, 10.0, 0.1, 5);
        assert!((n - 5.0).abs() < 1e-4 && done);
        let (n, done) = step_typewriter(5.0, 10.0, 0.1, 5);
        assert!((n - 5.0).abs() < 1e-4 && !done);
    }

    #[test]
    fn wave_is_zero_at_the_start_and_peaks_a_quarter_cycle_later() {
        let [x, y] = glyph_shift(0, 0.0, 4.0, 1.0, 0.0, 0.0);
        assert_eq!((x, y), (0.0, 0.0));
        let [_, y] = glyph_shift(0, 0.25, 4.0, 1.0, 0.0, 0.0);
        assert!((y - 4.0).abs() < 1e-3, "{y}");
    }

    #[test]
    fn label_keys_carry_values() {
        let k = label_key("@coin count=3 name=\"Ada B\"").unwrap();
        assert_eq!(k.key, "coin");
        assert_eq!(k.values, vec![("count".into(), "3".into()), ("name".into(), "Ada B".into())]);
        assert!(label_key("hello").is_none());
        assert!(label_key("@").is_none());
    }
}
