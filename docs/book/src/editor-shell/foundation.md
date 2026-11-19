# Foundation

The shell is the place later tools plug in. It does not ship a renderer, a play session, or a scripting editor. Those stay behind traits and disabled commands.

## Modules

`EditorModule` is the only registration path. A module adds document kinds during startup. The built-in panels use the same path (`core-panels`), so a later tool does not need a second mechanism. Modules are compiled in. Nothing is loaded from a file.

## Commands

Menus, shortcuts, the palette, and the in-process agent bus all call the command registry. A command has a stable id, a localized title, an argument schema, and flags for whether an agent may run it and whether it can be undone. An alias keeps an old id working. `when` expressions read context keys such as `hasProject` and `panelFocus`. Text input beats a shortcut that has no modifier. `Ctrl+K Ctrl+S` opens the keymap.

The palette understands `>` commands, `@` panels, `#` recent projects, and `?` help.

## Settings, layouts, and recovery

Settings are layered: the built-in value, then the user, then the project. Named layouts can be saved, loaded, renamed, exported, and imported without closing documents. Reset keeps the previous layout so it can be put back. A locked layout refuses the reset.

The scratch journal is still a line log, so a torn last line is ignored. New checksummed records live beside it: length, kind, sequence, payload, CRC32C. A short or corrupt tail is dropped.

## What the window does with this

Closing with unsaved text asks first. Escape keeps the text. A dropped project folder opens. Any other dropped file is reported and ignored. Full screen, zoom, and the status bar read the same session the tests drive. Play stays disabled and says why.
