# Filled shapes and sprite sheets

A filled shape paints one picture across its points. A line can vary its width and colour, and can end in a cap. Two filled shapes can be joined or cut. A sprite sheet can be cut into cells, and each cell can carry a pivot, a border and an outline.

## Filled shapes and lines

Select an object and open **2D Shape** in the Inspector.

| Field | What it does |
| --- | --- |
| Picture | Picture painted across a filled shape. Empty keeps the flat colour. |
| Picture Offset | Shifts the picture, in widths and heights of the picture. |
| Picture Scale | Scales the picture around the middle of the shape. |
| Picture Rotation | Turns the picture around the middle, in degrees. |
| Begin Cap, End Cap | How a line ends: None, Square or Round. |
| Width Curve | Multiplies the line width from the start of the line to the end. Empty keeps the width. |
| Colour Gradient | Colour along the line. Empty keeps the colour. |
| Offset | Distance, in scene units, used by Apply Offset. A positive value grows a filled shape. |

Select two filled shapes, then **Edit > Shape > Merge**, **Subtract** or **Intersect**. The result is written onto the first shape. The same commands are on the viewport right-click menu, under **Shape**. **Offset** uses the Inspector distance, or 0.1 when that field has not been changed.

The same four operations are graph nodes: `offset_polygon`, `merge_polygon`, `clip_polygon` and `intersect_polygon`. Add one from the graph search. Each takes a list of points and returns a list of points. `clip_polygon` is the first outline with the second cut out of it.

## Sprite sheets

Open a sprite animation in the Asset Editor.

| Control | What it does |
| --- | --- |
| Columns, Rows, Slice by Cell Count | Cuts the picture into that many equal cells. |
| Cut Automatically | Cuts on fully clear rows and columns. |
| Minimum Size, Padding, Slice by Islands | Cuts one frame around each separate opaque picture. Pictures smaller than the minimum are dropped. Padding grows each picture by that many pixels. |
| Pivot X, Pivot Y | Point of the cell that sits on the object. 0, 0 is the bottom left. Pause the preview and drag the point to move it. |
| Border Left, Right, Top, Bottom | Nine-slice insets of this frame, in pixels. All zero keeps the object's border. |
| Generate Outline | Writes a tight outline of the opaque pixels. |
| Generate Physics Outline | Writes a convex outline for collision. |
| Collider from Sprite | Sets the selected object's polygon collider from the physics outline of its current frame. |

Pivot, border and outlines are stored on the frame and kept when the file is saved.
