# Merge, object files and post volumes

## Merge Objects

Select two or more sprites, shapes, or meshes. Entity > Merge Objects, or the same command in the viewport or outliner right-click menu, replaces them with one object. Sprites become one group drawn together, one batch per picture. Shapes and meshes become one mesh. Undo restores the separate objects.

## One object per file

Scene Settings has **One object per file**. When it is on, saving the scene writes an index at the scene path and one file per top-level object in a folder with the scene's name. A scene stored as `scenes/level.seal` keeps its objects in `scenes/level/`. Opening that scene loads the same objects as a single scene file. Turning the option off writes one scene file again and removes the folder.

## Nested prefabs

A prefab may contain instances of other prefabs. Editing the inner prefab and choosing **Update Prefab Copies**, or finishing the prefab edit, updates those nested instances inside every outer copy. Fields listed as that nested copy's own changes stay, and the outer copy's prefab menu shows them as nested prefab changes.

## Post process volume

Create > Areas > Post Process adds a volume. In the Inspector, Kind is Post Process. The fields are glow threshold, glow intensity, tint, exposure, saturation, blend distance, and priority. While the camera is inside the box, those settings blend in. Outside, they do not. The blend distance is the fade inside the edge of the box. Where two volumes overlap, the higher priority is applied last.
