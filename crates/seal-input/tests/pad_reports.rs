//! Light-bar reports, motion calibration, and which pad is player 1.

use std::f64::consts::PI;

use seal_input::{
    controller_notes, crc32_ieee, descriptor_is_gamepad, light_report, match_hid_index, pad_family,
    parse_pro_calibration, parse_pro_motion, parse_pro_reply, pick_interface, pro_enable_imu,
    pro_read_factory_imu, pro_report_mode, report_crc, resolve, sony_calibration, sony_motion,
    PadFamily, PadId, PadSensors, ProScale, Resolved, Wire, NOTE_NOT_FOUND, NOTE_NO_LIGHT,
    NOTE_NO_MOTION, NOTE_NO_PAD,
};

fn put_i16(buf: &mut [u8], at: usize, value: i16) {
    buf[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn near(got: f32, expect: f64) {
    assert!((f64::from(got) - expect).abs() < 1e-4, "{got} != {expect}");
}

#[test]
fn checksum_matches_the_known_answer_and_signs_bluetooth_reports() {
    assert_eq!(crc32_ieee(b"123456789"), 0xCBF4_3926);
    let red = light_report(PadFamily::Ds4, Wire::Bluetooth, [1.0, 0.0, 0.5], 0).unwrap();
    assert_eq!(red.len(), 78);
    assert_eq!(red[0], 0x11);
    assert_eq!(red[1], 0xC0);
    assert_eq!(red[3], 0x02);
    assert_eq!(&red[8..11], &[255, 0, 128]);
    let crc = report_crc(0xA2, &red[..74]);
    assert_eq!(&red[74..], &crc.to_le_bytes());
    let other = light_report(PadFamily::Ds4, Wire::Bluetooth, [0.0, 1.0, 0.0], 0).unwrap();
    assert_ne!(&red[74..], &other[74..]);

    let usb = light_report(PadFamily::Ds4, Wire::Usb, [1.0, 0.0, 0.5], 0).unwrap();
    assert_eq!(usb.len(), 32);
    assert_eq!(usb[0], 0x05);
    assert_eq!(usb[1], 0x02);
    assert_eq!(&usb[6..9], &[255, 0, 128]);
    assert!(usb[9..].iter().all(|byte| *byte == 0));

    let bar = light_report(PadFamily::DualSense, Wire::Usb, [1.0, 0.0, 0.5], 0).unwrap();
    assert_eq!(bar.len(), 63);
    assert_eq!(bar[0], 0x02);
    assert_eq!(bar[2], 0x04);
    assert_eq!(&bar[45..48], &[255, 0, 128]);

    let bt = light_report(PadFamily::DualSense, Wire::Bluetooth, [1.0, f32::NAN, -1.0], 3).unwrap();
    assert_eq!(bt.len(), 78);
    assert_eq!(bt[0], 0x31);
    assert_eq!(bt[1], 0x30);
    assert_eq!(bt[2], 0x10);
    assert_eq!(bt[4], 0x04);
    assert_eq!(&bt[47..50], &[255, 0, 0]);
    assert_eq!(&bt[74..], &report_crc(0xA2, &bt[..74]).to_le_bytes());
    assert!(light_report(PadFamily::Pro, Wire::Usb, [1.0, 0.0, 0.0], 0).is_err());
}

fn ds4_cal(wire: Wire, pitch_plus: i16, pitch_minus: i16) -> Vec<u8> {
    let len = if wire == Wire::Usb { 37 } else { 41 };
    let mut buf = vec![0u8; len];
    buf[0] = if wire == Wire::Usb { 0x02 } else { 0x05 };
    put_i16(&mut buf, 7, pitch_plus);
    if wire == Wire::Bluetooth {
        put_i16(&mut buf, 9, 111);
        put_i16(&mut buf, 11, 1000);
        put_i16(&mut buf, 13, pitch_minus);
        put_i16(&mut buf, 15, -1000);
        put_i16(&mut buf, 17, -1000);
    } else {
        put_i16(&mut buf, 9, pitch_minus);
        put_i16(&mut buf, 11, 1000);
        put_i16(&mut buf, 13, -1000);
        put_i16(&mut buf, 15, 1000);
        put_i16(&mut buf, 17, -1000);
    }
    put_i16(&mut buf, 19, 100);
    put_i16(&mut buf, 21, 100);
    for axis in 0..3 {
        put_i16(&mut buf, 23 + axis * 4, 8192);
        put_i16(&mut buf, 25 + axis * 4, -8192);
    }
    if wire == Wire::Bluetooth {
        let crc = report_crc(0xA3, &buf[..37]);
        buf[37..].copy_from_slice(&crc.to_le_bytes());
    }
    buf
}

fn expect_deg(raw: i16, plus: i16, minus: i16) -> f64 {
    let denom = (i32::from(plus) - 0).abs() + (i32::from(minus) - 0).abs();
    let counts = 200i64 * 1024 * i64::from(raw) / i64::from(denom);
    counts as f64 / 1024.0 * PI / 180.0
}

#[test]
fn sony_reports_apply_calibration_and_reject_a_damaged_one() {
    let cal =
        sony_calibration(PadFamily::Ds4, Wire::Usb, &ds4_cal(Wire::Usb, 1000, -1000)).unwrap();
    let mut input = vec![0u8; 64];
    input[0] = 0x01;
    put_i16(&mut input, 13, 500);
    put_i16(&mut input, 19, 8192);
    let (gyro, accel) = sony_motion(PadFamily::Ds4, Wire::Usb, &input, &cal).unwrap();
    near(gyro[0], expect_deg(500, 1000, -1000));
    assert_eq!(gyro[1], 0.0);
    assert_eq!(gyro[2], 0.0);
    near(accel[0], 1.0);
    assert_eq!(accel[1], 0.0);

    let bt_cal = ds4_cal(Wire::Bluetooth, 2000, -500);
    let scale = sony_calibration(PadFamily::Ds4, Wire::Bluetooth, &bt_cal).unwrap();
    let mut bt = vec![0u8; 78];
    bt[0] = 0x11;
    put_i16(&mut bt, 15, 2500);
    let crc = report_crc(0xA1, &bt[..74]);
    bt[74..].copy_from_slice(&crc.to_le_bytes());
    let (gyro, _) = sony_motion(PadFamily::Ds4, Wire::Bluetooth, &bt, &scale).unwrap();
    near(gyro[0], expect_deg(2500, 2000, -500));
    bt[20] ^= 0xFF;
    assert!(sony_motion(PadFamily::Ds4, Wire::Bluetooth, &bt, &scale).is_err());

    let mut broken = ds4_cal(Wire::Bluetooth, 2000, -500);
    broken[40] ^= 0xFF;
    assert!(sony_calibration(PadFamily::Ds4, Wire::Bluetooth, &broken).is_err());
    assert!(sony_calibration(PadFamily::Ds4, Wire::Usb, &bt_cal[..4]).is_err());

    let zero = sony_calibration(PadFamily::DualSense, Wire::Usb, &{
        let mut buf = vec![0u8; 41];
        buf[0] = 0x05;
        buf
    })
    .unwrap();
    let mut sense = vec![0u8; 64];
    sense[0] = 0x01;
    put_i16(&mut sense, 16, 32767);
    let (gyro, _) = sony_motion(PadFamily::DualSense, Wire::Usb, &sense, &zero).unwrap();
    let counts = 2048i64 * 1024 * 32767 / 32767;
    near(gyro[0], counts as f64 / 1024.0 * PI / 180.0);
    assert_eq!(gyro[1], 0.0);

    let mut wireless = vec![0u8; 78];
    wireless[0] = 0x31;
    put_i16(&mut wireless, 17, 500);
    let crc = report_crc(0xA1, &wireless[..74]);
    wireless[74..].copy_from_slice(&crc.to_le_bytes());
    let (gyro, _) = sony_motion(PadFamily::DualSense, Wire::Bluetooth, &wireless, &cal).unwrap();
    near(gyro[0], expect_deg(500, 1000, -1000));
}

#[test]
fn pro_controller_enables_sensors_and_scales_the_newest_sample() {
    let enable = pro_enable_imu(4, false);
    assert_eq!(enable.len(), 12);
    assert_eq!(enable[0], 0x01);
    assert_eq!(enable[1], 4);
    assert_eq!(&enable[2..10], &[0; 8]);
    assert_eq!(enable[10], 0x40);
    assert_eq!(enable[11], 0x01);
    let usb = pro_enable_imu(4, true);
    assert_eq!(usb.len(), 64);
    assert_eq!(&usb[..12], &enable[..]);
    assert!(usb[12..].iter().all(|byte| *byte == 0));
    let mode = pro_report_mode(1, false);
    assert_eq!(mode[10], 0x03);
    assert_eq!(mode[11], 0x30);
    let read = pro_read_factory_imu(2, false);
    assert_eq!(read[10], 0x10);
    assert_eq!(&read[11..16], &[0x20, 0x60, 0x00, 0x00, 24]);

    let mut blob = [0u8; 24];
    put_i16(&mut blob, 6, 8192);
    put_i16(&mut blob, 8, 8192);
    put_i16(&mut blob, 10, 8192);
    put_i16(&mut blob, 0, 100);
    put_i16(&mut blob, 12, 100);
    put_i16(&mut blob, 18, 13371);
    put_i16(&mut blob, 20, 13371);
    put_i16(&mut blob, 22, 13371);
    let scale = parse_pro_calibration(&blob).unwrap();
    assert!(parse_pro_calibration(&blob[..8]).is_err());
    assert_eq!(parse_pro_calibration(&[0; 24]).unwrap(), ProScale::defaults());

    let mut report = vec![0u8; 49];
    report[0] = 0x30;
    put_i16(&mut report, 13, 1);
    put_i16(&mut report, 37, 4096);
    put_i16(&mut report, 43, 14347);
    let (gyro, accel) = parse_pro_motion(&report, &scale).unwrap();
    let accel_div = 8192 - 100;
    let accel_counts = 4096i64 * 8192 / i64::from(accel_div);
    near(accel[0], accel_counts as f64 / 4096.0);
    let gyro_div = 13371 - 100;
    let gyro_counts = 1000i64 * (14347 - 100) * 13371 / i64::from(gyro_div);
    near(gyro[0], gyro_counts as f64 / 14247.0 * PI / 180.0);
    assert_eq!(gyro[1], 0.0);
    assert!(parse_pro_motion(&report[..10], &scale).is_err());

    let mut reply = vec![0u8; 44];
    reply[0] = 0x21;
    reply[13] = 0x80;
    reply[14] = 0x10;
    reply[20..44].copy_from_slice(&blob);
    assert_eq!(parse_pro_reply(&reply).unwrap().accel_offset[0], 100);
    reply[13] = 0;
    assert!(parse_pro_reply(&reply).is_err());
    assert!(parse_pro_reply(&reply[..8]).is_err());
}

#[test]
fn player_one_matches_vendor_product_and_order() {
    let players = [(0x054C, 0x09CC), (0x045E, 0x028E), (0x054C, 0x09CC)];
    let devices = [(0x045E, 0x028E), (0x054C, 0x09CC), (0x054C, 0x09CC)];
    assert_eq!(match_hid_index(&players, &devices, 0), Some(1));
    assert_eq!(match_hid_index(&players, &devices, 1), Some(0));
    assert_eq!(match_hid_index(&players, &devices, 2), Some(2));
    assert_eq!(match_hid_index(&players, &devices, 3), None);
    assert_eq!(pick_interface(&[false, true, true], 0), Some(1));
    assert_eq!(pick_interface(&[false, true, true], 1), Some(2));
    assert_eq!(pick_interface(&[false, false], 1), Some(1));
    assert_eq!(pick_interface(&[false], 1), None);

    assert_eq!(resolve(None, &[]), Resolved::NoPad);
    assert_eq!(resolve(None, &[(0x045E, 0x028E)]), Resolved::Unsupported);
    assert_eq!(
        resolve(None, &[(0x054C, 0x0CE6)]),
        Resolved::Open { family: PadFamily::DualSense, nth: 0, vendor: 0x054C, product: 0x0CE6 }
    );
    assert_eq!(resolve(Some(&players), &devices).family_vendor(), (PadFamily::Ds4, 0x054C, 0x09CC));
    assert_eq!(resolve(Some(&[(0x045E, 0x028E)]), &devices), Resolved::Unsupported);
    assert_eq!(resolve(Some(&[(0x054C, 0x05C4)]), &devices), Resolved::NotFound);
    assert_eq!(resolve(Some(&[]), &devices), Resolved::NoPad);
    assert_eq!(pad_family(0x057E, 0x2009), Some(PadFamily::Pro));
    assert_eq!(pad_family(0x054C, 0x0DF2), Some(PadFamily::DualSense));
    assert!(pad_family(0x045E, 0x028E).is_none());
}

trait FamilyVendor {
    fn family_vendor(self) -> (PadFamily, u16, u16);
}

impl FamilyVendor for Resolved {
    fn family_vendor(self) -> (PadFamily, u16, u16) {
        match self {
            Resolved::Open { family, vendor, product, .. } => (family, vendor, product),
            other => panic!("expected an open pad, got {other:?}"),
        }
    }
}

#[test]
fn notes_name_an_unsupported_pad_and_a_missing_one() {
    let (light, motion) = controller_notes(None, false);
    assert_eq!(light, NOTE_NO_PAD);
    assert_eq!(motion, NOTE_NO_PAD);
    let (light, motion) = controller_notes(None, true);
    assert_eq!(light, NOTE_NO_LIGHT);
    assert_eq!(motion, NOTE_NO_MOTION);
    let (light, motion) = controller_notes(Some(PadFamily::Pro), true);
    assert_eq!(light, NOTE_NO_LIGHT);
    assert!(motion.is_empty());
    let (light, motion) = controller_notes(Some(PadFamily::Ds4), true);
    assert!(light.is_empty() && motion.is_empty());

    let mut pads = PadSensors::new();
    pads.sync(Some(&[]));
    assert_eq!(pads.light_note(), NOTE_NO_PAD);
    assert_eq!(pads.motion().unsupported, NOTE_NO_PAD);
    assert!(pads.set_light([1.0, 0.0, 0.0]).is_err());

    pads.sync(Some(&[PadId { vendor: 0x045E, product: 0x02FF }]));
    assert_eq!(pads.light_note(), NOTE_NO_LIGHT);
    assert_eq!(pads.motion().unsupported, NOTE_NO_MOTION);
    assert_eq!(pads.set_light([0.0, 1.0, 0.0]).unwrap_err(), NOTE_NO_LIGHT);

    let missing = resolve(Some(&[(0x054C, 0x09CC)]), &[]);
    assert_eq!(missing, Resolved::NotFound);
    assert_eq!(NOTE_NOT_FOUND, "the gamepad was not found on the device bus");
}

#[test]
fn a_damaged_report_descriptor_is_rejected() {
    let gamepad = [0x05, 0x01, 0x09, 0x05, 0xA1, 0x01, 0xC0];
    assert_eq!(descriptor_is_gamepad(&gamepad).unwrap(), true);
    let keyboard = [0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0xC0];
    assert_eq!(descriptor_is_gamepad(&keyboard).unwrap(), false);
    assert_eq!(descriptor_is_gamepad(&[]).unwrap(), false);
    assert!(descriptor_is_gamepad(&[0x05]).is_err());
    assert!(descriptor_is_gamepad(&[0x06, 0x00]).is_err());
}
