//! Message lookup. User-facing text lives in Fluent files. Log lines do not.
#![forbid(unsafe_code)]

pub mod audit;

use std::cell::RefCell;

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use unic_langid::LanguageIdentifier;

/// One locale, with an optional English catalog behind it.
pub struct Catalog {
    bundle: FluentBundle<FluentResource>,
    fallback: Option<Box<Catalog>>,
    missing: RefCell<Vec<String>>,
}

impl Catalog {
    /// `locale` is a language id such as `en` or `ru`. `ftl` is the file text.
    /// A catalog with one inert message. Lookups of other ids return the id.
    pub fn blank() -> Self {
        match Self::from_ftl("en", "ok = ok\n") {
            Ok(catalog) => catalog,
            Err(_) => Self {
                bundle: FluentBundle::new(vec![]),
                fallback: None,
                missing: RefCell::new(Vec::new()),
            },
        }
    }

    pub fn from_ftl(locale: &str, ftl: &str) -> Result<Self, String> {
        let lang = match locale.parse::<LanguageIdentifier>() {
            Ok(lang) => lang,
            Err(err) => return Err(err.to_string()),
        };
        let resource = FluentResource::try_new(ftl.to_string()).map_err(|(_, errors)| {
            errors.iter().map(|err| err.to_string()).collect::<Vec<_>>().join("; ")
        })?;
        let mut bundle = FluentBundle::new(vec![lang]);
        bundle.add_resource(resource).map_err(|errors| {
            errors.into_iter().map(|err| format!("{err:?}")).collect::<Vec<_>>().join("; ")
        })?;
        Ok(Self { bundle, fallback: None, missing: RefCell::new(Vec::new()) })
    }

    pub fn with_fallback(mut self, fallback: Catalog) -> Self {
        self.fallback = Some(Box::new(fallback));
        self
    }

    pub fn get(&self, id: &str) -> String {
        self.format(id, &[])
    }

    pub fn format(&self, id: &str, args: &[(&str, &str)]) -> String {
        if let Some(text) = self.format_here(id, args) {
            return text;
        }
        self.note_missing(id);
        if let Some(fallback) = &self.fallback {
            return fallback.format(id, args);
        }
        id.to_string()
    }

    /// Ids that were requested and were not in this catalog. Each id is recorded once.
    pub fn missing(&self) -> Vec<String> {
        self.missing.borrow().clone()
    }

    fn format_here(&self, id: &str, args: &[(&str, &str)]) -> Option<String> {
        let message = self.bundle.get_message(id)?;
        let pattern = message.value()?;
        let mut fluent_args = FluentArgs::new();
        for (key, value) in args {
            fluent_args.set(key.to_string(), value.to_string());
        }
        let mut errors = Vec::new();
        let text = self.bundle.format_pattern(pattern, Some(&fluent_args), &mut errors);
        Some(text.to_string())
    }

    fn note_missing(&self, id: &str) {
        let mut missing = self.missing.borrow_mut();
        if !missing.iter().any(|item| item == id) {
            missing.push(id.to_string());
        }
    }
}

/// Group digits for display. English uses commas. Russian uses spaces.
pub fn format_integer(locale: &str, value: i64) -> String {
    let separator = if locale.starts_with("ru") { ' ' } else { ',' };
    let negative = value < 0;
    let digits = value.unsigned_abs().to_string();
    let mut grouped = String::new();
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(separator);
        }
        grouped.push(ch);
    }
    let mut text: String = grouped.chars().rev().collect();
    if negative {
        text.insert(0, '-');
    }
    text
}

/// Binary size for the status bar and asset browser. 1024 bytes is `1.0 KB`.
pub fn format_bytes(locale: &str, bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < units.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        return format!("{bytes} {}", units[0]);
    }
    let mut text = format!("{value:.1}");
    if locale.starts_with("ru") {
        text = text.replace('.', ",");
    }
    format!("{text} {}", units[unit])
}

/// Message ids that appear more than once in `ftl`. Terms and other entry kinds are ignored.
pub fn duplicate_ids(ftl: &str) -> Vec<String> {
    let mut counts: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    for line in ftl.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with(|c: char| c.is_ascii_whitespace() || c == '-' || c == '.') {
            continue;
        }
        let Some((id, _)) = line.split_once('=') else {
            continue;
        };
        let id = id.trim();
        if message_id_token(id) {
            *counts.entry(id.to_string()).or_insert(0) += 1;
        }
    }
    counts.into_iter().filter(|(_, count)| *count > 1).map(|(id, _)| id).collect()
}

/// Message ids declared in `ftl` that do not appear in `used`.
pub fn unknown_ids(ftl: &str, used: &[String]) -> Vec<String> {
    let mut declared: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for line in ftl.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with(|c: char| c.is_ascii_whitespace() || c == '-' || c == '.') {
            continue;
        }
        let Some((id, _)) = line.split_once('=') else {
            continue;
        };
        let id = id.trim();
        if message_id_token(id) {
            declared.insert(id.to_string());
        }
    }
    declared.into_iter().filter(|id| !used.iter().any(|item| item == id)).collect()
}

fn message_id_token(id: &str) -> bool {
    let Some(first) = id.chars().next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    id.chars().skip(1).all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    const EN: &str = "hello = Hello, { $name }.\nonly-en = English only\n";

    #[test]
    fn a_missing_message_falls_back_and_is_recorded_once() {
        let english = Catalog::from_ftl("en", EN).unwrap();
        let partial =
            Catalog::from_ftl("en", "hello = Hello, { $name }.\n").unwrap().with_fallback(english);
        let hello = partial.format("hello", &[("name", "Seal")]);
        assert!(hello.contains("Hello"), "{hello}");
        assert!(hello.contains("Seal"), "{hello}");
        assert_eq!(partial.get("only-en"), "English only");
        assert_eq!(partial.missing(), vec!["only-en".to_string()]);
        assert_eq!(partial.get("only-en"), "English only");
        assert_eq!(partial.missing(), vec!["only-en".to_string()]);
        assert_eq!(format_integer("en", 1024), "1,024");
        assert_eq!(format_bytes("en", 1024), "1.0 KB");
    }

    #[test]
    fn unknown_ids_are_declared_messages_nobody_used() {
        let ftl = "used = Used\nextra = Extra\n# note\n";
        assert_eq!(unknown_ids(ftl, &["used".into()]), vec!["extra".to_string()]);
        assert!(unknown_ids(ftl, &["used".into(), "extra".into()]).is_empty());
    }
}
