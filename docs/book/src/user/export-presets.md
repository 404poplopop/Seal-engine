# Export presets and the game window

**File > Export Game** opens the Export panel. The same panel opens from the toolbar **Platforms** menu (**Package**).

A preset is saved with the project. It has:

- **Name**
- **Target folder** — where the packaged game is written. Empty uses a folder next to the project.
- **Build profile** — Release or Debug. The choice is written into `game.txt`.
- **Target platform** — Windows, Linux, macOS, Web, Android or iOS. Export applies that platform’s picture import overrides (max size, compression, format) and the matching atlas page size from project import settings.
- **Include** — patterns, separated by semicolons. Empty includes every project file. A pattern with no slash matches a file name (`*.seal`). `**` crosses folders.
- **Exclude** — patterns that drop matching files.
- **Extra files** — `from => to`, separated by semicolons. `from` is inside the project. `to` is inside the package.
- **Clean before export** — removes files in the target folder that this export does not write.
- **Before export** and **After export** — `copy from => to`, or `zip`. `zip` writes a zip of the export folder beside it. Those are the only steps.

**Export** runs the selected preset. **Build and Run** runs it and starts the packaged program. **Platforms > Cook** exports. **Platforms > Quick Launch** builds and runs.

**Edit > Project Settings**, category **Display**:

- **Window title** — empty uses the project name.
- **Window icon** — a picture in the project.
- **Start mode** — Windowed, Fullscreen, or Borderless.
- **Window width** and **Window height**
- **Resizable**
- **Always on top**
- **VSync**
- **Target frame rate** — frames per second. 0 is unlimited.

The packaged game reads these when its window opens. **Play in New Window** (the play options on the toolbar) uses the same fields for the editor window while that play runs, and restores the editor window when play stops. **HDR output** requests an HDR swap format when the display supports it, with **Paper white** in nits (80–400). A graph node **Set Window Mode** (`set_window_mode`) takes `mode`: `windowed`, `fullscreen`, or `borderless`.

**Single archive** on a preset writes `data.pack` instead of a `data/` folder; `game.txt` points the runtime at that pack. **Windows Store package (MSIX)** writes `AppxManifest.xml` and an `msix_layout` folder; when the **MakeAppx** tool is available it also builds a `.msix` file, otherwise `msix-build.txt` explains what is missing. With the MSIX target chosen, the preset shows **Publisher** (the `CN=...` name of the store account) and **Package version** (four numbers, like `1.0.0.0`), and the MakeAppx path. When MakeAppx is missing, **Download MakeAppx** fetches the Windows SDK Build Tools package (about 22 MB, no installer or administrator rights) into the user's local data folder under `Seal/tools`. The `.msix` is unsigned; the store signs it on submission.

Graph nodes **Open Game Window**, **Move Game Window**, **Resize Game Window**, and **Close Game Window** control extra play windows. **Request Attention** asks the OS to flash the taskbar icon once.
