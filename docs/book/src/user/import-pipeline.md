# Import pipeline

Batch import, folder drop, and watcher reimport plan work on the Worker Threads pool, then write files in source order. The same files and settings produce the same output bytes and the same output hash with any thread count. Import Activity lists that hash for each file.

## Project Settings

Edit > Import opens Project Settings. The Import section is at the bottom of Settings.

- **Kind** — Picture, sound, font, effect, or file.
- **Script** — Graph that runs after an asset of that kind imports. None means no script. The field can stay empty.
- **Create Script** — Writes `logic/import-<kind>.graph` and selects it. The graph has nodes to set the pivot, rename the file, and add a tag.
- **Pivot X, Pivot Y** — Numbers written into the script's pivot node. `0, 0` is the bottom left and `0.5, 0.5` is the centre. An unfinished number is kept as a draft and is not saved.
- **Rename** — New file name. Empty leaves the original name. A name with no extension keeps the old extension.
- **Tag** — Tag stored with the imported file. Empty adds no tag.
- **Custom importers** — Extension (without a dot) and a graph script. The script is chosen for files of that extension. An empty extension is not saved.
- **Reindex** — Walks the project and writes the asset index when the walk finishes.

The script and the importer list are saved in `import-pipeline.seal` at the project root.

## Progress

Importing many files, Export Game, and Reindex show a progress bar and a Cancel button on the status bar. Cancel stops before the next file is committed. A file that was only partly written is removed. Files already committed stay.
