//! Reader for the copy-paste text format of classic visual-script nodes:
//! `Begin Object Class=... Name="..."` blocks with `Key=Value` lines and
//! `CustomProperties Pin (...)` lines.

use std::fmt;

/// A value in a property: plain text or a parenthesised group.
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Text(String),
    Group(Vec<Item>),
}

/// One entry of a group: `Key=Value` or a bare value (as in `LinkedTo`).
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub key: Option<String>,
    pub val: Val,
}

impl Val {
    /// The text with one layer of double quotes removed.
    pub fn text(&self) -> Option<&str> {
        match self {
            Val::Text(s) => Some(s),
            Val::Group(_) => None,
        }
    }
    pub fn get(&self, key: &str) -> Option<&Val> {
        match self {
            Val::Group(items) => {
                items.iter().find(|i| i.key.as_deref() == Some(key)).map(|i| &i.val)
            }
            Val::Text(_) => None,
        }
    }
    pub fn items(&self) -> &[Item] {
        match self {
            Val::Group(items) => items,
            Val::Text(_) => &[],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pin {
    pub id: String,
    pub name: String,
    pub output: bool,
    pub category: String,
    pub sub_category: String,
    /// The struct or class a pin carries, such as `.../CoreUObject.Vector2D`.
    pub sub_object: String,
    /// Element kind for arrays, sets and maps; empty for single values.
    pub container: String,
    pub default: Option<String>,
    /// An object or class literal (`DefaultObject=...`), such as the class
    /// a spawn node creates.
    pub default_object: Option<String>,
    pub hidden: bool,
    /// `(node name, pin id)` for each link.
    pub linked: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    pub class: String,
    pub name: String,
    pub props: Vec<(String, Val)>,
    pub pins: Vec<Pin>,
    pub line: usize,
}

impl Object {
    /// The class without its package path, e.g. `K2Node_IfThenElse`.
    pub fn short_class(&self) -> &str {
        self.class.rsplit(['.', '/']).next().unwrap_or(&self.class)
    }
    pub fn prop(&self, key: &str) -> Option<&Val> {
        self.props.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    pub fn pin(&self, name: &str) -> Option<&Pin> {
        self.pins.iter().find(|p| p.name == name)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// Strip one layer of surrounding double quotes and undo `\"` and `\\`.
/// The source string of a text literal: `NSLOCTEXT("ns", "key", "Hi")`,
/// `LOCTEXT("key", "Hi")`, `INVTEXT("Hi")` or a plain quoted string. A
/// string-table reference has no source text here, so it gives `None`.
pub fn text_literal(raw: &str) -> Option<String> {
    let t = raw.trim();
    let Some(open) = t.find('(').filter(|_| t.ends_with(')')) else {
        return Some(unquote(t));
    };
    let args = &t[open + 1..t.len() - 1];
    let mut parts = Vec::new();
    let (mut start, mut quoted, mut esc) = (0, false, false);
    for (i, c) in args.char_indices() {
        match c {
            _ if esc => esc = false,
            '\\' if quoted => esc = true,
            '"' => quoted = !quoted,
            ',' if !quoted => {
                parts.push(&args[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&args[start..]);
    let want = match &t[..open] {
        "NSLOCTEXT" => 3,
        "LOCTEXT" => 2,
        "INVTEXT" => 1,
        "LOCTABLE" | "LOCGEN_NUMBER" | "LOCGEN_FORMAT_NAMED" | "LOCGEN_FORMAT_ORDERED" => {
            return None
        }
        // Plain text that happens to end in brackets.
        _ => return Some(t.to_string()),
    };
    (parts.len() == want).then(|| unquote(parts[want - 1]))
}

pub fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        let mut out = String::new();
        let mut chars = s[1..s.len() - 1].chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some(o) => out.push(o),
                    None => {}
                }
            } else {
                out.push(c);
            }
        }
        out
    } else {
        s.to_string()
    }
}

/// The quoted path inside a reference like `Class'"/Script/Pkg.Name"'`.
pub fn ref_path(s: &str) -> &str {
    match (s.find('"'), s.rfind('"')) {
        (Some(a), Some(b)) if b > a => &s[a + 1..b],
        _ => s.trim_matches('\''),
    }
}

struct Cursor<'a> {
    s: &'a [u8],
    i: usize,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.i += 1;
        }
    }
    /// Read raw text up to a top-level `,` or `)`, keeping quotes.
    fn raw(&mut self) -> String {
        let start = self.i;
        let (mut depth, mut quoted) = (0i32, false);
        while let Some(c) = self.peek() {
            if quoted {
                if c == b'\\' {
                    self.i += 1;
                } else if c == b'"' {
                    quoted = false;
                }
            } else {
                match c {
                    b'"' => quoted = true,
                    b'(' => depth += 1,
                    b')' if depth == 0 => break,
                    b')' => depth -= 1,
                    b',' if depth == 0 => break,
                    _ => {}
                }
            }
            self.i += 1;
        }
        String::from_utf8_lossy(&self.s[start..self.i.min(self.s.len())]).trim().to_string()
    }
    fn value(&mut self) -> Val {
        self.skip_ws();
        if self.peek() == Some(b'(') {
            self.i += 1;
            Val::Group(self.group())
        } else {
            Val::Text(unquote(&self.raw()))
        }
    }
    /// Items until the matching `)`.
    fn group(&mut self) -> Vec<Item> {
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            match self.peek() {
                None => break,
                Some(b')') => {
                    self.i += 1;
                    break;
                }
                Some(b',') => {
                    self.i += 1;
                    continue;
                }
                _ => {}
            }
            // A key is an identifier path followed by `=`.
            let start = self.i;
            while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == b'_' || c == b'.')
            {
                self.i += 1;
            }
            let key = (self.peek() == Some(b'=') && self.i > start)
                .then(|| String::from_utf8_lossy(&self.s[start..self.i]).to_string());
            if key.is_some() {
                self.i += 1;
            } else {
                self.i = start;
            }
            let val = self.value();
            items.push(Item { key, val });
        }
        items
    }
}

/// Parse `(a=1,b="x",c=(d=2))`.
pub fn parse_group(text: &str) -> Val {
    let mut c = Cursor { s: text.trim().as_bytes(), i: 0 };
    c.value()
}

fn begin_attr(line: &str, key: &str) -> Option<String> {
    let at = line.find(&format!("{key}="))? + key.len() + 1;
    let rest = &line[at..];
    let end = if let Some(inner) = rest.strip_prefix('"') {
        inner.find('"').map(|e| e + 2).unwrap_or(rest.len())
    } else {
        rest.find(' ').unwrap_or(rest.len())
    };
    Some(unquote(&rest[..end]))
}

fn pin_from(group: &Val) -> Pin {
    let text = |k: &str| group.get(k).and_then(Val::text).unwrap_or("").to_string();
    let linked = group
        .get("LinkedTo")
        .map(|l| {
            l.items()
                .iter()
                .filter_map(|i| i.val.text())
                .filter_map(|t| {
                    t.split_once(' ').map(|(n, p)| (n.to_string(), p.trim().to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    Pin {
        id: text("PinId"),
        name: text("PinName"),
        output: text("Direction") == "EGPD_Output",
        category: text("PinType.PinCategory"),
        sub_category: text("PinType.PinSubCategory"),
        sub_object: text("PinType.PinSubCategoryObject"),
        container: text("PinType.ContainerType"),
        default: group
            .get("DefaultValue")
            .and_then(Val::text)
            .map(str::to_string)
            .or_else(|| group.get("DefaultTextValue").and_then(Val::text).and_then(text_literal)),
        default_object: group
            .get("DefaultObject")
            .and_then(Val::text)
            .map(str::to_string)
            .filter(|t| t.trim() != "None"),
        hidden: text("bHidden").eq_ignore_ascii_case("true"),
        linked,
    }
}

/// Read every top-level object. Nested objects are skipped.
pub fn parse(text: &str) -> Result<Vec<Object>, ParseError> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut current: Option<Object> = None;
    for (n, raw) in text.lines().enumerate() {
        let line_no = n + 1;
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("Begin Object") {
            depth += 1;
            if depth == 1 {
                let class = begin_attr(rest, "Class").unwrap_or_default();
                let name = begin_attr(rest, "Name")
                    .ok_or(ParseError { line: line_no, message: "object without a Name".into() })?;
                current = Some(Object {
                    class,
                    name,
                    props: Vec::new(),
                    pins: Vec::new(),
                    line: line_no,
                });
            }
            continue;
        }
        if line == "End Object" {
            if depth == 0 {
                return Err(ParseError {
                    line: line_no,
                    message: "End Object without Begin Object".into(),
                });
            }
            depth -= 1;
            if depth == 0 {
                out.extend(current.take());
            }
            continue;
        }
        if depth != 1 {
            if depth == 0 {
                return Err(ParseError {
                    line: line_no,
                    message: format!("text outside an object: {line}"),
                });
            }
            continue;
        }
        let obj = current.as_mut().expect("depth 1 has an object");
        if let Some(rest) = line.strip_prefix("CustomProperties ") {
            if let Some(body) = rest.strip_prefix("Pin ") {
                obj.pins.push(pin_from(&parse_group(body)));
            }
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            obj.props.push((k.trim().to_string(), parse_group(v)));
        }
    }
    if depth != 0 {
        return Err(ParseError {
            line: text.lines().count(),
            message: "unterminated Begin Object".into(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_links_and_quotes() {
        let g = parse_group(
            r#"(PinId=AB12,PinName="In \"x\"",LinkedTo=(Node_1 FF00,Node_2 EE11,),Ref=Class'"/Script/Pkg.Lib"',Text=NSLOCTEXT("", "k", "v"))"#,
        );
        assert_eq!(g.get("PinName").unwrap().text(), Some("In \"x\""));
        let links: Vec<_> =
            g.get("LinkedTo").unwrap().items().iter().map(|i| i.val.text().unwrap()).collect();
        assert_eq!(links, ["Node_1 FF00", "Node_2 EE11"]);
        assert_eq!(ref_path(g.get("Ref").unwrap().text().unwrap()), "/Script/Pkg.Lib");
        assert_eq!(g.get("Text").unwrap().text(), Some(r#"NSLOCTEXT("", "k", "v")"#));
    }

    #[test]
    fn objects_pins_and_nesting() {
        let text = r#"
Begin Object Class=/Script/BlueprintGraph.K2Node_IfThenElse Name="K2Node_IfThenElse_0" ExportPath="x"
   NodePosX=10
   Begin Object Class=/Script/Other.Inner Name="Inner_0"
      Ignored=1
   End Object
   CustomProperties Pin (PinId=P1,PinName="Condition",PinType.PinCategory="bool",DefaultValue="true",LinkedTo=(K2Node_X_0 P9,),)
   CustomProperties Pin (PinId=P2,PinName="then",Direction="EGPD_Output",PinType.PinCategory="exec",)
End Object
"#;
        let objs = parse(text).unwrap();
        assert_eq!(objs.len(), 1);
        let o = &objs[0];
        assert_eq!(
            (o.short_class(), o.name.as_str()),
            ("K2Node_IfThenElse", "K2Node_IfThenElse_0")
        );
        assert_eq!(o.props.len(), 1);
        let cond = o.pin("Condition").unwrap();
        assert_eq!((cond.output, cond.default.as_deref()), (false, Some("true")));
        assert_eq!(cond.linked, [("K2Node_X_0".to_string(), "P9".to_string())]);
        assert!(o.pin("then").unwrap().output);
        assert!(parse("Begin Object Class=A Name=\"B\"\n").is_err());
    }
}
