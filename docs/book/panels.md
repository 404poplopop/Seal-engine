# How to add a panel

Built-in panels live in `crates/seal-editor-panels`. Each one has a stable id, a title message id, a default dock, a row in `register_builtin`, and a branch in `show`. Layout and commands refer to the id; changing it later breaks saved layouts and key bindings.

## Stable panel id

Add a `pub const` next to the other panel ids in `crates/seal-editor-panels/src/lib.rs`. Use a short lowercase token with no spaces, for example `my-tool`. That string is the panel id everywhere: registry, layout files, and the `show` match.

```rust
pub const MY_TOOL: &str = "my-tool";
```

The registry rejects duplicate ids and empty ids. Pick a name once and keep it.

## Title message id

Visible titles come from `crates/seal-editor-core/locales/en.ftl`, not from string literals in the panel code. Add a line such as:

```fluent
panel-my-tool-title = My Tool
```

Use the `panel-…-title` pattern so titles stay easy to grep. In `show`, load the title with `session.messages.get("panel-my-tool-title")` (or pass the same key into `desc` when registering).

## DockPreference

`DockPreference` tells the default layout where to place a new panel: `Left`, `Center`, `Right`, or `Bottom`. It does not prevent the user from moving the panel later; it only sets the first home.

Choose the edge that matches how the panel is used. Lists and hierarchies usually sit on `Left`. Inspectors and property sheets fit `Right`. Logs and browsers often use `Bottom`. Primary editing surfaces use `Center`.

## Register in `register_builtin`

`register_builtin` loads all built-in `PanelDesc` values into the `PanelRegistry` at shell startup. Extend `builtin_descs()` with another `desc` call:

```rust
desc(MY_TOOL, "panel-my-tool-title", DockPreference::Right),
```

Each `PanelDesc` holds the id, title key, and `default_dock`. Registration runs from `crates/seal-editor-shell` when the session starts. If registration fails, startup reports the error instead of continuing with a half-registered set.

Later you can register additional panels from another crate by calling `PanelRegistry::register` yourself. Built-ins should stay in `register_builtin` so one function lists every stock panel.

## Branch in `show`

`show` is the only entry point the shell uses to draw a panel. Add a match arm that calls your draw function:

```rust
pub fn show(frame: &mut PanelFrame, id: &str, session: &EditorSession) -> PanelEvent {
    match id {
        // …existing arms…
        MY_TOOL => my_tool(frame, session),
        _ => {
            frame.title(&session.messages.format("unknown-panel", &[("id", id)]));
            frame.muted(&session.messages.get("unknown-panel-kept"));
            PanelEvent::None
        }
    }
}
```

Implement `my_tool` in the same crate. Use `PanelFrame` for layout and return `PanelEvent::Command` (or `PanelEvent::None`) when the user acts. Command ids must already be registered on `EditorSession`; see `docs/book/src/editor-shell/commands.md`.

Draw only through `PanelFrame`. Do not embed toolkit types in this crate’s public API.

## Checklist

1. `pub const` id in `seal-editor-panels`.
2. `panel-…-title` entry in `en.ftl`.
3. `desc(…)` with a `DockPreference` in `builtin_descs`.
4. Match arm and draw function in `show`.
5. Commands the panel emits are registered and covered when tests return.

Until registration and `show` both know the id, the shell may list the panel in layout data but cannot draw it, or the opposite. Keep the two steps in the same change.
