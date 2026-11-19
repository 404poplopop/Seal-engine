# Music

Music plays from a sound file on its own bus. The file is read from disk in small pieces while it plays, so a long file does not have to sit in memory all at once. Sound effects use other buses and are not changed by the music volume.

## Scene Settings

Open **Scene Settings** from the Window menu. **Music** is a project path to a sound file, such as `Audio/Theme.wav`. The same field accepts FLAC, OGG Vorbis and MP3. The field is saved with the scene.

When play begins, that file starts on the music bus and loops from the beginning. An empty field plays nothing.

## Graph nodes

In a graph, the music nodes are:

| Node | Fields |
| --- | --- |
| Play Music | `file` (project path), `looping`, `fade` (seconds), `loop_start` (seconds) |
| Stop Music | `fade` (seconds) |
| Set Music Volume | `volume` (1 is as recorded, 0 is silent) |

`fade` on Play Music is how long the new track takes to replace the one already playing. Zero starts the new track at once. When `looping` is on, playback jumps back to `loop_start` at the end of the file. `loop_start` is measured in seconds from the start of the file, and 0 is the beginning.

Pausing play pauses the music at the current moment. Continuing play resumes it there. Stopping play stops the music.

A packaged game uses the same nodes and the same Music field.
