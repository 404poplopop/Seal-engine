# Layered pictures

Files ending in `.psd` are layered pictures. The Asset Browser shows the flattened picture as the thumbnail. Double-click the file (or right-click it and choose Open) to open it in the Asset Editor.

The Asset Editor shows the flattened picture and the layer list. Hidden layers are marked hidden. A group is marked group, and its layers are indented under it.

**Layers as sprites** is a checkbox. It is saved beside the file, so it is still set after the Asset Editor is closed and opened again.

**Import** writes a PNG of the visible layers in the same folder. The same action is on the file's right-click menu as **Import Layered Picture**, and it is the command Import Layered Picture.

When Layers as sprites is checked, Import also writes:

- one PNG per visible layer, trimmed to pixels that are not fully transparent, under `{name}_layers/`. A group becomes a folder in that tree
- a prefab `{name}.prefab`. Each sprite is placed at the centre of its trimmed pixels. Distances are in pixels from the bottom left of the picture, with Y up. The sprite's scale is the trimmed width and height. The prefab root sits at the centre of the picture. A higher layer has a higher order. A group is a parent object with no picture of its own

A hidden layer is left out. Layer opacity is stored in the picture. A blend other than normal is drawn as normal, and the Console says so once for that file. A clipped layer is drawn in full, and the Console says so once.

Only 8-bit RGB pictures are imported. Channel data may be raw, run-length, ZIP or ZIP with prediction. Any other colour mode or depth is refused with an error, and a damaged file is refused without crashing.

Import will not replace a PNG or a prefab that it did not generate from this file. When the layered picture is newer than the files it generated, those pictures are written again in place. Object ids and picture paths stay the same, so references to them keep working. This runs while the file is open in the Asset Editor, and while its folder is the one shown in the Asset Browser. Each import or rebuild adds a row to Import Activity.
