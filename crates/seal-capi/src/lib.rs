//! The C interface declared in `include/seal.h`.
//!
//! Every entry point checks its pointers, turns errors into a status code with
//! a per-thread message, and catches panics so they never unwind into C.

#![deny(unsafe_op_in_unsafe_fn)]

use std::cell::RefCell;
use std::ffi::{c_char, c_int, CStr};
use std::panic::{catch_unwind, AssertUnwindSafe};

use seal_ecs::{Entity, World};

pub const SEAL_ABI_VERSION: u32 = 1;
pub const SEAL_ENTITY_NONE: u64 = u64::MAX;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SealStatus {
    Ok = 0,
    Error = 1,
    BufferTooSmall = 2,
    InvalidArgument = 3,
    NoSuchEntity = 4,
    NotFound = 5,
    Panic = 6,
}

/// Opaque to C.
pub struct SealWorld {
    world: World,
}

thread_local! {
    static LAST_ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

fn set_error(message: impl Into<String>) {
    LAST_ERROR.with(|e| *e.borrow_mut() = message.into());
}

fn fail(status: SealStatus, message: impl Into<String>) -> SealStatus {
    set_error(message);
    status
}

/// Run `body`, turning a panic into `SealStatus::Panic`.
fn guard(body: impl FnOnce() -> SealStatus) -> SealStatus {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(status) => status,
        Err(payload) => {
            let text = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "panic".into());
            fail(SealStatus::Panic, format!("internal error: {text}"))
        }
    }
}

fn guard_value<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or_else(|_| {
        set_error("internal error");
        fallback
    })
}

/// # Safety
/// `ptr` is NULL or a NUL-terminated string valid for reads.
unsafe fn text<'a>(ptr: *const c_char, what: &str) -> Result<&'a str, SealStatus> {
    if ptr.is_null() {
        return Err(fail(SealStatus::InvalidArgument, format!("{what} is NULL")));
    }
    // SAFETY: caller contract.
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|_| fail(SealStatus::InvalidArgument, format!("{what} is not UTF-8")))
}

/// Copy `out` into the caller's buffer with a trailing NUL.
///
/// # Safety
/// `buf` is NULL or valid for `cap` writes; `len_out` is NULL or writable.
unsafe fn write_out(out: &str, buf: *mut c_char, cap: usize, len_out: *mut usize) -> SealStatus {
    if !len_out.is_null() {
        // SAFETY: caller contract.
        unsafe { *len_out = out.len() };
    }
    if buf.is_null() || cap < out.len() + 1 {
        return fail(SealStatus::BufferTooSmall, format!("needs {} bytes", out.len() + 1));
    }
    // SAFETY: cap >= len + 1 checked above; regions cannot overlap a Rust String.
    unsafe {
        std::ptr::copy_nonoverlapping(out.as_ptr(), buf.cast::<u8>(), out.len());
        *buf.add(out.len()) = 0;
    }
    SealStatus::Ok
}

/// # Safety
/// `world` is NULL or came from `seal_world_new` and was not freed.
unsafe fn world_ref<'a>(world: *const SealWorld) -> Result<&'a World, SealStatus> {
    // SAFETY: caller contract.
    unsafe { world.as_ref() }
        .map(|w| &w.world)
        .ok_or_else(|| fail(SealStatus::InvalidArgument, "world is NULL"))
}

/// # Safety
/// As `world_ref`, and no other reference to the world is live.
unsafe fn world_mut<'a>(world: *mut SealWorld) -> Result<&'a mut World, SealStatus> {
    // SAFETY: caller contract.
    unsafe { world.as_mut() }
        .map(|w| &mut w.world)
        .ok_or_else(|| fail(SealStatus::InvalidArgument, "world is NULL"))
}

fn live(world: &World, bits: u64) -> Result<Entity, SealStatus> {
    let entity = Entity::from_bits(bits);
    if bits == SEAL_ENTITY_NONE || !world.contains(entity) {
        return Err(fail(SealStatus::NoSuchEntity, format!("no entity {entity:?}")));
    }
    Ok(entity)
}

macro_rules! tri {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(status) => return status,
        }
    };
}

#[no_mangle]
pub extern "C" fn seal_abi_version() -> u32 {
    SEAL_ABI_VERSION
}

/// # Safety
/// `buf` is NULL or valid for `cap` writes.
#[no_mangle]
pub unsafe extern "C" fn seal_last_error(buf: *mut c_char, cap: usize) -> usize {
    let message = LAST_ERROR.with(|e| e.borrow().clone());
    if !buf.is_null() && cap > 0 {
        let n = message.len().min(cap - 1);
        // SAFETY: n < cap.
        unsafe {
            std::ptr::copy_nonoverlapping(message.as_ptr(), buf.cast::<u8>(), n);
            *buf.add(n) = 0;
        }
    }
    message.len()
}

#[no_mangle]
pub extern "C" fn seal_world_new() -> *mut SealWorld {
    guard_value(std::ptr::null_mut(), || {
        let mut world = World::new();
        seal_scene::register_scene(&mut world);
        Box::into_raw(Box::new(SealWorld { world }))
    })
}

/// # Safety
/// `world` is NULL or came from `seal_world_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn seal_world_free(world: *mut SealWorld) {
    if !world.is_null() {
        // SAFETY: caller contract; ownership returns to Rust.
        let boxed = unsafe { Box::from_raw(world) };
        let _ = catch_unwind(AssertUnwindSafe(move || drop(boxed)));
    }
}

/// # Safety
/// See `seal.h`.
#[no_mangle]
pub unsafe extern "C" fn seal_world_entity_count(world: *const SealWorld) -> usize {
    // SAFETY: caller contract.
    unsafe { world.as_ref() }.map_or(0, |w| w.world.len())
}

/// # Safety
/// See `seal.h`.
#[no_mangle]
pub unsafe extern "C" fn seal_entity_spawn(world: *mut SealWorld) -> u64 {
    // SAFETY: caller contract.
    let Ok(w) = (unsafe { world_mut(world) }) else { return SEAL_ENTITY_NONE };
    guard_value(SEAL_ENTITY_NONE, || w.spawn(()).to_bits())
}

/// # Safety
/// See `seal.h`.
#[no_mangle]
pub unsafe extern "C" fn seal_entity_despawn(world: *mut SealWorld, entity: u64) -> SealStatus {
    // SAFETY: caller contract.
    let w = tri!(unsafe { world_mut(world) });
    guard(|| {
        let e = tri!(live(w, entity));
        w.despawn(e);
        SealStatus::Ok
    })
}

/// # Safety
/// See `seal.h`.
#[no_mangle]
pub unsafe extern "C" fn seal_entity_alive(world: *const SealWorld, entity: u64) -> c_int {
    // SAFETY: caller contract.
    unsafe { world.as_ref() }
        .is_some_and(|w| entity != SEAL_ENTITY_NONE && w.world.contains(Entity::from_bits(entity)))
        as c_int
}

/// # Safety
/// See `seal.h`.
#[no_mangle]
pub unsafe extern "C" fn seal_entity_set_parent(
    world: *mut SealWorld,
    child: u64,
    parent: u64,
) -> SealStatus {
    // SAFETY: caller contract.
    let w = tri!(unsafe { world_mut(world) });
    guard(|| {
        let child = tri!(live(w, child));
        let parent = if parent == SEAL_ENTITY_NONE { None } else { Some(tri!(live(w, parent))) };
        match w.set_parent(child, parent) {
            Ok(()) => SealStatus::Ok,
            Err(e) => fail(SealStatus::Error, format!("{e:?}")),
        }
    })
}

/// # Safety
/// See `seal.h`.
#[no_mangle]
pub unsafe extern "C" fn seal_entity_parent(world: *const SealWorld, child: u64) -> u64 {
    // SAFETY: caller contract.
    let Some(w) = (unsafe { world.as_ref() }) else { return SEAL_ENTITY_NONE };
    if child == SEAL_ENTITY_NONE {
        return SEAL_ENTITY_NONE;
    }
    w.world.parent(Entity::from_bits(child)).map_or(SEAL_ENTITY_NONE, Entity::to_bits)
}

/// # Safety
/// See `seal.h`.
#[no_mangle]
pub unsafe extern "C" fn seal_component_set(
    world: *mut SealWorld,
    entity: u64,
    component: *const c_char,
    sealtext: *const c_char,
) -> SealStatus {
    // SAFETY: caller contract for all three pointers.
    let w = tri!(unsafe { world_mut(world) });
    let name = tri!(unsafe { text(component, "component") });
    let source = tri!(unsafe { text(sealtext, "value") });
    guard(|| {
        let e = tri!(live(w, entity));
        if w.component_fields(name).is_none() {
            return fail(SealStatus::NotFound, format!("unknown component {name}"));
        }
        let value = match seal_sealtext::parse(source) {
            Ok(v) => v,
            Err(err) => return fail(SealStatus::Error, format!("{name}: {err}")),
        };
        match w.set_component_value(e, name, &value) {
            Ok(()) => SealStatus::Ok,
            Err(err) => fail(SealStatus::Error, err),
        }
    })
}

/// # Safety
/// See `seal.h`.
#[no_mangle]
pub unsafe extern "C" fn seal_component_get(
    world: *const SealWorld,
    entity: u64,
    component: *const c_char,
    buf: *mut c_char,
    cap: usize,
    len_out: *mut usize,
) -> SealStatus {
    // SAFETY: caller contract.
    let w = tri!(unsafe { world_ref(world) });
    let name = tri!(unsafe { text(component, "component") });
    guard(|| {
        let e = tri!(live(w, entity));
        let Some(value) = w.component_value(e, name) else {
            return fail(SealStatus::NotFound, format!("{e:?} has no {name}"));
        };
        // SAFETY: caller contract.
        unsafe { write_out(value.to_text().trim_end(), buf, cap, len_out) }
    })
}

/// # Safety
/// See `seal.h`.
#[no_mangle]
pub unsafe extern "C" fn seal_component_remove(
    world: *mut SealWorld,
    entity: u64,
    component: *const c_char,
) -> SealStatus {
    // SAFETY: caller contract.
    let w = tri!(unsafe { world_mut(world) });
    let name = tri!(unsafe { text(component, "component") });
    guard(|| {
        let e = tri!(live(w, entity));
        match w.remove_component_named(e, name) {
            Ok(true) => SealStatus::Ok,
            Ok(false) => fail(SealStatus::NotFound, format!("{e:?} has no {name}")),
            Err(err) => fail(SealStatus::NotFound, err),
        }
    })
}

/// # Safety
/// See `seal.h`.
#[no_mangle]
pub unsafe extern "C" fn seal_scene_save(
    world: *const SealWorld,
    buf: *mut c_char,
    cap: usize,
    len_out: *mut usize,
) -> SealStatus {
    // SAFETY: caller contract.
    let w = tri!(unsafe { world_ref(world) });
    guard(|| {
        let saved = w.save_scene(None);
        // SAFETY: caller contract.
        unsafe { write_out(&saved.text, buf, cap, len_out) }
    })
}

/// # Safety
/// See `seal.h`.
#[no_mangle]
pub unsafe extern "C" fn seal_scene_load(
    world: *mut SealWorld,
    sealtext: *const c_char,
) -> SealStatus {
    // SAFETY: caller contract.
    let w = tri!(unsafe { world_mut(world) });
    let source = tri!(unsafe { text(sealtext, "scene") });
    guard(|| match w.load_scene(source) {
        Ok(_) => SealStatus::Ok,
        Err(err) => fail(SealStatus::Error, err),
    })
}
