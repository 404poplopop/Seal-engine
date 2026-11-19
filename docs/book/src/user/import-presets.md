# Import presets

Import presets are named sets of import settings for pictures and sounds.
They live in `import-presets.seal` at the project root so the whole team shares them.
Each file keeps its own settings in a sidecar next to the file (`name.png.import.seal`).

## Platform overrides

Open a picture in the Asset Editor. Under **Import settings**, the **Platform overrides** section has a **Platform** dropdown (Windows, Linux, macOS, Web, Android, iOS). For the selected platform you can override **Max Size**, **Compression** and **Format**. **Default** on a field inherits the values from the main import settings above.

The muted line shows which values apply when you export with a preset whose **Target platform** matches.

In **Settings**, **Import presets**, **Atlas page size by platform** sets the atlas packing page size per platform for export. **Default** keeps 2048 pixels on the longest side.

## Picture fields

- **Filter** — Nearest or Linear. Nearest keeps hard pixel edges. Linear blends neighbouring pixels when the picture is drawn larger or smaller than its pixels. The viewport, the Asset Editor preview, play and an exported game all use this.
- **Pixels per Unit** — `0` keeps the object's scale as the sprite's world size (a scale of 1 is 1 by 1 world units, whatever the pixel size). A number above zero sets the world size of a sprite that still has the default scale of 1 to picture pixels divided by that number. A sprite whose scale was edited keeps that scale.
- **Max Size** — Original keeps the file's pixel size. A power of two from 32 to 4096 shrinks the picture on import so the longer side is at most that many pixels. The Asset Editor size readout, the viewport and an exported game use the smaller picture. `0` in the sidecar means Original.
- **Mipmaps** — Off is today's path. On samples a half-size picture when the sprite is drawn at half size or smaller.
- **Colour Space** — sRGB or Linear. sRGB draws the stored bytes as they are. Linear treats those bytes as linear light and brightens them for the display, so a mid grey looks lighter. An HDR picture (`.hdr`) is stored as linear light. With no colour space in the sidecar, that file draws as Linear and every other picture draws as sRGB. Choosing a colour space stores it and that choice wins.
- **Wrap** — Clamp, Repeat or Mirror. Clamp continues the edge pixel. Repeat tiles the picture. Mirror tiles it and flips each copy.
- **Sprite Mode** — Single draws the whole picture. Multiple cuts a horizontal sheet into square cells, left to right, the same layout as sprite frames, and draws one cell. It applies when the object still has a single frame. An object that already sets frames, or a sprite animation file, keeps those frames.
- **Pivot** — Centre, a corner, an edge, or Custom with Pivot X and Pivot Y. `0, 0` is the bottom left and `0.5, 0.5` is the centre. The viewport draws the sprite around that point. Centre is how pictures were drawn before this setting existed.

The formats the editor, the Asset Editor and an exported game decode are listed in [Picture import](picture-import.md). Changing any field saves the sidecar and reimports the picture.

A sidecar written before these fields existed still loads. Missing keys keep today's behaviour: original size, no mipmaps, sRGB (except an HDR picture with no colour space stored), clamp, a single sprite and a centre pivot.

## Sound fields

- **Normalize loudness** — when on, the sound is scaled on load so its loudest sample is full volume. Off leaves the file as stored. Playback in the Asset Editor and during play uses the same samples.

Fonts have no import fields. The text drawer uses the font file as it is.

## Asset Editor

Open a picture or sound in the Asset Editor. The **Import settings** section shows
the active preset (or **Custom** when the file does not match any preset), a **Preset**
dropdown, the fields for that kind, **Save as Preset…** and **Manage Presets…**.

Editing a field saves at once, is one undo step, and shows in the open viewport and the Asset Editor preview without restarting. Editing a field marks the file as customized. Customized files keep their settings when folder rules or default presets change.

Built-in presets: **Pixel Art** (nearest filter), **Smooth** (linear filter), **Music** (loudness left as stored) and **Effect** (loudness normalised). Built-in presets cannot be renamed or deleted.

## Asset Browser

Right-click a picture or sound and choose **Apply Import Preset** and a preset name.
The change is one undo step. Several files can be selected when the command is run with
a multi-line `paths` argument.

## Project settings

Open **Settings** and scroll to **Import presets**. Set the default preset per kind and
add folder rules (folder path, kind, preset). On first import, the deepest matching folder
rule wins; otherwise the default preset for that kind applies.