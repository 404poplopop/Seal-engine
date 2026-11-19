use std::path::PathBuf;

use crate::error::CoreError;

/// What the saved startup settings ask for when no project was passed on the command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupChoice {
    pub project: Option<String>,
    pub show_welcome: bool,
}

pub fn startup_choice(
    open_last: bool,
    show_welcome: bool,
    recent: &[String],
    has_explicit_project: bool,
) -> StartupChoice {
    let project = if has_explicit_project || !open_last { None } else { recent.first().cloned() };
    StartupChoice { project, show_welcome }
}

/// How the editor was asked to start. Parsed from process arguments.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaunchOptions {
    pub project: Option<PathBuf>,
    pub reset_layout: bool,
    pub safe_mode: bool,
    pub no_restore: bool,
    pub log_level: Option<String>,
    pub user_data: Option<PathBuf>,
    pub help: bool,
    pub self_check: bool,
    pub screenshot: Option<PathBuf>,
    /// Open straight into play with only the game view, as a packaged game does.
    pub game: bool,
    /// Run project test scripts and exit. Used by a packaged game in CI.
    pub run_tests: bool,
    /// JUnit XML path for `--run-tests`. Empty prints the summary only.
    pub tests_report: Option<PathBuf>,
}

impl LaunchOptions {
    pub fn parse<I, S>(args: I) -> Result<Self, CoreError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut options = Self::default();
        let mut args = args.into_iter().peekable();
        while let Some(arg) = args.next() {
            let arg = arg.as_ref();
            match arg {
                "--help" | "-h" => options.help = true,
                "--self-check" => options.self_check = true,
                "--reset-layout" => options.reset_layout = true,
                "--safe-mode" => options.safe_mode = true,
                "--no-restore" => options.no_restore = true,
                "--game" => options.game = true,
                "--run-tests" => {
                    options.run_tests = true;
                    if let Some(next) = args.peek() {
                        let next = next.as_ref();
                        if !next.starts_with('-') {
                            options.tests_report =
                                Some(PathBuf::from(args.next().expect("peeked").as_ref()));
                        }
                    }
                }
                "--screenshot" => {
                    options.screenshot = Some(PathBuf::from(next_value(&mut args, arg)?));
                }
                "--log-level" => {
                    options.log_level = Some(next_value(&mut args, arg)?);
                }
                "--user-data" => {
                    options.user_data = Some(PathBuf::from(next_value(&mut args, arg)?));
                }
                other if other.starts_with('-') => {
                    return Err(CoreError::new(format!("unknown argument: {other}")));
                }
                path => {
                    if options.project.is_some() {
                        return Err(CoreError::new("only one project path is accepted"));
                    }
                    options.project = Some(PathBuf::from(path));
                }
            }
        }
        Ok(options)
    }
}

fn next_value<I, S>(args: &mut std::iter::Peekable<I>, flag: &str) -> Result<String, CoreError>
where
    I: Iterator<Item = S>,
    S: AsRef<str>,
{
    args.next()
        .map(|value| value.as_ref().to_string())
        .ok_or_else(|| CoreError::new(format!("{flag} needs a value")))
}

pub const USAGE: &str = "\
seal-editor [PROJECT_PATH] [options]
seal-editor commands [--json]
seal-editor describe COMMAND [--json]
seal-editor run COMMAND [--project PATH] [--user-data DIR] [--json] [key=value ...]
seal-editor mcp [--port N]   connect an AI tool (MCP over stdio) to the running editor
seal-editor PROJECT --run-tests [REPORT.xml]

  --run-tests [FILE] run project test scripts and exit (packaged games too)
  --reset-layout     open with the default layout
  --safe-mode        default layout and settings, do not change saved state
  --no-restore       do not replay unsaved edits
  --log-level LEVEL  error, warn, info, debug, or trace
  --user-data DIR    per-user state directory
  --self-check       run headless checks and exit
  --screenshot FILE  draw the default layout to a PPM image and exit
  --help             show this help
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_project_path_and_startup_flags() {
        let options = LaunchOptions::parse([
            "game",
            "--safe-mode",
            "--no-restore",
            "--reset-layout",
            "--log-level",
            "warn",
            "--user-data",
            "/tmp/seal-user",
        ])
        .unwrap();
        assert_eq!(options.project.unwrap(), PathBuf::from("game"));
        assert!(options.safe_mode);
        assert!(options.no_restore);
        assert!(options.reset_layout);
        assert_eq!(options.log_level.as_deref(), Some("warn"));
        assert_eq!(options.user_data.unwrap(), PathBuf::from("/tmp/seal-user"));
    }

    #[test]
    fn run_tests_takes_an_optional_report_path() {
        let options = LaunchOptions::parse(["game", "--run-tests", "out/tests.xml"]).unwrap();
        assert!(options.run_tests);
        assert_eq!(options.tests_report.unwrap(), PathBuf::from("out/tests.xml"));
        assert_eq!(options.project.unwrap(), PathBuf::from("game"));
        let bare = LaunchOptions::parse(["--run-tests", "--safe-mode"]).unwrap();
        assert!(bare.run_tests);
        assert!(bare.tests_report.is_none());
        assert!(bare.safe_mode);
    }

    #[test]
    fn rejects_an_unknown_flag_and_a_missing_value() {
        assert!(LaunchOptions::parse(["--nope"]).is_err());
        assert!(LaunchOptions::parse(["--log-level"]).is_err());
        assert!(LaunchOptions::parse(["one", "two"]).is_err());
    }

    #[test]
    fn welcome_off_opens_the_last_project() {
        let recent = vec!["/games/demo".to_string()];
        let choice = startup_choice(true, false, &recent, false);
        assert_eq!(choice.project.as_deref(), Some("/games/demo"));
        assert!(!choice.show_welcome);
        let kept = startup_choice(false, true, &recent, false);
        assert!(kept.project.is_none());
        assert!(kept.show_welcome);
        let explicit = startup_choice(true, false, &recent, true);
        assert!(explicit.project.is_none());
    }
}
