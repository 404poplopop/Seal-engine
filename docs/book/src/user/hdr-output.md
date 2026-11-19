# HDR output

HDR output draws the game window on a surface that can store colours brighter than paper white. The editor window stays as it is.

## Where

With a project open, choose **Settings** on the toolbar, then **Project Settings**. Under **Display**:

- **HDR output** — checkbox. Off by default. On asks the game window for an HDR surface.
- **Paper white** — brightness of ordinary white (interface, sprites, and scene white) in nits, from 80 to 400. The default is 203.

The values are saved in the project and written into an export as `window.txt`.

## What you see

**Play in New Window** (the menu on the Play button) and an exported game use these settings.

When HDR output is on, that game window opens on an HDR surface when the display offers one (a 16-bit float surface in extended linear when the display supports it, otherwise the next HDR format the display offers). Ordinary white is drawn at the paper-white level. Lit faces and glow brighter than white stay brighter instead of being clipped.

When HDR output is off, the display has no HDR format, or the graphics device does not start, the game uses the ordinary window and colours are unchanged. The log says which of those happened.

Window title, size, start mode, resizable, always on top, and VSync follow the other Display rows.
