# Sprite atlases

**File > Export Game** opens the Export panel. Each preset packs the project's small pictures onto shared pages when the game is exported. The pictures themselves stay in the package as well.

The preset stores:

- **Tight packing** — pack each picture by its opaque bounds. The offset is stored, so the picture still draws in the same place.
- **Allow rotation** — turn a picture by a right angle when that is what fits on the page. Drawing turns it back.
- **Max page size** — 1024, 2048, 4096, or 8192 pixels. One preset can use 2048 and another 4096. Pictures that do not fit spill onto further pages.
- **Atlas scale** — a percent of the picture's size written into the page. 100 is full size. 50 writes half the texels. The game draws the picture at the same world size.

Pictures larger than 512 pixels on a side keep their own texture. Pages for nearest and linear pictures stay separate.
