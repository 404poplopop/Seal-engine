//! Light-bar and motion reports for the pads that expose them.
//!
//! Bytes only: no device is opened here. USB and Bluetooth differ by a short
//! header and, on Bluetooth, a trailing CRC. Gyro comes out in radians per
//! second and acceleration in gravities.

use std::f64::consts::PI;

/// Why a light bar cannot be set when no pad is connected.
pub const NOTE_NO_PAD: &str = "no gamepad is connected";
/// Why a light bar cannot be set on this pad.
pub const NOTE_NO_LIGHT: &str = "the gamepad light bar is not supported by this controller";
/// Why motion sensors cannot be read on this pad.
pub const NOTE_NO_MOTION: &str = "motion sensors are not supported by this controller";
/// Why a light bar cannot be set on this operating system.
pub const NOTE_NO_LIGHT_OS: &str = "the gamepad light bar is not supported on this system";
/// Why motion sensors cannot be read on this operating system.
pub const NOTE_NO_MOTION_OS: &str = "motion sensors are not supported on this system";
/// The pad is connected to the button reader but has no matching raw device.
pub const NOTE_NOT_FOUND: &str = "the gamepad was not found on the device bus";

const VENDOR_SONY: u16 = 0x054C;
const DS4_A: u16 = 0x05C4;
const DS4_B: u16 = 0x09CC;
const DUALSENSE: u16 = 0x0CE6;
const DUALSENSE_EDGE: u16 = 0x0DF2;
const VENDOR_PRO: u16 = 0x057E;
const PRO_CONTROLLER: u16 = 0x2009;

const GYRO_RES_PER_DEG_S: i32 = 1024;
const ACC_RES_PER_G: i32 = 8192;
const GYRO_RANGE: i32 = 2048 * GYRO_RES_PER_DEG_S;
const ACC_RANGE: i32 = 4 * ACC_RES_PER_G;
const I16_MAX: i32 = 32767;

const PRO_ACCEL_PER_G: f64 = 4096.0;
const PRO_GYRO_PER_DPS: f64 = 14247.0;
const PRO_GYRO_SCALE: i64 = 1000;
const PRO_DEFAULT_ACCEL_SCALE: i16 = 16384;
const PRO_DEFAULT_GYRO_SCALE: i16 = 13371;

/// How the pad is attached. Bluetooth output reports carry a checksum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wire {
    Usb,
    Bluetooth,
}

/// A pad whose light bar or motion sensors we know how to speak to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadFamily {
    /// DualShock 4, both USB product ids.
    Ds4,
    /// DualSense, including the Edge model.
    DualSense,
    /// Switch Pro Controller. Motion only; it has no light bar.
    Pro,
}

impl PadFamily {
    pub fn light_bar(self) -> bool {
        !matches!(self, Self::Pro)
    }

    pub fn label(self, product: u16) -> &'static str {
        match self {
            Self::Ds4 => "DualShock 4",
            Self::DualSense if product == DUALSENSE_EDGE => "DualSense Edge",
            Self::DualSense => "DualSense",
            Self::Pro => "Pro Controller",
        }
    }
}

/// `None` when this vendor and product are not one of the pads above.
pub fn pad_family(vendor: u16, product: u16) -> Option<PadFamily> {
    match (vendor, product) {
        (VENDOR_SONY, DS4_A | DS4_B) => Some(PadFamily::Ds4),
        (VENDOR_SONY, DUALSENSE | DUALSENSE_EDGE) => Some(PadFamily::DualSense),
        (VENDOR_PRO, PRO_CONTROLLER) => Some(PadFamily::Pro),
        _ => None,
    }
}

/// Light-bar note and motion note. The motion note is empty when sensors apply.
/// `present` is false when no pad is player 1.
pub fn controller_notes(family: Option<PadFamily>, present: bool) -> (String, String) {
    if !present {
        return (NOTE_NO_PAD.to_string(), NOTE_NO_PAD.to_string());
    }
    match family {
        Some(PadFamily::Pro) => (NOTE_NO_LIGHT.to_string(), String::new()),
        Some(PadFamily::Ds4 | PadFamily::DualSense) => (String::new(), String::new()),
        None => (NOTE_NO_LIGHT.to_string(), NOTE_NO_MOTION.to_string()),
    }
}

/// IEEE CRC-32 of `data` (the checksum used as a known answer for report CRCs).
pub fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc = crc_step(crc, byte);
    }
    !crc
}

/// CRC-32 of `seed` followed by `data`, the Bluetooth report checksum.
pub fn report_crc(seed: u8, data: &[u8]) -> u32 {
    let mut crc = crc_step(0xFFFF_FFFFu32, seed);
    for &byte in data {
        crc = crc_step(crc, byte);
    }
    !crc
}

fn crc_step(mut crc: u32, byte: u8) -> u32 {
    crc ^= u32::from(byte);
    for _ in 0..8 {
        let mask = (crc & 1).wrapping_neg();
        crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
    }
    crc
}

/// Index of `players[player]` inside `devices`, matched by vendor, product, and
/// how many earlier players share that id. `None` when that id is not listed.
pub fn match_hid_index(
    players: &[(u16, u16)],
    devices: &[(u16, u16)],
    player: usize,
) -> Option<usize> {
    let id = *players.get(player)?;
    let nth = players[..player].iter().filter(|other| **other == id).count();
    devices.iter().enumerate().filter(|(_, dev)| **dev == id).nth(nth).map(|(index, _)| index)
}

/// Which probed interface is the `nth` physical pad.
/// A true entry answered the pad's own report. When none did, the `nth` slot
/// is still that pad's first listed interface.
pub fn pick_interface(probed_ok: &[bool], nth: usize) -> Option<usize> {
    let ready: Vec<usize> =
        probed_ok.iter().enumerate().filter(|(_, ok)| **ok).map(|(i, _)| i).collect();
    if !ready.is_empty() {
        return ready.get(nth).copied();
    }
    (nth < probed_ok.len()).then_some(nth)
}

/// What player 1 should do with this device list.
/// `players == None` means there is no button reader: the first listed device is player 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolved {
    NoPad,
    Unsupported,
    NotFound,
    Open { family: PadFamily, nth: usize, vendor: u16, product: u16 },
}

pub fn resolve(players: Option<&[(u16, u16)]>, devices: &[(u16, u16)]) -> Resolved {
    let Some(players) = players else {
        let Some(&(vendor, product)) = devices.first() else {
            return Resolved::NoPad;
        };
        return match pad_family(vendor, product) {
            Some(family) => Resolved::Open { family, nth: 0, vendor, product },
            None => Resolved::Unsupported,
        };
    };
    let Some(&(vendor, product)) = players.first() else {
        return Resolved::NoPad;
    };
    let Some(family) = pad_family(vendor, product) else {
        return Resolved::Unsupported;
    };
    if match_hid_index(players, devices, 0).is_none() {
        Resolved::NotFound
    } else {
        Resolved::Open { family, nth: 0, vendor, product }
    }
}

/// One axis after the calibration report is applied. `numer / denom` turns a
/// raw count into the unit the converter expects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AxisScale {
    bias: i32,
    numer: i32,
    denom: i32,
}

/// Gyro and accelerometer scales from a calibration report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SonyScale {
    gyro: [AxisScale; 3],
    accel: [AxisScale; 3],
}

impl SonyScale {
    /// Full-scale stand-in used when the report's span is zero.
    pub fn nominal() -> Self {
        let gyro = AxisScale { bias: 0, numer: GYRO_RANGE, denom: I16_MAX };
        let accel = AxisScale { bias: 0, numer: ACC_RANGE, denom: I16_MAX };
        Self { gyro: [gyro; 3], accel: [accel; 3] }
    }
}

/// Read a calibration feature report. A short or checksum-failed report is an error.
/// A zero span falls back to [`SonyScale::nominal`] for that axis.
pub fn sony_calibration(family: PadFamily, wire: Wire, report: &[u8]) -> Result<SonyScale, String> {
    let (expect_id, min_len, crc) = match (family, wire) {
        (PadFamily::Ds4, Wire::Usb) => (0x02u8, 37usize, false),
        (PadFamily::Ds4, Wire::Bluetooth) => (0x05, 41, true),
        (PadFamily::DualSense, _) => (0x05, 41, matches!(wire, Wire::Bluetooth)),
        (PadFamily::Pro, _) => {
            return Err("this controller has no light-bar calibration report".into());
        }
    };
    check_report(report, expect_id, min_len, crc, 0xA3)?;
    let bias = [i16_at(report, 1), i16_at(report, 3), i16_at(report, 5)];
    let (plus, minus) = if matches!((family, wire), (PadFamily::Ds4, Wire::Bluetooth)) {
        (
            [i16_at(report, 7), i16_at(report, 9), i16_at(report, 11)],
            [i16_at(report, 13), i16_at(report, 15), i16_at(report, 17)],
        )
    } else {
        (
            [i16_at(report, 7), i16_at(report, 11), i16_at(report, 15)],
            [i16_at(report, 9), i16_at(report, 13), i16_at(report, 17)],
        )
    };
    let speed = i32::from(i16_at(report, 19)) + i32::from(i16_at(report, 21));
    let mut gyro = [AxisScale { bias: 0, numer: 0, denom: 1 }; 3];
    for i in 0..3 {
        let denom = (i32::from(plus[i]) - i32::from(bias[i])).abs()
            + (i32::from(minus[i]) - i32::from(bias[i])).abs();
        gyro[i] = if denom == 0 {
            AxisScale { bias: 0, numer: GYRO_RANGE, denom: I16_MAX }
        } else {
            AxisScale { bias: 0, numer: speed.saturating_mul(GYRO_RES_PER_DEG_S), denom }
        };
    }
    let mut accel = [AxisScale { bias: 0, numer: 0, denom: 1 }; 3];
    for i in 0..3 {
        let off = 23 + i * 4;
        let plus = i32::from(i16_at(report, off));
        let minus = i32::from(i16_at(report, off + 2));
        let range = plus - minus;
        accel[i] = if range == 0 {
            AxisScale { bias: 0, numer: ACC_RANGE, denom: I16_MAX }
        } else {
            AxisScale { bias: plus - range / 2, numer: 2 * ACC_RES_PER_G, denom: range }
        };
    }
    Ok(SonyScale { gyro, accel })
}

/// Gyro (radians per second) and acceleration (gravities) from one input report.
pub fn sony_motion(
    family: PadFamily,
    wire: Wire,
    report: &[u8],
    scale: &SonyScale,
) -> Result<([f32; 3], [f32; 3]), String> {
    let (id, size, base) = sony_input_layout(family, wire)?;
    check_report(report, id, size, matches!(wire, Wire::Bluetooth), 0xA1)?;
    let gyro_at = base + if matches!(family, PadFamily::Ds4) { 12 } else { 15 };
    let gyro_raw =
        [i16_at(report, gyro_at), i16_at(report, gyro_at + 2), i16_at(report, gyro_at + 4)];
    let accel_at = gyro_at + 6;
    let accel_raw =
        [i16_at(report, accel_at), i16_at(report, accel_at + 2), i16_at(report, accel_at + 4)];
    let mut gyro = [0.0; 3];
    let mut accel = [0.0; 3];
    for i in 0..3 {
        let counts = mul_div(scale.gyro[i].numer, i32::from(gyro_raw[i]), scale.gyro[i].denom);
        let deg = counts as f64 / f64::from(GYRO_RES_PER_DEG_S);
        gyro[i] = (deg * PI / 180.0) as f32;
        let delta = i32::from(accel_raw[i]) - scale.accel[i].bias;
        let counts = mul_div(scale.accel[i].numer, delta, scale.accel[i].denom);
        accel[i] = (counts as f64 / f64::from(ACC_RES_PER_G)) as f32;
    }
    Ok((finite3(gyro), finite3(accel)))
}

fn sony_input_layout(family: PadFamily, wire: Wire) -> Result<(u8, usize, usize), String> {
    match (family, wire) {
        (PadFamily::Ds4, Wire::Usb) => Ok((0x01, 64, 1)),
        (PadFamily::Ds4, Wire::Bluetooth) => Ok((0x11, 78, 3)),
        (PadFamily::DualSense, Wire::Usb) => Ok((0x01, 64, 1)),
        (PadFamily::DualSense, Wire::Bluetooth) => Ok((0x31, 78, 2)),
        (PadFamily::Pro, _) => Err("this controller does not use that motion report".into()),
    }
}

/// Output report that sets the light bar. Channels outside 0..1 are clamped.
/// Bluetooth reports end with the checksum. A Pro Controller has no light bar.
pub fn light_report(
    family: PadFamily,
    wire: Wire,
    rgb: [f32; 3],
    seq: u8,
) -> Result<Vec<u8>, String> {
    let rgb = [byte_channel(rgb[0]), byte_channel(rgb[1]), byte_channel(rgb[2])];
    let mut buf = match (family, wire) {
        (PadFamily::Ds4, Wire::Usb) => {
            let mut buf = vec![0u8; 32];
            buf[0] = 0x05;
            buf[1] = 0x02;
            buf[6] = rgb[0];
            buf[7] = rgb[1];
            buf[8] = rgb[2];
            buf
        }
        (PadFamily::Ds4, Wire::Bluetooth) => {
            let mut buf = vec![0u8; 78];
            buf[0] = 0x11;
            buf[1] = 0xC0;
            buf[3] = 0x02;
            buf[8] = rgb[0];
            buf[9] = rgb[1];
            buf[10] = rgb[2];
            buf
        }
        (PadFamily::DualSense, Wire::Usb) => {
            let mut buf = vec![0u8; 63];
            buf[0] = 0x02;
            buf[2] = 0x04;
            buf[45] = rgb[0];
            buf[46] = rgb[1];
            buf[47] = rgb[2];
            buf
        }
        (PadFamily::DualSense, Wire::Bluetooth) => {
            let mut buf = vec![0u8; 78];
            buf[0] = 0x31;
            buf[1] = (seq & 0x0F) << 4;
            buf[2] = 0x10;
            buf[4] = 0x04;
            buf[47] = rgb[0];
            buf[48] = rgb[1];
            buf[49] = rgb[2];
            buf
        }
        (PadFamily::Pro, _) => return Err(NOTE_NO_LIGHT.into()),
    };
    if matches!(wire, Wire::Bluetooth) {
        stamp_crc(&mut buf, 0xA2);
    }
    Ok(buf)
}

/// Subcommand output. USB is padded to 64 bytes. Bluetooth is the packet only.
pub fn pro_output(subcmd: u8, data: &[u8], counter: u8, usb: bool) -> Vec<u8> {
    let bare = 12 + data.len().saturating_sub(1);
    let len = if usb { 64 } else { bare.max(12) };
    let mut buf = vec![0u8; len];
    buf[0] = 0x01;
    buf[1] = counter & 0x0F;
    buf[10] = subcmd;
    let room = buf.len().saturating_sub(11);
    let n = data.len().min(room);
    buf[11..11 + n].copy_from_slice(&data[..n]);
    buf
}

pub fn pro_enable_imu(counter: u8, usb: bool) -> Vec<u8> {
    pro_output(0x40, &[0x01], counter, usb)
}

pub fn pro_report_mode(counter: u8, usb: bool) -> Vec<u8> {
    pro_output(0x03, &[0x30], counter, usb)
}

pub fn pro_read_factory_imu(counter: u8, usb: bool) -> Vec<u8> {
    let mut data = [0u8; 5];
    data[..4].copy_from_slice(&0x6020u32.to_le_bytes());
    data[4] = 24;
    pro_output(0x10, &data, counter, usb)
}

/// Factory motion calibration: three offsets and three scales for each sensor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProScale {
    pub accel_offset: [i16; 3],
    pub accel_scale: [i16; 3],
    pub gyro_offset: [i16; 3],
    pub gyro_scale: [i16; 3],
}

impl ProScale {
    pub fn defaults() -> Self {
        Self {
            accel_offset: [0; 3],
            accel_scale: [PRO_DEFAULT_ACCEL_SCALE; 3],
            gyro_offset: [0; 3],
            gyro_scale: [PRO_DEFAULT_GYRO_SCALE; 3],
        }
    }

    fn divisors(self) -> ([i32; 3], [i32; 3]) {
        let mut accel = [1; 3];
        let mut gyro = [1; 3];
        for i in 0..3 {
            let a = i32::from(self.accel_scale[i]) - i32::from(self.accel_offset[i]);
            let g = i32::from(self.gyro_scale[i]) - i32::from(self.gyro_offset[i]);
            if a != 0 {
                accel[i] = a;
            }
            if g != 0 {
                gyro[i] = g;
            }
        }
        (accel, gyro)
    }
}

/// 24-byte factory block. A shorter block is damaged. All-zero scales use defaults.
pub fn parse_pro_calibration(bytes: &[u8]) -> Result<ProScale, String> {
    if bytes.len() < 24 {
        return Err("the motion calibration report is too short".into());
    }
    let mut scale = ProScale {
        accel_offset: [i16_at(bytes, 0), i16_at(bytes, 2), i16_at(bytes, 4)],
        accel_scale: [i16_at(bytes, 6), i16_at(bytes, 8), i16_at(bytes, 10)],
        gyro_offset: [i16_at(bytes, 12), i16_at(bytes, 14), i16_at(bytes, 16)],
        gyro_scale: [i16_at(bytes, 18), i16_at(bytes, 20), i16_at(bytes, 22)],
    };
    if scale.accel_scale == [0; 3] && scale.gyro_scale == [0; 3] {
        scale = ProScale::defaults();
    }
    Ok(scale)
}

/// A subcommand reply that carries the 24-byte factory block.
pub fn parse_pro_reply(report: &[u8]) -> Result<ProScale, String> {
    if report.first().copied() != Some(0x21) {
        return Err("the motion calibration reply has the wrong report".into());
    }
    if report.len() < 44 {
        return Err("the motion calibration reply is too short".into());
    }
    if report[13] & 0x80 == 0 || report[14] != 0x10 {
        return Err("the motion calibration reply was damaged".into());
    }
    parse_pro_calibration(&report[20..44])
}

/// Latest gyro and accelerometer sample in a standard full input report.
pub fn parse_pro_motion(report: &[u8], scale: &ProScale) -> Result<([f32; 3], [f32; 3]), String> {
    if report.first().copied() != Some(0x30) {
        return Err("the motion report has the wrong id".into());
    }
    if report.len() < 49 {
        return Err("the motion report is too short".into());
    }
    // Three samples, oldest first. The last one is the newest.
    let at = 13 + 24;
    let accel_raw = [i16_at(report, at), i16_at(report, at + 2), i16_at(report, at + 4)];
    let gyro_raw = [i16_at(report, at + 6), i16_at(report, at + 8), i16_at(report, at + 10)];
    let (accel_div, gyro_div) = scale.divisors();
    let mut gyro = [0.0; 3];
    let mut accel = [0.0; 3];
    for i in 0..3 {
        let numer = PRO_GYRO_SCALE * (i64::from(gyro_raw[i]) - i64::from(scale.gyro_offset[i]));
        let counts = numer * i64::from(scale.gyro_scale[i]) / i64::from(gyro_div[i]);
        let deg = counts as f64 / PRO_GYRO_PER_DPS;
        gyro[i] = (deg * PI / 180.0) as f32;
        let counts =
            i64::from(accel_raw[i]) * i64::from(scale.accel_scale[i]) / i64::from(accel_div[i]);
        accel[i] = (counts as f64 / PRO_ACCEL_PER_G) as f32;
    }
    Ok((finite3(gyro), finite3(accel)))
}

/// Whether a report descriptor's application collection is a game pad or joystick.
/// A truncated item is an error. An empty descriptor is not a game pad.
pub fn descriptor_is_gamepad(desc: &[u8]) -> Result<bool, String> {
    let mut i = 0;
    let mut page = 0u16;
    let mut usage = 0u16;
    let mut gamepad = false;
    while i < desc.len() {
        let prefix = desc[i];
        if prefix == 0xFE {
            return Err("the report descriptor was damaged".into());
        }
        let size = match prefix & 0x03 {
            0 => 0,
            1 => 1,
            2 => 2,
            _ => 4,
        };
        let data_at = i + 1;
        if data_at + size > desc.len() {
            return Err("the report descriptor was damaged".into());
        }
        let value = match size {
            0 => 0u32,
            1 => u32::from(desc[data_at]),
            2 => u32::from(u16::from_le_bytes([desc[data_at], desc[data_at + 1]])),
            _ => u32::from_le_bytes([
                desc[data_at],
                desc[data_at + 1],
                desc[data_at + 2],
                desc[data_at + 3],
            ]),
        };
        let kind = (prefix >> 2) & 0x03;
        let tag = prefix >> 4;
        if kind == 1 && tag == 0 {
            page = value as u16;
        } else if kind == 2 && tag == 0 {
            usage = value as u16;
        } else if kind == 0
            && tag == 0x0A
            && value == 1
            && page == 0x01
            && matches!(usage, 0x04 | 0x05)
        {
            gamepad = true;
        }
        i = data_at + size;
    }
    Ok(gamepad)
}

fn check_report(report: &[u8], id: u8, size: usize, crc: bool, seed: u8) -> Result<(), String> {
    if report.first().copied() != Some(id) {
        return Err("the report has the wrong id".into());
    }
    if report.len() < size {
        return Err("the report is too short".into());
    }
    if crc {
        let body = &report[..size - 4];
        let found = u32::from_le_bytes(report[size - 4..size].try_into().unwrap_or([0; 4]));
        if report_crc(seed, body) != found {
            return Err("the report was damaged".into());
        }
    }
    Ok(())
}

fn stamp_crc(buf: &mut [u8], seed: u8) {
    if buf.len() < 4 {
        return;
    }
    let n = buf.len() - 4;
    let crc = report_crc(seed, &buf[..n]);
    buf[n..].copy_from_slice(&crc.to_le_bytes());
}

fn i16_at(bytes: &[u8], at: usize) -> i16 {
    bytes.get(at..at + 2).and_then(|s| s.try_into().ok()).map(i16::from_le_bytes).unwrap_or(0)
}

fn byte_channel(v: f32) -> u8 {
    if !v.is_finite() {
        return 0;
    }
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn mul_div(numer: i32, value: i32, denom: i32) -> i64 {
    if denom == 0 {
        return 0;
    }
    i64::from(numer) * i64::from(value) / i64::from(denom)
}

fn finite3(v: [f32; 3]) -> [f32; 3] {
    v.map(|n| if n.is_finite() { n } else { 0.0 })
}
