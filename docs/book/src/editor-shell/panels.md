# Panels, themes, and diagnostics

The built-in panels are Welcome, Outliner, Inspector, Viewport, Console, Asset Browser, and Settings. They read `EditorSession` and return commands. Keyboard-only use goes through those commands: `outliner.rename`, `inspector.set`, `view.focus-next`, `settings.set`, and the others registered beside them.

The outliner and inspector edit a `MockWorld`. That type is the stand-in for a later world and is not an engine type.

## How to add a panel

1. Add a stable id and a title message.
2. Register a `PanelDesc` in `register_builtin` or, later, from your own crate through `PanelRegistry`.
3. Draw it from `show` with `PanelFrame`. Do not name the UI toolkit there.
4. When the user acts, return `PanelEvent::Command` with an id that is already registered.
5. Give the panel a dock preference so a layout can place it.

## Appearance

`themes/dark.seal`, `themes/light.seal`, and `themes/contrast.seal` are the three themes. `ui.scale` is clamped from 0.5 to 3. Colors are paired with text labels so a value is not color alone.

## Diagnostics

`write_diagnostics` writes the version, platform, recovery note, and recent log. A panic hook writes `crash.txt`. A clean exit writes `clean-shutdown`; if that file is missing on the next start, the shell treats the last run as unclean.

Play is present and disabled. Its tooltip says play arrives with the runtime.
