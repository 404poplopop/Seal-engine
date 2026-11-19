# Raw input, pen and gamepad sensors

## Raw pointer motion

Cursor motion is the pointer's movement in the window. Raw motion is the pointing device's own movement, without pointer acceleration.

Both are axes on the project's input bindings:

| Axis | Meaning |
| --- | --- |
| `mouse.dx`, `mouse.dy` | Cursor motion. 64 pixels is a full deflection. |
| `mouse.raw_dx`, `mouse.raw_dy` | Raw motion, on the same scale. |

Graph nodes **Raw Mouse Delta** and **Mouse Look** read the pixel values. **Mouse Delta** stays the cursor.

**Raw mouse look** is a project setting. Open it from **Edit > Input Bindings**, or from **Tools > Settings** under **Input**. On uses raw motion for **Mouse Look** when the window reported it. Off uses the cursor. The default is off.

The same page lists devices that reported input this frame, including the system pointer, a pen, and connected gamepads.

## Pen

A pen or touch event that includes pressure fills these axes while it is down:

| Axis or button | Meaning |
| --- | --- |
| `pen.pressure` | 0 to 1. |
| `pen.tilt_x`, `pen.tilt_y` | Lean, -1 to 1. 0 is upright. |
| `pen.tip`, `pen.eraser` | Which end is down. |

Graph nodes: **Pen Pressure**, **Pen Tilt**, **Pen Eraser**.

When the event has pressure but no tilt, tilt stays 0 and the bindings page says tilt was not reported. The eraser button is set only when the event names the eraser end.

## Gamepad light bar

**Light bar** on the bindings page, and **Gamepad light** under **Settings > Input**, stores a colour on the project as red, green and blue from 0 to 255. Empty leaves the light unchanged. The colour is sent to the connected gamepad.

The graph node **Set Pad Light** takes red, green and blue from 0 to 1. A gamepad or system that cannot set the light bar reports that in the log and on the bindings page, and the colour is left unchanged.

The colour is sent to player 1 when that pad is a DualShock 4 (USB ids 054C:05C4 and 054C:09CC) or a DualSense (054C:0CE6, Edge 054C:0DF2), over USB or Bluetooth. A Pro Controller (057E:2009) has no light bar. Any other pad leaves the colour unchanged and the page says the light bar is not supported by this controller. No connected pad says that no gamepad is connected.

## Motion sensors

Gyro axes are `pad.gyro_x`, `pad.gyro_y`, `pad.gyro_z` (radians per second; 4 is a full deflection). Accelerometer axes are `pad.accel_x`, `pad.accel_y`, `pad.accel_z` (gravities; 2 is a full deflection).

Graph nodes **Pad Gyro** and **Pad Accelerometer** read the unscaled values. **Pad Motion Note** is empty when they were read, and a sentence when this gamepad cannot read them. The bindings page shows the same sentence.

Player 1 reports those values when it is a DualShock 4, a DualSense, or a Pro Controller. The pad's calibration report is applied. Gyro is radians per second and acceleration is gravities. Other pads show that motion sensors are not supported by this controller. Unplugging the pad clears the reading. The same pad is the one the light bar uses: vendor, product, and order match the first connected gamepad.

## Binding an axis

On **Edit > Input Bindings**, choose a device axis, type a name, and choose **Add Axis**. The new axis appears with the others and is saved in the bindings file.
