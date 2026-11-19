//! Browser demo. Playback exports are compiled for wasm32.

#[cfg(target_arch = "wasm32")]
mod demo {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsValue;

    use seal_audio::web::{load_sound_bytes, WebOutputConfig};
    use seal_audio::web_out::WebOut;
    use seal_audio::{Mixer, PlayParams, Sound};

    #[wasm_bindgen]
    pub struct Demo {
        mixer: Mixer,
        out: WebOut,
    }

    #[wasm_bindgen]
    impl Demo {
        #[wasm_bindgen(constructor)]
        pub fn new(sample_rate: u32, buffer_frames: u32) -> Result<Demo, JsValue> {
            let rate = if sample_rate == 0 { None } else { Some(sample_rate) };
            let buffer = if buffer_frames == 0 { None } else { Some(buffer_frames) };
            let config = WebOutputConfig::resolve(rate, buffer);
            let out = WebOut::start(&config).map_err(|error| JsValue::from_str(&error))?;
            let mixer = Mixer::new(out.requested_rate());
            Ok(Demo { mixer, out })
        }

        pub fn resume(&self) {
            self.out.resume();
        }

        pub fn play_tone(&mut self) {
            let rate = self.mixer.sample_rate;
            let tone = Sound::tone(rate, 440.0, 1.0, 0.2);
            let id = self.mixer.add_sound(tone);
            let _ = self.mixer.play(id, PlayParams::default());
            self.out.pump(&mut self.mixer);
        }

        pub fn play_file(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
            let sound =
                load_sound_bytes(bytes).map_err(|error| JsValue::from_str(&error.to_string()))?;
            let id = self.mixer.add_sound(sound);
            let _ = self.mixer.play(id, PlayParams::default());
            self.out.pump(&mut self.mixer);
            Ok(())
        }

        pub fn pump(&mut self) {
            self.out.pump(&mut self.mixer);
        }

        /// One line for automated checks: context state, frames played, loudest sample.
        pub fn report(&self) -> String {
            format!(
                "state={} played={} peak={:.3}",
                self.out.state(),
                self.out.frames_played(),
                self.out.peak()
            )
        }

        pub fn status(&self) -> String {
            format!(
                "Web Audio {} Hz, buffer {} frames",
                self.out.device_rate(),
                self.out.callback_frames()
            )
        }
    }
}
