# Tile Brush

The **Create** panel includes a **Tile Brush** section for 2D scenes.

## Tools

Pick the tool in the **Tool** list.

- **Brush** — click or drag to paint single cells along the stroke.
- **Rectangle** — drag a box; every cell inside is filled on mouse release.
- **Line** — drag a straight line of cells.
- **Bucket Fill** — click a region of the same picture on the current layer.
- **Picker** — click a cell to copy its picture into the brush.
- **Eraser** — remove tiles on the layer.
- **Random** — each painted cell picks from weighted pictures (`path:weight;...`) using the seed field.
- **Pattern Stamp** — paint a saved pattern from `tilepatterns/` in the project. Turn on **Save selection as pattern** and drag a box to store a new `.pattern` file (name field).

Shortcuts work while the Create panel is focused (`B` brush, `U` rectangle, `N` line, `G` fill, `I` picker, `X` eraser, `Y` random, `P` pattern). Rebind them in **Settings > Keymap**.

## Grid, layer, solid

Choose **Square**, **Isometric**, or **Hex** grid, the **Layer** index, and **Solid** so painted tiles use the ground role.

## Tile rules and metadata

Open a `.tiles` file in the **Asset Editor** to edit per-picture fields: **Custom Data** (`key=value`), **Occluder**, **Navigation blocked**, and **Prefab tile** path. Lines are stored in the `.tiles` file under each `tile <picture>` header. Animated tiles use `animate <frames...> fps <number>` in the file.

Painting applies metadata to new instances. Occluder tiles cast 2D light shadows; navigation-blocked tiles block the 2D navigation grid.

## Prefab Brush

In the **Place Tiles** section, **Prefab Brush** lists every `.prefab` file in the project. Click a name to make it the active brush (the button stays lit), then use Brush, Rectangle, Line, Bucket Fill, or Eraser in the flat viewport. Each stroke places one prefab copy per cell, in a single undo step.

## Tile Data in the Inspector

Select a painted tile in the scene. The **Tile Data** section shows custom data key/value pairs (editable; use **Add** and **Remove**), read-only **Occluder** and **Navigation** from the tile set, the source **Tile Set** path, and **Open Tile Set** to edit the `.tiles` file in the Asset Editor. The section does not appear for prefab copies or objects that are not grid-aligned tiles.