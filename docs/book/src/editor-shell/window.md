# Window and docking

The editor binary is `seal-editor`.

```
cargo run -p seal-editor
cargo run -p seal-editor -- --self-check
```

`--self-check` saves a layout, loads it, checks text input, and checks that an accessibility tree is produced. It does not open a window.

## Performance budgets (headless)

`cargo test -p seal-editor-core performance` exercises timed checks on the headless session. Constructing an `EditorSession` must finish within **1.5 seconds** on CI (`headless_session_startup_within_one_point_five_seconds` in `crates/seal-editor-core/tests/performance_budget.rs`). The same suite times a 100,000-row outliner query plus visible window, a asset-browser query on a seeded folder, and console buffer filtering. Set `SEAL_PERF_LARGE=1` to add the one-million-line console case and the 100,000-file content query; the default test run does not allocate that many log lines or files.

## Crates

| Crate | Role |
| --- | --- |
| `seal-editor` | Binary |
| `seal-editor-shell` | Window, menus, autosave |
| `seal-editor-core` | Layout, panel registry, messages, atomic writes. No UI crate |
| `seal-editor-panels` | Built-in panel ids and placeholder bodies |
| `seal-ui` | The only crate that names the UI toolkit |
| `seal-sealtext` | Reader and writer for `.seal` files |
| `seal-log` | Editor log and the cheaper ship mode |

## Layout file

The shell writes `layout.seal` under the OS data directory for Seal. The write goes to a temporary file in the same directory, is flushed, and then replaces the previous file. The previous complete file is kept as `layout.seal.bak`. A file that does not parse is moved aside and the backup is loaded. If both are unusable, the shell starts from the default layout.

Closing the window saves immediately. Other changes save about two seconds after the last edit.

## Adding a panel later

Register a `PanelDesc` on `PanelRegistry` and draw it through `seal-ui`'s panel frame. The dock stores the panel id as a string. Do not import the UI toolkit from a panel crate; `seal-ui` is the only crate that names it.
