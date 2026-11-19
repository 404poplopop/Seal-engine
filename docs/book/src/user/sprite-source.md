# Pixel animation files

Files ending in `.ase` or `.aseprite` are pixel animation files. The Asset Browser shows the first frame as the thumbnail. Double-click the file (or right-click it and choose Open) to open it in the Asset Editor. The editor labels it **Aseprite file**.

The preview is read only. It shows the flattened frames, the tags, and the slices. Pick a frame in the strip, or use Previous Frame and Next Frame when there are many. The chosen frame is remembered with the open file.

**Import Sprite Animation** (also on the file's right-click menu, and as the command Import Sprite Animation) writes two files in the same folder:

- a PNG, one cell per frame, packed left to right and then top to bottom
- a sprite animation (`.frames`) whose clips are the tags

A tag named `run` from frame 1 through 4 becomes a clip `run`. Forward keeps that order. Reverse reverses it. Ping-pong plays forward and then back, without repeating the end frames. A tag that loops becomes a looping clip. A tag that plays once, or a fixed number of times, becomes a clip that plays once; extra repeats are dropped. A file with no tags gets one looping clip, `all`, of every frame.

Each clip has one speed. Durations are in milliseconds. A duration of 0 counts as 100 ms, or as the file's default speed when that is set. When the durations in a clip share a step and repeating frames to match them stays within 256 frames, the speed is 1000 divided by that step, kept to three decimal places, and a longer frame is repeated. Otherwise each frame is kept once and the speed is 1000 divided by the average duration, kept to three decimal places. The Console names any clip that used the average.

Layers are drawn from the bottom up. A hidden layer is left out. Layer and cel opacity are both applied. A blend other than normal is drawn as normal, and the Console says so once for that file. A tile layer is skipped, and the Console says so once.

Slices are listed in the preview (name, position, size, frame). A sprite animation has no named cells, so slices are not written into the `.frames` file.

If `{name}.frames` or `{name}.png` already exists and was not generated from this file, Import refuses to replace it. Generated files start with a comment that names the source. Editing the sprite animation keeps that comment. When the source file is newer, the picture and the sprite animation are rebuilt while that file is open in the Asset Editor, or while its folder is the one shown in the Asset Browser.
