# Project templates

The **New Project** wizard (**File → New Project**, Ctrl+N) lists starter templates.

Each template has a title, a one-line description, and a small preview image in the wizard panel.

| Template | Contents |
| --- | --- |
| **2D (empty scene, flat view)** | `project.seal`, `scene.seal` with a single root object, flat Front view. |
| **3D (floor, lights, player start)** | `project.seal` and the default 3D starter scene when you first play or open the scene. |
| **Puzzle (UI-heavy)** | A 3×3 grid puzzle: `ui/*.sealui` screens, `ui/puzzle.sealtheme`, `logic/game.graph`, `input.sealinput`, generated sprites, `audio/click.wav`, and the scene music `audio/music.wav` (a four-second loop). Progress is stored in save slot **progress**. |
| **Top-Down Adventure** | Village and cave scenes, hero, gems, an enemy, an elder, a door, a shrine save point, `ui/Hud.sealui`, `ui/Pause.sealui`, `ui/Dialog.sealui`, and `logic/*.graph`. The floor tiles sit in a **Tiles** folder. |
| **Platformer** | Two levels (`scene.seal`, `scenes/level2.seal`), coins, spikes, a one-way ledge, a moving platform, parallax layers, `ui/Hud.sealui`, `ui/Pause.sealui`, `ui/Win.sealui`, and `logic/*.graph`. |
| **First-Person 3D** | An arena with walls and boxes, a dynamic player, mouse look (hold the right mouse button), WASD and jump, a goal pickup, HUD, and `logic/*.graph`. |
| **Third-Person 3D** | The same arena with a visible player capsule, an orbit camera (hold the right mouse button) that moves in front of walls, WASD movement relative to the camera, goal pickup, and HUD. |

The 3D templates open in the Perspective view. In that view, Play looks through the scene's play camera (the object tagged `camera`) in perspective, hides editor icons and the grid, and W, A, S and D walk relative to where the camera faces. A camera's rotation uses the viewport's angles: yaw 0 looks along +Z like the Front view, and a positive pitch looks up. An exported 3D game with a play camera shows the game through it the same way.

Each template also writes `tutorial/guide.sealtutorial` and opens the Tutorial panel when the project loads.

Template registration lives in `crates/seal-editor-core/src/template_registry.rs`. Puzzle content is written by `template_puzzle::create`. The play graph is generated there: the board is a list of nine numbers, and a loop paints and checks the cells.

### Puzzle

Pressing a cell switches that cell and the cells above, below, left and right of it between On and Off. The level is won when every cell is On. There are three start boards. Winning level 1 stores `unlocked = 2` in save slot `progress`; winning level 2 stores `3`. Level 3 stays at `3`. Level Select labels a level `Locked` until that value allows it.

The level screen has a move counter, **Undo Move** (one press back) and **Restart** (the level's start board, counter at 0). After level 1 or 2 the win panel's button reads **Next Level**. After level 3 it reads **Back to Menu**.

Screens: Main Menu (**Play**, **Level Select**, **Options**, **Quit**), Level Select, the level, the win panel, Options (music and sound sliders, rebind of `ui_confirm`), and Pause. **Quit** ends play. Mouse clicks, arrow keys plus Enter, and a gamepad d-pad plus the south button all use the same focus on those buttons. Sound volume scales the click. Music volume sets the music bus for the scene music (0.8 until the slider changes); both are kept in the same save slot.

### Top-Down Adventure

The hero starts with 3 hearts. Touching an enemy costs one heart, then the hero is safe for 0.5 seconds. Enemies chase the hero at 1.5 units per second; the village enemy starts in the far corner. At 0 hearts the game loads the shrine save if there is one, otherwise it restarts the village with 3 hearts and 0 gems. Gems add to the **Gems** counter. The door opens the cave scene.

### Platformer

Level 1 needs 2 coins at the flag to open level 2. A spike restarts the current level and puts back the coin count the level started with. The flag on level 2 ends the game: the win sound plays, the world pauses and the **Win** screen shows **Play Again** (level 1 with 0 coins) and **Quit** (ends play). **Esc** or the gamepad start button opens Pause: **Resume**, **Restart**, **Quit**.
