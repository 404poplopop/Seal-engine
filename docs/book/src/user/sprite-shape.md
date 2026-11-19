# Sprite shapes

A sprite shape is spline terrain in a flat scene. In the Create panel, choose Sprite Shape. The new object is a closed bump, and it already generates a collider.

Select the object and look at it in the flat front view. Drag a point to move it. A smooth point also has a tangent handle; drag the handle to bend the curve. Double-click the outline to insert a point. Select a point and press Delete to remove it. Right-click a point and choose Smooth or Corner.

The Inspector shows a Sprite Shape section:

- Points, as `x y corner` or `x y smooth ox oy`, separated by semicolons. `ox oy` is the outgoing tangent of a smooth point. The incoming tangent is the opposite, so the curve stays continuous.
- Closed.
- Fill Picture and Fill Tiling. The picture is clipped to the closed shape and repeated. Fill Tiling is the width and height of one repeat.
- Top Edge, Side Edge and Bottom Edge, plus Edge Thickness and Corner (Miter, Bevel or Round). Each picture runs along the outline. A segment uses the top, side or bottom picture when its direction, in degrees, falls in that angle range. A range wraps when the first number is past the second. Top is checked first, then bottom, then side.
- Layer, Order and Tint.
- Generate Collider and Tolerance. The collider follows the spline. Tolerance is how far the collider may leave the curve, and how many segments that produces. Editing the spline updates the collider. A platformer character can stand on it.

Each drag is one undo step. The scene autosaves after the edit.
