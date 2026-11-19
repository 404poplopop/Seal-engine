# Sprite sorting, nine-slice and pivot

## Sorting

Each sprite has a **Layer** and **Order**. By default, higher layers draw on top; within a layer, higher order draws on top, then depth along the view axis.

In **Project Settings**, **Sprite sorting layers** lists `layer:mode` pairs. Modes are **default**, **y_sort**, or **axis**. **Sprite sort axis** sets X and Y weights when a layer uses **axis** (for example `1, 0.5` for isometric scenes).

On an object with a sprite, **Sort Mode** in the Sprite section overrides the layer rule: **Y Sort** orders by the pivot height; **Custom Axis** uses the project axis weights; **Sort Group** draws every child sprite as one batch at this object's place in the order.

## Nine-slice

**Nine-Slice** border pixels define corners that keep their size. **Slice Mode** is **Stretch** (default) or **Tile** for edges and the centre. **Slice Size** sets the world width and height when set; leave it empty to keep using the object's scale.

UI **Image** nodes in `.sealui` screens expose the same **Slice** and **Slice Mode** fields in the UI designer.

## Pivot

**Pivot** is a point inside the picture from 0 to 1 (0,0 is bottom left). It moves where the picture is drawn, how rotation applies, and where Y sort measures height. Preset names include **center**, **bottom_center**, and **top_left**. New sprites default to centre; picture import can supply a pivot in the import sidecar when set.

## Play

Play and an exported game draw with these same fields. Sorting layers, the sort axis and each sprite's sort mode set the draw order. The pivot is the point the picture sits on and turns around. **Tile** repeats the nine-slice edges and centre, and **Slice Size** is the world width and height. **Window > Scene Settings > Glow** (**Threshold**, **Intensity**, **Radius**) is applied over the finished frame.