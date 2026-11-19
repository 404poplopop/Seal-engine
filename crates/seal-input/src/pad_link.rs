//! Opens the player-1 pad and trades light-bar and motion reports with it.
//!
//! Windows uses the HID device interface. Linux uses the raw HID nodes under
//! `/dev`. Other systems report that the sensors are not available. No pad is
//! required to build this crate: the bytes live in [`super::pad_reports`].

use super::devices::PadMotion;
use super::pad_reports::{
    controller_notes, light_report, pad_family, parse_pro_motion, parse_pro_reply, resolve,
    sony_motion, PadFamily, ProScale, Resolved, SonyScale, Wire, NOTE_NOT_FOUND, NOTE_NO_LIGHT,
    NOTE_NO_LIGHT_OS, NOTE_NO_MOTION_OS,
};

/// Vendor and product of one pad, in the button reader's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PadId {
    pub vendor: u16,
    pub product: u16,
}

/// Light bar and motion sensors for player 1.
pub struct PadSensors {
    light_note: String,
    motion: PadMotion,
    label: Option<String>,
    #[cfg(any(windows, target_os = "linux"))]
    link: Link,
}

impl Default for PadSensors {
    fn default() -> Self {
        Self::new()
    }
}

impl PadSensors {
    pub fn new() -> Self {
        let hid = cfg!(any(windows, target_os = "linux"));
        Self {
            light_note: if hid {
                super::pad_reports::NOTE_NO_PAD.into()
            } else {
                NOTE_NO_LIGHT_OS.into()
            },
            motion: PadMotion::unsupported(if hid {
                super::pad_reports::NOTE_NO_PAD
            } else {
                NOTE_NO_MOTION_OS
            }),
            label: None,
            #[cfg(any(windows, target_os = "linux"))]
            link: Link::default(),
        }
    }

    pub fn motion(&self) -> PadMotion {
        self.motion.clone()
    }

    pub fn light_note(&self) -> String {
        self.light_note.clone()
    }

    /// Name of player 1 when a pad is present, for the bindings page.
    pub fn label(&self) -> Option<String> {
        self.label.clone()
    }

    /// `players` is the button reader's order. `None` means there is no button
    /// reader, so the first raw game pad is player 1.
    pub fn sync(&mut self, players: Option<&[PadId]>) {
        #[cfg(any(windows, target_os = "linux"))]
        self.sync_hid(players);
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            let ids: Vec<(u16, u16)> =
                players.unwrap_or(&[]).iter().map(|pad| (pad.vendor, pad.product)).collect();
            let (light, motion) = if ids.is_empty() {
                controller_notes(None, false)
            } else if pad_family(ids[0].0, ids[0].1).is_none() {
                controller_notes(None, true)
            } else {
                (NOTE_NO_LIGHT_OS.to_string(), NOTE_NO_MOTION_OS.to_string())
            };
            self.light_note = light;
            self.motion = if motion.is_empty() {
                PadMotion::ready([0.0; 3], [0.0; 3])
            } else {
                PadMotion::unsupported(motion)
            };
            self.label = if ids.is_empty() {
                None
            } else {
                Some(
                    pad_family(ids[0].0, ids[0].1)
                        .map(|family| family.label(ids[0].1).to_string())
                        .unwrap_or_else(|| "Gamepad".into()),
                )
            };
        }
    }

    pub fn set_light(&mut self, rgb: [f32; 3]) -> Result<(), String> {
        let rgb =
            rgb.map(|channel| if channel.is_finite() { channel.clamp(0.0, 1.0) } else { 0.0 });
        #[cfg(any(windows, target_os = "linux"))]
        {
            self.link.last_rgb = Some(rgb);
            return self.write_light(rgb);
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            let _ = rgb;
            Err(self.light_note.clone())
        }
    }
}

#[cfg(any(windows, target_os = "linux"))]
struct Link {
    last_rgb: Option<[f32; 3]>,
    seq: u8,
    scanned_at: Option<std::time::Instant>,
    cached: Vec<io::Listed>,
    opened: Option<Opened>,
}

#[cfg(any(windows, target_os = "linux"))]
impl Default for Link {
    fn default() -> Self {
        Self { last_rgb: None, seq: 0, scanned_at: None, cached: Vec::new(), opened: None }
    }
}

#[cfg(any(windows, target_os = "linux"))]
struct Opened {
    key: String,
    vendor: u16,
    product: u16,
    nth: usize,
    family: PadFamily,
    wire: Wire,
    dev: io::Device,
    sony: Option<SonyScale>,
    pro: Option<ProScale>,
}

#[cfg(any(windows, target_os = "linux"))]
const NOTE_WAITING: &str = "the gamepad has not reported motion yet";

#[cfg(any(windows, target_os = "linux"))]
impl PadSensors {
    fn sync_hid(&mut self, players: Option<&[PadId]>) {
        let ids: Vec<(u16, u16)> =
            players.unwrap_or(&[]).iter().map(|pad| (pad.vendor, pad.product)).collect();
        let players = players.map(|_| ids.as_slice());
        if let Some(list) = players {
            if list.is_empty() {
                self.close();
                self.set_pair(controller_notes(None, false));
                self.label = None;
                return;
            }
            if pad_family(list[0].0, list[0].1).is_none() {
                self.close();
                self.set_pair(controller_notes(None, true));
                self.label = Some("Gamepad".into());
                return;
            }
        }
        self.refresh_list();
        let devices: Vec<(u16, u16)> =
            self.link.cached.iter().map(|dev| (dev.vendor, dev.product)).collect();
        match resolve(players, &devices) {
            Resolved::NoPad => {
                self.close();
                self.set_pair(controller_notes(None, false));
                self.label = None;
            }
            Resolved::Unsupported => {
                self.close();
                self.set_pair(controller_notes(None, true));
                self.label = Some("Gamepad".into());
            }
            Resolved::NotFound => self.mark_missing(pad_family(
                players.and_then(|list| list.first()).map(|id| id.0).unwrap_or(0),
                players.and_then(|list| list.first()).map(|id| id.1).unwrap_or(0),
            )),
            Resolved::Open { family, nth, vendor, product } => {
                let same = self.link.opened.as_ref().is_some_and(|open| {
                    open.vendor == vendor
                        && open.product == product
                        && open.nth == nth
                        && open.dev.alive()
                        && self.link.cached.iter().any(|dev| dev.key == open.key)
                });
                if same {
                    self.pull(0);
                } else {
                    self.reopen(family, nth, vendor, product);
                }
            }
        }
    }

    fn refresh_list(&mut self) {
        let due = self
            .link
            .scanned_at
            .map_or(true, |at| at.elapsed() >= std::time::Duration::from_millis(500));
        if !due {
            return;
        }
        match io::list() {
            Ok(list) => {
                self.link.cached = list;
                self.link.scanned_at = Some(std::time::Instant::now());
            }
            Err(err) => {
                if self.link.opened.is_none() && self.link.cached.is_empty() {
                    self.light_note = err.clone();
                    self.motion = PadMotion::unsupported(err);
                    self.label = None;
                }
            }
        }
    }

    fn reopen(&mut self, family: PadFamily, nth: usize, vendor: u16, product: u16) {
        self.close();
        let subset: Vec<io::Listed> = self
            .link
            .cached
            .iter()
            .filter(|dev| dev.vendor == vendor && dev.product == product)
            .cloned()
            .collect();
        let mut fatal = None;
        let mut seen = 0usize;
        for listed in &subset {
            match io::open(listed, family) {
                Ok(session) => {
                    if seen == nth {
                        self.attach(listed, family, nth, session);
                        return;
                    }
                    seen += 1;
                }
                Err(io::Fail::Wrong) => {}
                Err(io::Fail::Fatal(msg)) => fatal = Some(msg),
            }
        }
        self.mark_missing_msg(family, fatal.unwrap_or_else(|| NOTE_NOT_FOUND.into()));
    }

    fn attach(&mut self, listed: &io::Listed, family: PadFamily, nth: usize, session: io::Session) {
        let warn = session.warn.clone();
        self.link.opened = Some(Opened {
            key: listed.key.clone(),
            vendor: listed.vendor,
            product: listed.product,
            nth,
            family,
            wire: listed.wire,
            dev: session.dev,
            sony: session.sony,
            pro: session.pro,
        });
        self.label = Some(family.label(listed.product).to_string());
        self.light_note = if family.light_bar() { String::new() } else { NOTE_NO_LIGHT.into() };
        self.motion = PadMotion::unsupported(warn.unwrap_or_else(|| NOTE_WAITING.into()));
        self.pull(0);
        if let Some(rgb) = self.link.last_rgb {
            if family.light_bar() {
                let _ = self.write_light(rgb);
            }
        }
    }

    fn mark_missing(&mut self, family: Option<PadFamily>) {
        self.close();
        self.mark_missing_msg(family.unwrap_or(PadFamily::Ds4), NOTE_NOT_FOUND.into());
        if family.is_none() {
            self.set_pair(controller_notes(None, true));
            self.label = Some("Gamepad".into());
        }
    }

    fn mark_missing_msg(&mut self, family: PadFamily, msg: String) {
        self.close();
        self.label = Some(family.label(0).to_string());
        if family.light_bar() {
            self.light_note.clone_from(&msg);
            self.motion = PadMotion::unsupported(msg);
        } else {
            self.light_note = NOTE_NO_LIGHT.into();
            self.motion = PadMotion::unsupported(msg);
        }
    }

    fn pull(&mut self, wait_ms: u32) {
        let pulled = match self.link.opened.as_mut() {
            Some(open) => open.dev.drain(wait_ms),
            None => return,
        };
        match pulled {
            Ok(reports) => {
                for report in reports {
                    self.ingest(&report);
                }
            }
            Err(err) => self.fail_open(err),
        }
    }

    fn ingest(&mut self, report: &[u8]) {
        let parsed = {
            let Some(open) = self.link.opened.as_mut() else {
                return;
            };
            match open.family {
                PadFamily::Ds4 | PadFamily::DualSense => open
                    .sony
                    .and_then(|scale| sony_motion(open.family, open.wire, report, &scale).ok()),
                PadFamily::Pro => {
                    if report.first() == Some(&0x21) {
                        if let Ok(scale) = parse_pro_reply(report) {
                            open.pro = Some(scale);
                        }
                    }
                    open.pro.as_ref().and_then(|scale| parse_pro_motion(report, scale).ok())
                }
            }
        };
        if let Some((gyro, accel)) = parsed {
            self.motion = PadMotion::ready(gyro, accel);
        }
    }

    fn write_light(&mut self, rgb: [f32; 3]) -> Result<(), String> {
        let Some((family, wire)) = self.link.opened.as_ref().map(|open| (open.family, open.wire))
        else {
            return Err(self.light_note.clone());
        };
        if !family.light_bar() {
            self.light_note = NOTE_NO_LIGHT.into();
            return Err(self.light_note.clone());
        }
        let bytes = light_report(family, wire, rgb, self.link.seq)?;
        let wrote = {
            let Some(open) = self.link.opened.as_mut() else {
                return Err(self.light_note.clone());
            };
            open.dev.write(&bytes)
        };
        match wrote {
            Ok(()) => {
                self.link.seq = self.link.seq.wrapping_add(1) & 0x0F;
                self.light_note.clear();
                Ok(())
            }
            Err(err) => {
                self.light_note.clone_from(&err);
                Err(err)
            }
        }
    }

    fn fail_open(&mut self, err: String) {
        let pro = self.link.opened.as_ref().is_some_and(|open| !open.family.light_bar());
        self.close();
        self.motion = PadMotion::unsupported(err.clone());
        self.light_note = if pro { NOTE_NO_LIGHT.into() } else { err };
    }

    fn set_pair(&mut self, notes: (String, String)) {
        self.light_note = notes.0;
        self.motion = if notes.1.is_empty() {
            PadMotion::ready([0.0; 3], [0.0; 3])
        } else {
            PadMotion::unsupported(notes.1)
        };
    }

    fn close(&mut self) {
        self.link.opened = None;
    }
}

#[cfg(any(windows, target_os = "linux"))]
mod io;

#[cfg(test)]
mod hardware_check {
    /// Run on a machine with controllers: `cargo test -p seal-input -- --ignored --nocapture hid_pads`.
    #[test]
    #[ignore = "reads the real HID devices of this machine"]
    fn hid_pads_on_this_machine() {
        let list = super::io::list().expect("HID devices can be listed");
        for pad in &list {
            let family = crate::pad_reports::pad_family(pad.vendor, pad.product);
            println!("pad {:04x}:{:04x} {:?} {:?}", pad.vendor, pad.product, pad.wire, family);
        }
        println!("pads with a light bar or motion: {}", list.len());
    }
}
