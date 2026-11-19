use crate::error::CoreError;
use crate::session::EditorSession;

/// A named editor action. Menus, shortcuts, the palette, and tests all call these.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    pub id: String,
    pub title_key: String,
    pub category: String,
    pub description_key: String,
    pub alias_of: Option<String>,
    pub agent_safe: bool,
    pub in_menu: bool,
    /// Top-level menu bucket when set. Otherwise `category` is used.
    pub menu_path: Option<String>,
    pub non_undoable_reason: Option<String>,
    pub requires: Vec<String>,
    pub args: Vec<ArgSpec>,
}

/// One typed argument a command publishes to scripts and the agent bus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgSpec {
    pub name: String,
    pub kind: String,
    pub required: bool,
}

impl CommandSpec {
    pub fn new(
        id: impl Into<String>,
        title_key: impl Into<String>,
        category: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title_key: title_key.into(),
            category: category.into(),
            description_key: String::new(),
            alias_of: None,
            agent_safe: true,
            in_menu: true,
            menu_path: None,
            non_undoable_reason: None,
            requires: Vec::new(),
            args: Vec::new(),
        }
    }

    pub fn menu(mut self, shown: bool) -> Self {
        self.in_menu = shown;
        self
    }

    pub fn menu_path(mut self, path: impl Into<String>) -> Self {
        self.menu_path = Some(path.into());
        self
    }

    pub fn menu_bucket(&self) -> &str {
        self.menu_path.as_deref().unwrap_or(self.category.as_str())
    }

    pub fn agent_safe(mut self, safe: bool) -> Self {
        self.agent_safe = safe;
        self
    }

    pub fn non_undoable(mut self, reason: impl Into<String>) -> Self {
        self.non_undoable_reason = Some(reason.into());
        self
    }

    pub fn requires(mut self, part: impl Into<String>) -> Self {
        self.requires.push(part.into());
        self
    }

    pub fn arg(mut self, name: impl Into<String>, kind: impl Into<String>, required: bool) -> Self {
        self.args.push(ArgSpec { name: name.into(), kind: kind.into(), required });
        self
    }

    /// JSON Schema fragment for the argument object. Agents discover commands from this.
    pub fn schema_json(&self) -> String {
        let mut required = Vec::new();
        let mut props = String::from("{");
        for (index, arg) in self.args.iter().enumerate() {
            if index > 0 {
                props.push(',');
            }
            props.push_str(&format!(
                "\"{}\":{{\"type\":\"{}\"}}",
                escape_json(&arg.name),
                escape_json(&arg.kind)
            ));
            if arg.required {
                required.push(format!("\"{}\"", escape_json(&arg.name)));
            }
        }
        props.push('}');
        format!(
            "{{\"type\":\"object\",\"properties\":{props},\"required\":[{}]}}",
            required.join(",")
        )
    }
}

pub type CommandHandler = fn(&mut EditorSession, &CommandArgs) -> Result<(), CoreError>;
pub type CommandEnabled = fn(&EditorSession) -> bool;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandArgs {
    values: Vec<(String, String)>,
}

impl CommandArgs {
    pub fn new(pairs: &[(&str, &str)]) -> Self {
        Self {
            values: pairs
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str())
    }

    pub fn from_json_object(text: &str) -> Result<Self, CoreError> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|err| CoreError::new(err.to_string()))?;
        let Some(object) = value.as_object() else {
            return Err(CoreError::new("command arguments must be an object"));
        };
        let mut values = Vec::new();
        for (key, item) in object {
            let text = match item {
                serde_json::Value::String(text) => text.clone(),
                serde_json::Value::Bool(flag) => flag.to_string(),
                serde_json::Value::Number(number) => number.to_string(),
                serde_json::Value::Null => String::new(),
                _ => {
                    return Err(CoreError::new(format!(
                        "argument '{key}' must be a string, number, or boolean"
                    )))
                }
            };
            values.push((key.clone(), text));
        }
        Ok(Self { values })
    }

    /// Arguments for the debug log. Values whose names look like secrets are replaced.
    pub fn redacted(&self) -> String {
        let mut out = String::from("{");
        for (index, (key, value)) in self.values.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let hidden =
                key.contains("secret") || key.contains("token") || key.contains("password");
            let shown = if hidden { "***" } else { value.as_str() };
            out.push_str(&format!("\"{}\":\"{}\"", escape_json(key), escape_json(shown)));
        }
        out.push('}');
        out
    }
}

struct Entry {
    spec: CommandSpec,
    enabled: CommandEnabled,
    handler: CommandHandler,
}

#[derive(Default)]
pub struct CommandRegistry {
    entries: Vec<Entry>,
}

impl CommandRegistry {
    pub fn register(
        &mut self,
        spec: CommandSpec,
        enabled: CommandEnabled,
        handler: CommandHandler,
    ) -> Result<(), CoreError> {
        if spec.id.is_empty() {
            return Err(CoreError::new("command id must not be empty"));
        }
        if self.entries.iter().any(|entry| entry.spec.id == spec.id) {
            return Err(CoreError::new(format!("command '{}' is already registered", spec.id)));
        }
        self.entries.push(Entry { spec, enabled, handler });
        Ok(())
    }

    /// Every registered command, in registration order.
    pub fn specs(&self) -> impl Iterator<Item = &CommandSpec> {
        self.entries.iter().map(|entry| &entry.spec)
    }

    pub fn get(&self, id: &str) -> Option<&CommandSpec> {
        self.entries.iter().find(|entry| entry.spec.id == id).map(|entry| &entry.spec)
    }

    /// Follow one alias hop. The id itself is returned when it is a real command.
    pub fn canonical(&self, id: &str) -> Option<String> {
        let entry = self.entries.iter().find(|entry| entry.spec.id == id)?;
        if let Some(target) = &entry.spec.alias_of {
            if self.entries.iter().any(|entry| entry.spec.id == *target) {
                return Some(target.clone());
            }
        }
        Some(id.to_string())
    }

    pub fn register_alias(&mut self, mut spec: CommandSpec, target: &str) -> Result<(), CoreError> {
        if self.get(target).is_none() {
            return Err(CoreError::new(format!(
                "alias '{}' targets unknown command '{target}'",
                spec.id
            )));
        }
        spec.alias_of = Some(target.to_string());
        spec.in_menu = false;
        self.register(spec, always_enabled, alias_noop)
    }

    pub fn is_enabled(&self, id: &str, session: &EditorSession) -> bool {
        let Some(id) = self.canonical(id) else {
            return false;
        };
        self.entries
            .iter()
            .find(|entry| entry.spec.id == id)
            .is_some_and(|entry| (entry.enabled)(session))
    }

    pub fn handler(&self, id: &str) -> Option<CommandHandler> {
        let id = self.canonical(id)?;
        self.entries.iter().find(|entry| entry.spec.id == id).map(|entry| entry.handler)
    }

    pub fn iter(&self) -> impl Iterator<Item = &CommandSpec> {
        self.entries.iter().map(|entry| &entry.spec)
    }

    pub fn in_category<'a>(&'a self, category: &'a str) -> impl Iterator<Item = &'a CommandSpec> {
        self.entries
            .iter()
            .filter(move |entry| entry.spec.category == category)
            .map(|entry| &entry.spec)
    }

    pub fn catalog_json(&self, title: impl Fn(&str) -> String) -> String {
        let mut out = String::from("[");
        for (index, entry) in self.entries.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"id\":\"{}\",\"title\":\"{}\",\"category\":\"{}\"}}",
                escape_json(&entry.spec.id),
                escape_json(&title(&entry.spec.title_key)),
                escape_json(&entry.spec.category),
            ));
        }
        out.push(']');
        out
    }
}

pub fn always_enabled(_session: &EditorSession) -> bool {
    true
}

fn alias_noop(_session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    Ok(())
}

fn escape_json(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            ch => out.push(ch),
        }
    }
    out
}

pub const MENU_CATEGORIES: &[&str] =
    &["file", "edit", "view", "window", "tools", "build", "select", "entity", "help"];

/// Palette prefix: `>` commands, `@` panels, `#` recent projects, `?` help.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteMode {
    Commands,
    Panels,
    RecentProjects,
    Help,
}

impl PaletteMode {
    pub fn split(query: &str) -> (Self, &str) {
        let query = query.trim();
        if let Some(rest) = query.strip_prefix('>') {
            (Self::Commands, rest.trim_start())
        } else if let Some(rest) = query.strip_prefix('@') {
            (Self::Panels, rest.trim_start())
        } else if let Some(rest) = query.strip_prefix('#') {
            (Self::RecentProjects, rest.trim_start())
        } else if let Some(rest) = query.strip_prefix('?') {
            (Self::Help, rest.trim_start())
        } else {
            (Self::Commands, query)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteHit {
    pub id: String,
    pub title: String,
    pub shortcut: String,
    pub score: i32,
}

pub fn palette_hits(
    commands: &CommandRegistry,
    query: &str,
    recent: &[String],
    title_of: impl Fn(&str) -> String,
    shortcut_of: impl Fn(&str) -> String,
) -> Vec<PaletteHit> {
    let mut hits = Vec::new();
    for spec in commands.iter() {
        if spec.alias_of.is_some() {
            continue;
        }
        let title = title_of(&spec.title_key);
        let Some(score) = fuzzy_score(query, &title).or_else(|| fuzzy_score(query, &spec.id))
        else {
            continue;
        };
        let recent_bonus = recent
            .iter()
            .position(|id| id == &spec.id)
            .map(|index| 100 - index as i32)
            .unwrap_or(0);
        hits.push(PaletteHit {
            id: spec.id.clone(),
            title,
            shortcut: shortcut_of(&spec.id),
            score: score + recent_bonus,
        });
    }
    hits.sort_by(|left, right| {
        right.score.cmp(&left.score).then_with(|| left.title.cmp(&right.title))
    });
    hits
}

fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    let query: Vec<char> = query.chars().flat_map(|ch| ch.to_lowercase()).collect();
    if query.is_empty() {
        return Some(1);
    }
    let text: Vec<char> = text.chars().flat_map(|ch| ch.to_lowercase()).collect();
    let mut query_index = 0;
    let mut score = 0;
    let mut previous = None;
    for (index, ch) in text.iter().enumerate() {
        if query_index < query.len() && *ch == query[query_index] {
            score += if previous == Some(index.wrapping_sub(1)) { 4 } else { 1 };
            previous = Some(index);
            query_index += 1;
        }
    }
    if query_index == query.len() {
        Some(score)
    } else {
        None
    }
}
