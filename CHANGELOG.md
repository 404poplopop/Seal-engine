# Changelog

## 0.2

Seal 0.2 brings the 2D tools up to a full set. Package version `0.2.0`.

- Sprite animation: `.frames` sprite animations with named clips, fps and looping, made from sheet cells or separate pictures, and edited in the Asset Editor. Sheets can be sliced by grid or automatically.
- Sprites gain flip, tint, opacity, sorting layers, order in layer and 9-slice borders. Export Game packs sprites onto atlas pages.
- Tilemaps on square, isometric and hex grids, with layers, rule tiles and solid tiles that act as ground.
- 2D lights (point, spot and global) with collider shadows and normal maps. Parallax backgrounds can repeat across the view.
- 2D camera with follow, smoothing, limits, zoom, shake and pixel snap.
- 2D physics: capsule, polygon and edge shapes, seven joint types, surface presets, 32 collision layers, one-way platforms and fast-object sweeps.
- 2D navigation with a walkable grid built from tiles and bodies, A* routes and walkers that steer around each other.
- Lines, filled polygons and path following.
- 2D material effects and `.shade2d` per-pixel formulas.
- In-game UI: `.sealui` screens and `.sealtheme` themes, with anchors, containers, ten control types and gamepad or keyboard focus.
- TTF and OTF font import with a preview in the Asset Editor.
- Gamepad input with rumble, action maps with dead zones, and rebinding in the game. Edit > Input Bindings edits the map.
- Save games: script nodes to save, load and delete slots, plus a project save version. Older saves still load, and Edit > Save Games shows every slot.

- Scenes load at runtime: open, add, load in the background and unload, with objects kept across scenes.
- Music streams from disk; `.soundset` sound containers pick, shuffle or layer sounds.
- Timeline sequences animate any number field, with easing per key.
- Graphs gain functions and state machines.
- Inspector fields for lists, flags, curves and gradients.
- Undo History panel, play-mode tint, split viewport with four panes, and a measure tool.
- Sprite shapes, sprite masks, Aseprite import and import presets.
- Areas: wind, water, gravity and attractor.
- Three starter templates: Platformer, Top-Down Adventure and Puzzle.

- Sprite sockets, OGG, MP3 and FLAC decoding, permanent asset ids with a reference viewer, a 2D character controller with slopes and steps, 2D physics sub-steps and interpolation, and export presets with Build and Run.
- Kill, damage, checkpoint and trigger volumes in the Create panel; tile brush tools (rectangle, line, fill, picker, eraser, random, pattern stamp) and a prefab brush.
- Particle effects: bursts, emit shapes, rotation, drag, noise, orbit, and billboard, stretched, flipbook and trail renderers. **+ Add > Particle Effect** makes a new `.fx` file.

- Typed data assets with number, text, list, flag and choice fields, and custom Inspector layouts for script variables (order, sections, hidden fields, help and slider ranges).
- Script runtime and script core: quaternions, planes, typed JSON, UTF-16 text, text layout, unscaled time and time warping in sequences.
- 2D joint friction and break force; frame events in sprite animations; camera drag margins.
- Particles: mesh and sphere emit shapes, soft particles faded against scene depth, and flipbook billboard materials.
- Pictures: SVG, EXR, KTX and DDS import; GPU texture formats; texture arrays, volume textures and cubemaps in the Asset Editor; atlas pages capped per platform.
- Project Settings: MSAA, anisotropic filtering and render scale, with per-platform overrides for settings and imports.
- Import pipeline: batch import on worker threads with progress and cancel, custom importers written as graphs, and Reindex. Asset database: naming rules, duplicate merging, cloud save and encrypted save files.
- Layered PSD import, nested prefabs and post-process volumes.
- First-Person and Third-Person templates, a tutorial per template and recipes under Help.
- Windows packaging into one archive, and store packaging.
- Raw mouse motion, pen pressure and tilt, gamepad motion sensors and light bar.
- Audio: IMA ADPCM compression, Ogg Vorbis files kept as they are, and the project sample rate and buffer applied when the device offers them.
- Physics: cylinder, triangle-mesh and heightfield colliders.
- Platform services, remote control over HTTP and WebSocket with screenshots of the editor window, and a test runner for scripted play tests.
- In the Perspective view, Play looks through the scene's play camera.
- Mesh textures: a Base Colour Texture per mesh (picture, texture array layer, volume slice or cubemap), with UV tiling and anisotropic filtering; a Sky Cubemap in Scene Settings. Textures upload to the graphics card in BC1 to BC7, ETC2 or ASTC when the card supports it.
- Texture compression at export: BC1 to BC7, ETC2 and ASTC encoders write KTX2 files that games load; the texture Asset Editor shows the size and quality of each encode.
- Game pictures decode in the background and stream their detail levels within Project Settings > Rendering > Texture memory budget; Window > Statistics shows texture memory and pending decodes.
- Render scale draws the lit 3D scene at a share of the view size; the interface stays sharp.
- 2D blend modes (additive, multiply, screen, premultiplied) composite with what is behind sprites, shapes and particles.
- HDR output for game windows on displays that offer it, with a paper-white setting.
- Gamepad light bar and motion sensors for DualShock 4, DualSense and Switch Pro controllers.
- Web Audio output for browser builds of the audio mixer, with a demo page.
- Cloud saves over HTTPS; Windows Store packages get a publisher and version and can download MakeAppx from Export Game; PSD layers with ZIP compression import; remote control clients receive selection changes.

Not tested on hardware for 0.2: the gamepad light bar and motion sensors (no controller attached to the test machines) and HDR output (no HDR display). Browser audio was checked in Microsoft Edge without a visible window.

## 0.1

Seal 0.1 is the first engine release: you can build, play and ship a game. Package version `0.1.0`.

- Play in the editor for 2D and 3D scenes: platformer character, 2D and 3D physics, sprites, layers and sprite animations, game rules, simple AI.
- Scripting graphs with breakpoints and stepping, a timeline for sequences, particle effects, and network play.
- Lit 3D viewport with shadows, light probes and bounced light (Show > Global Illumination).
- File > Export Game builds a folder with a game program that players start directly.
- A user manual under docs/book, opened from Help > Manual.
- Every file loader survives damaged input; tests feed generated garbage.

- Scene edits autosave, survive close, and undo after add, delete, duplicate, and group. Settings and panel state are written next to the layout.
- Shell tools for a selection: Shift adds to it, Group and Ungroup change the hierarchy, each outliner row has an eye and a lock. The inspector edits material, light, and camera values. Play can step one frame, start from the selection, and run at a time scale.
- The viewport fills its cell when the split moves, and the chrome uses a graphite surface with one warm accent.
- Scene world and play. Entities save as SealText. Play steps rigid bodies with Rapier and restores the scene on stop. The viewport draws colored shapes at collider size from the front, top, or side, with move, rotate, and scale tools. A drag is one undo step. Hide, lock, snap to the floor, and reset transform act on the selection.
- Denser editor: a Create panel and toolbar menu add cubes, spheres, planes, empties, cameras, and lights; Scene Settings sets gravity; Statistics shows frame time and scene counts. The outliner has search, a type column, and a row menu. The inspector edits location, shape, motion, and role. Delete and duplicate now change the scene itself. The status bar shows entity count, selection, and frame rate.

## 0.01

Seal 0.01 is the editor shell. Package version `0.0.1`.

- Editor shell: window, docking, commands, journal, projects, panels, themes, and startup options (`--safe-mode`, `--reset-layout`, `--no-restore`, `--user-data`, `--log-level`).
- Shell foundation for later tools: command list, aliases, argument schemas, when-expressions, two-step shortcuts, layered settings, named layouts, modules, document kinds, notices, modals, tasks, the agent bus stub, CRC journals, `seal-i18n`, and `seal-testkit`. Play, the viewport host, and dynamic plugins stay reserved.
