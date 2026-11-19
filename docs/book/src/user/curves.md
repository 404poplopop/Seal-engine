# Curves and gradients

A curve is a list of keys. Each key has a time, a value, an in tangent, an out tangent, and an easing. The easing is tangent, constant, linear, ease in, ease out, or ease in-out. Time on a particle effect runs from 0 at birth to 1 at the end of the particle's life. Keys may sit outside that range when the curve is not tied to a life.

A gradient is a list of colour stops and a separate list of alpha stops, both over 0 to 1. Colour and alpha blend on their own.

Both are stored as text values in the effect file. An older effect without those values still fades size and colour in a straight line from the start value to the end value.

## Where they are

Open an effect file (a name ending in `.fx`) from the Asset Browser. The Asset Editor shows Size over life, Alpha over life, and Colour over life under the other effect settings.

The same rows are in the Inspector. Select an object, open the Particles section, and choose an effect. The rows edit that effect file.

The row draws a small graph or a colour strip. Click it to open the editor.

## Curve editor

Double-click the graph to add a key. Drag a key to move it. Delete key removes the selected key. Constant, Linear, Ease in, Ease out, and Ease in-out replace the keys, keeping the end values (Constant keeps the first value). Time, Value, In tangent, Out tangent, and Easing edit the selected key. Tangents apply when the easing is tangent.

Use straight fade removes the stored curve. Size or alpha then follows the start and end numbers again.

A drag, including a drag on a number, is one undo step. Each change is written to the effect file.

## Gradient editor

Double-click the upper half of the strip to add a colour stop. Double-click the lower half to add an alpha stop. Drag a stop to move it. Delete stop removes the selected stop. A colour stop uses the colour fields (red, green, and blue). An alpha stop has a number from 0 to 1.

Use straight fade removes the stored gradient. Colour then follows the start colour and the end colour again.

## Damaged text

If the stored text cannot be read, the straight fade is used and the row shows an error. The rest of the effect still runs.
