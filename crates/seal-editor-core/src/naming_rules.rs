//! Per-kind asset naming rules stored in `naming-rules.seal`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use seal_sealtext::{parse, SealValue};

use crate::asset_browser::kind_of;
use crate::asset_index::{begin_content_path_change, normalize_rel};
use crate::error::CoreError;
use crate::persist::atomic_write;
use crate::session::EditorSession;

pub const RULES_FILE: &str = "naming-rules.seal";
pub const RULES_SCHEMA: u64 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamingRule {
    pub kind: String,
    pub prefix: String,
    pub suffix: String,
    /// `none`, `lower`, `upper`, or `snake`.
    pub case: String,
    pub pattern: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NamingRuleBook {
    pub rules: Vec<NamingRule>,
    pub violations: Vec<NamingViolation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamingViolation {
    pub path: String,
    pub kind: String,
    pub reason: String,
    pub suggested: String,
}

/// Lowercase names for pictures, sounds and animations: what every built-in template uses.
pub fn default_rules() -> Vec<NamingRule> {
    vec![
        NamingRule {
            kind: "picture".into(),
            prefix: "".into(),
            suffix: "".into(),
            case: "lower".into(),
            pattern: "".into(),
        },
        NamingRule {
            kind: "sound".into(),
            prefix: "".into(),
            suffix: "".into(),
            case: "lower".into(),
            pattern: "".into(),
        },
        NamingRule {
            kind: "animation".into(),
            prefix: "".into(),
            suffix: "".into(),
            case: "lower".into(),
            pattern: "".into(),
        },
    ]
}

pub fn write_template_defaults(root: &Path) -> Result<(), CoreError> {
    let book = NamingRuleBook { rules: default_rules(), violations: Vec::new() };
    save_book(root, &book)
}

fn rules_path(root: &Path) -> PathBuf {
    root.join(RULES_FILE)
}

pub fn load_book(root: &Path) -> NamingRuleBook {
    let path = rules_path(root);
    let Ok(text) = fs::read_to_string(&path) else {
        return NamingRuleBook { rules: default_rules(), violations: Vec::new() };
    };
    parse_book(&text)
        .unwrap_or_else(|_| NamingRuleBook { rules: default_rules(), violations: Vec::new() })
}

fn parse_book(text: &str) -> Result<NamingRuleBook, String> {
    let value = parse(text).map_err(|e| format!("{e}"))?;
    let mut book = NamingRuleBook::default();
    if let Some(SealValue::Array(rows)) = value.get("rules") {
        for row in rows {
            if let Some(rule) = parse_rule(row) {
                book.rules.push(rule);
            }
        }
    }
    Ok(book)
}

fn parse_rule(value: &SealValue) -> Option<NamingRule> {
    let SealValue::Array(parts) = value else {
        return None;
    };
    if parts.len() < 5 {
        return None;
    }
    let kind = string_at(parts, 0)?;
    Some(NamingRule {
        kind,
        prefix: string_at(parts, 1).unwrap_or_default(),
        suffix: string_at(parts, 2).unwrap_or_default(),
        case: string_at(parts, 3).unwrap_or_else(|| "none".into()),
        pattern: string_at(parts, 4).unwrap_or_default(),
    })
}

fn string_at(parts: &[SealValue], index: usize) -> Option<String> {
    match parts.get(index)? {
        SealValue::String(text) => Some(text.clone()),
        _ => None,
    }
}

pub fn save_book(root: &Path, book: &NamingRuleBook) -> Result<(), CoreError> {
    let rows: Vec<SealValue> = book
        .rules
        .iter()
        .map(|rule| {
            SealValue::Array(vec![
                SealValue::String(rule.kind.clone()),
                SealValue::String(rule.prefix.clone()),
                SealValue::String(rule.suffix.clone()),
                SealValue::String(rule.case.clone()),
                SealValue::String(rule.pattern.clone()),
            ])
        })
        .collect();
    let map = SealValue::Map(
        vec![
            ("schema_version".into(), SealValue::Number(RULES_SCHEMA.to_string())),
            ("rules".into(), SealValue::Array(rows)),
        ]
        .into(),
    );
    atomic_write(&rules_path(root), map.to_text().as_bytes())
}

pub fn load_for_project(session: &mut EditorSession) {
    let Some(project) = session.project.clone() else {
        return;
    };
    session.models.naming_rules = load_book(&project.root);
}

fn rule_for<'a>(book: &'a NamingRuleBook, kind: &str) -> Option<&'a NamingRule> {
    book.rules.iter().find(|r| r.kind == kind)
}

pub fn base_name(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.rsplit_once('.').map(|(stem, _)| stem.to_string()).unwrap_or_else(|| file.to_string())
}

pub fn check_name(book: &NamingRuleBook, rel: &str) -> Option<NamingViolation> {
    let rel = normalize_rel(rel);
    if rel.is_empty() {
        return None;
    }
    let is_dir = rel.ends_with('/');
    let kind = kind_of(&rel, is_dir);
    if kind == "folder" || kind == "other" || kind == "scene" {
        return None;
    }
    let rule = rule_for(book, kind)?;
    let name = base_name(&rel);
    if name.is_empty() {
        return None;
    }
    let reason = violation_reason(rule, &name)?;
    let suggested = suggested_name(rule, &name, &rel);
    Some(NamingViolation { path: rel, kind: kind.to_string(), reason, suggested })
}

fn violation_reason(rule: &NamingRule, name: &str) -> Option<String> {
    if !rule.prefix.is_empty() && !name.starts_with(&rule.prefix) {
        return Some(format!("name must start with {}", rule.prefix));
    }
    if !rule.suffix.is_empty() && !name.ends_with(&rule.suffix) {
        return Some(format!("name must end with {}", rule.suffix));
    }
    match rule.case.as_str() {
        "lower" if name != name.to_ascii_lowercase() => {
            return Some("name must use lowercase letters".into());
        }
        "upper" if name != name.to_ascii_uppercase() => {
            return Some("name must use uppercase letters".into());
        }
        "snake" if !is_snake_case(name) => {
            return Some("name must use snake_case".into());
        }
        _ => {}
    }
    if !rule.pattern.is_empty() {
        match regex::Regex::new(&rule.pattern) {
            Ok(re) if !re.is_match(name) => {
                return Some(format!("name must match {}", rule.pattern))
            }
            Ok(_) => {}
            Err(_) => {
                return Some(format!(
                    "the rule pattern {} is not a valid regular expression",
                    rule.pattern
                ))
            }
        }
    }
    None
}

fn is_snake_case(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

pub fn suggested_name(rule: &NamingRule, name: &str, rel: &str) -> String {
    let mut stem = name.to_string();
    if !rule.prefix.is_empty() && !stem.starts_with(&rule.prefix) {
        stem = format!("{}{stem}", rule.prefix);
    }
    if !rule.suffix.is_empty() && !stem.ends_with(&rule.suffix) {
        stem = format!("{stem}{}", rule.suffix);
    }
    stem = match rule.case.as_str() {
        "lower" => stem.to_ascii_lowercase(),
        "upper" => stem.to_ascii_uppercase(),
        "snake" => to_snake_case(&stem),
        _ => stem,
    };
    let ext = rel.rsplit_once('.').map(|(_, ext)| format!(".{ext}")).unwrap_or_default();
    let parent = rel.rsplit_once('/').map(|(dir, _)| format!("{dir}/")).unwrap_or_default();
    format!("{parent}{stem}{ext}")
}

fn to_snake_case(name: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            let lower = ch.to_ascii_lowercase();
            if ch.is_ascii_uppercase() && prev_lower {
                out.push('_');
            }
            out.push(lower);
            prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else if ch == '-' || ch == ' ' {
            if !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
            prev_lower = false;
        }
    }
    out
}

pub fn scan_project(session: &mut EditorSession) -> Result<(), CoreError> {
    let project = session.project.clone().ok_or_else(|| CoreError::new("no project is open"))?;
    let ignore = project.ignore.clone();
    let mut paths = Vec::new();
    crate::asset_index::walk_for_scan(&project.root, &project.root, &ignore, &mut paths)?;
    let book = session.models.naming_rules.clone();
    let mut violations = Vec::new();
    for rel in paths {
        if let Some(v) = check_name(&book, &rel) {
            violations.push(v);
        }
    }
    session.models.naming_rules.violations = violations.clone();
    for v in &violations {
        session.notify(
            session.messages.format(
                "naming-violation-line",
                &[("path", v.path.as_str()), ("reason", v.reason.as_str())],
            ),
            false,
        );
    }
    if !violations.is_empty() {
        session.reveal_panel = Some("naming-violations".into());
        if !session.foundation.open_panels.iter().any(|p| p == "naming-violations") {
            session.foundation.open_panels.push("naming-violations".into());
        }
    }
    Ok(())
}

pub fn note_violation(session: &mut EditorSession, rel: &str) {
    let book = session.models.naming_rules.clone();
    if let Some(v) = check_name(&book, rel) {
        session.models.naming_rules.violations.retain(|row| row.path != v.path);
        session.models.naming_rules.violations.push(v.clone());
        session.notify(
            session.messages.format(
                "naming-violation-line",
                &[("path", v.path.as_str()), ("reason", v.reason.as_str())],
            ),
            true,
        );
    }
}

pub fn fix_violation(session: &mut EditorSession, rel: &str) -> Result<(), CoreError> {
    let rel = normalize_rel(rel);
    let book = session.models.naming_rules.clone();
    let violation = check_name(&book, &rel)
        .ok_or_else(|| CoreError::new("this name already matches the rules"))?;
    let from = violation.path.clone();
    let to = violation.suggested.clone();
    begin_content_path_change(session, "rename", &from, &to)?;
    session.models.naming_rules.violations.retain(|v| v.path != from);
    Ok(())
}

pub fn upsert_rule(session: &mut EditorSession, rule: NamingRule) -> Result<(), CoreError> {
    let project = session.project.clone().ok_or_else(|| CoreError::new("no project is open"))?;
    session.models.naming_rules.rules.retain(|r| r.kind != rule.kind);
    session.models.naming_rules.rules.push(rule);
    save_book(&project.root, &session.models.naming_rules)
}

pub fn collect_paths(root: &Path, ignore: &[String]) -> BTreeSet<String> {
    let mut paths = Vec::new();
    let _ = crate::asset_index::walk_for_scan(root, root, ignore, &mut paths);
    paths.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_and_case_are_enforced() {
        let mut rules = default_rules();
        rules[0].prefix = "spr_".into();
        let book = NamingRuleBook { rules, violations: Vec::new() };
        let v = check_name(&book, "sprites/Hero.png").expect("violation");
        assert!(v.suggested.contains("spr_"));
        let defaults = NamingRuleBook { rules: default_rules(), violations: Vec::new() };
        assert!(check_name(&defaults, "sprites/hero.png").is_none(), "template names pass");
        assert!(v.suggested.contains("hero"));
    }

    #[test]
    fn damaged_rules_file_falls_back_to_defaults() {
        let dir = std::env::temp_dir().join(format!("seal-naming-rules-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(RULES_FILE), "rules = not-array\n").unwrap();
        let book = load_book(&dir);
        assert!(!book.rules.is_empty());
    }
}
