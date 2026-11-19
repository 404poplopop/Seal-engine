# Physics queries and colliders

A 2D scene can ask what its colliders touch, build a body out of several shapes, merge solid tiles, and edit a polygon in the viewport.

## Query nodes

The graph palette has six query nodes. A layer mask of 0, or any number below 0, means every layer. A positive mask hits a collider when the collider's layer bits overlap that mask.

- Raycast All. A ray from `from` along `direction`, up to `max_distance`. It lists every hit, nearest first: the objects, the points, the normals and the distances.
- Shape Cast. Sweeps a circle or a box from `from` along `direction`. For a circle, the size's x is the diameter. The outputs are the first object, the distance, the normal, the point and whether anything was hit. Sensors are skipped.
- Point Query. Every object whose collider contains `point`, lowest id first.
- Overlap Box. Every object overlapping an axis-aligned box. `size` is the full width and height.
- Overlap Circle. Every object overlapping a circle of `radius`.
- Test Move. Whether `entity` would hit another solid if it moved by `delta`. The object itself and sensors are skipped.

## Compound colliders

A child object with no body of its own can still be a collider. In the inspector, set Body to None and turn Collider on, then set the shape. During play that shape is attached to the parent body. Contacts are reported on the parent, and the child's mass is part of the parent's mass. The child picture follows the parent.

## Solid tiles

Adjacent solid tiles become a few rectangles instead of one collider per tile. A gap stays split. The rectangles cover the solid cells and do not overlap each other.

## Collider tool

Choose the Collider tool in the viewport toolbar, or press C. Select one object whose 2D physics shape is Polygon or Edge, and switch the view to 2D (Front and Back work too).

The points are handles. Drag a point; that drag is one undo step. Double-click an edge to add a point. Press Delete or Backspace to remove the point under the pointer. A polygon keeps at least three points and an edge keeps at least two.

When the shape is Polygon and the object has a sprite, the 2D Physics section has Outline from Sprite. It traces the picture's opaque pixels into the polygon. A blank or damaged picture is refused. The outline is one undo step.
