# Shortcut profiles and project settings

## Shortcut profiles

Open **Tools → Keyboard** (or the **Keyboard** panel). Use **Profiles** to save the current shortcuts under a name or switch profiles. **Export profile** and **Import profile** open the host save and open dialogs. When the dialog is not available, a path field appears at the top of the window; type a full path and choose **Use path**. Conflicting chords list at the bottom; **Fix** removes the first command on that chord.

Default chords include **Shift+G** for grid snap, **Ctrl+[** / **Ctrl+]** for grid snap step, and **Shift+A** for angle snap.

## Editor preferences

Open **Tools → Settings**. Under **External tools**:

- **External code editor** — path and arguments (`{file}`, `{line}`). The Asset Editor toolbar shows **Open in External Editor** for an open text file when this path is set. The button stays off for pictures, sounds, and other non-text files, and when the path is empty.
- **Diff tool** — path and arguments (`{left}`, `{right}`). **Compare with Saved** on the same toolbar is on when this path is set and the open text file differs from the copy that was on disk when the file was opened. `{left}` is that opened copy and `{right}` is the current text.
- **Worker threads** — `auto` (one worker per available processor, at most 64) or a number from 1 to 64. Import, export, and thumbnail jobs use this count. Reset any row with **Reset to default**.

## Project settings

With a project open, the same Settings panel shows project layers and audio:

- **Collision layers** and **Sorting layers** — thirty-two names each; they appear in layer and mask pickers.
- **Master**, **Music**, and **Effects** volume — percentages applied to the live mixer when the value changes and when play starts. Effects sounds use the effects bus. Music uses the music bus, then the master bus.
- **Output sample rate** — optional mixer rate in Hz. Empty keeps the device default. A set rate is applied with the bus volumes.

## Camera cull mask

On a **Camera** object with **2D Camera** on, the **Cull mask** field in the Inspector lists sorting layers. Layers turned off are omitted from the 2D viewport draw list.