use crate::error::CoreError;
use crate::kinds::{DocumentKind, KindRegistry};
use crate::session::EditorSession;

/// The only extension point. Modules are compiled in; nothing is loaded from disk.
pub trait EditorModule {
    fn id(&self) -> &str;
    fn register(&self, host: &mut ModuleHost<'_>) -> Result<(), CoreError>;
}

/// What a module is allowed to add while the editor is starting.
pub struct ModuleHost<'a> {
    kinds: &'a mut KindRegistry,
}

impl ModuleHost<'_> {
    pub fn register_kind(&mut self, kind: DocumentKind) -> Result<(), CoreError> {
        self.kinds.register(kind)
    }
}

/// Built-in panels and document kinds, registered the same way a later module will be.
pub struct CorePanels;

impl EditorModule for CorePanels {
    fn id(&self) -> &str {
        "core-panels"
    }

    fn register(&self, host: &mut ModuleHost<'_>) -> Result<(), CoreError> {
        host.register_kind(DocumentKind {
            id: "scratch".into(),
            title_key: "welcome-scratch".into(),
            extensions: Vec::new(),
        })?;
        host.register_kind(DocumentKind {
            id: "project".into(),
            title_key: "command-file-open-project".into(),
            extensions: vec!["seal".into()],
        })?;
        host.register_kind(DocumentKind {
            id: "text".into(),
            title_key: "panel-content-title".into(),
            extensions: vec!["txt".into(), "md".into()],
        })?;
        Ok(())
    }
}

impl EditorSession {
    pub fn install_module(&mut self, module: &dyn EditorModule) -> Result<(), CoreError> {
        if self.foundation.modules.iter().any(|id| id == module.id()) {
            return Err(CoreError::new(format!("module '{}' is already registered", module.id())));
        }
        self.foundation.modules.push(module.id().to_string());
        let mut host = ModuleHost { kinds: &mut self.foundation.kinds };
        module.register(&mut host)?;
        Ok(())
    }
}
