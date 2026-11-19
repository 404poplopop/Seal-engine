# Timeline and sequences

A sequence moves objects over time: a door opening, a camera sweep, a short cutscene. Sequences are text files ending in `.seq` inside your project. **New Sequence** in the **Timeline** panel creates an empty five-second sequence in `sequences/` and shows it. The panel lists every sequence in the project; drag the time bar to scrub, and press play to run it.

Scrubbing while you edit only previews: objects return to their saved places, and nothing is written to the scene. Scrubbing during play pauses the game and poses the objects at that moment.

## File format

```text
length 2              # seconds
loop false            # true repeats forever
object Door           # object name or id; the tracks below belong to it
track position.x linear
key 0 0               # time value
key 2 8
track visible step
key 1.5 0
event 1 opened        # fires once when play passes 1 s
```

Tracks can drive `position.x/y/z`, `rotation.x/y/z`, `scale.x/y/z`, `color.r/g/b`, `visible` and `sprite_frame`. Each track blends between keys with `linear`, `step` or `cubic`. Key times must increase.

When play passes an event, its name is written to the Console as `event <name>`.

## Property tracks

A property track drives any numeric, boolean, colour, vector or text field on one object. In the Timeline panel, select the object in the Outliner, open **Add Property Track**, and pick a field. The list shows only the fields that object has. **Key** stores that field's current value at the playhead, replacing a key that is already there. Drag a key along its row, or edit **Key time**, to move it. **Easing** sets the curve into that key: Step, Linear, Ease In, Ease Out or Ease In-Out. Boolean, enum and text keys always step. **Delete Key** removes the key at the playhead. These edits undo and redo with the usual Edit commands while the Timeline panel is focused, and each edit is written to the `.seq` file immediately.

Field paths:

| Path | Value |
| --- | --- |
| `transform.translation`, `transform.rotation`, `transform.scale` | vector; rotation turns the short way |
| `transform.translation.x` and the other channels | one number; rotation channels are angles |
| `sprite_image.tint` | colour, one channel each |
| `sprite_image.opacity`, `sprite_image.flip_x`, `sprite_image.flip_y` | number or boolean |
| `light.intensity`, `light.color` | number or colour |
| `camera2d.zoom` | number |
| `script.<name>` | a variable on the object's logic graph |
| `ui.<control>.<property>` | a control on a screen, such as `ui.Health.value` |

A path that is not on the object is skipped. The Console shows one warning for that path, and the rest of the sequence keeps playing.

Property tracks are stored as SealText (`key = value`). A sequence file in the line form above still opens unchanged. Saving it from the Timeline writes SealText, and the original tracks stay in the `tracks` list.
