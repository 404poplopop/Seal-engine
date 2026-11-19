# Adding a command

Every menu item, palette row, and default shortcut dispatches a command on `EditorSession`. Registration ties together a stable **command id**, an English **message id** for the visible title, a **category** for menus, a **handler**, and optionally a **keymap** binding.

## Command id

Pick a dotted id and keep it stable. Menus, the palette, `EditorSession::invoke_json`, saved keymaps, and aliases all refer to this string.

Use a short namespace and an action, for example `file.open-project`, `view.command-palette`, or `console.clear`. Once shipped, treat the id as part of the public surface: add an alias in `catalog::register_foundation` if an older name must keep working.

## English title (`en.ftl`)

Visible text never lives in Rust. Add a message id in `crates/seal-editor-core/locales/en.ftl`. Command titles use the `command-` prefix and mirror the id with hyphens:

```ftl
command-file-open-project = Open Project
```

The registration step passes this key as the **title key** (for example `command-file-open-project`), not the English sentence. `EditorSession` resolves it through the Fluent catalog when building menus, the palette, and `catalog_json`.

## Category

`CommandSpec::new` takes a category string. Standard menu buckets are:

| Category | Typical use |
| -------- | ----------- |
| `file` | Projects, documents, content on disk |
| `edit` | Undoable edits, selection, rename |
| `view` | Panels, zoom, console, outliner, viewport |
| `window` | Layout, docking, recovery |
| `tools` | Settings, keymap, runtime controls |
| `help` | About, manual, diagnostics |

Commands with `in_menu` left at its default (`true`) and one of these categories appear under that top-level menu. Palette and shortcuts are not limited by category.

## Registration

Wire the id, title key, category, and handler into the registry. Choose the table that matches what the command touches:

| Location | When to use |
| -------- | ----------- |
| `EditorSession::register_commands` in `session.rs` | Core session lifecycle: quit, project open/close, undo/redo, palette, layout snapshots, about, `keymap.rebind` |
| `catalog::register_foundation` in `catalog.rs` | Most shell commands: files, edit clipboard, panels, layouts, settings, recovery, help |
| `models::register_model_commands` in `models.rs` | Mock scene, outliner, inspector, asset browser, viewport, console |

Each row is a tuple `(id, title_key, category, handler)`. Register with `CommandSpec::new` and `commands.register`:

```rust
("help.about", "command-help-about", "help", about),
// ...
self.commands.register(
    CommandSpec::new(id, title_key, category),
    always_enabled,
    handler,
)?;
```

The **handler** has type `fn(&mut EditorSession, &CommandArgs) -> Result<(), CoreError>`. It should change session state and return `Ok(())` on success. Use `CommandSpec` builders in `catalog.rs` when a command needs requirements (`requires("project")`), extra arguments (`arg(...)`), menu hiding (`menu(false)`), or agent-safety flags.

Startup order is: `register_commands`, then `register_model_commands`, then `register_foundation`, then built-in panel modules. A duplicate id makes registration fail at startup.

## Keymap binding

Default shortcuts live in `Keymap::defaults` in `crates/seal-editor-core/src/keymap.rs`. After the command is registered, bind it with a chord string:

```rust
("view.command-palette", "ctrl+shift+p"),
```

For a single chord, `override_bind` attaches the command. For a sequence (for example `Ctrl+K` then `Ctrl+S` for `keymap.open`) or a binding that only applies in some UI state, use `override_steps` with an optional `when` expression (see `console.clear` in the same file).

Bindings must not steal a chord another command already owns unless you intentionally replace it. Users override defaults in `keymap.seal` under the per-user editor directory; the `keymap.rebind` command takes `command` and `chord` arguments and persists the change.

At runtime, `EditorSession::dispatch` resolves the canonical id, checks requirements and enabled state, then runs the handler. Shortcuts flow through `Keymap::resolve` on the UI thread; the palette and menus call the same dispatch path.

## Checklist

1. Add `command-… = …` in `en.ftl`.
2. Add `(id, title_key, category, handler)` to the right registration table and implement the handler.
3. If the command should have a factory default shortcut, add `(id, chord)` to `Keymap::defaults`.

After a restart, the command should show its localized title in the menu for its category, appear in the command palette (`>` mode), and run from a bound chord when enabled.
