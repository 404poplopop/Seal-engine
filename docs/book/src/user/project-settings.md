# Project and rendering settings

## Settings panel

Open **Settings** from the toolbar **Settings** menu (**Project Settings** or **Preferences**). Use the search field to filter rows.

### Rendering

The **Rendering** category lists the active **Quality level** and the values it applies:

- **Multisample anti-aliasing**
- **Shadow map size** (lit 3D view and exports)
- **Texture max size** (packaged games)
- **Texture anisotropy** (mesh textures in the lit 3D view, Play and packaged games, when the graphics card offers anisotropic filtering)
- **Sprite glow** (2D view and packaged games)
- **Render scale** (25 to 100 percent): the lit 3D scene is drawn at that share of the view's pixel size and stretched over the view with smooth filtering; interface, sprites and text stay at full resolution. Applies in the editor's lit Perspective view, Play and packaged games on the graphics card path.
- **Texture memory budget (MB)** (see the Textures page)

Choose **Low**, **Medium**, **High**, or **Ultra** from the toolbar **Settings** menu to change the level. The same presets appear in the **Quality level** dropdown in Settings.

### Per-platform overrides

With a project open, the **Override preview platform** dropdown at the top of Settings chooses which export target you are editing (Windows, Linux, macOS, Web, Android, iOS). Each row can enable **Override for preview platform** and set a platform-specific value. The line under the row shows which value applies on the preview platform. Export uses the target platform's effective values (host platform when not specified).

Overrides are stored in `project.seal` under `settings.platform_overrides`.

### Project default dimension

**Default scene dimension** is a project setting: **2D** or **3D**. **Create → New Scene** uses it:

- **2D**: flat **Front** view, orthographic 2D camera, zero gravity, **Pixel Art** picture import default.
- **3D**: empty root-only scene.

New projects from 2D templates set **2D**; 3D templates set **3D**.

### Editor language

**Editor language** under **Appearance** lists installed UI catalogs (English today). Changing it requires an editor restart.

## Packaged games

Export writes `render.txt` beside `window.txt` with the effective rendering values for the export platform. The game applies **Texture max size** when loading pictures and **Sprite glow** when enabled.