# Themes

The editor paints its chrome from **theme data**, not from hard-coded colors in panel code. A theme is a `.seal` map with three **base colors** and, after load, a full table of **thirty-six named roles**. Built-in files ship inside `seal-editor-core`; you can override any built-in id by placing a file in the per-user state directory.

## Base colors

Every theme defines exactly three RGB triplets (each channel `0..=255`):

| Field | Role |
| ----- | ---- |
| `background` | Main surface behind panels and the viewport frame |
| `text` | Primary readable ink on that surface |
| `accent` | Links, focus, and other emphasis |

The file must also set `id` to a short name (`dark`, `light`, or `contrast` for the stock themes). Example from the default dark theme:

```sealtext
id = "dark"
background = [30, 31, 34]
text = [214, 211, 204]
accent = [201, 122, 68]
```

Only these three colors are required in the file. The loader fills in every other role unless you override a role explicitly (see below).

## Thirty-six derived roles

After parse, `ThemeFile` holds one color per name in `THEME_ROLE_NAMES`. Together with the three bases (which are also role names), the editor resolves **thirty-six** strings. Panels and the shell ask for a role by name; `theme_role_color` returns the map entry or falls back to `background` if the name is unknown.

The stock derivation (`derive_roles` in `crates/seal-editor-core/src/theme.rs`) treats the theme as **dark** or **light** by comparing relative luminance of `background` and `text`. Surfaces step slightly lighter or darker than the base; borders and dividers mix background toward text; muted and disabled text mix text toward background; buttons and tabs blend background and text; accent variants lighten or darken the accent for hover and pressed states. Semantic colors (`success`, `warning`, `error`, `info`) use fixed RGB values in normal themes so status does not drift with skin tone.

| Group | Role names |
| ----- | ---------- |
| Bases | `background`, `text`, `accent` |
| Accent interaction | `accent_hover`, `accent_pressed`, `focus_ring` |
| Surfaces | `surface`, `surface_raised`, `panel`, `panel_header`, `title_bar`, `status_bar` |
| Structure | `border`, `border_strong`, `divider` |
| Text variants | `text_muted`, `text_disabled`, `text_link` |
| Selection | `selection`, `selection_text` |
| Controls | `button`, `button_hover`, `button_text`, `input_background`, `input_border`, `tab`, `tab_active`, `tab_hover`, `scrollbar`, `scrollbar_hover`, `menu`, `menu_hover` |
| Feedback | `success`, `warning`, `error`, `info` |

You may set any of these keys in the `.seal` file to pin a color. Omitted keys still derive from the three bases (or from the contrast rules when `id` is `contrast`).

## Contrast theme

`themes/contrast.seal` uses a black background, white text, and a yellow accent. When `id == "contrast"`, the loader takes the **high contrast** path: many chrome roles collapse to pure background, text, or accent; borders and dividers use text; status hues move to saturated primaries. The UI layer also treats a large gap between background and text brightness as **stark** and draws stronger edges and selection fills so controls stay separable without subtle gray steps.

`ui.theme` accepts `dark`, `light`, or `contrast`. With `ui.follow_os_theme`, the stored choice can be replaced by the OS light or dark hint before load.

## Built-in themes

Three files are embedded at build time:

| `id` | File |
| ---- | ---- |
| `dark` | `crates/seal-editor-core/themes/dark.seal` (default) |
| `light` | `crates/seal-editor-core/themes/light.seal` |
| `contrast` | `crates/seal-editor-core/themes/contrast.seal` |

`load_theme` reads a user file first, then falls back to these strings. `theme_rgb` still exposes only the three base triplets for callers that have not moved to the role table yet.

## User overrides

Per-user editor state lives under the OS data directory (`%APPDATA%/Seal/editor` on Windows, `~/Library/Application Support/Seal/editor` on macOS, `$XDG_DATA_HOME/seal/editor` or `~/.local/share/seal/editor` elsewhere). See `user_state_dir` in `crates/seal-editor-core/src/workspace.rs`.

To override a built-in id, create:

```text
<user_state_dir>/themes/{id}.seal
```

Use the same `id` as the built-in you are replacing (`dark`, `light`, or `contrast`). If the file is present and parses, `load_user_theme` wins over the embedded copy. A parse error does not silently fall back: loading fails for that path and the loader uses the built-in text instead.

A custom theme only needs `id`, the three bases, and any role overrides you care about; the same derivation runs for ids other than `contrast` unless you supply every role yourself.

## Switching themes

The Settings panel and the `view.theme_set` command write `ui.theme` into user settings. The change applies on the next style refresh for the session. Settings persist in `settings.seal` under the same user state directory.
