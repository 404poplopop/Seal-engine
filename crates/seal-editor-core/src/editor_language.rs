//! Editor UI language preference (catalogs available today).

use crate::error::CoreError;
use crate::messages::Messages;
use crate::settings::SettingsStore;

pub const SETTING: &str = "ui.editor_language";

pub const AVAILABLE: &[&str] = &["en"];

pub fn validate(locale: &str) -> Result<(), CoreError> {
    if AVAILABLE.contains(&locale) {
        Ok(())
    } else {
        Err(CoreError::new("ui.editor_language must match an installed catalog"))
    }
}

pub fn messages_for_store(store: &SettingsStore) -> Messages {
    let locale = store.get(SETTING).unwrap_or("en");
    Messages::for_locale(locale)
}

pub fn catalog_label_key(locale: &str) -> String {
    format!("editor-language-{locale}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_setting_validates_against_catalogs() {
        assert!(validate("en").is_ok());
        assert!(validate("xx").is_err());
    }
}