# Split viewport

The viewport can show the scene in more than one pane at once.

## Where to change the layout

- Viewport options menu, under Layouts.
- Menu bar: View > Viewport Layout.

The layouts are:

| Layout | Panes |
| --- | --- |
| Single | One pane |
| Two Side by Side | Two panes in a row |
| Two Stacked | Two panes in a column |
| Three | One large pane and two smaller panes |
| Four | Four panes |

## Each pane

Every pane has a header. The header sets that pane's view, shading, and show flags.

Views are Perspective, Top, Front, Side, and 2D. 2D is the flat orthographic view. A 2D scene starts every pane in that view. Axes and units are in [Coordinate system and units](coordinate-system.md).

Each pane keeps its own camera position and zoom. Shading and show flags are stored on the pane as well.

The pane under the pointer is the active pane. It is drawn with an accent border. Navigation keys and the current tool go to that pane. Selection is shared, and the transform gizmo is drawn in every pane.

## Splitters and maximize

Drag the bar between panes to resize them. The fractions are remembered.

Double-click the grip on a pane header, or press that pane's Maximize button, to fill the viewport with the pane. Do it again to restore the split, including the splitter positions. The shortcut is Alt+Shift+W.

## What is saved

The layout, the splitter positions, which pane is active, whether a pane is maximized, and each pane's view, camera, shading, and show flags are written into the project's panel state and restored when the project is opened again.

## Play

While the game is playing or paused, the game picture is shown in the active pane. The other panes keep their editor views.
