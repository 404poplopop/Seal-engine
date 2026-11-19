//! Project test scripts: a scene played headless for N frames, then assertion nodes.
//!
//! A `.sealtest` file is SealText. The Test Runner lists these files, and the
//! exported game runs the same set with `--run-tests`.

use std::fs;
use std::path::{Path, PathBuf};

use seal_sealtext::{SealMap, SealValue};
use seal_world::{Entity, World};

use crate::asset_editor::{asset_kind, open_asset, show_asset, AssetKind};
use crate::command::{CommandArgs, CommandHandler};
use crate::error::CoreError;
use crate::persist::atomic_write;
use crate::session::EditorSession;

pub const EXT: &str = "sealtest";
const SCHEMA: i64 = 1;
const FRAME_DT: f32 = 1.0 / 60.0;
const MAX_FRAMES: u32 = 100_000;
const RESULTS_KEY: &str = "test-runner.results";

type CommandFn = CommandHandler;

pub const COMMANDS: &[(&str, &str, &str, CommandFn)] = &[
    ("tests.run", "command-tests-run", "tools", cmd_run),
    ("tests.new", "command-tests-new", "file", cmd_new),
    ("tests.set", "command-tests-set", "edit", cmd_set),
    ("tests.assert_add", "command-tests-assert-add", "edit", cmd_assert_add),
    ("tests.assert_remove", "command-tests-assert-remove", "edit", cmd_assert_remove),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compare {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Compare {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "eq" => Self::Eq,
            "ne" => Self::Ne,
            "lt" => Self::Lt,
            "le" => Self::Le,
            "gt" => Self::Gt,
            "ge" => Self::Ge,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Lt => "lt",
            Self::Le => "le",
            Self::Gt => "gt",
            Self::Ge => "ge",
        }
    }

    fn word(self) -> &'static str {
        match self {
            Self::Eq => "equal",
            Self::Ne => "not equal",
            Self::Lt => "less than",
            Self::Le => "less or equal",
            Self::Gt => "greater than",
            Self::Ge => "greater or equal",
        }
    }
}

/// One assertion node. `object` and `expected` may be empty until the user fills them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssertNode {
    pub id: u32,
    pub object: String,
    pub property: String,
    pub op: Compare,
    pub expected: String,
}

/// A graph test script: play `scene` for `frames`, then check each node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestScript {
    pub name: String,
    pub scene: String,
    pub frames: u32,
    pub nodes: Vec<AssertNode>,
}

impl TestScript {
    pub fn template(name: &str) -> Self {
        Self {
            name: name.to_string(),
            scene: String::new(),
            frames: 1,
            nodes: vec![AssertNode {
                id: 1,
                object: String::new(),
                property: "name".into(),
                op: Compare::Eq,
                expected: String::new(),
            }],
        }
    }

    pub fn to_text(&self) -> String {
        let nodes = self
            .nodes
            .iter()
            .map(|node| {
                SealValue::Map(SealMap::new(vec![
                    ("id".into(), SealValue::Number(node.id.to_string())),
                    ("kind".into(), SealValue::String("assert".into())),
                    ("object".into(), SealValue::String(node.object.clone())),
                    ("property".into(), SealValue::String(node.property.clone())),
                    ("op".into(), SealValue::String(node.op.as_str().into())),
                    ("value".into(), SealValue::String(node.expected.clone())),
                ]))
            })
            .collect();
        SealValue::Map(SealMap::new(vec![
            ("schema_version".into(), SealValue::Number(SCHEMA.to_string())),
            ("seal_test".into(), SealValue::String(self.name.clone())),
            ("scene".into(), SealValue::String(self.scene.clone())),
            ("frames".into(), SealValue::Number(self.frames.to_string())),
            ("nodes".into(), SealValue::Array(nodes)),
        ]))
        .to_text()
    }
}

/// Read a test script. Garbage and the wrong schema are errors.
pub fn load_script(text: &str) -> Result<TestScript, String> {
    let value = seal_sealtext::parse(text).map_err(|err| format!("line {}: {}", err.line, err.message))?;
    if value.as_map().is_none() {
        return Err("a test script must be a map".into());
    }
    match value.get("schema_version").and_then(number_text).and_then(|n| n.parse::<i64>().ok()) {
        Some(SCHEMA) => {}
        Some(other) => {
            return Err(format!("test schema {other} is newer than this build reads ({SCHEMA})"));
        }
        None => return Err("not a test script: no schema_version".into()),
    }
    let name = match value.get("seal_test") {
        Some(SealValue::String(name)) => name.clone(),
        _ => return Err("not a test script: no seal_test name".into()),
    };
    let scene = match value.get("scene") {
        None => String::new(),
        Some(SealValue::String(scene)) => scene.clone(),
        Some(_) => return Err("scene must be text".into()),
    };
    let frames = match value.get("frames") {
        None => 1,
        Some(SealValue::Number(text)) => {
            let frames = text.parse::<u32>().map_err(|_| format!("frames `{text}` is not a whole number"))?;
            if frames == 0 || frames > MAX_FRAMES {
                return Err(format!("frames must be from 1 to {MAX_FRAMES}"));
            }
            frames
        }
        Some(_) => return Err("frames must be a number".into()),
    };
    let nodes = match value.get("nodes") {
        None => Vec::new(),
        Some(SealValue::Array(items)) => {
            let mut nodes = Vec::new();
            for item in items {
                nodes.push(load_node(item)?);
            }
            nodes
        }
        Some(_) => return Err("nodes must be a list".into()),
    };
    Ok(TestScript { name, scene, frames, nodes })
}

fn load_node(item: &SealValue) -> Result<AssertNode, String> {
    let id = item
        .get("id")
        .and_then(number_text)
        .and_then(|n| n.parse::<u32>().ok())
        .ok_or_else(|| "an assertion node has no id".to_string())?;
    let kind = match item.get("kind") {
        Some(SealValue::String(kind)) => kind.as_str(),
        _ => return Err(format!("node {id} has no kind")),
    };
    if kind != "assert" {
        return Err(format!("node {id} has unknown kind `{kind}`"));
    }
    let object = string_field(item, "object").unwrap_or_default();
    let property = string_field(item, "property").unwrap_or_else(|| "name".into());
    if !matches!(property.as_str(), "x" | "y" | "z" | "name" | "tag" | "hidden") {
        return Err(format!("node {id} has unknown property `{property}`"));
    }
    let op = match item.get("op") {
        Some(SealValue::String(text)) => {
            Compare::parse(text).ok_or_else(|| format!("node {id} has unknown compare `{text}`"))?
        }
        _ => return Err(format!("node {id} has no compare")),
    };
    let expected = string_field(item, "value").unwrap_or_default();
    Ok(AssertNode { id, object, property, op, expected })
}

fn string_field(item: &SealValue, key: &str) -> Option<String> {
    match item.get(key) {
        Some(SealValue::String(text)) => Some(text.clone()),
        _ => None,
    }
}

fn number_text(value: &SealValue) -> Option<&str> {
    match value {
        SealValue::Number(text) => Some(text),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseResult {
    pub path: String,
    pub name: String,
    pub passed: bool,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuiteReport {
    pub cases: Vec<CaseResult>,
}

impl SuiteReport {
    pub fn all_passed(&self) -> bool {
        self.cases.iter().all(|case| case.passed)
    }

    pub fn summary(&self) -> String {
        let mut out = String::new();
        for case in &self.cases {
            if case.passed {
                out.push_str(&format!("pass  {}\n", case.path));
            } else {
                out.push_str(&format!("fail  {}  {}\n", case.path, case.message));
            }
        }
        let failed = self.cases.iter().filter(|case| !case.passed).count();
        out.push_str(&format!("{} tests, {failed} failed\n", self.cases.len()));
        out
    }

    pub fn to_junit(&self) -> String {
        let failures = self.cases.iter().filter(|case| !case.passed).count();
        let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        out.push_str(&format!(
            "<testsuites>\n  <testsuite name=\"seal-tests\" tests=\"{}\" failures=\"{failures}\" errors=\"0\">\n",
            self.cases.len()
        ));
        for case in &self.cases {
            let name = xml_escape(&case.name);
            let class = xml_escape(&case.path);
            if case.passed {
                out.push_str(&format!("    <testcase name=\"{name}\" classname=\"{class}\"/>\n"));
            } else {
                let message = xml_escape(&case.message);
                out.push_str(&format!(
                    "    <testcase name=\"{name}\" classname=\"{class}\">\n      <failure message=\"{message}\">{message}</failure>\n    </testcase>\n"
                ));
            }
        }
        out.push_str("  </testsuite>\n</testsuites>\n");
        out
    }

    pub fn to_state(&self) -> String {
        let cases = self
            .cases
            .iter()
            .map(|case| {
                SealValue::Map(SealMap::new(vec![
                    ("path".into(), SealValue::String(case.path.clone())),
                    ("name".into(), SealValue::String(case.name.clone())),
                    ("passed".into(), SealValue::Bool(case.passed)),
                    ("message".into(), SealValue::String(case.message.clone())),
                ]))
            })
            .collect();
        SealValue::Map(SealMap::new(vec![("cases".into(), SealValue::Array(cases))])).to_text()
    }

    pub fn from_state(text: &str) -> Result<Self, String> {
        let value = seal_sealtext::parse(text).map_err(|err| err.to_string())?;
        let Some(SealValue::Array(items)) = value.get("cases") else {
            return Err("results have no cases".into());
        };
        let mut cases = Vec::new();
        for item in items {
            let path = string_field(item, "path").unwrap_or_default();
            let name = string_field(item, "name").unwrap_or_default();
            let passed = matches!(item.get("passed"), Some(SealValue::Bool(true)));
            let message = string_field(item, "message").unwrap_or_default();
            cases.push(CaseResult { path, name, passed, message });
        }
        Ok(Self { cases })
    }
}

pub fn xml_escape(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedTest {
    pub path: String,
    pub name: String,
    pub problem: Option<String>,
}

/// Every `.sealtest` file under the project, in path order.
pub fn list_tests(root: &Path) -> Vec<ListedTest> {
    let mut paths = Vec::new();
    collect_tests(root, root, &mut paths);
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            match fs::read_to_string(&path).map_err(|err| err.to_string()).and_then(|text| load_script(&text)) {
                Ok(script) => {
                    let name = if script.name.is_empty() { rel.clone() } else { script.name };
                    ListedTest { path: rel, name, problem: None }
                }
                Err(err) => ListedTest { path: rel.clone(), name: rel, problem: Some(err) },
            }
        })
        .collect()
}

fn collect_tests(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name == "target" {
            continue;
        }
        if path.is_dir() {
            collect_tests(root, &path, out);
        } else if path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case(EXT))
        {
            out.push(path);
        }
    }
}

/// Play every test script. A failed assertion is a case result, not an error.
pub fn run_project(root: &Path, only: Option<&str>) -> Result<SuiteReport, String> {
    if !root.is_dir() {
        return Err(format!("{} is not a project folder", root.display()));
    }
    let mut paths: Vec<String> = list_tests(root).into_iter().map(|item| item.path).collect();
    if let Some(only) = only {
        let only = only.replace('\\', "/");
        paths.retain(|path| path == &only);
        if paths.is_empty() {
            paths.push(only);
        }
    }
    let mut cases = Vec::new();
    for rel in paths {
        cases.push(run_file(root, &rel));
    }
    Ok(SuiteReport { cases })
}

fn run_file(root: &Path, rel: &str) -> CaseResult {
    let full = root.join(rel);
    let text = match fs::read_to_string(&full) {
        Ok(text) => text,
        Err(err) => {
            return CaseResult {
                path: rel.to_string(),
                name: rel.to_string(),
                passed: false,
                message: format!("could not read {rel}: {err}"),
            };
        }
    };
    let script = match load_script(&text) {
        Ok(script) => script,
        Err(err) => {
            return CaseResult {
                path: rel.to_string(),
                name: rel.to_string(),
                passed: false,
                message: err,
            };
        }
    };
    let name = if script.name.is_empty() { rel.to_string() } else { script.name.clone() };
    match play_and_check(root, &script) {
        Ok(()) => CaseResult { path: rel.to_string(), name, passed: true, message: String::new() },
        Err(message) => CaseResult { path: rel.to_string(), name, passed: false, message },
    }
}

fn play_and_check(root: &Path, script: &TestScript) -> Result<(), String> {
    if script.scene.trim().is_empty() {
        return Err("scene is not set".into());
    }
    let scene_rel = checked_rel(&script.scene).map_err(|err| err.to_string())?;
    let scene_path = root.join(&scene_rel);
    let scene_text = fs::read_to_string(&scene_path)
        .map_err(|err| format!("scene {scene_rel} could not be read: {err}"))?;
    let mut world = World::from_text(&scene_text)
        .map_err(|err| format!("scene {scene_rel} could not be read: {err}"))?;
    world.project_dir = Some(root.to_path_buf());
    world.play();
    for _ in 0..script.frames {
        world.step(FRAME_DT);
    }
    let mut failures = Vec::new();
    for node in &script.nodes {
        if let Err(message) = check_node(&world, node) {
            failures.push(message);
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

fn check_node(world: &World, node: &AssertNode) -> Result<(), String> {
    if node.object.trim().is_empty() {
        return Err(format!("node {}: object is not set", node.id));
    }
    let Some(entity) = find_object(world, &node.object) else {
        return Err(format!("{}: no object with that name", node.object));
    };
    let actual = read_property(entity, &node.property)
        .map_err(|err| format!("node {}: {err}", node.id))?;
    let matched = compare_values(node.op, &actual, &node.expected)
        .map_err(|err| format!("{}.{}: {err}", node.object, node.property))?;
    if matched {
        Ok(())
    } else {
        Err(format!(
            "{}.{}: expected {} {}, got {}",
            node.object,
            node.property,
            node.op.word(),
            node.expected,
            actual
        ))
    }
}

fn find_object<'a>(world: &'a World, name: &str) -> Option<&'a Entity> {
    world
        .entities
        .iter()
        .find(|entity| entity.name == name)
        .or_else(|| world.entities.iter().find(|entity| entity.id == name))
}

fn read_property(entity: &Entity, property: &str) -> Result<String, String> {
    Ok(match property {
        "x" => format_number(entity.translation[0]),
        "y" => format_number(entity.translation[1]),
        "z" => format_number(entity.translation[2]),
        "name" => entity.name.clone(),
        "tag" => entity.tag.clone(),
        "hidden" => if entity.hidden { "true".into() } else { "false".into() },
        other => return Err(format!("unknown property {other}")),
    })
}

fn format_number(value: f32) -> String {
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() { "0".into() } else { trimmed.to_string() }
}

fn compare_values(op: Compare, actual: &str, expected: &str) -> Result<bool, String> {
    if let (Some(left), Some(right)) = (parse_number(actual), parse_number(expected)) {
        let gap = (left - right).abs();
        return Ok(match op {
            Compare::Eq => gap <= 1e-3,
            Compare::Ne => gap > 1e-3,
            Compare::Lt => left < right,
            Compare::Le => left < right || gap <= 1e-3,
            Compare::Gt => left > right,
            Compare::Ge => left > right || gap <= 1e-3,
        });
    }
    match op {
        Compare::Eq => Ok(actual == expected),
        Compare::Ne => Ok(actual != expected),
        _ => Err(format!("{actual} is not a number")),
    }
}

fn parse_number(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.eq_ignore_ascii_case("true") {
        return Some(1.0);
    }
    if text.eq_ignore_ascii_case("false") {
        return Some(0.0);
    }
    text.parse().ok()
}

/// Run the project tests for a packaged game or CI. Writes JUnit XML when `junit` is set.
pub fn run_for_ci(root: &Path, junit: Option<&Path>) -> Result<(bool, String), String> {
    let report = run_project(root, None)?;
    if let Some(path) = junit {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|err| err.to_string())?;
            }
        }
        fs::write(path, report.to_junit()).map_err(|err| err.to_string())?;
    }
    Ok((report.all_passed(), report.summary()))
}

fn cmd_run(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let root = project_root(session)?;
    let only = args.get("only").filter(|text| !text.is_empty());
    let report = run_project(&root, only).map_err(CoreError::new)?;
    session.models.panel_states.insert(RESULTS_KEY.into(), report.to_state());
    if let Some(junit) = args.get("junit").filter(|text| !text.trim().is_empty()) {
        let dest = resolve_under(&root, junit)?;
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        atomic_write(&dest, report.to_junit().as_bytes())?;
    }
    if report.all_passed() {
        Ok(())
    } else {
        let failed = report.cases.iter().filter(|case| !case.passed).count();
        Err(CoreError::new(format!("{failed} test(s) failed")))
    }
}

fn cmd_new(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let mut rel = checked_rel(args.get("path").unwrap_or("tests/check"))?;
    if !rel.to_ascii_lowercase().ends_with(&format!(".{EXT}")) {
        rel.push('.');
        rel.push_str(EXT);
    }
    let root = project_root(session)?;
    let mut full = root.join(&rel);
    if full.exists() {
        if args.get("unique") != Some("yes") {
            return Err(CoreError::new(format!("{rel} already exists")));
        }
        let stem = rel.trim_end_matches(&format!(".{EXT}")).to_string();
        let mut n = 2;
        while full.exists() {
            rel = format!("{stem}_{n}.{EXT}");
            full = root.join(&rel);
            n += 1;
        }
    }
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent)?;
    }
    let stem = Path::new(&rel)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "check".into());
    atomic_write(&full, TestScript::template(&stem).to_text().as_bytes())?;
    show_asset(session, &rel)
}

fn cmd_set(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let (rel, full, mut script) = open_script(session, args)?;
    let field = args.get("field").unwrap_or("");
    let value = args.get("value").unwrap_or("");
    match field {
        "name" => script.name = value.to_string(),
        "scene" => {
            if !value.trim().is_empty() {
                checked_rel(value)?;
            }
            script.scene = value.trim().replace('\\', "/");
        }
        "frames" => {
            let frames = value.parse::<u32>().map_err(|_| CoreError::new("frames must be a whole number"))?;
            if frames == 0 || frames > MAX_FRAMES {
                return Err(CoreError::new(format!("frames must be from 1 to {MAX_FRAMES}")));
            }
            script.frames = frames;
        }
        "object" | "property" | "op" | "value" => {
            let index = arg_index(args)?;
            let node = script
                .nodes
                .get_mut(index)
                .ok_or_else(|| CoreError::new("no assertion at that index"))?;
            match field {
                "object" => node.object = value.to_string(),
                "property" => {
                    if !matches!(value, "x" | "y" | "z" | "name" | "tag" | "hidden") {
                        return Err(CoreError::new(format!("unknown property {value}")));
                    }
                    node.property = value.to_string();
                }
                "op" => {
                    node.op = Compare::parse(value)
                        .ok_or_else(|| CoreError::new(format!("unknown compare {value}")))?;
                }
                "value" => node.expected = value.to_string(),
                _ => {}
            }
        }
        _ => return Err(CoreError::new(format!("unknown test field {field}"))),
    }
    save_script(&full, &script)?;
    let _ = rel;
    Ok(())
}

fn cmd_assert_add(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let (_rel, full, mut script) = open_script(session, args)?;
    let id = script.nodes.iter().map(|node| node.id).max().unwrap_or(0).saturating_add(1);
    script.nodes.push(AssertNode {
        id,
        object: String::new(),
        property: "name".into(),
        op: Compare::Eq,
        expected: String::new(),
    });
    save_script(&full, &script)
}

fn cmd_assert_remove(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let (_rel, full, mut script) = open_script(session, args)?;
    let index = arg_index(args)?;
    if index >= script.nodes.len() {
        return Err(CoreError::new("no assertion at that index"));
    }
    script.nodes.remove(index);
    save_script(&full, &script)
}

fn arg_index(args: &CommandArgs) -> Result<usize, CoreError> {
    args.get("index")
        .unwrap_or("0")
        .parse::<usize>()
        .map_err(|_| CoreError::new("index must be a whole number"))
}

fn open_script(
    session: &EditorSession,
    args: &CommandArgs,
) -> Result<(String, PathBuf, TestScript), CoreError> {
    let root = project_root(session)?;
    let rel = if let Some(path) = args.get("path").filter(|text| !text.is_empty()) {
        checked_rel(path)?
    } else {
        let (rel, _) =
            open_asset(session).ok_or_else(|| CoreError::new("no test script is open"))?;
        rel
    };
    if asset_kind(&rel) != AssetKind::Test {
        return Err(CoreError::new("the file is not a test script"));
    }
    let full = root.join(&rel);
    let text = fs::read_to_string(&full)?;
    let script = load_script(&text).map_err(|err| CoreError::new(format!("{rel}: {err}")))?;
    Ok((rel, full, script))
}

fn save_script(full: &Path, script: &TestScript) -> Result<(), CoreError> {
    atomic_write(full, script.to_text().as_bytes())
}

fn project_root(session: &EditorSession) -> Result<PathBuf, CoreError> {
    session
        .project
        .as_ref()
        .map(|project| project.root.clone())
        .ok_or_else(|| CoreError::new("open a project first"))
}

fn checked_rel(rel: &str) -> Result<String, CoreError> {
    let rel = rel.trim().replace('\\', "/");
    if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|part| part == "..") {
        return Err(CoreError::new(format!("'{rel}' is not a path inside the project")));
    }
    Ok(rel)
}

fn resolve_under(root: &Path, raw: &str) -> Result<PathBuf, CoreError> {
    let path = PathBuf::from(raw.trim());
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(root.join(checked_rel(raw)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Layout;
    use seal_testkit::ScratchDir;

    fn project_file() -> String {
        "schema_version = 1\nname = \"Tests\"\nid = \"11111111-0000-7000-8000-000000000022\"\ncreated = 1\neditor_version = \"0\"\nmodules = []\nsettings = {}\nignore = [\".seal-editor/\"]\n".into()
    }

    fn open(dir: &Path) -> EditorSession {
        fs::write(dir.join("project.seal"), project_file()).unwrap();
        fs::write(dir.join("scene.seal"), "scene\n").unwrap();
        let mut session = EditorSession::new(Layout::default());
        let path = dir.to_string_lossy().to_string();
        session
            .dispatch("file.open-project", &CommandArgs::new(&[("path", path.as_str())]))
            .unwrap();
        session
    }

    fn move_graph() -> &'static str {
        r#"
schema_version = 1
seal_graph = "move"
vars = []
nodes = [
  { id = 1, kind = "event", name = "tick" },
  { id = 2, kind = "self" },
  { id = 3, kind = "const", type = "vec3", value = [5.0, 2.0, 0.0] },
  { id = 4, kind = "world_set", component = "transform", field = "translation", type = "vec3" },
]
links = [
  "1.then -> 4.in",
  "2.out -> 4.entity",
  "3.out -> 4.value",
]
"#
    }

    fn marker_scene(root: &Path) {
        let mut world = World::from_text(&World::empty_scene_text("Marker")).unwrap();
        world.entities[0].script = "move.graph".into();
        fs::write(root.join("move.graph"), move_graph()).unwrap();
        fs::write(root.join("marker.scene"), world.to_text()).unwrap();
    }

    fn write_script(root: &Path, rel: &str, script: &TestScript) {
        let full = root.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, script.to_text()).unwrap();
    }

    #[test]
    fn damaged_text_is_rejected() {
        assert!(load_script("not a script").is_err());
        assert!(load_script("schema_version = 1\n").unwrap_err().contains("seal_test"));
        let newer = "schema_version = 9\nseal_test = \"x\"\n";
        assert!(load_script(newer).unwrap_err().contains("newer"));
        let bad_kind = r#"
schema_version = 1
seal_test = "bad"
scene = ""
frames = 1
nodes = [ { id = 1, kind = "nope", object = "A", property = "x", op = "eq", value = "1" } ]
"#;
        assert!(load_script(bad_kind).unwrap_err().contains("unknown kind"));
    }

    #[test]
    fn a_passing_script_and_a_failing_one_report_in_junit() {
        let scratch = ScratchDir::new("functest");
        let root = scratch.path();
        marker_scene(root);
        write_script(
            root,
            "tests/holds.sealtest",
            &TestScript {
                name: "marker moves".into(),
                scene: "marker.scene".into(),
                frames: 2,
                nodes: vec![AssertNode {
                    id: 1,
                    object: "Marker".into(),
                    property: "x".into(),
                    op: Compare::Eq,
                    expected: "5".into(),
                }],
            },
        );
        write_script(
            root,
            "tests/misses.sealtest",
            &TestScript {
                name: "marker stays".into(),
                scene: "marker.scene".into(),
                frames: 2,
                nodes: vec![AssertNode {
                    id: 1,
                    object: "Marker".into(),
                    property: "x".into(),
                    op: Compare::Eq,
                    expected: "99".into(),
                }],
            },
        );
        let report = run_project(root, None).unwrap();
        assert_eq!(report.cases.len(), 2);
        assert!(report.cases[0].passed, "{}", report.cases[0].message);
        assert!(!report.cases[1].passed);
        assert!(
            report.cases[1].message.contains("99") && report.cases[1].message.contains("got"),
            "{}",
            report.cases[1].message
        );
        let xml = report.to_junit();
        assert!(xml.contains("marker moves"));
        assert!(xml.contains("marker stays"));
        assert!(xml.contains("<failure"));
        assert!(xml_well_formed(&xml), "{xml}");
        let junit = root.join("out").join("tests.xml");
        let (ok, summary) = run_for_ci(root, Some(&junit)).unwrap();
        assert!(!ok);
        assert!(summary.contains("fail"));
        let written = fs::read_to_string(&junit).unwrap();
        assert!(written.contains("marker moves") && written.contains("marker stays"));
    }

    #[test]
    fn the_runner_command_keeps_results_after_reload() {
        let scratch = ScratchDir::new("functest-ui");
        let root = scratch.path();
        marker_scene(root);
        write_script(
            root,
            "tests/holds.sealtest",
            &TestScript {
                name: "marker moves".into(),
                scene: "marker.scene".into(),
                frames: 1,
                nodes: vec![AssertNode {
                    id: 1,
                    object: "Marker".into(),
                    property: "name".into(),
                    op: Compare::Eq,
                    expected: "Marker".into(),
                }],
            },
        );
        let mut session = open(root);
        session
            .dispatch("tests.run", &CommandArgs::new(&[("junit", "report.xml")]))
            .unwrap();
        let stored = session.models.panel_states.get(RESULTS_KEY).cloned().unwrap();
        let again = SuiteReport::from_state(&stored).unwrap();
        assert!(again.cases.iter().any(|case| case.passed && case.name == "marker moves"));
        assert!(root.join("report.xml").is_file());
        let state = root.join("session.seal");
        session.save_panel_state(&state).unwrap();
        let mut loaded = EditorSession::new(Layout::default());
        loaded.load_panel_state(&state).unwrap();
        assert_eq!(loaded.models.panel_states.get(RESULTS_KEY).map(String::as_str), Some(stored.as_str()));
    }

    #[test]
    fn xml_escapes_markup_in_the_failure() {
        let report = SuiteReport {
            cases: vec![CaseResult {
                path: "a.sealtest".into(),
                name: "a<b".into(),
                passed: false,
                message: "1 & 2".into(),
            }],
        };
        let xml = report.to_junit();
        assert!(xml.contains("a&lt;b"));
        assert!(xml.contains("1 &amp; 2"));
        assert!(!xml.contains("a<b"));
    }

    fn xml_well_formed(text: &str) -> bool {
        let mut stack: Vec<String> = Vec::new();
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'<' {
                i += 1;
                continue;
            }
            if text[i..].starts_with("<?") {
                let Some(end) = text[i..].find("?>") else { return false };
                i += end + 2;
                continue;
            }
            let Some(end) = text[i..].find('>') else { return false };
            let raw = &text[i + 1..i + end];
            i += end + 1;
            let body = raw.trim();
            if body.starts_with('/') {
                let name = body[1..].split_whitespace().next().unwrap_or("");
                if stack.pop().as_deref() != Some(name) {
                    return false;
                }
            } else if let Some(stripped) = body.strip_suffix('/') {
                if stripped.split_whitespace().next().unwrap_or("").is_empty() {
                    return false;
                }
            } else {
                let name = body.split_whitespace().next().unwrap_or("");
                if name.is_empty() {
                    return false;
                }
                stack.push(name.to_string());
            }
        }
        stack.is_empty()
    }
}
