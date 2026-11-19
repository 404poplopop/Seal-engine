<p align="center"><img src="assets/logo.png" width="200" alt="Seal Engine logo"></p>

# Seal Engine

**A game engine made for people first, with AI built in.**

Seal is a 2D and 3D game engine and editor, written from scratch in Rust. You can build a whole game in editor: pick a template, place objects, set their values in the Inspector, and wire up behaviour with visual graphs. When you want help, AI tools can work inside the same editor. Every change they make goes through the same undo history as yours, and you approve it before it happens.

Current release: **0.2**, the complete 2D feature set. See [CHANGELOG.md](CHANGELOG.md).

## Why Seal

- **Made for people first.** Start from a template, change anything in the Inspector, undo every step. Autosave and a crash-safe layout mean you don't lose work. The editor works with screen readers, and game UI supports keyboard and gamepad navigation.
- **AI built in, and you stay in charge.** The editor has a built-in MCP server, so AI assistants can read your project and make changes through the same commands as the menus. With **Ask before each change** on, nothing happens until you press **Allow**. Every change is one undo step, and every request is logged in the project.
- **Plain-text projects.** Scenes, prefabs, graphs, UI screens and settings are readable text files. They diff cleanly in version control, and both people and AI tools can read and edit them.
- **Fast and safe.** Rust throughout. Packaged games run their graphs compiled ahead of time to Rust. Physics runs in deterministic mode, and every file loader is fuzzed with damaged input.
- **A fair license.** Free until a game earns US$250,000; after that, 4% of revenue above that amount. Modding is encouraged.

## Getting started

Seal is built from source for now. You need [Rust](https://rustup.rs) (stable).

```
git clone https://github.com/404poplopop/Seal-engine.git
cd Seal-engine
cargo run --release -p seal-editor
```

On Linux the editor also needs the usual desktop libraries: pkg-config, libxkbcommon, Wayland, GTK 3, X11, XCB and a GL library.

Then choose **File > New Project** and pick a template:

| Template | What you get |
| --- | --- |
| **Platformer** | Two levels, coins, spikes, a one-way ledge, a moving platform, parallax, HUD, pause and win screens |
| **Top-Down Adventure** | Village and cave scenes, a hero, gems, an enemy, dialog, a door and a save point |
| **Puzzle** | A grid puzzle driven by UI screens, a theme and a graph |
| **First-Person 3D** | An arena with mouse look, WASD, jumping and a goal pickup |
| **Third-Person 3D** | The same arena with an orbit camera and camera-relative movement |
| **2D** / **3D** | A blank 2D scene, or a 3D floor with lights and a player start |

Each template comes with a step-by-step tutorial. The full manual is in the editor under **Help > Manual**, and in [`docs/book`](docs/book).

## Working with AI

Open **Window > Assistant** and switch on **Let AI tools connect**.

- **Connect a tool** shows the snippet to paste into your AI tool's MCP settings. The tool starts `seal-editor mcp`, which passes its requests to the running editor.
- The editor listens only on this computer (port 47700, or `SEAL_AGENT_PORT`).
- Assistants can read the scene and run editor commands, the same ones behind the menus. Commands that are unsafe for automation are marked and refused.
- **Ask before each change** holds every change in the panel until you press **Allow** or **Deny**.
- **What assistants did** lists every request, and the same list is saved in `.seal-editor/assistant-log.txt`.

For scripts and automated pipelines there is also `seal-agent`, a standalone MCP server over a Seal world. It can find, spawn and edit objects, work with prefabs, and save or load scenes. A `batch` call runs several changes as one all-or-nothing step. The editor can also be controlled over HTTP and WebSocket (Remote Control in the manual).

## Features

### 2D

- **Sprites:** pictures and sprite sheets, slicing by grid or automatically, animations with clips and frame events, flip, tint, opacity, sorting layers, 9-slice, masks, sprite shapes, atlases, and layered PSD and Aseprite import.
- **Tilemaps:** square, isometric and hex grids, layers, rule tiles, tile collision, and brush tools (rectangle, line, fill, random, pattern stamp, prefab brush).
- **Look:** 2D lights with collider shadows and normal maps, parallax, lines and filled shapes, 2D material effects, blend modes and custom per-pixel shaders.
- **Camera:** follow, smoothing, limits, zoom, shake, drag margins and pixel snapping.
- **Physics:** boxes, circles, capsules, polygons and edges, joints with limits, motors and break force, surface presets, 32 collision layers, one-way platforms, sweeps for fast objects, physics queries, and wind, water, gravity and attractor areas.
- **Movement:** platformer character controller with slopes and steps, path following, and navigation from tiles and colliders with A* and agent avoidance.

### 3D

Primitive shapes, cameras, point, spot and directional lights, shadows, light probes and bounced light, rigid bodies with cylinder, triangle-mesh and heightfield colliders, mesh textures, sky cubemaps, and HDR output.

### Scripting

Visual graphs with events, variables, branches, loops, functions and state machines, plus nodes for objects, physics, sound, animation, AI, input, saves and game rules. Breakpoints and node-by-node stepping during play.

### Game systems

- **In-game UI:** screens with anchors and containers, ten control types, themes, and gamepad or keyboard focus.
- **Input:** keyboard, mouse, pen and gamepad action maps with dead zones, rumble, motion sensors and rebinding in the game.
- **Saves:** save slots, save versions that keep older saves loading, cloud saves and encrypted save files.
- **Audio:** a mixer with buses, streamed music, sound containers that pick, shuffle or layer sounds, OGG, MP3 and FLAC.
- **Also included:** timelines, particles with flipbooks and trails, prefabs with variants, typed data assets, network play, and in-project test runs.

### Editor

- Docking panels, outliner, Inspector with per-value reset, lists, curves and gradients, split viewport and a measure tool.
- Asset Editor for pictures, textures, sounds, effects, fonts, input maps and saves.
- Import pipeline with presets, batch import on worker threads, and per-platform overrides.
- Undo History panel, play-mode tint, and shortcut profiles.

### Packaging

**File > Export Game** writes a standalone game for Windows or Linux, with sprites packed onto atlas pages and textures compressed for the GPU (BC1–BC7, ETC2, ASTC). Windows games can also be packed into one archive or a Microsoft Store package.

## Technology

- **Language:** Rust, with a C interface (`seal-capi`, `include/seal.h`) for native code.
- **Rendering:** wgpu, drawing through Vulkan, Direct3D 12 or Metal.
- **Physics:** Rapier for 2D and 3D, in deterministic mode.
- **Audio:** Seal's own mixer, played through cpal, with a Web Audio output for browser builds.
- **UI:** egui for the editor, Seal's own UI for games, and AccessKit for accessibility.
- **Text:** Fluent for localised messages, and skrifa for fonts.
- **Files:** SealText (`.seal` and related extensions), plain and diff-friendly.
- **Tests:** 1,540 automatic tests, including snapshots of every editor panel. CI builds and tests on Windows, Linux and macOS.

## Roadmap

Each release has an exit test: a real game or task that has to work before the release ships.

| Release | Theme | Exit test | Status |
| --- | --- | --- | --- |
| 0.1 | First release | Build, play and ship a game | Shipped |
| **0.2** | **Complete 2D** | A platformer, a top-down RPG and a UI-heavy puzzle game, built only by clicking | **Shipped** (96 of 96 features) |
| 0.3 | Code scripting and project system | The same three games written in text scripts, with hot reload | Next |
| 0.4 | 3D foundation | Import a glTF level, light it, and walk around it in first person | Planned |
| 0.5 | 3D physics, animation and navigation | A third-person character with skeletal animation, ragdoll and AI enemies | Planned |
| 0.6 | Modern rendering | PBR, shadows, global illumination, post-processing and effects at 60 fps on a mid-range GPU | Planned |
| 0.7 | Platforms | The same game shipped to Windows, Linux, macOS, Android, iOS and the web | Planned |
| 0.8 | Multiplayer and online services | A 16-player dedicated-server shooter | Planned |
| 0.9 | Open worlds and high-end rendering | A 16 km² streamed world with virtualised geometry and dynamic GI | Planned |
| 1.0 | Polish | Planned |

## Goals

- A complete, fast and deterministic engine that people enjoy using.
- AI that helps without taking over: every assistant change is visible, approved and undoable.
- A strong modding foundation. But if a user obfuscates or protects his code, its modding tools may be restricted or removed.
- Accessibility in the editor and the game UI from the start.
- Clean-room legality from day one: all engine code is original work.
- Full documentation in the manual under `docs/book`.

## Status and license

Seal source code is publicly viewable in this repository, but Seal is **not open source**. Reading the code does not grant any right beyond the [license](LICENSE.md).

A short summary of the terms (the license is the authority if anything is unclear):

- A Product owes **4%** of its Gross Revenue above **US$250,000** lifetime per Product.
- Modding is encouraged.
- Seal does not include copyleft-licensed dependencies. A copyleft dependency is a bug and will be removed.

See [LICENSE.md](LICENSE.md) for the full terms, including royalty reporting, the audit right, source-sharing rules, contributions, and trademarks.

Contact: [seal@poplocorp.com](mailto:seal@poplocorp.com).

## Contributions

Patches and other contributions may be sent to this repository. By sending a contribution, you license it to the Licensor as described in section 9 of [LICENSE.md](LICENSE.md), which also sets the rules for AI-generated contributions.

A contribution must not copy code from any other engine or from any project whose license does not allow it. Seal is developed as a clean room from the start. The Licensor may reject, revert, or rewrite any contribution that looks copied or too similar to existing code.

Copyright (c) 2026 404poplopop. All rights reserved except as granted in LICENSE.md.
