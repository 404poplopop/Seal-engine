# Accessibility

This page is the manual screen-reader checklist for the editor shell. Automated checks in `seal-ui` cover the AccessKit tree and headless IME commits; they do not replace listening with a screen reader on a real desktop session.

## Before you start

- Turn on the platform screen reader (NVDA on Windows, VoiceOver on macOS, Orca on Linux).
- Use a recent build with AccessKit enabled (default for the desktop editor).
- Keep the default English locale so panel titles match the `panel-*-title` message ids.

## Checklist per built-in panel

Open each panel from the layout (Welcome, Place, Outliner, Viewport, Inspector, Scene Settings, Asset Browser, Console, Statistics, Settings, Commands, Notices, Recovery). For every panel:

1. **Landmark** — Tab into the panel body. The reader announces the dock tab title (for example “Welcome”, “Asset Browser”).
2. **Focus order** — Tab through controls in roughly top-to-bottom, left-to-right order. Nothing is skipped unless it is decorative.
3. **Names** — Every button, link, toggle, and text field has a spoken name (not only an icon or color).
4. **Tooltips** — With keyboard focus on a control, wait about half a second. The tooltip text is announced or readable in the reader’s review cursor.
5. **Lists** — Console and Notices expose log lines as readable text; Outliner rows behave as selectable items.

## Docking without a mouse

1. Focus a panel (for example Outliner).
2. Run the dock commands bound to **Ctrl+Alt+arrow** keys (`panel.dock_left`, `panel.dock_right`, `panel.dock_up`, `panel.dock_down`).
3. Confirm the reader still announces the same panel title after the tab moves.

Headless tests in `seal-ui` call `DockHost::apply_side` the same way the shell does when those commands run.

## Reduced motion

1. Open **Settings** and enable **Reduced motion** (`ui.reduced_motion`).
2. Collapse a section or switch tabs. Interface motion should stop or become instant (animation time zero).

## Text input and IME

1. Open the scratch document field (or any single-line field in a panel).
2. Type Latin text and confirm it appears normally.
3. With a system IME, compose a non-Latin phrase and commit it. The committed text should appear in the field.

CI runs `ime_preedit_then_commit_keeps_arabic_order_and_cjk` on Ubuntu, Windows, and macOS. The test sends `ImeEvent::Preedit` and then `ImeEvent::Commit` for Arabic (`مرحبا`) plus CJK (`世界`). The preview appears in the field, and the committed string stays in logical order, including the right-to-left letters. A physical candidate window on each desktop is still a manual check when text input changes.

## When to re-run this checklist

- After changing `seal-ui` widgets, docking, themes, or localization for panel titles.
- After upgrading the window toolkit or AccessKit integration.
- Before a release that touches accessibility requirements ACC-001 through ACC-012.
