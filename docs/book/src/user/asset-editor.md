# Opening files

Double-click a file in the Asset Browser (or select it and press Enter, or
right-click and choose Open). Every kind of file opens inside Seal:

| File | Opens in |
| ---- | -------- |
| Folder | The Asset Browser enters it |
| Scene (`.seal`) | The Viewport, after saving the current scene |
| `project.seal` of another project | That project |
| Prefab (`.prefab`) | Prefab editing in the Viewport |
| Logic graph (`.graph`) | The Graph panel |
| Sequence (`.seq`) | The Timeline |
| Pictures, sounds, effects, text | The Asset Editor |

## Asset Editor

- **Pictures** (PNG, JPG): zoom to fit, 100 %, in and out; a pixel grid when
  zoomed in; the pixel position and colour under the mouse. *Use on Selected
  Object* sets the selected object's sprite; *Place in Scene* adds a sprite object.
- **Sounds** (WAV, FLAC, OGG Vorbis, MP3): waveform, length, rate and channels,
  Play / Stop with a playhead, and a preview volume.
- **Effects** (`.fx`): a live preview and every emitter setting. Each value has
  its own reset button when it differs from the default. *Edit as text* shows the
  file itself; a file that cannot be read opens as text with the error shown.
- **Text** files (`.txt`, `.md`, `.json`, `.toml`, shaders and so on): a plain
  text editor.

Every change is written to the file as you make it, and the open file is
remembered when Seal restarts.

## Dragging files

Drag from the Asset Browser into the Viewport or onto the Outliner:

- a picture (PNG, JPG) places a sprite object;
- a prefab places a copy;
- an effect (`.fx`) places a particle object that plays it;
- a logic graph (`.graph`) becomes the script of the object it is dropped on
  (in the Viewport, the selected object).
