use crate::error::CoreError;

/// A file type the shell can open. Later tools register their own kinds here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentKind {
    pub id: String,
    pub title_key: String,
    pub extensions: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct KindRegistry {
    kinds: Vec<DocumentKind>,
}

impl KindRegistry {
    pub fn register(&mut self, kind: DocumentKind) -> Result<(), CoreError> {
        if kind.id.is_empty() {
            return Err(CoreError::new("document kind id must not be empty"));
        }
        if self.kinds.iter().any(|existing| existing.id == kind.id) {
            return Err(CoreError::new(format!(
                "document kind '{}' is already registered",
                kind.id
            )));
        }
        self.kinds.push(kind);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&DocumentKind> {
        self.kinds.iter().find(|kind| kind.id == id)
    }

    pub fn for_extension(&self, extension: &str) -> Option<&DocumentKind> {
        let extension = extension.trim_start_matches('.');
        self.kinds.iter().find(|kind| kind.extensions.iter().any(|item| item == extension))
    }

    pub fn iter(&self) -> impl Iterator<Item = &DocumentKind> {
        self.kinds.iter()
    }
}
