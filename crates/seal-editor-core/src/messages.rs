use seal_i18n::Catalog;

const ENGLISH: &str = include_str!("../locales/en.ftl");

pub struct Messages {
    catalog: Catalog,
}

impl Messages {
    pub fn for_locale(locale: &str) -> Self {
        if locale == "en" || locale.is_empty() {
            return Self::english();
        }
        crate::editor_language::validate(locale).ok();
        Self::english()
    }

    pub fn english() -> Self {
        let catalog = match Catalog::from_ftl("en", ENGLISH) {
            Ok(catalog) => catalog,
            Err(err) => {
                seal_log::warn!("english catalog failed to parse: {err}");
                Catalog::from_ftl("en", &format!("catalog-broken = {err}\n"))
                    .unwrap_or_else(|_| Catalog::blank())
            }
        };
        Self { catalog }
    }

    /// Return the English string for `id`. A missing id comes back unchanged
    /// so the shell can still draw something.
    pub fn get(&self, id: &str) -> String {
        self.catalog.get(id)
    }

    /// Fill placeholders in `id`. A missing id comes back unchanged.
    pub fn format(&self, id: &str, args: &[(&str, &str)]) -> String {
        self.catalog.format(id, args)
    }
}

impl Default for Messages {
    fn default() -> Self {
        Self::english()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_catalog_resolves_panel_titles() {
        let messages = Messages::english();
        assert_eq!(messages.get("panel-welcome-title"), "Welcome");
        assert_eq!(messages.get("missing-id"), "missing-id");
    }

    #[test]
    fn english_catalog_parses_without_duplicate_ids() {
        use seal_i18n::{duplicate_ids, Catalog};

        Catalog::from_ftl("en", ENGLISH).expect("english catalog must parse");
        let dupes = duplicate_ids(ENGLISH);
        assert!(dupes.is_empty(), "duplicate message ids: {dupes:?}");
    }

    #[test]
    fn the_english_catalog_matches_editor_sources() {
        crate::catalog_ids::verify_english_catalog().expect("English catalog");
    }
}
