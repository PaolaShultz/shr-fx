//! Additive single-owner prepared ABI. All objects and caller allocations must
//! be valid and disjoint; calls on an instance are serialized by its owner.
#![allow(clippy::missing_safety_doc)]
use crate::dsp::EmbeddedEffect;
use std::{
    ffi::c_void,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
};
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Settings {
    pub version: u32,
    pub size: u32,
    pub algorithm: u32,
    pub bypass: u32,
    pub time_ms: f64,
    pub amount: f64,
    pub damping: f64,
    pub rate_hz: f64,
    pub gain: f64,
    pub seed: u32,
    pub reserved: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Capabilities {
    pub version: u32,
    pub size: u32,
    pub algorithm_mask: u32,
    pub sample_bits: u32,
    pub min_rate: u32,
    pub max_rate: u32,
    pub max_block: u32,
    pub pending_capacity: u32,
    pub retired_capacity: u32,
    pub transition_frames_per_second: u32,
    pub max_prepared_bytes: u32,
    pub hardware_budget_available: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Status {
    pub version: u32,
    pub size: u32,
    pub rate: u32,
    pub max_block: u32,
    pub revision: u64,
    pub accepted_request: u64,
    pub applied_request: u64,
    pub last_request: u64,
    pub request_state: u32,
    pub request_result: i32,
    pub last_result: i32,
    pub fault: u32,
    pub transition: u32,
    pub intentional_delay_frames: u32,
    pub current: Settings,
    pub target: Settings,
}
struct Prepared {
    effect: EmbeddedEffect,
    settings: Settings,
    rate: u32,
    block: u32,
}
struct Instance {
    active: Box<Prepared>,
    pending: Option<Box<Prepared>>,
    retired: Option<Box<Prepared>>,
    status: Status,
    phase: u32,
}
fn overlaps(h: &Instance, start: usize, end: usize) -> bool {
    let a = h as *const Instance as usize;
    (start < a + size_of::<Instance>() && a < end)
        || [&*h.active]
            .into_iter()
            .chain(h.pending.as_deref())
            .chain(h.retired.as_deref())
            .any(|p| {
                let a = p as *const Prepared as usize;
                (start < a + size_of::<Prepared>() && a < end) || p.effect.overlaps(start, end)
            })
}
fn guarded(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-7)
}
fn shape<T>(p: *const T) -> bool {
    !p.is_null() && p.is_aligned() && (p as usize).checked_add(size_of::<T>()).is_some()
}
fn output<T>(p: *mut T, v: u32, n: u32) -> bool {
    v == 2 && n as usize == size_of::<T>() && shape(p)
}
fn valid(s: &Settings) -> bool {
    s.version == 2
        && s.size as usize == size_of::<Settings>()
        && s.bypass <= 1
        && s.reserved == 0
        && [s.time_ms, s.amount, s.damping, s.rate_hz, s.gain]
            .iter()
            .all(|x| x.is_finite())
        && (0.0..=1.0).contains(&s.gain)
        && match s.algorithm {
            1 => {
                (1.0..=2000.0).contains(&s.time_ms)
                    && (0.0..=0.9).contains(&s.amount)
                    && (0.0..=1.0).contains(&s.damping)
                    && s.rate_hz == 0.0
            }
            2 => {
                (0.0..=200.0).contains(&s.time_ms)
                    && (0.0..=0.9).contains(&s.amount)
                    && (0.0..=1.0).contains(&s.damping)
                    && s.rate_hz == 0.0
            }
            3 => {
                (1.0..=30.0).contains(&s.time_ms)
                    && (0.0..=9.0).contains(&s.amount)
                    && s.time_ms - s.amount >= 1.0
                    && s.time_ms + s.amount <= 39.0
                    && s.damping == 0.0
                    && (0.05..=10.0).contains(&s.rate_hz)
            }
            _ => false,
        }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_validate(s: *const Settings) -> i32 {
    guarded(|| {
        if !shape(s) {
            return -1;
        }
        if valid(unsafe { &*s }) { 0 } else { -6 }
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_defaults(a: u32, out: *mut Settings, v: u32, n: u32) -> i32 {
    guarded(|| {
        if !output(out, v, n) {
            return -1;
        }
        let (time_ms, amount, damping, rate_hz) = match a {
            1 => (20.0, 0.25, 0.35, 0.0),
            2 => (0.0, 0.5, 0.35, 0.0),
            3 => (12.0, 3.0, 0.0, 0.5),
            _ => return -6,
        };
        unsafe {
            out.write(Settings {
                version: 2,
                size: size_of::<Settings>() as u32,
                algorithm: a,
                bypass: 0,
                time_ms,
                amount,
                damping,
                rate_hz,
                gain: 0.5,
                seed: 558345748,
                reserved: 0,
            })
        };
        0
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_capabilities(out: *mut Capabilities, v: u32, n: u32) -> i32 {
    guarded(|| {
        if !output(out, v, n) {
            return -1;
        }
        unsafe {
            out.write(Capabilities {
                version: 2,
                size: n,
                algorithm_mask: 14,
                sample_bits: 64,
                min_rate: 8000,
                max_rate: 192000,
                max_block: 8192,
                pending_capacity: 1,
                retired_capacity: 1,
                transition_frames_per_second: 100,
                max_prepared_bytes: 8 * 1024 * 1024,
                hardware_budget_available: 0,
            })
        };
        0
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_prepare(
    rate: u32,
    block: u32,
    s: *const Settings,
) -> *mut c_void {
    catch_unwind(AssertUnwindSafe(|| {
        if !(8000..=192000).contains(&rate)
            || !(1..=8192).contains(&block)
            || !shape(s)
            || !valid(unsafe { &*s })
        {
            return ptr::null_mut();
        }
        let settings = unsafe { *s };
        Box::into_raw(Box::new(Prepared {
            effect: EmbeddedEffect::new(rate, &settings),
            settings,
            rate,
            block,
        }))
        .cast()
    }))
    .unwrap_or(ptr::null_mut())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_create(
    rate: u32,
    block: u32,
    s: *const Settings,
) -> *mut c_void {
    catch_unwind(AssertUnwindSafe(|| {
        let p = unsafe { shr_fx_v2_prepare(rate, block, s) };
        if p.is_null() {
            return p;
        }
        let active = unsafe { Box::from_raw(p.cast::<Prepared>()) };
        let current = active.settings;
        Box::into_raw(Box::new(Instance {
            active,
            pending: None,
            retired: None,
            phase: 0,
            status: Status {
                version: 2,
                size: size_of::<Status>() as u32,
                rate,
                max_block: block,
                intentional_delay_frames: (current.time_ms * rate as f64 / 1000.0).round() as u32,
                current,
                target: current,
                ..Status::default()
            },
        }))
        .cast()
    }))
    .unwrap_or(ptr::null_mut())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_publish(
    h: *mut c_void,
    p: *mut c_void,
    request: u64,
    base: u64,
) -> i32 {
    guarded(|| {
        if !shape(h.cast::<Instance>()) || !shape(p.cast::<Prepared>()) || h == p {
            return -1;
        }
        let h = unsafe { &mut *h.cast::<Instance>() };
        let p_ref = unsafe { &*p.cast::<Prepared>() };
        let result = if request <= h.status.accepted_request {
            -5
        } else if base != h.status.revision {
            -4
        } else if h.pending.is_some() || h.retired.is_some() || h.phase != 0 {
            -2
        } else if p_ref.rate != h.status.rate || p_ref.block != h.status.max_block {
            -6
        } else {
            0
        };
        h.status.last_request = request;
        h.status.request_result = result;
        h.status.request_state = if result == 0 { 1 } else { 5 };
        h.status.last_result = result;
        if result != 0 {
            return result;
        }
        h.status.target = p_ref.settings;
        h.status.accepted_request = request;
        h.pending = Some(unsafe { Box::from_raw(p.cast::<Prepared>()) });
        h.phase = 1;
        h.status.transition = 1;
        0
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_retire(h: *mut c_void) -> *mut c_void {
    catch_unwind(AssertUnwindSafe(|| {
        if !shape(h.cast::<Instance>()) {
            return ptr::null_mut();
        }
        unsafe { &mut *h.cast::<Instance>() }
            .retired
            .take()
            .map_or(ptr::null_mut(), |p| Box::into_raw(p).cast())
    }))
    .unwrap_or(ptr::null_mut())
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_cancel(p: *mut c_void) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if shape(p.cast::<Prepared>()) {
            drop(unsafe { Box::from_raw(p.cast::<Prepared>()) })
        }
    }));
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_destroy(h: *mut c_void) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if shape(h.cast::<Instance>()) {
            drop(unsafe { Box::from_raw(h.cast::<Instance>()) })
        }
    }));
}
fn reset(h: &mut Instance) {
    if h.phase != 0 && h.status.last_request == h.status.accepted_request {
        h.status.request_state = 4;
    }
    h.active.effect.clear();
    if let Some(p) = h.pending.take() {
        h.retired = Some(p)
    }
    h.phase = 0;
    h.status.transition = 0;
    h.status.target = h.status.current;
    h.status.fault = 0;
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_reset(h: *mut c_void) -> i32 {
    guarded(|| {
        if !shape(h.cast::<Instance>()) {
            return -1;
        }
        reset(unsafe { &mut *h.cast::<Instance>() });
        0
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_status(h: *mut c_void, out: *mut Status, v: u32, n: u32) -> i32 {
    guarded(|| {
        if !shape(h.cast::<Instance>()) || !output(out, v, n) {
            return -1;
        }
        let h = unsafe { &*h.cast::<Instance>() };
        if overlaps(h, out as usize, out as usize + size_of::<Status>()) {
            return -1;
        }
        unsafe { out.write(h.status) };
        0
    })
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_process(
    h: *mut c_void,
    input: *const f64,
    out: *mut f64,
    frames: u32,
) -> i32 {
    guarded(|| {
        if !shape(h.cast::<Instance>()) {
            return -1;
        }
        let h = unsafe { &mut *h.cast::<Instance>() };
        if frames > h.status.max_block {
            h.status.last_result = -2;
            return -2;
        }
        if frames == 0 {
            return 0;
        }
        let bytes = frames as usize * 16;
        let a = input as usize;
        let b = out as usize;
        if !shape(input)
            || !shape(out)
            || a.checked_add(bytes).is_none()
            || b.checked_add(bytes).is_none()
            || (a != b && a.abs_diff(b) < bytes)
        {
            h.status.last_result = -1;
            return -1;
        }
        if overlaps(h, a, a + bytes) || overlaps(h, b, b + bytes) {
            h.status.last_result = -1;
            return -1;
        }
        // Bypass-only edits preserve the active tail and do not replace its DSP.
        if let Some(p) = h.pending.as_ref() {
            let mut old = h.active.settings;
            old.bypass = p.settings.bypass;
            if old == p.settings {
                h.active.settings = p.settings;
                h.status.current = p.settings;
                h.status.revision = h.status.accepted_request;
                h.status.applied_request = h.status.accepted_request;
                if h.status.last_request == h.status.accepted_request {
                    h.status.request_state = 3;
                }
                h.retired = h.pending.take();
                h.phase = 0;
                h.status.transition = 0;
            }
        }
        let fault = (0..frames as usize * 2).any(|i| {
            let x = unsafe { input.add(i).read() };
            !x.is_finite() || x.abs() > 16.0
        });
        if fault || h.status.fault != 0 {
            reset(h);
            h.status.fault = 1;
            h.status.last_result = -3;
            unsafe { out.write_bytes(0, frames as usize * 2) };
            return -3;
        }
        let length = (h.status.rate / 100).max(1);
        for i in 0..frames as usize {
            let x = unsafe { [input.add(i * 2).read(), input.add(i * 2 + 1).read()] };
            let gain = if h.phase == 0 {
                1.0
            } else if h.phase <= length {
                1.0 - h.phase as f64 / length as f64
            } else {
                (h.phase - length) as f64 / length as f64
            };
            let y = h
                .active
                .effect
                .tick(x, h.status.rate, &h.active.settings)
                .map(|x| x * gain);
            if y.iter().any(|x| !x.is_finite() || x.abs() > 256.0) {
                reset(h);
                h.status.fault = 1;
                h.status.last_result = -3;
                unsafe { out.write_bytes(0, frames as usize * 2) };
                return -3;
            }
            unsafe {
                out.add(i * 2).write(y[0]);
                out.add(i * 2 + 1).write(y[1])
            };
            if h.phase != 0 {
                if h.phase == length {
                    let new = h.pending.take().expect("pending transition");
                    h.retired = Some(std::mem::replace(&mut h.active, new));
                    h.status.current = h.active.settings;
                    h.status.revision = h.status.accepted_request;
                    if h.status.last_request == h.status.accepted_request {
                        h.status.request_state = 2;
                    }
                }
                if h.phase == length * 2 {
                    h.phase = 0;
                    h.status.transition = 0;
                    h.status.applied_request = h.status.accepted_request;
                    if h.status.last_request == h.status.accepted_request {
                        h.status.request_state = 3;
                    }
                } else {
                    h.phase += 1
                }
            }
        }
        h.status.intentional_delay_frames =
            (h.status.current.time_ms * h.status.rate as f64 / 1000.0).round() as u32;
        h.status.last_result = 0;
        0
    })
}

/// Parameter IDs: 1 time, 2 amount, 3 damping, 4 rate, 5 gain, 6 bypass.
/// Unit IDs: 0 ratio, 1 milliseconds, 2 Hertz, 3 boolean.
#[repr(C)]
#[derive(Default)]
pub struct Parameter {
    pub version: u32,
    pub size: u32,
    pub algorithm: u32,
    pub parameter: u32,
    pub available: u32,
    pub unit: u32,
    pub minimum: f64,
    pub maximum: f64,
    pub default_value: f64,
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_parameter(
    a: u32,
    id: u32,
    out: *mut Parameter,
    v: u32,
    n: u32,
) -> i32 {
    guarded(|| {
        if !output(out, v, n) {
            return -1;
        }
        if !(1..=3).contains(&a) || !(1..=6).contains(&id) {
            return -6;
        }
        let (available, unit, minimum, maximum, default_value) = match id {
            1 => match a {
                1 => (1, 1, 1.0, 2000.0, 20.0),
                2 => (1, 1, 0.0, 200.0, 0.0),
                _ => (1, 1, 1.0, 30.0, 12.0),
            },
            2 => match a {
                1 => (1, 0, 0.0, 0.9, 0.25),
                2 => (1, 0, 0.0, 0.9, 0.5),
                _ => (1, 1, 0.0, 9.0, 3.0),
            },
            3 => {
                if a == 3 {
                    (0, 0, 0.0, 0.0, 0.0)
                } else {
                    (1, 0, 0.0, 1.0, 0.35)
                }
            }
            4 => {
                if a == 3 {
                    (1, 2, 0.05, 10.0, 0.5)
                } else {
                    (0, 2, 0.0, 0.0, 0.0)
                }
            }
            5 => (1, 0, 0.0, 1.0, 0.5),
            _ => (1, 3, 0.0, 1.0, 0.0),
        };
        unsafe {
            out.write(Parameter {
                version: 2,
                size: n,
                algorithm: a,
                parameter: id,
                available,
                unit,
                minimum,
                maximum,
                default_value,
            })
        };
        0
    })
}
