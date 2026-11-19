//! Checks the English Fluent catalog against editor sources.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use super::{duplicate_ids, message_id_token};

/// Scan `roots` and compare message use to `ftl`.
pub fn check_english_catalog(
    ftl: &str,
    roots: &[PathBuf],
    extra_used: &[String],
) -> Result<(), String> {
    check_english_catalog_with_sources(ftl, roots, extra_used, &[])
}

/// Like [`check_english_catalog`], plus extra Rust source snippets (for unit tests).
pub fn check_english_catalog_with_sources(
    ftl: &str,
    roots: &[PathBuf],
    extra_used: &[String],
    extra_sources: &[&str],
) -> Result<(), String> {
    let declared = declared_ids(ftl);
    let mut used = scan_roots(roots);
    for text in extra_sources {
        for id in quoted_ids_in_source(text) {
            used.insert(id);
        }
    }
    for id in extra_used {
        used.insert(id.clone());
    }

    let mut problems = Vec::new();

    for id in duplicate_ids(ftl) {
        problems.push(format!("duplicate message id: {id}"));
    }

    // An id is in use when its exact quoted literal appears anywhere in the
    // sources, whichever helper it is passed through (menus, toolbars, tooltips).
    // Ids built as `format!("prefix-{value}")` count for every id with that prefix.
    let sources = all_sources(roots, extra_sources);
    let prefixes: BTreeSet<String> =
        sources.iter().flat_map(|text| dynamic_prefixes(text)).collect();
    for id in declared.difference(&used) {
        let literal = format!("\"{id}\"");
        let dynamic = prefixes.iter().any(|prefix| id.starts_with(prefix.as_str()));
        if !dynamic && !sources.iter().any(|text| text.contains(&literal)) {
            problems.push(format!("unused message id: {id}"));
        }
    }

    for id in used.difference(&declared) {
        problems.push(format!("missing message id: {id}"));
    }

    let mut format_calls = format_calls_in_roots(roots);
    for text in extra_sources {
        format_calls.extend(extract_format_calls(text));
    }
    for (id, args) in format_calls {
        if !declared.contains(&id) {
            continue;
        }
        let expected = placeholders_for_id(ftl, &id);
        let passed: BTreeSet<String> = args.into_iter().collect();
        if passed != expected {
            problems.push(format!(
                "placeholder mismatch for {id}: catalog {:?}, code {:?}",
                expected, passed
            ));
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        let mut text = String::from("English catalog check failed:\n");
        for line in problems {
            let _ = writeln!(text, "  {line}");
        }
        Err(text)
    }
}

/// Message ids declared at the top level of `ftl`.
pub fn declared_ids(ftl: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
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
            ids.insert(id.to_string());
        }
    }
    ids
}

/// Placeholder names referenced as `{ $name }` in the body of `id`.
pub fn placeholders_for_id(ftl: &str, id: &str) -> BTreeSet<String> {
    let Some(body) = message_body(ftl, id) else {
        return BTreeSet::new();
    };
    let mut names = BTreeSet::new();
    let mut rest = body.as_str();
    while let Some(open) = rest.find("{ $") {
        rest = &rest[open + 3..];
        let Some(close) = rest.find('}') else {
            break;
        };
        let name = rest[..close].trim();
        if !name.is_empty() {
            names.insert(name.to_string());
        }
        rest = &rest[close..];
    }
    names
}

fn message_body(ftl: &str, id: &str) -> Option<String> {
    let mut lines = ftl.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with(|c: char| c.is_ascii_whitespace() || c == '-' || c == '.') {
            continue;
        }
        let Some((head, value)) = trimmed.split_once('=') else {
            continue;
        };
        if head.trim() != id {
            continue;
        }
        let mut body = value.trim_start().to_string();
        while let Some(next) = lines.peek() {
            if next.is_empty() || next.starts_with('#') {
                lines.next();
                continue;
            }
            if next.starts_with(|c: char| c.is_ascii_whitespace()) && !next.contains('=') {
                body.push(' ');
                body.push_str(next.trim());
                lines.next();
            } else {
                break;
            }
        }
        return Some(body);
    }
    None
}

/// `prefix-` from every `"prefix-{` in a source.
fn dynamic_prefixes(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("-{") {
        let before = &rest[..at + 1];
        if let Some(open) = before.rfind('"') {
            let prefix = &before[open + 1..];
            if prefix.len() > 1
                && prefix
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
            {
                out.push(prefix.to_string());
            }
        }
        rest = &rest[at + 2..];
    }
    out
}

fn all_sources(roots: &[PathBuf], extra: &[&str]) -> Vec<String> {
    fn walk(dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    out.push(text);
                }
            }
        }
    }
    let mut out: Vec<String> = extra.iter().map(|s| s.to_string()).collect();
    for root in roots {
        walk(root, &mut out);
    }
    out
}

fn scan_roots(roots: &[PathBuf]) -> BTreeSet<String> {
    let mut used = BTreeSet::new();
    for root in roots {
        scan_dir(root, &mut used);
    }
    used
}

fn format_calls_in_roots(roots: &[PathBuf]) -> Vec<(String, Vec<String>)> {
    let mut calls = Vec::new();
    for root in roots {
        collect_format_calls_dir(root, &mut calls);
    }
    calls
}

fn scan_dir(dir: &Path, used: &mut BTreeSet<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_dir(&path, used);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for id in quoted_ids_in_source(&text) {
            used.insert(id);
        }
    }
}

fn collect_format_calls_dir(dir: &Path, calls: &mut Vec<(String, Vec<String>)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_format_calls_dir(&path, calls);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        calls.extend(extract_format_calls(&text));
    }
}

fn quoted_ids_in_source(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    ids.extend(lookup_ids_after_markers(text, ".get("));
    ids.extend(lookup_ids_after_markers(text, ".format("));
    for marker in ["text(\"", "text(&", "title_key: \"", "heading_key: \""] {
        let mut rest = text;
        while let Some(start) = rest.find(marker) {
            rest = &rest[start + marker.len()..];
            let Some(end) = rest.find('"') else {
                break;
            };
            let id = &rest[..end];
            if looks_like_message_id(id) && id != "missing-id" {
                ids.push(id.to_string());
            }
        }
    }
    for prefix in [
        "\"command-",
        "\"panel-",
        "\"modal-",
        "\"error-",
        "\"save-cannot\"",
        "\"save-elsewhere\"",
        "\"save-disk-full\"",
        "\"save-permission\"",
        "\"save-path-too-long\"",
        "\"save-locked\"",
        "\"recovery-",
        "\"console-level-",
        "\"wizard-error-",
        "\"content-error-",
        "\"content-view-",
        "\"inspector-",
        "\"recovery-doc-",
        "\"recovery-noun-",
        "\"status-saving\"",
        "\"status-saved\"",
        "\"dirty-mark\"",
        "\"saved-mark\"",
        "\"play-unavailable\"",
        "\"project-read-only\"",
    ] {
        let mut rest = text;
        while let Some(start) = rest.find(prefix) {
            rest = &rest[start + 1..];
            let Some(end) = rest.find('"') else {
                break;
            };
            let id = &rest[..end];
            if looks_like_message_id(id) {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

fn lookup_ids_after_markers(text: &str, marker: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(marker) {
        let after = &rest[start + marker.len()..];
        if let Some(id) = next_quoted_token(after) {
            if looks_like_message_id(&id) && id != "missing-id" {
                ids.push(id);
            }
        }
        rest = &rest[start + marker.len()..];
    }
    ids
}

fn next_quoted_token(mut text: &str) -> Option<String> {
    text = text.trim_start();
    if !text.starts_with('"') {
        return None;
    }
    text = &text[1..];
    let end = text.find('"')?;
    Some(text[..end].to_string())
}

fn extract_format_calls(text: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let marker = ".format(";
    let mut rest = text;
    while let Some(start) = rest.find(marker) {
        rest = &rest[start + marker.len()..];
        let Some(id) = next_quoted_token(rest) else {
            break;
        };
        if !looks_like_message_id(&id) {
            continue;
        }
        let Some(args_start) = rest.find("&[") else {
            continue;
        };
        let mut args_text = &rest[args_start..];
        let Some(args_end) = args_text.find(']') else {
            continue;
        };
        args_text = &args_text[..=args_end];
        let mut args = Vec::new();
        let mut chunk = args_text;
        while let Some(paren) = chunk.find("(\"") {
            chunk = &chunk[paren + 2..];
            let Some(close) = chunk.find('"') else {
                break;
            };
            let key = &chunk[..close];
            if looks_like_placeholder_key(key) {
                args.push(key.to_string());
            }
        }
        out.push((id, args));
        rest = &rest[args_start + args_end..];
    }
    out
}

fn looks_like_message_id(id: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "about",
        "app",
        "assistant",
        "timeline",
        "asset",
        "command",
        "console",
        "content",
        "diagnostics",
        "dev",
        "dirty",
        "draft",
        "drop",
        "duplicate",
        "edit",
        "error",
        "graph",
        "help",
        "inspector",
        "keymap",
        "layout",
        "layouts",
        "log",
        "menu",
        "modal",
        "motion",
        "notice",
        "notices",
        "outliner",
        "palette",
        "panel",
        "path",
        "place",
        "play",
        "project",
        "recovery",
        "restart",
        "save",
        "saved",
        "scene",
        "setting",
        "settings",
        "shape",
        "stats",
        "status",
        "theme",
        "toast",
        "toolbar",
        "unknown",
        "view",
        "viewport",
        "welcome",
        "wizard",
        "world",
    ];
    let Some((prefix, _)) = id.split_once('-') else {
        return false;
    };
    if !PREFIXES.contains(&prefix) {
        return false;
    }
    id.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

fn looks_like_placeholder_key(key: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn missing_id_appears_in_failure_text() {
        let ftl = "seen = Hello\n";
        let used = ["no-such-id".to_string()];
        let err = check_english_catalog(ftl, &[], &used).unwrap_err();
        assert!(err.contains("no-such-id"), "{err}");
    }

    #[test]
    fn scan_picks_shell_message_lookups() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../seal-editor-shell/src");
        let mut used = BTreeSet::new();
        scan_dir(&root, &mut used);
        assert!(used.contains("menu-close"), "{used:?}");
    }

    #[test]
    fn duplicate_and_unused_ids_fail() {
        let ftl = "dup = One\ndup = Two\nlonely = Alone\nseen = Hello\n";
        let err = check_english_catalog(ftl, &[], &["seen".into()]).unwrap_err();
        assert!(err.contains("duplicate message id: dup"), "{err}");
        assert!(err.contains("unused message id: lonely"), "{err}");
    }

    #[test]
    fn placeholder_mismatch_fails() {
        let ftl = "status-test = Hi { $name }\n";
        let src = r#"messages.format("status-test", &[("other", "x")]);"#;
        let err = check_english_catalog_with_sources(ftl, &[], &["status-test".into()], &[src])
            .unwrap_err();
        assert!(err.contains("placeholder mismatch for status-test"), "{err}");
    }
}
