# Inspector pickers

The Inspector uses a picker for asset paths, object references, colours, and map or struct values. The command palette searches more than commands.

## Asset fields

A picture, script, particles, shader, font, theme, sound, or music path shows a thumbnail or type chip, the file name, and a search list of project files of that kind. Scene music, a screen's theme file, and a font on a screen or theme use the same row. Drop a file from the Asset Browser onto the row. **Show in Asset Browser** selects that file. **Clear** removes the reference. The stored value is the project path.

## Object fields

Camera follow, a physics joint target, a navigation target, and a follow path list scene objects by name with a type icon. **Pick in view** then a click in the viewport or the Outliner writes that object's id. **Clear** removes it. A navigation target can still be typed as two numbers, `x, y`. A value that is not an object id stays visible as text.

## Colour

Tint, shape colour, material colour, and the object colour row open a popup: a saturation square, a hue bar, alpha, a hex box, and recent colours. Recent colours are kept with the project and come back when the project is opened again. One finished edit is one undo step. The object colour row stores red, green, and blue together.

## Maps and structs

A text value that is a map or a struct (a record with a schema version and a kind) is edited as keys and nested values instead of a single line. Kinds are text, number, bool, colour, map, and struct. A damaged entry is kept and shown. On a graph script, string variables that hold a record are listed under **Variables**. **Add map variable** creates an empty map on that graph. The graph type list is unchanged; the record is stored as the variable's text.

## Command palette

Open the command palette and type. The list includes commands, assets, scene objects, settings, manual pages, and console lines. `type:` keeps one kind (`command`, `asset`, `object`, `setting`, `doc`, `console`). `tag:` keeps a tag such as a folder, a role, or a console level. Several types match any of them. Several tags must all match. **Pin search** stores the current query with the project. A pinned query fills the box again. An empty query lists commands only, and the status-bar suggestions stay commands.
