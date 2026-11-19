# Web audio output

Browser builds play the mixer through Web Audio. Desktop builds keep the desktop output device.

## Project Settings

Open **Create → Project Settings** and open the **Audio** category.

**Web output backend** offers Web Audio. **Web output sample rate** is the context rate in hertz. Empty uses 48000. Allowed values are 8000–192000. **Web output buffer** is the callback size in frames. Empty uses 512. Allowed values are 32–4096. The running build snaps that size to 256, 512, 1024, 2048 or 4096.

The context stays suspended until the first click or key press, then it resumes. Each frame, mixed stereo is written into a ring. The browser callback reads that ring.

## Browser demo

The `seal-web-audio` crate is a page that plays a mixer tone and a sound file.

Build it with the wasm-bindgen command-line tool:

```
rustup target add wasm32-unknown-unknown
cargo build -p seal-web-audio --target wasm32-unknown-unknown --release
wasm-bindgen --target web --out-dir examples/seal-web-audio/pkg target/wasm32-unknown-unknown/release/seal_web_audio.wasm
```

Serve `examples/seal-web-audio` as a static directory and open `index.html`. **Play tone** starts a sine through the mixer. **Play file** decodes `tone.wav` and plays it. Both buttons resume the context.

When the page is cross-origin isolated, an audio worklet reads a shared ring. Otherwise a script processor reads the ring on the page thread.
