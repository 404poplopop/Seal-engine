# Coordinate system and units

Seal uses a **left-handed** scene. One scene unit is **one metre**. Physics 2D treats one world unit as one metre, and scene gravity is in metres per second squared. The default gravity is 9.81.

## Axes

- **X** points to the right.
- **Y** points up. Gravity pulls downward, along negative Y.
- **Z** points away from the viewer in the default front view (into the screen). A larger view depth is farther from the viewer.

The flat **2D** view uses yaw 0 and pitch 0, so X runs across the screen and Y runs up. That view looks along positive Z. Character 2D and Physics 2D move on this X–Y plane.

## Rotation

Object rotation is stored in **degrees**, around X, then Y, then Z. The Inspector shows those three fields in degrees. Physics 2D converts the Z angle from degrees when it steps.

## Viewport

The corner axis widget and the move gizmo use the same projection as the scene. **X** is red, **Y** is green, and **Z** is blue. Grid spacing in the Measure tool is in these scene units.
