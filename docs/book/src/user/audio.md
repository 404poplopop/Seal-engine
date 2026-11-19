# Audio output and listeners

## Project Settings

Open **Create → Project Settings** and open the **Audio** category.

**Master volume**, **Music volume** and **Effects volume** set bus levels as a percent. **Output sample rate**, **Output buffer** and **Output backend** apply when no platform override is set. **Linux**, **Windows**, **macOS** and **Web** rows override sample rate, buffer and backend for that build target.

Backends are **default**, **wasapi**, **coreaudio**, **pulse**, **alsa** and **web**. An empty field keeps the platform default.

## Sound import

In the Asset Editor, open a sound file import preset.

**Compression** chooses how Export Game writes the file: **PCM** (16-bit WAV), **IMA ADPCM** (4 bits per sample in a WAV file, about a quarter of the PCM size) or **Vorbis**. Seal has no Vorbis encoder: with **Vorbis**, a source that is already OGG Vorbis ships unchanged and any other source is written as IMA ADPCM. **Vorbis quality** (0–10) is kept for that setting but does not change IMA ADPCM output. **Load type** is **In memory** or **Streamed**. Streamed sounds are read from disk on each play instead of staying decoded in memory.

## Audio Listener

Select a camera or any object. In the Details panel, open **Audio Listener**.

**Audio Listener** enables positional panning and distance attenuation for script sounds that supply a world position. **Max distance** is the range in world units where level reaches silence. **Rolloff** controls how quickly level falls off (1 is linear). New cameras enable the listener by default.