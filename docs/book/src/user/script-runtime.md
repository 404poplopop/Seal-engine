# Script update order, remote transform and colliders

## Update group

Select an object that has a script. The Inspector Script section has Update Group and Order.

Update Group chooses when that object's graph runs:

- Before Physics. The frame tick and the fixed tick run before the physics step. This is the group when the field is left as it is.
- After Physics. Both run after the physics step.
- Late. Both run after the after-physics group.

Order is a whole number. Lower values in the same group run first. Equal values keep the order the graphs were started in. The scene file stores Order only when it is not 0, and Update Group only when it is not Before Physics.

A graph can still listen for the `fixed_tick` and `late_tick` events. `fixed_tick` runs in the object's group. `late_tick` runs after every group, in Order, for every graph that listens for it.

## Remote Transform

Add Component, then Remote Transform. The Inspector Remote Transform section has a Target object, and a checkbox each for Position, Rotation and Scale.

During play, each frame copies the checked channels from this object onto the target. The target is chosen with the object picker. An empty target copies nothing. Turning the component off removes it. The scene file stores it only while it is on.

## Collider nodes

The graph palette has two nodes that change a 2D collider while the game runs. The physics body uses the new shape on the next step.

- Set Collider Shape. `shape` is `box`, `circle`, `capsule` or `polygon`. `size` is the full width and height. A circle uses the width as its diameter. `points` are the polygon corners in the object's own units. A polygon needs at least three points. Any other shape is refused and the collider stays as it was.
- Set Collider Enabled. Off, the collider no longer blocks and queries do not see it. On puts it back.

## Overlap Shape

Overlap Shape lists every object overlapping a box, circle, capsule or polygon, lowest id first. `center` is the middle, `angle` is the turn in radians, and `points` are polygon corners in local units. `mask` is layer bits. 0, or any number below 0, means every layer. A shape name it does not know, or a polygon with fewer than three points, returns an empty list.

Ray Cast All lists every object a ray meets, nearest first, with the point, the normal and the distance. It uses the same mask.
