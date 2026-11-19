//! Browser playback of the mixer.
//!
//! An audio context pulls frames from [`crate::ring::StereoRing`]. When the
//! page can share memory, an audio worklet reads a shared ring. Otherwise a
//! script processor reads the same ring on the main thread. The context stays
//! suspended until the first click or key, which is what browsers require.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    AudioContext, AudioContextOptions, AudioNode, AudioProcessingEvent, AudioWorkletNode,
    BaseAudioContext, Blob, BlobPropertyBag, EventTarget, ScriptProcessorNode, Url, Worklet,
};

use crate::frames::MixerFeed;
use crate::ring::StereoRing;
use crate::web::WebOutputConfig;
use crate::Mixer;

const WORKLET_NAME: &str = "seal-output";

const WORKLET_JS: &str = r#"
class SealOutputProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.state = null;
    this.left = null;
    this.right = null;
    this.cap = 0;
    this.port.onmessage = (event) => {
      const data = event.data;
      if (!data || !data.sab) {
        return;
      }
      const cap = data.cap >>> 0;
      this.cap = cap;
      this.state = new Int32Array(data.sab, 0, 2);
      this.left = new Float32Array(data.sab, 8, cap);
      this.right = new Float32Array(data.sab, 8 + cap * 4, cap);
    };
  }
  process(_inputs, outputs) {
    const channel = outputs[0];
    if (!channel || !channel[0]) {
      return true;
    }
    const left = channel[0];
    const right = channel[1] || left;
    const count = left.length;
    if (!this.state || this.cap === 0) {
      left.fill(0);
      if (right !== left) {
        right.fill(0);
      }
      return true;
    }
    let read = Atomics.load(this.state, 0) >>> 0;
    const write = Atomics.load(this.state, 1) >>> 0;
    const occupied = (write - read) >>> 0;
    if (occupied > this.cap) {
      Atomics.store(this.state, 0, write | 0);
      left.fill(0);
      if (right !== left) {
        right.fill(0);
      }
      return true;
    }
    const mask = this.cap - 1;
    for (let i = 0; i < count; i++) {
      if (read === write) {
        left[i] = 0;
        if (right !== left) {
          right[i] = 0;
        }
        continue;
      }
      const slot = read & mask;
      left[i] = this.left[slot];
      if (right !== left) {
        right[i] = this.right[slot];
      }
      read = (read + 1) >>> 0;
    }
    Atomics.store(this.state, 0, read | 0);
    return true;
  }
}
registerProcessor("seal-output", SealOutputProcessor);
"#;

struct Shared {
    ring: StereoRing,
    feed: RefCell<MixerFeed>,
    script_on: Cell<bool>,
    worklet_on: Cell<bool>,
    sab: RefCell<Option<SabRing>>,
    script: RefCell<Option<ScriptProcessorNode>>,
    context: AudioContext,
    requested_rate: u32,
    device_rate: u32,
    buffer_frames: u32,
    callback_frames: u32,
    /// Frames the script callback handed to the browser, and their loudest sample.
    played: Cell<u64>,
    peak: Cell<f32>,
}

#[derive(Clone)]
struct SabRing {
    sab: js_sys::SharedArrayBuffer,
    state: js_sys::Int32Array,
    left: js_sys::Float32Array,
    right: js_sys::Float32Array,
    cap: u32,
}

impl SabRing {
    fn filled(&self) -> usize {
        let Ok(read) = js_sys::Atomics::load(&self.state, 0) else {
            return 0;
        };
        let Ok(write) = js_sys::Atomics::load(&self.state, 1) else {
            return 0;
        };
        let gap = (write as u32).wrapping_sub(read as u32);
        if gap > self.cap {
            0
        } else {
            gap as usize
        }
    }

    fn push(&self, frames: &[[f32; 2]]) -> usize {
        let Ok(read) = js_sys::Atomics::load(&self.state, 0) else {
            return 0;
        };
        let Ok(write) = js_sys::Atomics::load(&self.state, 1) else {
            return 0;
        };
        let read = read as u32;
        let write = write as u32;
        let gap = write.wrapping_sub(read);
        if gap > self.cap {
            return 0;
        }
        let free = self.cap - gap;
        let count = (frames.len() as u32).min(free);
        for offset in 0..count {
            let slot = write.wrapping_add(offset) & (self.cap - 1);
            let frame = frames[offset as usize];
            self.left.set_index(slot, frame[0]);
            self.right.set_index(slot, frame[1]);
        }
        let _ = js_sys::Atomics::store(&self.state, 1, write.wrapping_add(count) as i32);
        count as usize
    }
}

/// Keeps a browser audio context alive and fed from the mixer.
pub struct WebOut {
    shared: Rc<Shared>,
    /// Closures the context and the page still call.
    #[allow(dead_code)]
    kept: Vec<JsValue>,
}

impl WebOut {
    /// Open the context at `config`'s sample rate and callback size.
    pub fn start(config: &WebOutputConfig) -> Result<Self, String> {
        let options = AudioContextOptions::new();
        options.set_sample_rate(config.sample_rate as f32);
        let context = AudioContext::new_with_context_options(&options).map_err(js_error)?;
        let device_rate = context.sample_rate().round().max(1.0) as u32;
        let callback_frames = config.callback_frames.max(256);
        let shared = Rc::new(Shared {
            ring: StereoRing::with_capacity(ring_frames(callback_frames)),
            feed: RefCell::new(MixerFeed::new(config.sample_rate, device_rate)),
            script_on: Cell::new(false),
            worklet_on: Cell::new(false),
            sab: RefCell::new(None),
            script: RefCell::new(None),
            context,
            requested_rate: config.sample_rate,
            device_rate,
            buffer_frames: config.buffer_frames,
            callback_frames,
            played: Cell::new(0),
            peak: Cell::new(0.0),
        });
        let mut kept = Vec::new();
        let script_ready = open_script(&shared, callback_frames, &mut kept).is_ok();
        let worklet_ready = try_worklet(&shared, &mut kept);
        if !script_ready && !worklet_ready {
            return Err("browser audio output is unavailable".into());
        }
        arm_gesture(&shared.context, &mut kept);
        Ok(Self { shared, kept })
    }

    pub fn requested_rate(&self) -> u32 {
        self.shared.requested_rate
    }

    pub fn device_rate(&self) -> u32 {
        self.shared.device_rate
    }

    pub fn buffer_frames(&self) -> u32 {
        self.shared.buffer_frames
    }

    pub fn callback_frames(&self) -> u32 {
        self.shared.callback_frames
    }

    /// Frames the browser has taken from the ring so far (worklet: its read position).
    pub fn frames_played(&self) -> u64 {
        if self.shared.worklet_on.get() {
            let read = self
                .shared
                .sab
                .borrow()
                .as_ref()
                .and_then(|sab| js_sys::Atomics::load(&sab.state, 0).ok())
                .unwrap_or(0);
            return read as u32 as u64;
        }
        self.shared.played.get()
    }

    /// Loudest sample the script callback has played (0 on the worklet path).
    pub fn peak(&self) -> f32 {
        self.shared.peak.get()
    }

    /// The audio context state: running, suspended or closed.
    pub fn state(&self) -> String {
        format!("{:?}", self.shared.context.state()).to_lowercase()
    }

    /// Resume the context. Call this from a click or key handler.
    pub fn resume(&self) {
        let _ = self.shared.context.resume();
    }

    /// Mix the next block into the ring the callback is reading.
    pub fn pump(&self, mixer: &mut Mixer) {
        let target = self.shared.callback_frames as usize * 2;
        let queued = if self.shared.worklet_on.get() {
            self.shared.sab.borrow().as_ref().map(SabRing::filled).unwrap_or(0)
        } else {
            self.shared.ring.filled()
        };
        if queued >= target {
            return;
        }
        let frames = self.shared.feed.borrow_mut().render(mixer, target - queued);
        if self.shared.worklet_on.get() {
            if let Some(sab) = self.shared.sab.borrow().as_ref() {
                sab.push(&frames);
            }
        } else {
            self.shared.ring.push(&frames);
        }
    }
}

fn ring_frames(callback: u32) -> usize {
    (callback as usize).saturating_mul(8).max(2048).next_power_of_two()
}

fn js_error(error: JsValue) -> String {
    error.as_string().unwrap_or_else(|| "browser audio failed".into())
}

fn retain<T: ?Sized + wasm_bindgen::closure::WasmClosure>(closure: Closure<T>) -> JsValue {
    let kept = closure.as_js_value().clone();
    closure.forget();
    kept
}

fn arm_gesture(context: &AudioContext, kept: &mut Vec<JsValue>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let context = context.clone();
    let closure = Closure::<dyn FnMut()>::own_aborting(move || {
        let _ = context.resume();
    });
    let target: &EventTarget = window.as_ref();
    let function: &js_sys::Function = closure.as_js_value().unchecked_ref();
    let _ = target.add_event_listener_with_callback("pointerdown", function);
    let _ = target.add_event_listener_with_callback("keydown", function);
    kept.push(retain(closure));
}

fn open_script(
    shared: &Rc<Shared>,
    callback_frames: u32,
    kept: &mut Vec<JsValue>,
) -> Result<(), String> {
    let node = shared
        .context
        .create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(
            callback_frames,
            0,
            2,
        )
        .map_err(js_error)?;
    let playback = Rc::clone(shared);
    let closure = Closure::<dyn FnMut(AudioProcessingEvent)>::own_aborting(
        move |event: AudioProcessingEvent| {
        if !playback.script_on.get() || playback.worklet_on.get() {
            return;
        }
        let Ok(buffer) = event.output_buffer() else {
            return;
        };
        let frames = buffer.length() as usize;
        let mut left = vec![0.0; frames];
        let mut right = vec![0.0; frames];
        playback.ring.pop_planar(&mut left, &mut right);
        playback.played.set(playback.played.get() + frames as u64);
        let loudest = left.iter().chain(right.iter()).fold(0.0f32, |m, s| m.max(s.abs()));
        playback.peak.set(playback.peak.get().max(loudest));
        let _ = buffer.copy_to_channel(&left, 0);
        if buffer.number_of_channels() > 1 {
            let _ = buffer.copy_to_channel(&right, 1);
        }
    });
    let function: &js_sys::Function = closure.as_js_value().unchecked_ref();
    node.set_onaudioprocess(Some(function));
    let destination = shared.context.destination();
    let destination: &AudioNode = destination.as_ref();
    node.connect_with_audio_node(destination).map_err(js_error)?;
    shared.script_on.set(true);
    *shared.script.borrow_mut() = Some(node);
    kept.push(retain(closure));
    Ok(())
}

fn try_worklet(shared: &Rc<Shared>, kept: &mut Vec<JsValue>) -> bool {
    let cap = ring_frames(shared.callback_frames) as u32;
    let Ok(sab) = sab_ring(cap) else {
        return false;
    };
    let Ok(blob) = worklet_blob() else {
        return false;
    };
    let Ok(url) = Url::create_object_url_with_blob(&blob) else {
        return false;
    };
    let Ok(audio_worklet) = shared.context.audio_worklet() else {
        return false;
    };
    let worklet: &Worklet = audio_worklet.as_ref();
    let Ok(promise) = worklet.add_module(&url) else {
        return false;
    };
    *shared.sab.borrow_mut() = Some(sab);
    let playback = Rc::clone(shared);
    let closure = Closure::<dyn FnMut(JsValue)>::own_aborting(move |_value: JsValue| {
        let _ = Url::revoke_object_url(&url);
        connect_worklet(&playback);
    });
    let _ = promise.then(&closure);
    kept.push(retain(closure));
    true
}

fn connect_worklet(shared: &Shared) {
    let base: &BaseAudioContext = shared.context.as_ref();
    let Ok(node) = AudioWorkletNode::new(base, WORKLET_NAME) else {
        return;
    };
    let Some(sab) = shared.sab.borrow().clone() else {
        return;
    };
    let Ok(port) = node.port() else {
        return;
    };
    let message = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&message, &JsValue::from_str("sab"), sab.sab.as_ref());
    let _ = js_sys::Reflect::set(
        &message,
        &JsValue::from_str("cap"),
        &JsValue::from_f64(f64::from(sab.cap)),
    );
    if port.post_message(&message).is_err() {
        return;
    }
    let destination = shared.context.destination();
    let destination: &AudioNode = destination.as_ref();
    if node.connect_with_audio_node(destination).is_err() {
        return;
    }
    shared.worklet_on.set(true);
    shared.script_on.set(false);
    if let Some(script) = shared.script.borrow().as_ref() {
        let _ = script.disconnect();
    }
    // The node stays alive through the context graph.
    let _ = node;
}

fn worklet_blob() -> Result<Blob, JsValue> {
    let parts = js_sys::Array::new();
    parts.push(&JsValue::from_str(WORKLET_JS));
    let options = BlobPropertyBag::new();
    options.set_type("application/javascript");
    Blob::new_with_str_sequence_and_options(&parts, &options)
}

fn sab_ring(cap: u32) -> Result<SabRing, JsValue> {
    let cap = cap.max(2).next_power_of_two();
    let bytes = 8 + cap.saturating_mul(8);
    let sab = shared_buffer(bytes)?;
    let view: &JsValue = sab.as_ref();
    let state = js_sys::Int32Array::new_with_byte_offset_and_length(view, 0, 2);
    let left = js_sys::Float32Array::new_with_byte_offset_and_length(view, 8, cap);
    let right = js_sys::Float32Array::new_with_byte_offset_and_length(view, 8 + cap * 4, cap);
    Ok(SabRing { sab, state, left, right, cap })
}

fn shared_buffer(bytes: u32) -> Result<js_sys::SharedArrayBuffer, JsValue> {
    let ctor = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("SharedArrayBuffer"))?;
    let ctor = ctor.dyn_into::<js_sys::Function>()?;
    let args = js_sys::Array::new();
    args.push(&JsValue::from_f64(f64::from(bytes)));
    let value = js_sys::Reflect::construct(&ctor, &args)?;
    Ok(value.unchecked_into())
}
