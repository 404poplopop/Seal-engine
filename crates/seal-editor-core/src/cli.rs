use std::path::{Path, PathBuf};

use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::launch::{LaunchOptions, USAGE};
use crate::layout::Layout;
use crate::session::{EditorSession, HostAction};

/// A parsed editor invocation. `Launch` opens the window. The other variants run headless.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CliRequest {
    Launch(LaunchOptions),
    Help {
        verb: String,
    },
    Commands {
        json: bool,
    },
    Describe {
        id: String,
        json: bool,
    },
    Run {
        id: String,
        project: Option<PathBuf>,
        user_data: Option<PathBuf>,
        json: bool,
        args: CommandArgs,
    },
}

/// Text produced by a headless invocation. `success` is false when the command itself failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliOutput {
    pub text: String,
    pub success: bool,
}

pub fn parse_cli<I, S>(args: I) -> Result<CliRequest, CoreError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = args.into_iter().map(|arg| arg.as_ref().to_string()).collect();
    let Some(first) = args.first() else {
        return Ok(CliRequest::Launch(LaunchOptions::default()));
    };
    match first.as_str() {
        "commands" => parse_commands(&args[1..]),
        "describe" => parse_describe(&args[1..]),
        "run" => parse_run(&args[1..]),
        _ => Ok(CliRequest::Launch(LaunchOptions::parse(&args)?)),
    }
}

pub fn execute_cli(request: &CliRequest) -> Result<CliOutput, CoreError> {
    match request {
        CliRequest::Launch(_) => Err(CoreError::new("window launch is not a headless command")),
        CliRequest::Help { verb } => Ok(CliOutput { text: verb_help(verb), success: true }),
        CliRequest::Commands { json } => {
            let session = EditorSession::new(Layout::default());
            Ok(CliOutput { text: command_catalog(&session, *json), success: true })
        }
        CliRequest::Describe { id, json } => {
            let session = EditorSession::new(Layout::default());
            let text = describe_command(&session, id, *json)?;
            Ok(CliOutput { text, success: true })
        }
        CliRequest::Run { id, project, user_data, json, args } => {
            Ok(run_command(id, project.as_deref(), user_data.as_deref(), *json, args))
        }
    }
}

fn parse_commands(args: &[String]) -> Result<CliRequest, CoreError> {
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "--help" | "-h" => return Ok(CliRequest::Help { verb: "commands".into() }),
            other => return Err(CoreError::new(format!("unknown argument: {other}"))),
        }
    }
    Ok(CliRequest::Commands { json })
}

fn parse_describe(args: &[String]) -> Result<CliRequest, CoreError> {
    let mut json = false;
    let mut id = None;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "--help" | "-h" => return Ok(CliRequest::Help { verb: "describe".into() }),
            other if other.starts_with('-') => {
                return Err(CoreError::new(format!("unknown argument: {other}")));
            }
            _other if id.is_some() => {
                return Err(CoreError::new("describe accepts one command id"));
            }
            other => id = Some(other.to_string()),
        }
    }
    let Some(id) = id else {
        return Err(CoreError::new("describe needs a command id"));
    };
    Ok(CliRequest::Describe { id, json })
}

fn parse_run(args: &[String]) -> Result<CliRequest, CoreError> {
    let mut json = false;
    let mut project = None;
    let mut user_data = None;
    let mut id = None;
    let mut pairs = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--help" | "-h" => return Ok(CliRequest::Help { verb: "run".into() }),
            "--project" => {
                let value = rest.next().ok_or_else(|| CoreError::new("--project needs a value"))?;
                project = Some(PathBuf::from(value));
            }
            "--user-data" => {
                let value =
                    rest.next().ok_or_else(|| CoreError::new("--user-data needs a value"))?;
                user_data = Some(PathBuf::from(value));
            }
            other if other.starts_with('-') => {
                return Err(CoreError::new(format!("unknown argument: {other}")));
            }
            other if id.is_none() && !other.contains('=') => id = Some(other.to_string()),
            other => {
                let Some((key, value)) = other.split_once('=') else {
                    return Err(CoreError::new(format!("unexpected argument '{other}'")));
                };
                if key.is_empty() {
                    return Err(CoreError::new(format!("bad argument '{other}'")));
                }
                pairs.push((key.to_string(), value.to_string()));
            }
        }
    }
    let Some(id) = id else {
        return Err(CoreError::new("run needs a command id"));
    };
    let borrowed: Vec<(&str, &str)> =
        pairs.iter().map(|(key, value)| (key.as_str(), value.as_str())).collect();
    Ok(CliRequest::Run { id, project, user_data, json, args: CommandArgs::new(&borrowed) })
}

fn verb_help(verb: &str) -> String {
    match verb {
        "commands" => "seal-editor commands [--json]\n".into(),
        "describe" => "seal-editor describe COMMAND [--json]\n".into(),
        "run" => {
            "seal-editor run COMMAND [--project PATH] [--user-data DIR] [--json] [key=value ...]\n"
                .into()
        }
        _ => USAGE.to_string(),
    }
}

fn command_catalog(session: &EditorSession, json: bool) -> String {
    let mut specs: Vec<_> = session.commands.iter().collect();
    specs.sort_by(|left, right| left.id.cmp(&right.id));
    if json {
        let rows: Vec<_> = specs
            .iter()
            .map(|spec| {
                serde_json::json!({
                    "id": spec.id,
                    "title": session.messages.get(&spec.title_key),
                    "category": spec.category,
                    "alias": spec.alias_of,
                })
            })
            .collect();
        return serde_json::Value::Array(rows).to_string();
    }
    let mut out = String::new();
    for spec in specs {
        let title = one_line(&session.messages.get(&spec.title_key));
        let alias = spec.alias_of.as_deref().unwrap_or("-");
        out.push_str(&format!("{}\t{}\t{}\t{}\n", spec.id, spec.category, title, alias));
    }
    out
}

fn describe_command(session: &EditorSession, id: &str, json: bool) -> Result<String, CoreError> {
    let spec = session
        .commands
        .get(id)
        .ok_or_else(|| CoreError::new(format!("unknown command '{id}'")))?;
    let title = session.messages.get(&spec.title_key);
    if json {
        let args: Vec<_> = spec
            .args
            .iter()
            .map(|arg| serde_json::json!({ "name": arg.name, "kind": arg.kind, "required": arg.required }))
            .collect();
        return Ok(serde_json::json!({
            "id": spec.id,
            "title": title,
            "category": spec.category,
            "alias": spec.alias_of,
            "args": args,
        })
        .to_string());
    }
    let mut out = format!(
        "id: {}\ntitle: {}\ncategory: {}\nalias: {}\n",
        spec.id,
        one_line(&title),
        spec.category,
        spec.alias_of.as_deref().unwrap_or("-")
    );
    if spec.args.is_empty() {
        out.push_str("args: -\n");
    } else {
        for arg in &spec.args {
            out.push_str(&format!(
                "arg: {} {}{}\n",
                arg.name,
                arg.kind,
                if arg.required { " required" } else { "" }
            ));
        }
    }
    Ok(out)
}

fn run_command(
    id: &str,
    project: Option<&Path>,
    user_data: Option<&Path>,
    json: bool,
    args: &CommandArgs,
) -> CliOutput {
    let mut session = EditorSession::new(Layout::default());
    if let Some(dir) = user_data {
        bind_state(&mut session, dir);
    }
    if let Some(path) = project {
        if !is_project_create(id) {
            let open = CommandArgs::new(&[("path", &path.display().to_string())]);
            if let Err(err) = session.dispatch("file.open-project", &open) {
                return failed(id, &err.to_string(), json);
            }
        }
    }
    match session.dispatch(id, args) {
        Ok(()) => report(&session, id, json),
        Err(err) => failed(id, &err.to_string(), json),
    }
}

fn is_project_create(id: &str) -> bool {
    matches!(id, "file.new-project" | "file.new_project")
}

fn bind_state(session: &mut EditorSession, dir: &Path) {
    session.recent_path = Some(dir.join("recent.seal"));
    session.user_scene = Some(dir.join("scene.seal"));
    session.scene_journal = Some(dir.join("scene.journal"));
    session.undo_journal = Some(dir.join("undo.journal"));
    session.layouts_dir = Some(dir.join("layouts"));
    session.memory_path = Some(dir.join("memory.seal"));
    session.keymap_path = Some(dir.join("keymap.seal"));
}

fn report(session: &EditorSession, id: &str, json: bool) -> CliOutput {
    let project = session.project.as_ref().map(|project| project.name.clone());
    let host = session.host_action.as_ref().map(host_label);
    let notice = session.notice.as_deref().map(one_line);
    let dirty = session.dirty_count();
    if json {
        let text = serde_json::json!({
            "ok": true,
            "id": id,
            "project": project,
            "dirty": dirty,
            "host": host,
            "notice": notice,
            "close": session.request_close,
        })
        .to_string();
        return CliOutput { text, success: true };
    }
    CliOutput {
        text: format!(
            "ok {id}\nproject {}\ndirty {dirty}\nhost {}\nnotice {}\n",
            project.as_deref().unwrap_or("-"),
            host.as_deref().unwrap_or("-"),
            notice.as_deref().unwrap_or("-")
        ),
        success: true,
    }
}

fn failed(id: &str, message: &str, json: bool) -> CliOutput {
    if json {
        let text = serde_json::json!({
            "ok": false,
            "id": id,
            "error": message,
        })
        .to_string();
        return CliOutput { text, success: false };
    }
    CliOutput { text: format!("error {id}\n{}\n", one_line(message)), success: false }
}

fn host_label(action: &HostAction) -> String {
    match action {
        HostAction::PickFolder { create } => format!("pick-folder create={create}"),
        HostAction::OpenPath(path) => format!("open-path {}", path.display()),
        HostAction::SaveLog => "save-log".into(),
        HostAction::SaveElsewhere => "save-elsewhere".into(),
        HostAction::SaveAs => "save-as".into(),
        HostAction::ImportFile => "import-file".into(),
        HostAction::ImportAsset => "import-asset".into(),
        HostAction::ExportFile => "export-file".into(),
        HostAction::Restart { safe_mode } => format!("restart safe_mode={safe_mode}"),
        HostAction::RunExternal { exe, args } => {
            format!("run-external {exe} {}", args.join(" "))
        }
        HostAction::SaveProfile { name } => format!("save-profile {name}"),
        HostAction::OpenProfile { name } => format!("open-profile {name}"),
        HostAction::PickMigrateTarget => "pick-migrate-target".into(),
    }
}

fn one_line(text: &str) -> String {
    text.replace(['\n', '\r', '\t'], " ")
}

#[cfg(test)]
mod tests {
    use std::panic::AssertUnwindSafe;

    use super::*;

    #[test]
    fn a_project_path_still_launches_the_window() {
        let request = parse_cli(["game", "--safe-mode"]).unwrap();
        match request {
            CliRequest::Launch(options) => {
                assert_eq!(options.project.unwrap(), PathBuf::from("game"));
                assert!(options.safe_mode);
            }
            other => panic!("expected a window launch, got {other:?}"),
        }
    }

    #[test]
    fn commands_lists_every_registered_id() {
        let output = execute_cli(&parse_cli(["commands"]).unwrap()).unwrap();
        assert!(output.success);
        assert!(output.text.contains("file.new-project\t"));
        assert!(output.text.contains("edit.undo\t"));
        assert!(output.text.contains("play.start\t"));
        let json = execute_cli(&parse_cli(["commands", "--json"]).unwrap()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json.text).unwrap();
        assert!(value.as_array().unwrap().iter().any(|row| row["id"] == "file.save"));
    }

    #[test]
    fn describe_prints_one_command_and_rejects_an_unknown_id() {
        let output = execute_cli(&parse_cli(["describe", "file.save"]).unwrap()).unwrap();
        assert!(output.text.contains("id: file.save"));
        assert!(execute_cli(&parse_cli(["describe", "no.such"]).unwrap()).is_err());
        assert!(parse_cli(["describe"]).is_err());
        assert!(parse_cli(["run"]).is_err());
    }

    #[test]
    fn run_creates_a_project_and_reports_it() {
        let dir = std::env::temp_dir().join(format!("seal-cli-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let project = dir.join("demo");
        let state = dir.join("state");
        let request = parse_cli([
            "run",
            "--user-data",
            state.to_str().unwrap(),
            "--json",
            "file.new-project",
            &format!("path={}", project.display()),
            "name=Demo",
        ])
        .unwrap();
        let output = execute_cli(&request).unwrap();
        assert!(output.success, "{}", output.text);
        let value: serde_json::Value = serde_json::from_str(&output.text).unwrap();
        assert_eq!(value["project"], "Demo");
        assert_eq!(value["ok"], true);
        assert!(project.join("project.seal").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_reports_a_command_error_without_panicking() {
        let output = execute_cli(&parse_cli(["run", "no.such"]).unwrap()).unwrap();
        assert!(!output.success);
        assert!(output.text.starts_with("error no.such"));
    }

    #[test]
    fn every_command_accepts_an_empty_argument_list() {
        let session = EditorSession::new(Layout::default());
        let ids: Vec<String> = session.commands.iter().map(|spec| spec.id.clone()).collect();
        assert!(ids.len() > 40, "expected the full command list");
        for id in ids {
            let mut session = EditorSession::new(Layout::default());
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                session.dispatch(&id, &CommandArgs::default())
            }));
            assert!(result.is_ok(), "{id} panicked");
        }
    }
}
