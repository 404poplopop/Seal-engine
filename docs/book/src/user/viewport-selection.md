# Viewport selection and overlays

## Selection

In the viewport with the **Select** tool (Q), click to select the front-most object. Click again at the same spot to cycle through overlapping objects. Shift+click adds to the selection; Ctrl+click toggles membership. Drag on empty space to box-select. Alt+drag on empty space draws a lasso; everything inside is selected when you release.

In the Outliner, enable the **Unselectable** column from the column menu. Click **yes** on a row to toggle whether that object can be picked or included in marquee or lasso selection. Locked objects stay unpickable.

**View → Isolate Selection** (viewport hamburger menu) hides every entity except the current selection. Run the same command again to restore visibility.

## View modes

Open the shade pill next to **Lit** in the viewport toolbar. **Lighting Only** shows 2D lights on sprites without their picture colour. **Overdraw** tints objects by how many sprites overlap at their centre (blue is one layer, red is eight or more).

## Overlays

Open the viewport hamburger menu (☰). **Viewport Overlays** can collapse the top toolbar to a single expand control and move the stats or FPS overlay to a corner. Layout is stored in the project layout under `.seal-editor/` and restores on reopen.

## Asset Browser thumbnails

Scene files get a front-view preview image when you save the scene (sprites and shapes in the scene bounds). Sounds show a waveform; effect files show their gradient colour; sprite animation files use the first frame. Cached images live in `.seal-editor/thumbs/` and refresh when the source file changes.