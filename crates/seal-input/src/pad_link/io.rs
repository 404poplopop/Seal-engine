//! Platform open/read/write for one raw game pad.

use super::PadFamily;
use crate::pad_reports::{
    pro_enable_imu, pro_read_factory_imu, pro_report_mode, sony_calibration, ProScale, SonyScale,
    Wire,
};

#[derive(Clone)]
pub struct Listed {
    pub vendor: u16,
    pub product: u16,
    pub key: String,
    pub wire: Wire,
    pub input_len: usize,
    pub feature_len: usize,
}

pub struct Session {
    pub dev: Device,
    pub sony: Option<SonyScale>,
    pub pro: Option<ProScale>,
    pub warn: Option<String>,
}

pub enum Fail {
    Wrong,
    Fatal(String),
}

#[cfg(windows)]
pub struct Win {
    handle: windows_sys::Win32::Foundation::HANDLE,
    event: windows_sys::Win32::Foundation::HANDLE,
    overlapped: windows_sys::Win32::System::IO::OVERLAPPED,
    buf: Vec<u8>,
    pending: bool,
    ready: Vec<Vec<u8>>,
}

pub struct Device {
    alive: bool,
    #[cfg(target_os = "linux")]
    fd: i32,
    #[cfg(target_os = "linux")]
    buf_len: usize,
    #[cfg(windows)]
    win: Box<Win>,
}

pub fn open(listed: &Listed, family: PadFamily) -> Result<Session, Fail> {
    let dev = Device::connect(listed)?;
    if family == PadFamily::Pro {
        return finish_pro(dev, listed.wire);
    }
    let (id, want) = if family == PadFamily::Ds4 && listed.wire == Wire::Usb {
        (0x02u8, 37usize)
    } else {
        (0x05, 41)
    };
    let len = if listed.feature_len > want && listed.feature_len <= 256 {
        listed.feature_len
    } else {
        want
    };
    let buf = dev.feature(id, len)?;
    let sony = sony_calibration(family, listed.wire, &buf).unwrap_or_else(|_| SonyScale::nominal());
    Ok(Session { dev, sony: Some(sony), pro: None, warn: None })
}

fn finish_pro(mut dev: Device, wire: Wire) -> Result<Session, Fail> {
    let usb = matches!(wire, Wire::Usb);
    let mut warn = None;
    let _ = dev.write(&pro_read_factory_imu(0, usb));
    let mut pro = ProScale::defaults();
    for _ in 0..8 {
        let reports = dev.drain(10).map_err(Fail::Fatal)?;
        let mut found = false;
        for report in reports {
            if let Ok(scale) = crate::pad_reports::parse_pro_reply(&report) {
                pro = scale;
                found = true;
            }
        }
        if found {
            break;
        }
    }
    if dev.write(&pro_enable_imu(1, usb)).is_err() || dev.write(&pro_report_mode(2, usb)).is_err() {
        warn = Some("the gamepad motion sensors could not be enabled".to_string());
    }
    Ok(Session { dev, sony: None, pro: Some(pro), warn })
}

#[cfg(target_os = "linux")]
mod sys {
    use super::{Device, Fail, Listed};
    use crate::pad_reports::{descriptor_is_gamepad, pad_family, Wire};

    impl Device {
        pub fn connect(listed: &Listed) -> Result<Self, Fail> {
            let buf_len =
                if listed.input_len == 0 { 256 } else { listed.input_len.clamp(64, 1024) };
            Ok(Self { alive: true, fd: open_node(&listed.key, true)?, buf_len })
        }

        pub fn alive(&self) -> bool {
            self.alive
        }

        pub fn feature(&self, id: u8, len: usize) -> Result<Vec<u8>, Fail> {
            if len == 0 || len > 256 {
                return Err(Fail::Wrong);
            }
            let mut buf = vec![0u8; len];
            buf[0] = id;
            let req = ioc(3, 0x07, len as u32);
            let rc = unsafe { libc::ioctl(self.fd, req as libc::Ioctl, buf.as_mut_ptr()) };
            if rc < 0 {
                return Err(fail_errno());
            }
            if buf[0] != id {
                return Err(Fail::Wrong);
            }
            Ok(buf)
        }

        pub fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
            let mut off = 0;
            let mut spins = 0;
            while off < bytes.len() {
                let n = unsafe {
                    libc::write(
                        self.fd,
                        bytes[off..].as_ptr().cast::<libc::c_void>(),
                        bytes.len() - off,
                    )
                };
                if n < 0 {
                    let err = errno();
                    if err == libc::EINTR {
                        continue;
                    }
                    if err == libc::EAGAIN && spins < 5 {
                        spins += 1;
                        std::thread::sleep(std::time::Duration::from_millis(1));
                        continue;
                    }
                    if err == libc::ENODEV || err == libc::EIO || err == libc::EBADF {
                        self.alive = false;
                    }
                    return Err(format!("the gamepad output report was not sent ({err})"));
                }
                if n == 0 {
                    return Err("the gamepad output report was not sent".into());
                }
                off += n as usize;
            }
            Ok(())
        }

        pub fn drain(&mut self, wait_ms: u32) -> Result<Vec<Vec<u8>>, String> {
            let mut out = self.read_some()?;
            if out.is_empty() && wait_ms > 0 {
                std::thread::sleep(std::time::Duration::from_millis(u64::from(wait_ms)));
                out = self.read_some()?;
            }
            Ok(out)
        }

        fn read_some(&mut self) -> Result<Vec<Vec<u8>>, String> {
            let mut out = Vec::new();
            let mut buf = vec![0u8; self.buf_len];
            loop {
                let n = unsafe {
                    libc::read(self.fd, buf.as_mut_ptr().cast::<libc::c_void>(), buf.len())
                };
                if n < 0 {
                    let err = errno();
                    if err == libc::EINTR {
                        continue;
                    }
                    if err == libc::EAGAIN || err == libc::EWOULDBLOCK {
                        break;
                    }
                    self.alive = false;
                    return Err(format!("the gamepad was disconnected ({err})"));
                }
                if n == 0 {
                    self.alive = false;
                    return Err("the gamepad was disconnected".into());
                }
                out.push(buf[..n as usize].to_vec());
                if out.len() == 8 {
                    break;
                }
            }
            Ok(out)
        }
    }

    impl Drop for Device {
        fn drop(&mut self) {
            unsafe { libc::close(self.fd) };
        }
    }

    pub fn list() -> Result<Vec<Listed>, String> {
        let dir =
            std::fs::read_dir("/dev").map_err(|_| "gamepads could not be listed".to_string())?;
        let mut names = Vec::new();
        for entry in dir.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with("hidraw") {
                names.push(name.to_string());
            }
        }
        names.sort_by_key(|name| {
            name.trim_start_matches("hidraw").parse::<u32>().unwrap_or(u32::MAX)
        });
        let mut out = Vec::new();
        for name in names {
            let key = format!("/dev/{name}");
            let Ok(fd) = open_node(&key, false) else { continue };
            let listed = read_info(fd, key);
            unsafe { libc::close(fd) };
            if let Some(listed) = listed {
                out.push(listed);
            }
        }
        Ok(out)
    }

    fn read_info(fd: i32, key: String) -> Option<Listed> {
        let mut info = RawInfo { bustype: 0, vendor: 0, product: 0 };
        let req = ioc(2, 0x03, std::mem::size_of::<RawInfo>() as u32);
        let rc = unsafe { libc::ioctl(fd, req as libc::Ioctl, &mut info) };
        if rc < 0 {
            return None;
        }
        let vendor = info.vendor as u16;
        let product = info.product as u16;
        if vendor == 0 {
            return None;
        }
        if pad_family(vendor, product).is_none() && !descriptor_gamepad(fd) {
            return None;
        }
        let wire = if info.bustype == 5 { Wire::Bluetooth } else { Wire::Usb };
        Some(Listed { vendor, product, key, wire, input_len: 0, feature_len: 0 })
    }

    fn descriptor_gamepad(fd: i32) -> bool {
        let mut size = 0i32;
        let req = ioc(2, 0x01, std::mem::size_of::<i32>() as u32);
        if unsafe { libc::ioctl(fd, req as libc::Ioctl, &mut size) } < 0 || size <= 0 {
            return false;
        }
        let mut desc = Box::new(ReportDesc { size: size as u32, value: [0; 4096] });
        let req = ioc(2, 0x02, std::mem::size_of::<ReportDesc>() as u32);
        if unsafe { libc::ioctl(fd, req as libc::Ioctl, desc.as_mut()) } < 0 {
            return false;
        }
        let n = (desc.size as usize).min(4096);
        descriptor_is_gamepad(&desc.value[..n]).unwrap_or(false)
    }

    fn open_node(path: &str, write: bool) -> Result<i32, Fail> {
        let Ok(path) = std::ffi::CString::new(path) else {
            return Err(Fail::Fatal("the gamepad path is not usable".into()));
        };
        let flag = |write| {
            libc::O_CLOEXEC | libc::O_NONBLOCK | if write { libc::O_RDWR } else { libc::O_RDONLY }
        };
        let fd = unsafe { libc::open(path.as_ptr(), flag(write)) };
        if fd >= 0 {
            return Ok(fd);
        }
        let err = errno();
        if write && (err == libc::EACCES || err == libc::EPERM) {
            let fd = unsafe { libc::open(path.as_ptr(), flag(false)) };
            if fd >= 0 {
                return Ok(fd);
            }
        }
        Err(fail_errno())
    }

    fn fail_errno() -> Fail {
        let err = errno();
        if err == libc::EACCES || err == libc::EPERM {
            Fail::Fatal(format!("the gamepad could not be opened ({err})"))
        } else if err == libc::ENODEV || err == libc::ENOENT {
            Fail::Fatal(format!("the gamepad was disconnected ({err})"))
        } else {
            Fail::Wrong
        }
    }

    fn errno() -> i32 {
        unsafe { *libc::__errno_location() }
    }

    fn ioc(dir: u32, nr: u32, size: u32) -> u64 {
        ((dir as u64) << 30) | ((size as u64) << 16) | ((b'H' as u64) << 8) | u64::from(nr)
    }

    #[repr(C)]
    struct RawInfo {
        bustype: u32,
        vendor: i16,
        product: i16,
    }

    #[repr(C)]
    struct ReportDesc {
        size: u32,
        value: [u8; 4096],
    }
}

#[cfg(windows)]
mod sys {
    use super::{Device, Fail, Listed, Win};
    use crate::pad_reports::{pad_family, Wire};
    use std::mem::size_of;
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
        SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
        SP_DEVICE_INTERFACE_DATA,
    };
    use windows_sys::Win32::Devices::HumanInterfaceDevice::{
        HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetFeature, HidD_GetPreparsedData,
        HidD_SetOutputReport, HidP_GetCaps, GUID_DEVINTERFACE_HID, HIDD_ATTRIBUTES, HIDP_CAPS,
        HIDP_STATUS_SUCCESS, PHIDP_PREPARSED_DATA,
    };
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_IO_PENDING, GENERIC_READ, GENERIC_WRITE, HANDLE,
        INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_OVERLAPPED,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
    use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};

    impl Device {
        pub fn connect(listed: &Listed) -> Result<Self, Fail> {
            let handle = open_path(&listed.key, true, true)?;
            let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
            if event.is_null() {
                unsafe { CloseHandle(handle) };
                return Err(Fail::Fatal(win_err("the gamepad could not be opened")));
            }
            let len = listed.input_len.clamp(64, 1024);
            Ok(Self {
                alive: true,
                win: Box::new(Win {
                    handle,
                    event,
                    overlapped: OVERLAPPED::default(),
                    buf: vec![0; len],
                    pending: false,
                    ready: Vec::new(),
                }),
            })
        }

        pub fn alive(&self) -> bool {
            self.alive
        }

        pub fn feature(&self, id: u8, len: usize) -> Result<Vec<u8>, Fail> {
            if len == 0 || len > 512 {
                return Err(Fail::Wrong);
            }
            let mut buf = vec![0u8; len];
            buf[0] = id;
            let ok = unsafe {
                HidD_GetFeature(self.win.handle, buf.as_mut_ptr().cast(), buf.len() as u32)
            };
            if !ok || buf[0] != id {
                return Err(Fail::Wrong);
            }
            Ok(buf)
        }

        pub fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
            self.pause();
            let ok = unsafe {
                HidD_SetOutputReport(self.win.handle, bytes.as_ptr().cast(), bytes.len() as u32)
            };
            let result = if ok { Ok(()) } else { self.write_overlapped(bytes) };
            let _ = self.arm();
            result
        }

        pub fn drain(&mut self, wait_ms: u32) -> Result<Vec<Vec<u8>>, String> {
            let mut out = Vec::new();
            out.append(&mut self.win.ready);
            let mut waited = false;
            loop {
                if !self.win.pending {
                    self.arm()?;
                    if !self.win.ready.is_empty() {
                        out.append(&mut self.win.ready);
                        if out.len() >= 8 {
                            break;
                        }
                        continue;
                    }
                }
                if !self.win.pending {
                    break;
                }
                let timeout = if out.is_empty() && !waited { wait_ms } else { 0 };
                waited = true;
                let woke = unsafe { WaitForSingleObject(self.win.event, timeout) };
                if woke != WAIT_OBJECT_0 {
                    break;
                }
                let mut n = 0u32;
                let ok = unsafe {
                    GetOverlappedResult(self.win.handle, &self.win.overlapped, &mut n, 0)
                };
                self.win.pending = false;
                if ok == 0 {
                    self.alive = false;
                    return Err(win_err("the gamepad was disconnected"));
                }
                if n > 0 {
                    out.push(self.win.buf[..n as usize].to_vec());
                }
                if out.len() >= 8 {
                    break;
                }
            }
            if !self.win.pending {
                let _ = self.arm();
            }
            Ok(out)
        }

        fn write_overlapped(&mut self, bytes: &[u8]) -> Result<(), String> {
            self.win.overlapped = OVERLAPPED::default();
            self.win.overlapped.hEvent = self.win.event;
            let mut n = 0u32;
            let ok = unsafe {
                WriteFile(
                    self.win.handle,
                    bytes.as_ptr(),
                    bytes.len() as u32,
                    &mut n,
                    &mut self.win.overlapped,
                )
            };
            if ok == 0 {
                let err = unsafe { GetLastError() };
                if err != ERROR_IO_PENDING {
                    return Err(win_err("the gamepad output report was not sent"));
                }
                let woke = unsafe { WaitForSingleObject(self.win.event, 50) };
                if woke != WAIT_OBJECT_0 {
                    unsafe { CancelIoEx(self.win.handle, &self.win.overlapped) };
                    return Err("the gamepad output report was not sent".into());
                }
                let ok = unsafe {
                    GetOverlappedResult(self.win.handle, &self.win.overlapped, &mut n, 0)
                };
                if ok == 0 {
                    return Err(win_err("the gamepad output report was not sent"));
                }
            }
            Ok(())
        }

        fn pause(&mut self) {
            if self.win.pending {
                unsafe {
                    CancelIoEx(self.win.handle, &self.win.overlapped);
                    WaitForSingleObject(self.win.event, 20);
                }
                self.win.pending = false;
            }
        }

        fn arm(&mut self) -> Result<(), String> {
            self.win.overlapped = OVERLAPPED::default();
            self.win.overlapped.hEvent = self.win.event;
            let mut n = 0u32;
            let ok = unsafe {
                ReadFile(
                    self.win.handle,
                    self.win.buf.as_mut_ptr(),
                    self.win.buf.len() as u32,
                    &mut n,
                    &mut self.win.overlapped,
                )
            };
            if ok != 0 {
                if n > 0 {
                    self.win.ready.push(self.win.buf[..n as usize].to_vec());
                }
                self.win.pending = false;
                return Ok(());
            }
            let err = unsafe { GetLastError() };
            if err == ERROR_IO_PENDING {
                self.win.pending = true;
                return Ok(());
            }
            self.alive = false;
            Err(win_err("the gamepad was disconnected"))
        }
    }

    impl Drop for Device {
        fn drop(&mut self) {
            self.pause();
            unsafe {
                CloseHandle(self.win.event);
                CloseHandle(self.win.handle);
            }
        }
    }

    pub fn list() -> Result<Vec<Listed>, String> {
        let set = unsafe {
            SetupDiGetClassDevsW(
                &GUID_DEVINTERFACE_HID,
                std::ptr::null(),
                std::ptr::null_mut(),
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            )
        };
        if set == INVALID_HANDLE_VALUE as isize {
            return Err(win_err("gamepads could not be listed"));
        }
        let mut out = Vec::new();
        for index in 0..128u32 {
            let mut data = SP_DEVICE_INTERFACE_DATA {
                cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
                ..Default::default()
            };
            let ok = unsafe {
                SetupDiEnumDeviceInterfaces(
                    set,
                    std::ptr::null(),
                    &GUID_DEVINTERFACE_HID,
                    index,
                    &mut data,
                )
            };
            if ok == 0 {
                break;
            }
            if let Some(path) = interface_path(set, &data) {
                if let Some(listed) = describe(&path) {
                    out.push(listed);
                }
            }
        }
        unsafe { SetupDiDestroyDeviceInfoList(set) };
        out.sort_by(|a, b| a.key.cmp(&b.key));
        Ok(out)
    }

    fn interface_path(set: isize, data: &SP_DEVICE_INTERFACE_DATA) -> Option<String> {
        let mut needed = 0u32;
        unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                set,
                data,
                std::ptr::null_mut(),
                0,
                &mut needed,
                std::ptr::null_mut(),
            );
        }
        if needed < 8 || needed > 16 * 1024 {
            return None;
        }
        let cb: u32 = if cfg!(target_pointer_width = "64") { 8 } else { 6 };
        let mut raw = vec![0u64; (needed as usize + 7) / 8];
        unsafe {
            raw.as_mut_ptr().cast::<u32>().write(cb);
            let ok = SetupDiGetDeviceInterfaceDetailW(
                set,
                data,
                raw.as_mut_ptr().cast(),
                needed,
                &mut needed,
                std::ptr::null_mut(),
            );
            if ok == 0 {
                return None;
            }
            let cap = raw.len() * 8;
            let n = (needed as usize).min(cap);
            let bytes = std::slice::from_raw_parts(raw.as_ptr().cast::<u8>(), n);
            let mut units = Vec::new();
            let mut i = cb as usize;
            while i + 1 < bytes.len() {
                let unit = u16::from_le_bytes([bytes[i], bytes[i + 1]]);
                if unit == 0 {
                    break;
                }
                units.push(unit);
                i += 2;
            }
            Some(String::from_utf16_lossy(&units))
        }
    }

    fn describe(path: &str) -> Option<Listed> {
        let handle = open_path(path, false, false).ok()?;
        let mut attr = HIDD_ATTRIBUTES {
            Size: size_of::<HIDD_ATTRIBUTES>() as u32,
            VendorID: 0,
            ProductID: 0,
            VersionNumber: 0,
        };
        let ok = unsafe { HidD_GetAttributes(handle, &mut attr) };
        if !ok || attr.VendorID == 0 {
            unsafe { CloseHandle(handle) };
            return None;
        }
        let mut usage_page = 0u16;
        let mut usage = 0u16;
        let mut input_len = 0usize;
        let mut feature_len = 0usize;
        unsafe {
            let mut pre: PHIDP_PREPARSED_DATA = 0;
            if HidD_GetPreparsedData(handle, &mut pre) && pre != 0 {
                let mut caps = HIDP_CAPS::default();
                if HidP_GetCaps(pre, &mut caps) == HIDP_STATUS_SUCCESS {
                    usage_page = caps.UsagePage;
                    usage = caps.Usage;
                    input_len = usize::from(caps.InputReportByteLength);
                    feature_len = usize::from(caps.FeatureReportByteLength);
                }
                HidD_FreePreparsedData(pre);
            }
            CloseHandle(handle);
        }
        let known = pad_family(attr.VendorID, attr.ProductID).is_some();
        let gamepad = usage_page == 0x01 && matches!(usage, 0x04 | 0x05);
        if !known && !gamepad {
            return None;
        }
        let upper = path.to_ascii_uppercase();
        let wire = if upper.contains("BTHENUM") || upper.contains("BTHLE") {
            Wire::Bluetooth
        } else {
            Wire::Usb
        };
        Some(Listed {
            vendor: attr.VendorID,
            product: attr.ProductID,
            key: path.to_string(),
            wire,
            input_len,
            feature_len,
        })
    }

    fn open_path(path: &str, write: bool, overlapped: bool) -> Result<HANDLE, Fail> {
        let mut wide: Vec<u16> = path.encode_utf16().collect();
        wide.push(0);
        let flags = if overlapped {
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED
        } else {
            FILE_ATTRIBUTE_NORMAL
        };
        let try_open = |access| unsafe {
            CreateFileW(
                wide.as_ptr(),
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                flags,
                std::ptr::null_mut(),
            )
        };
        let handle = try_open(if write { GENERIC_READ | GENERIC_WRITE } else { GENERIC_READ });
        if handle != INVALID_HANDLE_VALUE {
            return Ok(handle);
        }
        if write {
            let handle = try_open(GENERIC_READ);
            if handle != INVALID_HANDLE_VALUE {
                return Ok(handle);
            }
        }
        Err(Fail::Fatal(win_err("the gamepad could not be opened")))
    }

    fn win_err(prefix: &str) -> String {
        let code = unsafe { GetLastError() };
        format!("{prefix} ({code})")
    }
}

#[cfg(target_os = "linux")]
pub use sys::list;
#[cfg(windows)]
pub use sys::list;
