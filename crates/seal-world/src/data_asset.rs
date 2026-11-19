//! Data Type and Data assets: named typed fields and values stored as SealText.

use std::fmt;

pub const DATATYPE_EXT: &str = "datatype";
pub const DATA_EXT: &str = "data";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Number,
    Text,
    Bool,
    Colour,
    Asset,
    List,
    Choice,
}

impl FieldKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "number" | "float" | "int" => Some(Self::Number),
            "text" | "string" => Some(Self::Text),
            "bool" | "boolean" => Some(Self::Bool),
            "colour" | "color" => Some(Self::Colour),
            "asset" | "path" => Some(Self::Asset),
            "list" => Some(Self::List),
            "choice" | "enum" => Some(Self::Choice),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Number => "number",
            Self::Text => "text",
            Self::Bool => "bool",
            Self::Colour => "colour",
            Self::Asset => "asset",
            Self::List => "list",
            Self::Choice => "choice",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataFieldDef {
    pub name: String,
    pub kind: FieldKind,
    pub element: Option<FieldKind>,
    /// For choice fields: allowed values separated by `|`.
    pub options: String,
    pub default: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataTypeDef {
    pub name: String,
    pub fields: Vec<DataFieldDef>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataAsset {
    pub name: String,
    pub type_path: String,
    pub values: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataAssetError {
    Parse(String),
    TypeMismatch { field: String, expected: String },
}

impl fmt::Display for DataAssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(s) => write!(f, "{s}"),
            Self::TypeMismatch { field, expected } => {
                write!(f, "field {field} must be {expected}")
            }
        }
    }
}

impl std::error::Error for DataAssetError {}

pub fn is_datatype_path(path: &str) -> bool {
    path.trim_end_matches(['/', '\\'])
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case(DATATYPE_EXT))
}

pub fn is_data_path(path: &str) -> bool {
    path.trim_end_matches(['/', '\\'])
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case(DATA_EXT))
}

impl DataTypeDef {
    pub fn parse(text: &str) -> Result<Self, DataAssetError> {
        let mut name = String::new();
        let mut fields = Vec::new();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let err = |what: &str| DataAssetError::Parse(format!("line {}: {what}", n + 1));
            let mut parts = line.split_whitespace();
            let key = parts.next().ok_or_else(|| err("expected a keyword"))?;
            match key {
                "seal_datatype" => {}
                "name" => {
                    let rest = line.split_once('=').map(|(_, v)| v.trim()).unwrap_or("");
                    name = if rest.is_empty() {
                        return Err(err("name needs a value"));
                    } else {
                        rest.to_string()
                    };
                }
                "field" => {
                    let fname = parts.next().ok_or_else(|| err("field needs a name"))?.to_string();
                    let kind_s = parts.next().ok_or_else(|| err("field needs a type"))?;
                    let kind = FieldKind::parse(kind_s).ok_or_else(|| err("unknown field type"))?;
                    let mut element = None;
                    let mut options = String::new();
                    let mut default = String::new();
                    for token in parts {
                        if let Some(rest) = token.strip_prefix("list=") {
                            element = FieldKind::parse(rest);
                        } else if let Some(rest) = token.strip_prefix("options=") {
                            options = rest.to_string();
                        } else if let Some(rest) = token.strip_prefix("default=") {
                            default = rest.to_string();
                        }
                    }
                    if kind == FieldKind::List && element.is_none() {
                        return Err(err("list fields need list=<type>"));
                    }
                    if kind == FieldKind::Choice && options.is_empty() {
                        return Err(err("choice fields need options=a|b"));
                    }
                    fields.push(DataFieldDef {
                        name: fname,
                        kind,
                        element,
                        options,
                        default,
                    });
                }
                _ => return Err(err("unknown keyword")),
            }
        }
        if name.is_empty() {
            return Err(DataAssetError::Parse("data type has no name".into()));
        }
        Ok(Self { name, fields })
    }

    pub fn to_text(&self) -> String {
        let mut out = format!("seal_datatype = 1\nname = {}\n", self.name);
        for field in &self.fields {
            let mut line = format!("field {} {}", field.name, field.kind.name());
            if let Some(elem) = field.element {
                line.push_str(&format!(" list={}", elem.name()));
            }
            if !field.options.is_empty() {
                line.push_str(&format!(" options={}", field.options));
            }
            if !field.default.is_empty() {
                line.push_str(&format!(" default={}", field.default));
            }
            out.push_str(&line);
            out.push('\n');
        }
        out
    }
}

/// Read only the `type =` line from a `.data` file.
pub fn data_type_path(text: &str) -> Result<String, DataAssetError> {
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.starts_with("type") {
            let path = line.split_once('=').map(|(_, v)| v.trim()).unwrap_or("");
            if path.is_empty() {
                return Err(DataAssetError::Parse("data asset has no type".into()));
            }
            return Ok(path.to_string());
        }
    }
    Err(DataAssetError::Parse("data asset has no type".into()))
}

impl DataAsset {
    pub fn parse(text: &str, type_def: &DataTypeDef) -> Result<Self, DataAssetError> {
        let mut name = String::new();
        let mut type_path = String::new();
        let mut values = Vec::new();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let err = |what: &str| DataAssetError::Parse(format!("line {}: {what}", n + 1));
            let mut parts = line.split_whitespace();
            let key = parts.next().ok_or_else(|| err("expected a keyword"))?;
            match key {
                "seal_data" => {}
                "name" => {
                    let rest = line.split_once('=').map(|(_, v)| v.trim()).unwrap_or("");
                    name = rest.to_string();
                }
                "type" => {
                    let rest = line.split_once('=').map(|(_, v)| v.trim()).unwrap_or("");
                    type_path = if rest.is_empty() {
                        return Err(err("type needs a path"));
                    } else {
                        rest.to_string()
                    };
                }
                "value" => {
                    let field = parts.next().ok_or_else(|| err("value needs a field name"))?;
                    let rest = line.strip_prefix("value").unwrap_or("").trim();
                    let rest = rest.strip_prefix(field).unwrap_or(rest).trim();
                    let value = rest.strip_prefix('=').unwrap_or(rest).trim().to_string();
                    validate_value(type_def, field, &value)?;
                    values.push((field.to_string(), value));
                }
                _ => return Err(err("unknown keyword")),
            }
        }
        if type_path.is_empty() {
            return Err(DataAssetError::Parse("data asset has no type".into()));
        }
        Ok(Self { name, type_path, values })
    }

    pub fn to_text(&self) -> String {
        let mut out = format!("seal_data = 1\n");
        if !self.name.is_empty() {
            out.push_str(&format!("name = {}\n", self.name));
        }
        out.push_str(&format!("type = {}\n", self.type_path));
        for (field, value) in &self.values {
            out.push_str(&format!("value {field} = {value}\n"));
        }
        out
    }
}

/// Check one value against a data type field definition.
pub fn validate_value(type_def: &DataTypeDef, field: &str, value: &str) -> Result<(), DataAssetError> {
    let spec = type_def
        .fields
        .iter()
        .find(|f| f.name == field)
        .ok_or_else(|| DataAssetError::Parse(format!("unknown field {field}")))?;
    // An empty value is an unset field: it uses the type's default, it is not a mistake.
    if value.trim().is_empty() {
        return Ok(());
    }
    match spec.kind {
        FieldKind::Number => {
            if value.parse::<f64>().ok().filter(|v| v.is_finite()).is_none() {
                return Err(DataAssetError::TypeMismatch {
                    field: field.into(),
                    expected: "a number".into(),
                });
            }
        }
        FieldKind::Bool => {
            if !matches!(value, "true" | "false" | "yes" | "no" | "1" | "0") {
                return Err(DataAssetError::TypeMismatch {
                    field: field.into(),
                    expected: "true or false".into(),
                });
            }
        }
        FieldKind::Colour => {
            if !value.contains(',') && !value.starts_with('#') {
                return Err(DataAssetError::TypeMismatch {
                    field: field.into(),
                    expected: "a colour".into(),
                });
            }
        }
        FieldKind::List => {
            if !value.starts_with('[') {
                return Err(DataAssetError::TypeMismatch {
                    field: field.into(),
                    expected: "a list".into(),
                });
            }
        }
        FieldKind::Choice => {
            let allowed: Vec<&str> = spec.options.split('|').map(str::trim).filter(|s| !s.is_empty()).collect();
            if !value.is_empty() && !allowed.iter().any(|opt| *opt == value) {
                return Err(DataAssetError::TypeMismatch {
                    field: field.into(),
                    expected: "a choice from the type".into(),
                });
            }
        }
        FieldKind::Text | FieldKind::Asset => {}
    }
    Ok(())
}

/// Read one CSV cell without pulling in extra dependencies (RFC-style quoting kept minimal).
pub fn read_csv_cell(text: &str, row: usize, col: usize) -> Result<String, DataAssetError> {
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        rows.push(parse_csv_line(line));
    }
    rows.get(row)
        .and_then(|r| r.get(col))
        .cloned()
        .ok_or_else(|| DataAssetError::Parse("cell is out of range".into()))
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in line.chars() {
        match (quoted, ch) {
            (true, '"') => quoted = false,
            (false, '"') => quoted = true,
            (false, ',') => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    out.push(cur.trim().to_string());
    out
}

/// Read a JSON object into a Data asset, checking every field against `type_def`.
pub fn read_typed_json(json: &str, type_path: &str, type_def: &DataTypeDef) -> Result<DataAsset, DataAssetError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| DataAssetError::Parse(e.to_string()))?;
    let obj = value
        .as_object()
        .ok_or_else(|| DataAssetError::Parse("JSON root must be an object".into()))?;
    let mut values = Vec::new();
    for field in &type_def.fields {
        let Some(raw) = obj.get(&field.name) else {
            if !field.default.is_empty() {
                values.push((field.name.clone(), field.default.clone()));
            }
            continue;
        };
        let text = json_field_text(raw)?;
        validate_value(type_def, &field.name, &text)?;
        values.push((field.name.clone(), text));
    }
    Ok(DataAsset {
        name: String::new(),
        type_path: type_path.to_string(),
        values,
    })
}

fn json_field_text(value: &serde_json::Value) -> Result<String, DataAssetError> {
    Ok(match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(json_field_text).collect::<Result<_, _>>()?;
            format!("[{}]", inner.join(", "))
        }
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    })
}

/// Write a Data asset to a JSON object with field types enforced.
pub fn write_typed_json(asset: &DataAsset, type_def: &DataTypeDef) -> Result<String, DataAssetError> {
    let mut map = serde_json::Map::new();
    for (field, value) in &asset.values {
        validate_value(type_def, field, value)?;
        let spec = type_def
            .fields
            .iter()
            .find(|f| f.name == *field)
            .ok_or_else(|| DataAssetError::Parse(format!("unknown field {field}")))?;
        map.insert(field.clone(), typed_json_value(spec, value)?);
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(map))
        .map_err(|e| DataAssetError::Parse(e.to_string()))
}

fn typed_json_value(spec: &DataFieldDef, value: &str) -> Result<serde_json::Value, DataAssetError> {
    Ok(match spec.kind {
        FieldKind::Number => {
            let n: f64 = value.parse().map_err(|_| DataAssetError::TypeMismatch {
                field: spec.name.clone(),
                expected: "a number".into(),
            })?;
            serde_json::Number::from_f64(n)
                .map(serde_json::Value::Number)
                .ok_or_else(|| DataAssetError::TypeMismatch {
                    field: spec.name.clone(),
                    expected: "a finite number".into(),
                })?
        }
        FieldKind::Bool => serde_json::Value::Bool(matches!(value, "true" | "yes" | "1")),
        FieldKind::Text | FieldKind::Asset => serde_json::Value::String(value.to_string()),
        FieldKind::Colour => serde_json::Value::String(value.to_string()),
        FieldKind::Choice => {
            if !spec.options.is_empty() && !spec.options.split('|').any(|o| o.trim() == value) {
                return Err(DataAssetError::TypeMismatch {
                    field: spec.name.clone(),
                    expected: format!("one of {}", spec.options),
                });
            }
            serde_json::Value::String(value.to_string())
        }
        FieldKind::List => {
            let inner = value
                .trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| {
                    typed_json_value(
                        &DataFieldDef {
                            name: spec.name.clone(),
                            kind: spec.element.unwrap_or(FieldKind::Text),
                            element: None,
                            options: String::new(),
                            default: String::new(),
                        },
                        s,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            serde_json::Value::Array(inner)
        }
    })
}

/// Read a dotted path from JSON text (e.g. `stats.hp`).
pub fn read_json_value(text: &str, path: &str) -> Result<String, DataAssetError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| DataAssetError::Parse(e.to_string()))?;
    let mut current = &value;
    for part in path.split('.').filter(|p| !p.is_empty()) {
        current = current
            .get(part)
            .ok_or_else(|| DataAssetError::Parse(format!("json path {path} was not found")))?;
    }
    Ok(match current {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        other => other.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_type_round_trip_and_mismatch() {
        let text = "seal_datatype = 1\nname = Unit\nfield hp number default=100\nfield tag text\n";
        let dt = DataTypeDef::parse(text).unwrap();
        assert_eq!(DataTypeDef::parse(&dt.to_text()).unwrap().name, "Unit");
        let asset_text = "seal_data = 1\ntype = data/unit.datatype\nvalue hp = nope\n";
        let err = DataAsset::parse(asset_text, &dt).unwrap_err();
        assert!(matches!(err, DataAssetError::TypeMismatch { .. }));
    }

    #[test]
    fn damaged_datatype_is_refused() {
        assert!(DataTypeDef::parse("!!!").is_err());
    }

    #[test]
    fn typed_json_round_trip_and_type_error() {
        let dt = DataTypeDef::parse(
            "seal_datatype = 1\nname = Hero\nfield hp number\nfield alive bool\n",
        )
        .unwrap();
        let json = r#"{"hp":10,"alive":true}"#;
        let asset = read_typed_json(json, "types/hero.datatype", &dt).unwrap();
        let back = write_typed_json(&asset, &dt).unwrap();
        let again = read_typed_json(&back, "types/hero.datatype", &dt).unwrap();
        assert_eq!(again.values.len(), 2);
        let bad = read_typed_json(r#"{"hp":"nope"}"#, "types/hero.datatype", &dt).unwrap_err();
        assert!(matches!(bad, DataAssetError::TypeMismatch { .. }));
    }

    #[test]
    fn csv_and_json_readers_work() {
        let csv = "a,b\n1,2\n";
        assert_eq!(read_csv_cell(csv, 1, 1).unwrap(), "2");
        let json = r#"{"stats":{"hp":42}}"#;
        assert_eq!(read_json_value(json, "stats.hp").unwrap(), "42");
    }
}
