# Keymap

Shortcuts are data, not hard-coded UI behavior. The editor loads factory defaults, merges a per-user `keymap.seal` file, and resolves each key press through `Keymap::resolve` before `EditorSession::dispatch` runs the command. Command ids are the stable link between menus, the palette, handlers, and bindings.

## Default chord in `Keymap::defaults`

Factory shortcuts are assembled in `Keymap::defaults` in `crates/seal-editor-core/src/keymap.rs`. The function builds an empty `Keymap` and registers bindings in three ways:

1. **Single-chord table** — Most defaults are `(command_id, chord_string)` pairs in a literal list. Each string is parsed with `Chord::parse` (modifiers such as `ctrl`, `shift`, and `alt`, plus the key token). The pair is installed with `override_bind`, which is a thin wrapper around `override_steps` with one step and an empty `when` clause.

```rust
("view.command-palette", "ctrl+shift+p"),
// ...
keymap.override_bind(command, chord)?;
```

2. **Multi-step or conditional bindings** — Call `override_steps` directly with a `Vec<Chord>` and an optional `when` expression. For example `keymap.open` uses `ctrl+k` followed by `ctrl+s`, and `console.clear` uses `ctrl+l` only when `panelFocus == 'console'`.

3. **Several chords for one command** — When one command id should answer to more than one shortcut, append extra `Binding` rows with `bindings.push` instead of calling `override_bind` again (that API replaces every binding for the same command). Viewport bookmarks are the shipped example: `viewport.bookmark_store` owns `Ctrl+1` through `Ctrl+9`, and `viewport.bookmark_recall` owns `1` through `9`. The shell passes the pressed digit as the `slot` argument when dispatching.

Chord syntax accepts `ctrl`, `cmd`, or `primary` for the primary modifier, and `shift` / `alt` as shown in the defaults list. A binding must not claim a chord that already belongs to another command unless you intentionally replace that row in the defaults table or in the user file.

After you add a command handler and its `command-…` title in `en.ftl`, add a default row here if the feature should ship with a shortcut. Deeper command registration is described in [commands.md](commands.md).

## How a user rebind is saved

At startup the shell sets `EditorSession::keymap` from `Keymap::defaults`, then calls `load_user_keymap` on `keymap.seal` in the per-user editor state directory when the file exists. `Keymap::load` parses the SealText document; `Keymap::apply_user` overlays each stored binding onto the in-memory map with `override_steps`, matching the same command id and chord sequence.

The `keymap.rebind` command (registered in `EditorSession::register_commands`) takes `command` and `chord` arguments. The handler validates the command id, parses the chord, calls `Keymap::bind` (which rejects a chord already owned by a different command), and persists with `save_keymap`. Persistence uses `Keymap::save`, which writes the full current binding list through `atomic_write` so a failed save does not leave a half-written file.

The on-disk format is SealText: `schema_version` plus a `bindings` array. Each element has `command`, `chord` (space-separated tokens for multi-step chords), and optional `when`. Editing the file by hand is supported as long as the text stays valid SealText; the next launch merges it the same way as bindings recorded through `keymap.rebind`.

## One command, several chords

A `Keymap` holds a vector of `Binding` records. Each record points at one command id, one sequence of `Chord` steps (length one for a normal shortcut), and an optional `when` filter. Resolution scans the whole vector: `match_steps` returns the first binding whose steps equal the keys pressed (including a pending first step of a two-key sequence) and whose `when` clause allows the current `WhenContext`.

Therefore a single command id may own many bindings with different chord sequences. Runtime dispatch honors all of them. UI that shows a single shortcut string (`Keymap::shortcut`, menus, palette) uses the first binding for that command; additional chords still work but may not appear in the label until the UI lists every binding.

Adding another default chord for an existing command without removing the first one means pushing another `Binding` with the same `command` and a distinct `steps` vector, following the bookmark pattern in `Keymap::defaults`. Replacing or removing the primary shortcut for a command is done through `override_bind`, `keymap.rebind`, or a user-file row that targets that command id.
