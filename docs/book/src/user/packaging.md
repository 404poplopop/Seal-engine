# Packaging a game

Before you export, see [System requirements](system-requirements.md) for supported operating systems and hardware.

**Export** in the Export panel (**File > Export Game** or the **Build** menu) turns the open
project into a folder you can hand to players. The command **Package Project** does the same
with the default settings. A project with no saved preset exports with the one the panel shows. Nothing else is needed to run it.

The folder is created next to the project and named after it, for example
`SealHop Game`. It holds:

- `<Name>.exe`: the program players start.
- `data/`: a copy of the project (scenes, sprites, sequences, effects). Editor-only state
  in `.seal-editor/` and the `project.lock` file are left out.
- `game.txt`: tells the program which project to play.
- `manifest.txt`: one line per file with its hash and size, so a damaged copy can be spotted.
- `README.txt`: how to play.

## Playing a packaged game

Start `<Name>.exe`. It opens straight into the game with only the game view: no menus,
panels, grid or editor icons. Esc or closing the window quits.

From a terminal you can also play any project this way:

```
seal-editor --game path/to/project
```

## Packaging again

Packaging again replaces the earlier package in the same folder. Seal only replaces a folder
that holds a `game.txt` from an earlier package, so it never overwrites an unrelated folder,
and it refuses a target folder inside the project itself.

