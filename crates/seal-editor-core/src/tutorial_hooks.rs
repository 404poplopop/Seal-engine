//! Tutorials panel, recipes import, and template hooks.

use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::recipes;
use crate::session::EditorSession;

type CommandFn = fn(&mut EditorSession, &CommandArgs) -> Result<(), CoreError>;

pub const COMMANDS: &[(&str, &str, &str, CommandFn)] = &[
    ("tutorial.open", "command-tutorial-open", "help", tutorial_open),
    ("help.open_recipes", "command-help-recipes", "help", recipes_open),
    ("recipe.import", "command-recipe-import", "help", recipe_import),
];

fn tutorial_open(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    crate::project_tutorials::open_panel(session);
    Ok(())
}

pub fn recipes_open(session: &mut EditorSession, _args: &CommandArgs) -> Result<(), CoreError> {
    session.reveal_panel = Some("recipes".into());
    Ok(())
}

fn recipe_import(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    let id = args.get("id").ok_or_else(|| CoreError::new("choose a recipe"))?;
    recipes::import_recipe(session, id)
}

pub fn on_project_open(session: &mut EditorSession) {
    crate::project_tutorials::on_project_open(session);
}

pub fn after_apply_template(folder: &std::path::Path, template_id: &str) -> Result<(), CoreError> {
    crate::project_tutorials::write_shipped(folder, template_id)
}

pub fn on_play_tick(session: &mut EditorSession) {
    crate::project_tutorials::tick(session);
}