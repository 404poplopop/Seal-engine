# Measure tool

The Measure tool draws temporary rulers in the **Viewport**. Lines are editor-only: they are not saved in the scene.

## Where to find it

1. Focus the **Viewport** panel.
2. In the viewport toolbar (top of the view), pick **Measure** (ruler icon), or press **M**.
3. The readout also appears in the **status bar** at the bottom while the tool is active.

## Using it

- **Click and drag** to draw a segment. Length is shown in world units, with **dX**, **dY**, and **dZ** and the angle in degrees, counted counter-clockwise from the X axis.
- **Click** again after a segment to start another point on a multi-segment path. The running total length is shown when there is more than one segment.
- Hold **Shift** while dragging to lock the heading to 0°, 45°, or 90° steps.
- With **Grid snap** or **Vertex snap** on, segment ends snap to the grid and to entity pivots and shape corners inside the snap step.
- In the flat **Front** view, when a **Camera 2D** object sets **Pixels per Unit**, the readout also shows pixel distance.
- In **Perspective**, distances are measured on the ground plane (Z = 0).

Press **Esc** or choose another viewport tool (**Select**, **Move**, **Rotate**, or **Scale**) to clear the measurement.