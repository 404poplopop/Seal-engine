# Asset database

Each project keeps `asset-index.seal` at the project root. The file lists every asset with a permanent id, its project path, and a content hash. The index is built when the project opens and stays in sync when files change on disk. The Asset Browser does not list it (nor `project.lock`) unless hidden files are shown.

Renaming or moving a file or folder in the Asset Browser rewrites references in scenes, prefabs, graphs, UI files, effects, sprite animations, and sound containers, and moves import settings with the file. A dialog shows how many references will change. The update is one undo step.

Right-click an asset in the Asset Browser and choose **Find References** to open the **Reference Viewer** panel. It lists files that use the asset and files the asset depends on. Click a row to open that file.

**Window → Import Activity** opens a session log of imports and reimports. Each row shows the session time in milliseconds, the project path, the importer, the result (`ok` or `failed`), and the duration in milliseconds.

Deleting a file or folder in the Asset Browser (right-click **Delete**, or the Delete key) asks for confirmation when other project files reference it. The dialog title is **Delete Asset**. The body lists each use as the file and the line, up to 20 rows, then a line `and N more`. **Delete** confirms. **Cancel** leaves the file where it is. With **Use trash** on, Delete moves the item into `.trash`. With **Use trash** off, Delete removes it. An asset nothing references is deleted immediately, with the same trash behaviour.

When a picture, sound, font, or pixel animation in the folder open in the Asset Browser changes outside the editor, the editor reimports that file and adds one row to Import Activity. A text file changed in that folder does not add a row. A picture that cannot be read adds a row whose result is `failed` and stays on disk.

When two project files share the same content hash in `asset-index.seal`, right-click one copy in the Asset Browser and choose **Merge Duplicates…**. The dialog lists every matching path and names the file you clicked as the one to keep. **Merge** rewrites references from the other copies to the keeper, deletes the extra files, and makes one undo step.

Right-click an asset and choose **Copy to Project…** to copy it and every file it depends on into another project folder. Pick the destination project in the folder dialog. Files keep the same relative paths. New rows in the destination `asset-index.seal` get new ids. Existing files with different content are listed as conflicts and are not overwritten.

**Project Settings** includes **Naming rules**. Each rule applies to one asset kind (`picture`, `sound`, `animation`, and others). Fields are **Prefix**, **Suffix**, **Letter case**, and **Pattern**. **Save rule** writes `naming-rules.seal` in the project root. **Scan project** checks every asset and lists violations in **Window → Naming Violations** and the Message Log. **Rename to match rules** on a violation row runs the same reference rewrite as a manual rename.