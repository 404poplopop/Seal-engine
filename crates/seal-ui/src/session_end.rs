//! Save before Windows shuts down or signs the user out.
//!
//! The window library turns the X button, Alt+F4 and Task Manager's End task into a
//! close request, which the editor already answers by saving first. Shutdown and
//! sign-out skip that request, so on Windows the editor window is subclassed and
//! saves when Windows asks whether the session may end.

/// Run `save` when the operating system is about to end the session. `hwnd` is
/// the main window handle. Returns false when the window could not be watched;
/// always true outside Windows, where there is nothing to watch.
#[cfg(windows)]
pub fn install(hwnd: isize, save: Box<dyn FnMut()>) -> bool {
    imp::install(hwnd, save)
}

#[cfg(not(windows))]
pub fn install(_hwnd: isize, _save: Box<dyn FnMut()>) -> bool {
    true
}

#[cfg(windows)]
mod imp {
    use std::cell::RefCell;
    use std::ffi::c_void;

    type Hwnd = *mut c_void;
    type SubclassProc = unsafe extern "system" fn(Hwnd, u32, usize, isize, usize, usize) -> isize;

    #[link(name = "comctl32")]
    unsafe extern "system" {
        fn SetWindowSubclass(
            hwnd: Hwnd,
            proc_: Option<SubclassProc>,
            id: usize,
            data: usize,
        ) -> i32;
        fn DefSubclassProc(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> isize;
    }

    const WM_QUERYENDSESSION: u32 = 0x0011;
    const WM_ENDSESSION: u32 = 0x0016;
    const SUBCLASS_ID: usize = 0x5EA1;

    thread_local! {
        static SAVE: RefCell<Option<Box<dyn FnMut()>>> = RefCell::new(None);
    }

    fn save_now() {
        SAVE.with(|slot| {
            if let Ok(mut slot) = slot.try_borrow_mut() {
                if let Some(save) = slot.as_mut() {
                    save();
                }
            }
        });
    }

    unsafe extern "system" fn window_proc(
        hwnd: Hwnd,
        msg: u32,
        wparam: usize,
        lparam: isize,
        _id: usize,
        _data: usize,
    ) -> isize {
        match msg {
            WM_QUERYENDSESSION => {
                save_now();
                1
            }
            WM_ENDSESSION if wparam != 0 => {
                save_now();
                0
            }
            _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
        }
    }

    pub fn install(hwnd: isize, save: Box<dyn FnMut()>) -> bool {
        SAVE.with(|slot| *slot.borrow_mut() = Some(save));
        // SAFETY: `hwnd` is the live main window, created on this thread, and the
        // procedure stays valid for the life of the program.
        unsafe { SetWindowSubclass(hwnd as Hwnd, Some(window_proc), SUBCLASS_ID, 0) != 0 }
    }
}
