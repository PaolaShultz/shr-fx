//! Versioned device-independent stereo wet-return ABI. See `include/shr_fx.h`.
//!
//! Handles have one owner; callers serialize all operations on each handle.
//! The f64 boundary uses the shared delay algorithm with f64 state/arithmetic. Process
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
    sample_rate: u32,
    last_process_result: i32,
    reset_reason: u32,
    reset_count: u64,
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
        sample_rate,
        last_process_result: OK,
        reset_reason: 0,
        reset_count: 0,
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
        handle.clear(2);
        handle.last_process_result = CAPACITY;
        return CAPACITY;
    }
    if frames == 0 {
        handle.last_process_result = OK;
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
        handle.clear(3);
        handle.last_process_result = INVALID_ARGUMENT;
        return INVALID_ARGUMENT;
    }
    // Preflight the complete block before changing state or writing in-place.
    for index in 0..samples {
        // SAFETY: aligned readable extent is a caller precondition.
        let sample = unsafe { input.add(index).read() };
        if !sample.is_finite() || sample.abs() > 16.0 {
            handle.clear(4);
            handle.last_process_result = INVALID_SAMPLE;
            // SAFETY: the output extent is a caller precondition. f64 zero has
            // an all-zero byte representation; input reads have ended.
            unsafe { output.write_bytes(0, samples) };
            return INVALID_SAMPLE;
        }
    }
    for frame in 0..frames as usize {
        let index = frame * 2;
        // SAFETY: both reads precede writes, so exact in-place use is valid.
        let pair = unsafe { [input.add(index).read(), input.add(index + 1).read()] };
        let wet = handle.delay.tick(pair);
        if wet.iter().any(|sample| !sample.is_finite()) {
            handle.clear(4);
            handle.last_process_result = INVALID_SAMPLE;
            // SAFETY: the complete block belongs to the caller.
            unsafe { output.write_bytes(0, samples) };
            return INVALID_SAMPLE;
        }
        // SAFETY: disjoint or exactly equal arrays and valid output extent.
        unsafe {
            output.add(index).write(wet[0]);
            output.add(index + 1).write(wet[1]);
        }
    }
    handle.last_process_result = OK;
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
        unsafe { &mut *handle.cast::<Handle>() }.clear(1);
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

impl Handle {
    fn clear(&mut self, reason: u32) {
        self.delay.clear();
        self.reset_reason = reason;
        self.reset_count = self.reset_count.saturating_add(1);
    }
}

/// Fixed, read-only embedded capabilities; no hardware or rack claims.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CapabilitiesV1 {
    pub version: u32,
    pub size: u32,
    pub identity: [u8; 32],
    pub min_sample_rate: u32,
    pub max_sample_rate: u32,
    pub min_block_frames: u32,
    pub max_block_frames: u32,
    pub channels: u32,
    pub sample_bits: u32,
    pub reset_supported: u32,
    pub writable_parameters: u32,
    pub rack_available: u32,
    pub adapter_buffer_frames: u32,
    pub delay_ms: f64,
    pub feedback: f64,
    pub damping: f64,
    pub wet_gain: f64,
}

/// Numerical adapter history only. Hardware health is unavailable.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatusV1 {
    pub version: u32,
    pub size: u32,
    pub sample_rate: u32,
    pub max_block_frames: u32,
    pub intentional_delay_frames: u32,
    pub adapter_buffer_frames: u32,
    pub last_process_result: i32,
    /// 0 none, 1 explicit reset, 2 capacity, 3 pointer shape, 4 sample fault.
    pub reset_reason: u32,
    pub reset_count: u64,
}

fn valid_output<T>(output: *mut T, version: u32, size: u32) -> bool {
    version == 1
        && size as usize == size_of::<T>()
        && !output.is_null()
        && output.is_aligned()
        && (output as usize).checked_add(size_of::<T>()).is_some()
}

/// Query fixed capabilities into caller-owned storage. Errors leave output exact.
///
/// # Safety
/// Output must own a writable allocation of the exact declared size, unaliased
/// during this call. Pointer validity cannot be established by the ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v1_capabilities(
    output: *mut CapabilitiesV1,
    version: u32,
    size: u32,
) -> i32 {
    if !valid_output(output, version, size) {
        return INVALID_ARGUMENT;
    }
    let mut identity = [0; 32];
    let name = b"fx-a/fixed-delay-v1";
    identity[..name.len()].copy_from_slice(name);
    let value = CapabilitiesV1 {
        version: 1,
        size: size_of::<CapabilitiesV1>() as u32,
        identity,
        min_sample_rate: 8000,
        max_sample_rate: MAX_RATE,
        min_block_frames: 1,
        max_block_frames: MAX_FRAMES as u32,
        channels: 2,
        sample_bits: 64,
        reset_supported: 1,
        writable_parameters: 0,
        rack_available: 0,
        adapter_buffer_frames: 0,
        delay_ms: 20.0,
        feedback: 0.25,
        damping: 0.35,
        wet_gain: 0.5,
    };
    // SAFETY: validated shape and caller-owned writable extent.
    unsafe { output.write(value) };
    OK
}

/// Query actual adapter history. No concurrent process/reset/query/destroy.
/// Query errors leave output and handle state unchanged.
///
/// # Safety
/// Handle must be live and quiesced; output must be writable, unaliased and
/// disjoint from the entire handle allocation. No thread safety is implied.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v1_status(
    handle: *mut c_void,
    output: *mut StatusV1,
    version: u32,
    size: u32,
) -> i32 {
    if handle.is_null()
        || !handle.cast::<Handle>().is_aligned()
        || !valid_output(output, version, size)
    {
        return INVALID_ARGUMENT;
    }
    let start = handle as usize;
    let Some(end) = start.checked_add(size_of::<Handle>()) else {
        return INVALID_ARGUMENT;
    };
    let out = output as usize;
    if out < end && start < out + size_of::<StatusV1>() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: shape/overlap checked before reference; caller guarantees lifetime.
    let handle = unsafe { &*handle.cast::<Handle>() };
    if handle
        .delay
        .owned_spans()
        .iter()
        .any(|&(start, end)| out < end && start < out + size_of::<StatusV1>())
    {
        return INVALID_ARGUMENT;
    }
    let value = StatusV1 {
        version: 1,
        size: size_of::<StatusV1>() as u32,
        sample_rate: handle.sample_rate,
        max_block_frames: handle.max_block,
        intentional_delay_frames: handle.delay.frames(),
        adapter_buffer_frames: 0,
        last_process_result: handle.last_process_result,
        reset_reason: handle.reset_reason,
        reset_count: handle.reset_count,
    };
    // SAFETY: caller-owned valid disjoint output.
    unsafe { output.write(value) };
    OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_rejects_every_owned_delay_allocation_without_writes() {
        let raw = shr_fx_v1_create(48000, 1024);
        // SAFETY: exclusive live test handle. Each span has initialized doubles.
        unsafe {
            let handle = &mut *raw.cast::<Handle>();
            handle.delay.tick([1.0, -0.5]);
            let spans = handle.delay.owned_spans();
            for (start, _) in spans {
                let before = std::slice::from_raw_parts(start as *const u64, 5).to_vec();
                assert_eq!(
                    shr_fx_v1_status(raw, start as *mut StatusV1, 1, 40),
                    INVALID_ARGUMENT
                );
                assert_eq!(std::slice::from_raw_parts(start as *const u64, 5), before);
            }
            let handle = &*raw.cast::<Handle>();
            assert_eq!(handle.reset_count, 0);
            shr_fx_v1_destroy(raw);
        }
    }

    #[test]
    fn history_clear_count_saturates() {
        let raw = shr_fx_v1_create(8000, 1);
        // SAFETY: this test exclusively owns the newly created live handle.
        unsafe {
            let handle = &mut *raw.cast::<Handle>();
            handle.reset_count = u64::MAX;
            handle.clear(1);
            assert_eq!(handle.reset_count, u64::MAX);
            assert_eq!(handle.reset_reason, 1);
            shr_fx_v1_destroy(raw);
        }
    }
}
