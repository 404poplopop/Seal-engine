# Sound containers

A **sound container** (`.soundset` file) holds several sound files. Each time something plays that container, the engine picks one or more sounds according to the mode you set.

## Where to find them

- **Asset Browser**: filter by **Sound Container**, or use **+** → **Sound Container** in the current folder.
- **Create**: the command **New Sound Container** (`asset.soundset_new`) creates a file and opens it in the Asset Editor.
- **Asset Editor**: double-click a `.soundset` file to edit mode, volume, pitch and volume ranges, max simultaneous voices, cooldown, and the list of sounds (path, weight, volume per entry). **Play** auditions the container.

## Modes

- **Random**: weighted random pick; the same sound is not chosen twice in a row.
- **Shuffle**: each entry plays once before the list is shuffled and repeated.
- **Sequence**: entries play in list order, then wrap.
- **Layered**: every entry plays at once.

Scripts and graph **Play Sound** nodes accept a `.soundset` path the same way as a single sound file (WAV, FLAC, OGG Vorbis or MP3).