# Animation events and camera drag

## Frame events

A sprite animation frame can carry an event name. Open the file in the Asset Editor. Under the clip's Loop checkbox, each frame has a text field labelled Frame N event. An empty field fires nothing. The name is saved in the `.frames` file as soon as the field changes.

During play, when that frame starts, the object's graph runs `animation_event`. Add an Event node, set its name to `animation_event`, and give it one text parameter called `name`. The parameter is the text from the frame. Frames are reported in order, once each time the clip passes them, including when one step crosses several frames. A clip that does not loop runs `animation_finished` once when it ends. That event has no parameters. A looping clip does not run it.

## Drag margins

A 2D camera can leave a dead zone around the centre of the view. Select the camera and open the Camera section in the Inspector. Drag Left, Drag Right, Drag Top and Drag Bottom are fractions of the view, from 0 to 1. While the followed target stays inside that zone, the camera does not move. When the target leaves it, the camera moves by the amount it is past the edge. Smoothing still eases that move. 0 on every side keeps the target in the centre, which is how a camera without margins behaves.

In the flat orthographic 2D view, a selected camera with any margin above 0 draws the zone as a rectangle.
