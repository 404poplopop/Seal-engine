//! SealText is the human-readable value tree used by editor files.
//!
//! A file is either one value or a top-level map written as `key = value` lines.
//!
//! # Supported subset
//!
//! Values are maps, arrays, strings, numbers, booleans, and null. Numbers keep their
//! decimal spelling so conversion stays exact.
//!
//! Full-line `#` comments on a top-level map are preserved (file header and before
//! each entry) for round-trip load and write.
//!
//! Unknown keys stay in the map, so a newer file can be loaded and written back
//! without dropping fields this version does not understand.
//!
//! Default [`Limits`] for parse and write: nesting depth 128, file size 64 MiB, and
//! 16 MiB per string.
//!
//! JSON uses its own null and numbers. RON has no null, so null is written as `None`.
#![forbid(unsafe_code)]

use std::fmt::Write as _;
use std::str::FromStr;

/// Default parser and writer limits (STX-006).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub max_depth: u32,
    pub max_file_bytes: usize,
    pub max_string_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_depth: 128,
            max_file_bytes: 64 * 1024 * 1024,
            max_string_bytes: 16 * 1024 * 1024,
        }
    }
}

/// Full-line comments preserved for a top-level map.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SealComments {
    pub header: Vec<String>,
    pub before_entry: Vec<Vec<String>>,
}

/// A map value with optional preserved comments for round trips.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SealMap {
    pub entries: Vec<(String, SealValue)>,
    comments: SealComments,
}

impl SealMap {
    pub fn new(entries: Vec<(String, SealValue)>) -> Self {
        Self { entries, comments: SealComments::default() }
    }

    pub fn with_comments(entries: Vec<(String, SealValue)>, comments: SealComments) -> Self {
        Self { entries, comments }
    }

    pub fn comments(&self) -> &SealComments {
        &self.comments
    }

    pub fn comments_mut(&mut self) -> &mut SealComments {
        &mut self.comments
    }
}

impl From<Vec<(String, SealValue)>> for SealMap {
    fn from(entries: Vec<(String, SealValue)>) -> Self {
        Self::new(entries)
    }
}

/// A SealText value. Numbers keep their decimal spelling so the conversion is exact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SealValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<SealValue>),
    Map(SealMap),
}

/// Where a parse failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextError {
    pub line: u32,
    pub column: u32,
    pub message: String,
}

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for TextError {}

impl SealValue {
    pub fn as_map(&self) -> Option<&[(String, SealValue)]> {
        match self {
            SealValue::Map(map) => Some(&map.entries),
            _ => None,
        }
    }

    pub fn as_map_mut(&mut self) -> Option<&mut Vec<(String, SealValue)>> {
        match self {
            SealValue::Map(map) => Some(&mut map.entries),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&SealValue> {
        self.as_map()?.iter().find(|(name, _)| name == key).map(|(_, value)| value)
    }

    pub fn insert(&mut self, key: impl Into<String>, value: SealValue) {
        let key = key.into();
        let entries = match self {
            SealValue::Map(map) => &mut map.entries,
            _ => return,
        };
        if let Some((_, existing)) = entries.iter_mut().find(|(name, _)| *name == key) {
            *existing = value;
        } else {
            entries.push((key, value));
        }
    }

    pub fn to_text(&self) -> String {
        let open = Limits {
            max_depth: u32::MAX,
            max_file_bytes: usize::MAX,
            max_string_bytes: usize::MAX,
        };
        self.try_to_text(&open).unwrap_or_default()
    }

    pub fn try_to_text(&self, limits: &Limits) -> Result<String, TextError> {
        let mut out = String::new();
        write_value(&mut out, self, 0, true, limits)?;
        if !out.ends_with('\n') {
            out.push('\n');
        }
        if out.len() > limits.max_file_bytes {
            return Err(TextError {
                line: 1,
                column: 1,
                message: format!("output exceeds {} bytes", limits.max_file_bytes),
            });
        }
        Ok(out)
    }

    pub fn to_json(&self) -> serde_json::Value {
        match self {
            SealValue::Null => serde_json::Value::Null,
            SealValue::Bool(value) => serde_json::Value::Bool(*value),
            SealValue::Number(lexeme) => match serde_json::Number::from_str(lexeme) {
                Ok(number) => serde_json::Value::Number(number),
                Err(_) => serde_json::Value::String(lexeme.clone()),
            },
            SealValue::String(value) => serde_json::Value::String(value.clone()),
            SealValue::Array(items) => {
                serde_json::Value::Array(items.iter().map(SealValue::to_json).collect())
            }
            SealValue::Map(map) => {
                let mut object = serde_json::Map::new();
                for (key, value) in &map.entries {
                    object.insert(key.clone(), value.to_json());
                }
                serde_json::Value::Object(object)
            }
        }
    }

    pub fn from_json(value: &serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => SealValue::Null,
            serde_json::Value::Bool(value) => SealValue::Bool(*value),
            serde_json::Value::Number(value) => SealValue::Number(value.to_string()),
            serde_json::Value::String(value) => SealValue::String(value.clone()),
            serde_json::Value::Array(items) => {
                SealValue::Array(items.iter().map(SealValue::from_json).collect())
            }
            serde_json::Value::Object(map) => SealValue::Map(SealMap::new(
                map.iter().map(|(key, value)| (key.clone(), SealValue::from_json(value))).collect(),
            )),
        }
    }

    /// RON subset. Null is `None`.
    pub fn to_ron(&self) -> String {
        let mut out = String::new();
        write_ron(&mut out, self);
        out
    }

    pub fn from_ron(text: &str) -> Result<Self, TextError> {
        if text.len() > Limits::default().max_file_bytes {
            return Err(TextError {
                line: 1,
                column: 1,
                message: format!("input exceeds {} bytes", Limits::default().max_file_bytes),
            });
        }
        let mut parser = Parser::new(text, Limits::default());
        parser.skip();
        let value = parser.parse_ron_value(0)?;
        parser.skip();
        if !parser.eof() {
            return Err(parser.error("unexpected trailing input"));
        }
        Ok(value)
    }
}

fn write_ron(out: &mut String, value: &SealValue) {
    match value {
        SealValue::Null => out.push_str("None"),
        SealValue::Bool(true) => out.push_str("true"),
        SealValue::Bool(false) => out.push_str("false"),
        SealValue::Number(lexeme) => out.push_str(lexeme),
        SealValue::String(value) => push_quoted(out, value),
        SealValue::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write_ron(out, item);
            }
            out.push(']');
        }
        SealValue::Map(map) => {
            out.push('{');
            for (index, (key, item)) in map.entries.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                push_quoted(out, key);
                out.push_str(": ");
                write_ron(out, item);
            }
            out.push('}');
        }
    }
}

pub fn parse(text: &str) -> Result<SealValue, TextError> {
    parse_with_limits(text, &Limits::default())
}

pub fn parse_with_limits(text: &str, limits: &Limits) -> Result<SealValue, TextError> {
    if text.len() > limits.max_file_bytes {
        return Err(TextError {
            line: 1,
            column: 1,
            message: format!("input exceeds {} bytes", limits.max_file_bytes),
        });
    }
    let mut parser = Parser::new(text, *limits);
    parser.skip_whitespace_only();
    if parser.eof() {
        return Ok(SealValue::Map(SealMap::new(Vec::new())));
    }
    let value = if parser.looks_like_entries() {
        parser.parse_entries()?
    } else {
        parser.skip();
        parser.parse_value(0)?
    };
    parser.skip();
    if !parser.eof() {
        return Err(parser.error("unexpected trailing input"));
    }
    Ok(value)
}

struct Parser<'a> {
    src: &'a str,
    bytes: &'a [u8],
    index: usize,
    limits: Limits,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str, limits: Limits) -> Self {
        Self { src, bytes: src.as_bytes(), index: 0, limits }
    }

    fn eof(&self) -> bool {
        self.index >= self.bytes.len()
    }

    fn peek(&self) -> Option<char> {
        self.src[self.index..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.index += ch.len_utf8();
        Some(ch)
    }

    fn starts_with(&self, text: &str) -> bool {
        self.src[self.index..].starts_with(text)
    }

    fn error(&self, message: impl Into<String>) -> TextError {
        let (line, column) = self.position();
        TextError { line, column, message: message.into() }
    }

    fn position(&self) -> (u32, u32) {
        let mut line = 1u32;
        let mut column = 1u32;
        for ch in self.src[..self.index].chars() {
            if ch == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
        (line, column)
    }

    fn skip(&mut self) {
        loop {
            while matches!(self.peek(), Some(ch) if ch.is_whitespace()) {
                self.bump();
            }
            if self.peek() == Some('#') {
                while let Some(ch) = self.peek() {
                    self.bump();
                    if ch == '\n' {
                        break;
                    }
                }
                continue;
            }
            break;
        }
    }

    fn skip_whitespace_only(&mut self) {
        while matches!(self.peek(), Some(ch) if ch.is_whitespace()) {
            self.bump();
        }
    }

    fn read_comment_line(&mut self) -> Result<String, TextError> {
        if self.peek() != Some('#') {
            return Err(self.error("expected a comment"));
        }
        self.bump();
        let start = self.index;
        while let Some(ch) = self.peek() {
            if ch == '\n' {
                break;
            }
            self.bump();
        }
        Ok(self.src[start..self.index].trim().to_string())
    }

    fn collect_comments(&mut self) -> Result<Vec<String>, TextError> {
        let mut lines = Vec::new();
        loop {
            self.skip_whitespace_only();
            if self.peek() == Some('#') {
                lines.push(self.read_comment_line()?);
                if self.peek() == Some('\n') {
                    self.bump();
                }
            } else {
                break;
            }
        }
        Ok(lines)
    }

    fn looks_like_entries(&mut self) -> bool {
        let saved = self.index;
        let is_entry = self.parse_key().is_ok() && {
            self.skip();
            self.peek() == Some('=')
        };
        self.index = saved;
        is_entry
    }

    fn parse_entries(&mut self) -> Result<SealValue, TextError> {
        let mut comments = SealComments::default();
        let mut entries = Vec::new();
        comments.header = self.collect_comments()?;
        while !self.eof() {
            self.skip_whitespace_only();
            if self.eof() {
                break;
            }
            if self.peek() == Some(',') {
                self.bump();
                continue;
            }
            let before = self.collect_comments()?;
            let key = self.parse_key()?;
            self.skip();
            if self.bump() != Some('=') {
                return Err(self.error("expected '=' after a key"));
            }
            let value = self.parse_value(0)?;
            if entries.iter().any(|(name, _)| name == &key) {
                return Err(self.error(format!("duplicate key '{key}'")));
            }
            comments.before_entry.push(before);
            entries.push((key, value));
            self.skip_whitespace_only();
            if self.peek() == Some(',') {
                self.bump();
            }
        }
        Ok(SealValue::Map(SealMap::with_comments(entries, comments)))
    }

    fn parse_key(&mut self) -> Result<String, TextError> {
        self.skip();
        if self.peek() == Some('"') {
            return self.parse_string();
        }
        self.parse_ident()
    }

    fn parse_ident(&mut self) -> Result<String, TextError> {
        let start = self.index;
        match self.bump() {
            Some(ch) if is_ident_start(ch) => {}
            _ => return Err(self.error("expected a key")),
        }
        while matches!(self.peek(), Some(ch) if is_ident_continue(ch)) {
            self.bump();
        }
        Ok(self.src[start..self.index].to_string())
    }

    fn starts_value(&self) -> bool {
        matches!(self.peek(), Some('"' | '[' | '{' | '-' | '0'..='9' | 'n' | 't' | 'f'))
    }

    fn parse_value(&mut self, depth: u32) -> Result<SealValue, TextError> {
        if depth > self.limits.max_depth {
            return Err(self.error(format!("depth exceeds {}", self.limits.max_depth)));
        }
        self.skip();
        if self.starts_with("null") && !ident_continues_after(self, 4) {
            self.index += 4;
            return Ok(SealValue::Null);
        }
        if self.starts_with("true") && !ident_continues_after(self, 4) {
            self.index += 4;
            return Ok(SealValue::Bool(true));
        }
        if self.starts_with("false") && !ident_continues_after(self, 5) {
            self.index += 5;
            return Ok(SealValue::Bool(false));
        }
        match self.peek() {
            Some('"') => Ok(SealValue::String(self.parse_string()?)),
            Some('[') => self.parse_array(depth),
            Some('{') => self.parse_map(depth),
            Some('-') | Some('0'..='9') => Ok(SealValue::Number(self.parse_number()?)),
            _ => Err(self.error("expected a value")),
        }
    }

    fn parse_ron_value(&mut self, depth: u32) -> Result<SealValue, TextError> {
        if depth > self.limits.max_depth {
            return Err(self.error(format!("depth exceeds {}", self.limits.max_depth)));
        }
        self.skip();
        if self.starts_with("None") && !ident_continues_after(self, 4) {
            self.index += 4;
            return Ok(SealValue::Null);
        }
        if self.starts_with("true") && !ident_continues_after(self, 4) {
            self.index += 4;
            return Ok(SealValue::Bool(true));
        }
        if self.starts_with("false") && !ident_continues_after(self, 5) {
            self.index += 5;
            return Ok(SealValue::Bool(false));
        }
        match self.peek() {
            Some('"') => Ok(SealValue::String(self.parse_string()?)),
            Some('[') => self.parse_ron_array(depth),
            Some('{') => self.parse_ron_map(depth),
            Some('-') | Some('0'..='9') => Ok(SealValue::Number(self.parse_number()?)),
            _ => Err(self.error("expected a RON value")),
        }
    }

    fn parse_number(&mut self) -> Result<String, TextError> {
        let start = self.index;
        if self.peek() == Some('-') {
            self.bump();
        }
        if !matches!(self.peek(), Some('0'..='9')) {
            return Err(self.error("expected a number"));
        }
        while matches!(self.peek(), Some('0'..='9')) {
            self.bump();
        }
        if self.peek() == Some('.') {
            self.bump();
            if !matches!(self.peek(), Some('0'..='9')) {
                return Err(self.error("expected digits after the decimal point"));
            }
            while matches!(self.peek(), Some('0'..='9')) {
                self.bump();
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            self.bump();
            if matches!(self.peek(), Some('+' | '-')) {
                self.bump();
            }
            if !matches!(self.peek(), Some('0'..='9')) {
                return Err(self.error("expected digits in the exponent"));
            }
            while matches!(self.peek(), Some('0'..='9')) {
                self.bump();
            }
        }
        Ok(self.src[start..self.index].to_string())
    }

    fn parse_string(&mut self) -> Result<String, TextError> {
        if self.bump() != Some('"') {
            return Err(self.error("expected a string"));
        }
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return Err(self.error("unterminated string")),
                Some('"') => {
                    if out.len() > self.limits.max_string_bytes {
                        return Err(self.error(format!(
                            "string exceeds {} bytes",
                            self.limits.max_string_bytes
                        )));
                    }
                    return Ok(out);
                }
                Some('\\') => {
                    let escaped = self.bump().ok_or_else(|| self.error("unterminated escape"))?;
                    match escaped {
                        '"' => out.push('"'),
                        '\\' => out.push('\\'),
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'u' => out.push(self.parse_unicode_escape()?),
                        _ => return Err(self.error("unknown string escape")),
                    }
                    if out.len() > self.limits.max_string_bytes {
                        return Err(self.error(format!(
                            "string exceeds {} bytes",
                            self.limits.max_string_bytes
                        )));
                    }
                }
                Some(ch) => {
                    out.push(ch);
                    if out.len() > self.limits.max_string_bytes {
                        return Err(self.error(format!(
                            "string exceeds {} bytes",
                            self.limits.max_string_bytes
                        )));
                    }
                }
            }
        }
    }

    fn parse_unicode_escape(&mut self) -> Result<char, TextError> {
        if self.peek() == Some('{') {
            self.bump();
            let start = self.index;
            while matches!(self.peek(), Some(ch) if ch.is_ascii_hexdigit()) {
                self.bump();
            }
            if self.bump() != Some('}') {
                return Err(self.error("expected '}' after a unicode escape"));
            }
            let hex = &self.src[start..self.index - '}'.len_utf8()];
            let code =
                u32::from_str_radix(hex, 16).map_err(|_| self.error("invalid unicode escape"))?;
            return char::from_u32(code).ok_or_else(|| self.error("invalid unicode scalar"));
        }
        let start = self.index;
        for _ in 0..4 {
            if !matches!(self.bump(), Some(ch) if ch.is_ascii_hexdigit()) {
                return Err(self.error("expected four hex digits"));
            }
        }
        let code = u32::from_str_radix(&self.src[start..self.index], 16)
            .map_err(|_| self.error("invalid unicode escape"))?;
        char::from_u32(code).ok_or_else(|| self.error("invalid unicode scalar"))
    }

    fn parse_array(&mut self, depth: u32) -> Result<SealValue, TextError> {
        self.bump();
        let mut items = Vec::new();
        loop {
            self.skip();
            if self.peek() == Some(']') {
                self.bump();
                return Ok(SealValue::Array(items));
            }
            items.push(self.parse_value(depth.saturating_add(1))?);
            self.skip();
            if self.peek() == Some(',') {
                self.bump();
                continue;
            }
            if self.peek() == Some(']') {
                self.bump();
                return Ok(SealValue::Array(items));
            }
            if self.starts_value() {
                continue;
            }
            return Err(self.error("expected ',' or ']' in an array"));
        }
    }

    fn parse_map(&mut self, depth: u32) -> Result<SealValue, TextError> {
        self.bump();
        let mut entries = Vec::new();
        loop {
            self.skip();
            if self.peek() == Some('}') {
                self.bump();
                return Ok(SealValue::Map(SealMap::new(entries)));
            }
            let key = self.parse_key()?;
            self.skip();
            if self.bump() != Some('=') {
                return Err(self.error("expected '=' in a map"));
            }
            let value = self.parse_value(depth.saturating_add(1))?;
            if entries.iter().any(|(name, _)| name == &key) {
                return Err(self.error(format!("duplicate key '{key}'")));
            }
            entries.push((key, value));
            self.skip();
            if self.peek() == Some(',') {
                self.bump();
            }
        }
    }

    fn parse_ron_array(&mut self, depth: u32) -> Result<SealValue, TextError> {
        self.bump();
        let mut items = Vec::new();
        loop {
            self.skip();
            if self.peek() == Some(']') {
                self.bump();
                return Ok(SealValue::Array(items));
            }
            items.push(self.parse_ron_value(depth.saturating_add(1))?);
            self.skip();
            if self.peek() == Some(',') {
                self.bump();
                continue;
            }
            if self.peek() == Some(']') {
                self.bump();
                return Ok(SealValue::Array(items));
            }
            return Err(self.error("expected ',' or ']' in a RON array"));
        }
    }

    fn parse_ron_map(&mut self, depth: u32) -> Result<SealValue, TextError> {
        self.bump();
        let mut entries = Vec::new();
        loop {
            self.skip();
            if self.peek() == Some('}') {
                self.bump();
                return Ok(SealValue::Map(SealMap::new(entries)));
            }
            let key = self.parse_key()?;
            self.skip();
            if self.bump() != Some(':') {
                return Err(self.error("expected ':' in a RON map"));
            }
            let value = self.parse_ron_value(depth.saturating_add(1))?;
            if entries.iter().any(|(name, _)| name == &key) {
                return Err(self.error(format!("duplicate key '{key}'")));
            }
            entries.push((key, value));
            self.skip();
            if self.peek() == Some(',') {
                self.bump();
            }
        }
    }
}

fn ident_continues_after(parser: &Parser<'_>, len: usize) -> bool {
    parser.src[parser.index + len..].chars().next().is_some_and(is_ident_continue)
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

fn write_comments(out: &mut String, lines: &[String], limits: &Limits) -> Result<(), TextError> {
    for line in lines {
        if out.len() + line.len() + 3 > limits.max_file_bytes {
            return Err(TextError {
                line: 1,
                column: 1,
                message: format!("output exceeds {} bytes", limits.max_file_bytes),
            });
        }
        let _ = writeln!(out, "# {line}");
    }
    Ok(())
}

fn write_value(
    out: &mut String,
    value: &SealValue,
    indent: usize,
    root: bool,
    limits: &Limits,
) -> Result<(), TextError> {
    match value {
        SealValue::Map(map) if root => {
            write_comments(out, &map.comments.header, limits)?;
            for (index, (key, item)) in map.entries.iter().enumerate() {
                if let Some(lines) = map.comments.before_entry.get(index) {
                    write_comments(out, lines, limits)?;
                }
                if index > 0 && !out.ends_with('\n') {
                    out.push('\n');
                }
                write_key(out, key)?;
                out.push_str(" = ");
                write_value(out, item, indent, false, limits)?;
                out.push('\n');
            }
        }
        SealValue::Map(map) => {
            if map.entries.is_empty() {
                out.push_str("{}");
                return Ok(());
            }
            out.push_str("{\n");
            for (key, item) in &map.entries {
                push_indent(out, indent + 1);
                write_key(out, key)?;
                out.push_str(" = ");
                write_value(out, item, indent + 1, false, limits)?;
                out.push('\n');
            }
            push_indent(out, indent);
            out.push('}');
        }
        SealValue::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return Ok(());
            }
            let flat = items.iter().all(|item| {
                matches!(
                    item,
                    SealValue::Null
                        | SealValue::Bool(_)
                        | SealValue::Number(_)
                        | SealValue::String(_)
                )
            });
            if flat {
                out.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push_str(", ");
                    }
                    write_value(out, item, indent, false, limits)?;
                }
                out.push(']');
            } else {
                out.push_str("[\n");
                for item in items {
                    push_indent(out, indent + 1);
                    write_value(out, item, indent + 1, false, limits)?;
                    out.push('\n');
                }
                push_indent(out, indent);
                out.push(']');
            }
        }
        SealValue::Null => out.push_str("null"),
        SealValue::Bool(true) => out.push_str("true"),
        SealValue::Bool(false) => out.push_str("false"),
        SealValue::Number(lexeme) => out.push_str(lexeme),
        SealValue::String(value) => {
            if value.len() > limits.max_string_bytes {
                return Err(TextError {
                    line: 1,
                    column: 1,
                    message: format!("string exceeds {} bytes", limits.max_string_bytes),
                });
            }
            push_quoted(out, value);
        }
    }
    if out.len() > limits.max_file_bytes {
        return Err(TextError {
            line: 1,
            column: 1,
            message: format!("output exceeds {} bytes", limits.max_file_bytes),
        });
    }
    Ok(())
}

fn write_key(out: &mut String, key: &str) -> Result<(), TextError> {
    if is_bare_key(key) {
        out.push_str(key);
    } else {
        push_quoted(out, key);
    }
    Ok(())
}

fn is_bare_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(ch) if is_ident_start(ch) => chars.all(is_ident_continue),
        _ => false,
    }
}

fn push_quoted(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => {
                let _ = write!(out, "\\u{{{:x}}}", u32::from(ch));
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}

fn push_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("  ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_keeps_unknown_fields_and_spelling() {
        let text = r#"
# editor file
schema_version = 1
name = "Demo"
future_flag = true
ratio = 0.25
big = 9007199254740993
note = "line\nbreak"
modules = []
nested = { extra = "kept", count = 2 }
"#;
        let value = parse(text).unwrap();
        let again = parse(&value.to_text()).unwrap();
        assert_eq!(value, again);
        assert_eq!(value.get("future_flag"), Some(&SealValue::Bool(true)));
        assert_eq!(value.get("big"), Some(&SealValue::Number("9007199254740993".into())));
    }

    #[test]
    fn comments_survive_round_trip() {
        let text = "# tuned by hand\nschema_version = 1\n# still here\nname = \"Demo\"\n";
        let value = parse(text).unwrap();
        let written = value.to_text();
        assert!(written.contains("# tuned by hand"));
        assert!(written.contains("# still here"));
        let again = parse(&written).unwrap();
        assert_eq!(value, again);
    }

    #[test]
    fn json_and_ron_roundtrip() {
        let value = parse(
            r#"
schema_version = 1
name = "Demo"
optional = null
modules = ["a", "b"]
"#,
        )
        .unwrap();
        let json = value.to_json();
        assert_eq!(sorted_maps(SealValue::from_json(&json)), sorted_maps(value.clone()));
        let ron_text = value.to_ron();
        let parsed_by_ron: ron::Value = ron::from_str(&ron_text).unwrap();
        assert!(matches!(parsed_by_ron, ron::Value::Map(_)));
        assert_eq!(sorted_maps(SealValue::from_ron(&ron_text).unwrap()), sorted_maps(value));
        assert!(ron_text.contains("None"));
    }

    fn sorted_maps(value: SealValue) -> SealValue {
        match value {
            SealValue::Array(items) => {
                SealValue::Array(items.into_iter().map(sorted_maps).collect())
            }
            SealValue::Map(map) => {
                let mut entries = map.entries;
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                SealValue::Map(SealMap::new(
                    entries.into_iter().map(|(key, value)| (key, sorted_maps(value))).collect(),
                ))
            }
            other => other,
        }
    }

    #[test]
    fn duplicate_key_is_an_error() {
        let err = parse("a = 1\na = 2\n").unwrap_err();
        assert!(err.message.contains("duplicate"));
    }

    #[test]
    fn oversized_input_is_rejected() {
        let limits = Limits { max_file_bytes: 4, ..Limits::default() };
        let err = parse_with_limits("name = x\n", &limits).unwrap_err();
        assert!(err.message.contains("exceeds"));
    }
}
