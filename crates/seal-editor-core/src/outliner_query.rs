//! Outliner search query parsing (name, type, tag).

use crate::models::MockNode;

fn field<'n>(node: &'n MockNode, name: &str) -> &'n str {
    node.fields.iter().find(|(key, _)| key == name).map_or("", |(_, value)| value.as_str())
}

pub fn outliner_type_display(node: &MockNode) -> String {
    let raw = outliner_type_text(node);
    let mut chars = raw.chars();
    let Some(first) = chars.next() else {
        return raw;
    };
    format!("{}{}", first.to_uppercase(), chars.as_str())
}

/// Prefab/script rows expose a type-column link when `source_asset` is set.
pub struct OutlinerAssetEdit<'a> {
    pub kind: &'a str,
    pub asset: &'a str,
}

/// Visual row marks (unsaved, prefab badge, play italic, warning colour).
pub struct OutlinerRowMarks {
    pub unsaved: bool,
    pub prefab_badge: bool,
    pub play_spawned: bool,
    pub warning_key: Option<String>,
}

pub fn outliner_row_marks(node: &MockNode) -> OutlinerRowMarks {
    if node.id == "root" {
        return OutlinerRowMarks {
            unsaved: false,
            prefab_badge: false,
            play_spawned: false,
            warning_key: None,
        };
    }
    let role = field(node, "role");
    let warning = field(node, "outliner_warning");
    OutlinerRowMarks {
        unsaved: field(node, "unsaved") == "yes",
        prefab_badge: field(node, "source_kind") == "prefab" && role != "folder",
        play_spawned: field(node, "spawned_play") == "yes",
        warning_key: if warning.is_empty() { None } else { Some(warning.to_string()) },
    }
}

pub fn outliner_asset_edit(node: &MockNode) -> Option<OutlinerAssetEdit<'_>> {
    let kind = field(node, "source_kind");
    if kind != "prefab" && kind != "script" {
        return None;
    }
    let asset = field(node, "source_asset");
    if asset.is_empty() {
        return None;
    }
    Some(OutlinerAssetEdit { kind, asset })
}

/// Stable key for outliner type filters (`folder`, `light`, `cube`, …).
pub fn outliner_filter_type_key(node: &MockNode) -> String {
    let role = field(node, "role");
    if role == "folder" {
        return "folder".into();
    }
    if role == "light" {
        return "light".into();
    }
    if role == "camera" {
        return "camera".into();
    }
    let shape = field(node, "shape");
    if shape.is_empty() || shape == "none" {
        if role.is_empty() {
            "empty".into()
        } else {
            role.to_string()
        }
    } else {
        shape.to_string()
    }
}

pub fn outliner_type_text(node: &MockNode) -> String {
    let shape = field(node, "shape");
    if field(node, "source_kind") == "prefab" && !field(node, "source_asset").is_empty() {
        // A prefab copy reads as what it is: Coin, Seal, Flag.
        field(node, "source_asset").to_string()
    } else if !field(node, "sprite").is_empty() {
        "sprite".to_string()
    } else if matches!(field(node, "mask_shape"), "rect" | "circle" | "alpha") {
        "Sprite Mask".to_string()
    } else if shape.is_empty() || shape == "none" {
        field(node, "role").to_string()
    } else {
        shape.to_string()
    }
}

pub fn outliner_tag_text(node: &MockNode) -> &str {
    if field(node, "tag").is_empty() {
        field(node, "role")
    } else {
        field(node, "tag")
    }
}

#[derive(Debug, Clone)]
enum Token {
    Include(String),
    Exclude(String),
    Exact(String),
    Type(String),
}

fn parse_tokens(query: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut rest = query.trim();
    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        if let Some(stripped) = rest.strip_prefix('-') {
            let word = next_word(stripped);
            if !word.is_empty() {
                tokens.push(Token::Exclude(word.to_lowercase()));
            }
            rest = &stripped[word.len()..];
            continue;
        }
        if rest.starts_with('"') {
            if let Some(end) = rest[1..].find('"') {
                let phrase = rest[1..1 + end].to_string();
                tokens.push(Token::Exact(phrase.to_lowercase()));
                rest = &rest[1 + end + 1..];
                continue;
            }
        }
        if let Some(stripped) = rest.strip_prefix("type:") {
            let word = next_word(stripped);
            tokens.push(Token::Type(word.to_lowercase()));
            rest = &stripped[word.len()..];
            continue;
        }
        let word = next_word(rest);
        if word.is_empty() {
            break;
        }
        tokens.push(Token::Include(word.to_lowercase()));
        rest = &rest[word.len()..];
    }
    tokens
}

fn next_word(text: &str) -> &str {
    let end = text
        .char_indices()
        .find(|(_, ch)| ch.is_whitespace() || *ch == '"')
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    &text[..end]
}

pub fn outliner_matches(node: &MockNode, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    let name = node.name.to_lowercase();
    let ty = outliner_type_text(node).to_lowercase();
    let tag = outliner_tag_text(node).to_lowercase();
    let haystacks = [name.as_str(), ty.as_str(), tag.as_str()];
    for token in parse_tokens(query) {
        let ok = match token {
            Token::Include(needle) => haystacks.iter().any(|hay| hay.contains(needle.as_str())),
            Token::Exclude(needle) => !haystacks.iter().any(|hay| hay.contains(needle.as_str())),
            Token::Exact(phrase) => name == phrase.as_str(),
            Token::Type(kind) => ty == kind.as_str() || tag == kind.as_str(),
        };
        if !ok {
            return false;
        }
    }
    true
}

/// Compares names so runs of digits count as numbers: "grass 8" before "grass 10".
pub fn natural_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut a, mut b) = (left.chars().peekable(), right.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(c);
                        it.next();
                    }
                    digits
                };
                let (da, db) = (take(&mut a), take(&mut b));
                let (ta, tb) = (da.trim_start_matches('0'), db.trim_start_matches('0'));
                let order = ta
                    .len()
                    .cmp(&tb.len())
                    .then_with(|| ta.cmp(tb))
                    .then_with(|| da.len().cmp(&db.len()));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                if x != y {
                    return x.cmp(&y);
                }
                a.next();
                b.next();
            }
        }
    }
}

#[cfg(test)]
mod natural_tests {
    #[test]
    fn numbers_sort_by_value() {
        let mut names = vec!["grass 10", "grass 8", "Plane", "grass 34", "grass 9", "grass"];
        names.sort_by(|a, b| super::natural_cmp(a, b));
        assert_eq!(names, ["Plane", "grass", "grass 8", "grass 9", "grass 10", "grass 34"]);
    }
}
