//! Composite texture assets and GPU format settings.

use crate::command::CommandArgs;
use crate::error::CoreError;
use crate::session::EditorSession;
use crate::texture_asset_cmd::{
    cmd_texture_array_new, cmd_texture_cubemap_new, cmd_texture_export_encode, cmd_texture_set,
    cmd_texture_volume_new,
};

type CommandFn = fn(&mut EditorSession, &CommandArgs) -> Result<(), CoreError>;

pub const COMMANDS: &[(&str, &str, &str, CommandFn)] = &[
    (
        "asset.texture_array_new",
        "command-asset-texture-array-new",
        "file",
        texture_array_new,
    ),
    (
        "asset.texture_volume_new",
        "command-asset-texture-volume-new",
        "file",
        texture_volume_new,
    ),
    (
        "asset.texture_cubemap_new",
        "command-asset-texture-cubemap-new",
        "file",
        texture_cubemap_new,
    ),
    ("asset.texture_set", "command-asset-texture-set", "edit", texture_set),
    (
        "asset.texture_export_encode",
        "command-asset-texture-export-encode",
        "file",
        texture_export_encode,
    ),
];

fn texture_array_new(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    cmd_texture_array_new(session, args)
}

fn texture_volume_new(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    cmd_texture_volume_new(session, args)
}

fn texture_cubemap_new(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    cmd_texture_cubemap_new(session, args)
}

fn texture_set(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    cmd_texture_set(session, args)
}

fn texture_export_encode(session: &mut EditorSession, args: &CommandArgs) -> Result<(), CoreError> {
    cmd_texture_export_encode(session, args)
}