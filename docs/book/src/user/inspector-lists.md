# Inspector list fields

Some component properties are lists: polygon outline points for 2D physics and shape paths, and similar fields elsewhere in the inspector.

A list row shows the property name and how many items it has. Expand the row to edit items. Each item has its own editor, a drag handle to reorder, buttons to duplicate or remove, and **Add** at the bottom. Right-click the row for **Clear List**, which removes every item in one step.

When several objects are selected and their lists differ, the row shows **Multiple Values**. Edits from the list controls still apply to every selected object.

Changes save with the scene automatically and each list action is one undo step.