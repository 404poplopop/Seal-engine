# Commands, menus, and shortcuts

Every menu item, shortcut, and palette entry calls a command registered on `EditorSession`. The command id is stable. The title is a message id in `locales/en.ftl`.

Default shortcut for the command palette is Ctrl+Shift+P. Rebinding is the `keymap.rebind` command, with arguments `command` and `chord`. The user keymap is `keymap.seal` in the per-user editor directory. A chord already used by another command is rejected. A user binding replaces the default for that command.

The same commands are listed and run as JSON through `EditorSession::catalog_json` and `EditorSession::invoke_json`. That is the seam a later agent adapter will use. This milestone does not open a network port.

## How to add a command

1. Add an English string in `crates/seal-editor-core/locales/en.ftl`.
2. Register the id, title key, and menu category in `EditorSession::register_commands`.
3. Write a function that changes the session and returns `Result`.
4. If it needs a shortcut, add a default chord in `Keymap::defaults`.
5. Add a test that dispatches the id and checks the session.

Categories are `file`, `edit`, `view`, `window`, `tools`, and `help`. A command whose category is one of those appears in that menu.

## Documents and the journal

The scratch document lives on the Welcome panel. Each change is an undoable edit. Edits, undos, and redos are appended to `journal/scratch.journal`. A line without a terminating newline is ignored, so a crash keeps the previous edit. The next start replays the journal and undo still reaches the earlier text. Discard throws that journal away.
