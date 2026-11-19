# System requirements

Seal is built and tested on current 64-bit Linux, Windows, and macOS.

## Operating system

- **Linux** — A 64-bit desktop. The editor window needs the usual display libraries: pkg-config, libxkbcommon, Wayland, GTK 3, X11, XCB, and a GL library.
- **Windows** — 64-bit Windows, the same generation the continuous integration runners use.
- **macOS** — 64-bit macOS on Apple Silicon or Intel.

## Graphics

The game window asks the GPU through Vulkan on Linux, Direct3D 12 on Windows, or Metal on macOS. A driver from the last several years is enough for 2D games.

**HDR output** is a Display setting under **Edit > Project Settings**. When it is on, the game window requests a half-float surface format, then a 10-bit format, if the adapter lists one. If neither is offered, the window stays on the ordinary format. **Paper white** is the mid-grey brightness in nits, from 80 to 400. The default is 203. The packaged game reads both from `window.txt`.

## Memory and disk

The editor does not refuse to start below a fixed RAM or disk size. Undo history defaults to 200 steps and 64 MB (the setting `undo.max_memory_mb`).

A packaged game is the editor program plus the project. **Single archive** stores that project in one `data.pack` file; otherwise it is a `data/` folder. Disk space is the size of that copy. Export writes the package next to the project, so the disk needs room for a second copy of the project files.

See [Packaging a game](packaging.md) for the export layout. See [Coordinate system and units](coordinate-system.md) for scene units.
