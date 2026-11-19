# Inspector field controls

The **Inspector** panel edits component properties on the selected object or objects.

## Fixed choices

Fields with a known set of values use a **dropdown**: mesh shape, movement, movement mode, 2D light type, physics body, shape, surface and joint, 2D shape, follow mode, material effect, and role. Role also lists camera, light, prop and folder, and keeps any other tag. Unknown text for the other lists is reported in the **Console** and the field keeps its default.

## Collision layers and masks

On a **2D Physics** body, **Layer** and **Mask** use a multi-check list with **All** and **None**. Names are edited in **Project Settings** (toolbar **Settings**, **Project Settings**, **Physics**): 32 text boxes, saved with the project. An empty box is shown as Layer N.

## Numbers with bounds

Bounded values (field of view, opacity, roughness, camera zoom, light range, and others) use a number box with optional range bar and unit suffix. Drag the value area to scrub; one drag gesture is one undo step. Values are clamped to the allowed range.

## Reset and mixed selection

Right-click a property row and choose **Reset** to restore that field’s default. Fields that differ from the default show a brighter label and an inline reset control.

With several objects selected, fields that disagree show **Mixed** until you pick a single shared value.