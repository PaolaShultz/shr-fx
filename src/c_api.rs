//! Versioned device-independent stereo wet-return ABI. See `include/shr_fx.h`.
//!
//! Handles have one owner; callers serialize all operations on each handle.
//! The f64 wire boundary converts pairs through the existing f32 DSP. Process
//! and reset do not allocate, free, lock, perform I/O or inspect host clocks.

use crate::dsp::{IntegrationDelay, MAX_FRAMES, MAX_RATE};
use std::{ffi::c_void, ptr};

pub const OK: i32 = 0;
pub const INVALID_ARGUMENT: i32 = -1;
pub const CAPACITY: i32 = -2;
pub const INVALID_SAMPLE: i32 = -3;

struct Handle {
    delay: IntegrationDelay,
    max_block: u32,
}

/// Prepare the fixed stereo digital-delay preset off the processing thread.
/// Returns null for rates outside 8000..=192000 or block bounds outside 1..=8192.
#[unsafe(no_mangle)]
pub extern "C" fn shr_fx_v1_create(sample_rate: u32, max_block: u32) -> *mut c_void {
    if !(8000..=MAX_RATE).contains(&sample_rate) || !(1..=MAX_FRAMES as u32).contains(&max_block) {
        return ptr::null_mut();
    }
    Box::into_raw(Box::new(Handle {
        delay: IntegrationDelay::new(sample_rate),
        max_block,
    }))
    .cast()
}

/// Process interleaved `[L, R]` frames. Exact in-place operation is supported;
/// partial overlap is rejected. Zero frames allow null audio pointers.
///
/// Invalid capacity or audio-pointer shape clears delay history and writes no
/// output. Invalid sample data clears history and silences the whole block.
/// The host must mute any block with a nonzero return code before publication.
///
/// # Safety
/// A non-null handle must be a live object from create with exclusive access.
/// Non-null audio pointers must address readable/writable arrays respectively
/// of at least `frames * 2` doubles in their own allocations (or the same array
/// for exact in-place processing), and must not overlap handle storage. Memory
/// validity and ownership cannot be checked by this C ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v1_process(
    handle: *mut c_void,
    input: *const f64,
    output: *mut f64,
    frames: u32,
) -> i32 {
    if handle.is_null() || !handle.cast::<Handle>().is_aligned() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: the caller owns the live, exclusively accessed handle.
    let handle = unsafe { &mut *handle.cast::<Handle>() };
    if frames > handle.max_block {
        handle.delay.clear();
        return CAPACITY;
    }
    if frames == 0 {
        return OK;
    }
    let samples = frames as usize * 2;
    let bytes = samples * size_of::<f64>();
    let start_in = input as usize;
    let start_out = output as usize;
    let valid_shape = !input.is_null()
        && !output.is_null()
        && input.is_aligned()
        && output.is_aligned()
        && start_in.checked_add(bytes).is_some()
        && start_out.checked_add(bytes).is_some()
        && (start_in == start_out || start_in.abs_diff(start_out) >= bytes);
    if !valid_shape {
        handle.delay.clear();
        return INVALID_ARGUMENT;
    }
    // Preflight the complete block before changing state or writing in-place.
    for index in 0..samples {
        // SAFETY: aligned readable extent is a caller precondition.
        let sample = unsafe { input.add(index).read() };
        if !sample.is_finite() || sample.abs() > 16.0 {
            handle.delay.clear();
            // SAFETY: the output extent is a caller precondition. f64 zero has
            // an all-zero byte representation; input reads have ended.
            unsafe { output.write_bytes(0, samples) };
            return INVALID_SAMPLE;
        }
    }
    for frame in 0..frames as usize {
        let index = frame * 2;
        // SAFETY: both reads precede writes, so exact in-place use is valid.
        let pair = unsafe {
            [
                input.add(index).read() as f32,
                input.add(index + 1).read() as f32,
            ]
        };
        let wet = handle.delay.tick(pair);
        if wet.iter().any(|sample| !sample.is_finite()) {
            handle.delay.clear();
            // SAFETY: the complete block belongs to the caller.
            unsafe { output.write_bytes(0, samples) };
            return INVALID_SAMPLE;
        }
        // SAFETY: disjoint or exactly equal arrays and valid output extent.
        unsafe {
            output.add(index).write(wet[0] as f64);
            output.add(index + 1).write(wet[1] as f64);
        }
    }
    OK
}

/// Discard all pending echoes on a source discontinuity or epoch replacement.
/// Null is a no-op. This has constant work independent of retained audio length.
///
/// # Safety
/// A non-null handle must be live, created by this ABI, and exclusively owned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v1_reset(handle: *mut c_void) {
    if !handle.is_null() {
        // SAFETY: caller guarantees handle validity and exclusive access.
        unsafe { &mut *handle.cast::<Handle>() }.delay.clear();
    }
}

/// Return exact intentional first-tap delay in source frames; null returns zero.
/// This excludes device, network, scheduling and host playout latency.
///
/// # Safety
/// A non-null handle must be live and not concurrently modified or destroyed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v1_delay_frames(handle: *mut c_void) -> u32 {
    if handle.is_null() {
        return 0;
    }
    // SAFETY: caller guarantees validity and no concurrent handle mutation.
    unsafe { &*handle.cast::<Handle>() }.delay.frames()
}

/// Free the handle off the processing thread. Null is a no-op.
///
/// # Safety
/// A non-null handle must be live, created by this ABI, exclusively owned, and
/// never used again after destruction. Calling destroy twice is invalid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v1_destroy(handle: *mut c_void) {
    if !handle.is_null() {
        // SAFETY: this allocation came from Box::into_raw in create.
        drop(unsafe { Box::from_raw(handle.cast::<Handle>()) });
    }
}
