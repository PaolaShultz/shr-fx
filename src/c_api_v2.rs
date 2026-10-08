//! Additive prepared stereo wet-delay ABI. All calls are single-owner serialized.
//! C callers must supply live, correctly typed, aligned allocations of the stated
//! extent. Shape checks cannot establish allocation lifetime or concurrency safety.
use crate::c_api::{CAPACITY, INVALID_ARGUMENT, INVALID_SAMPLE, OK};
pub use crate::dsp::WetDelayControls as ChannelV2;
use crate::dsp::{MAX_FRAMES, MAX_RATE, PreparedWetChannel};
use std::{ffi::c_void, ptr};

pub const STALE: i32 = -4;
pub const BUSY: i32 = -5;
pub const TIMELINE: i32 = -6;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ConfigV2 {
    pub version: u32,
    pub size: u32,
    pub expected_generation: u64,
    pub generation: u64,
    pub channel: [ChannelV2; 2],
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CapabilitiesV2 {
    pub version: u32,
    pub size: u32,
    pub identity: [u8; 32],
    pub min_sample_rate: u32,
    pub max_sample_rate: u32,
    pub min_block_frames: u32,
    pub max_block_frames: u32,
    pub channels: u32,
    pub sample_bits: u32,
    pub rack_available: u32,
    pub adapter_buffer_frames: u32,
    pub min_delay_ms: f64,
    pub max_delay_ms: f64,
    pub max_feedback: f64,
    pub max_damping: f64,
    pub max_wet_gain: f64,
    pub transition_ms: u32,
    pub max_read_heads_per_channel: u32,
    pub max_delay_storage_bytes: u64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StatusV2 {
    pub version: u32,
    pub size: u32,
    pub sample_rate: u32,
    pub max_block_frames: u32,
    pub transition_frames: u32,
    pub transitioning_mask: u32,
    pub last_process_result: i32,
    pub reset_reason: u32,
    pub applied_generation: u64,
    pub settled_generation: u64,
    pub applied_source_frame: u64,
    pub settled_source_frame: u64,
    pub next_source_frame: u64,
    pub reset_count: u64,
    pub remaining_frames: [u32; 2],
    pub target: [ChannelV2; 2],
}
struct Prepared {
    config: ConfigV2,
    rate: u32,
}
struct Handle {
    channels: [PreparedWetChannel; 2],
    status: StatusV2,
}
fn bounds(rate: u32, block: u32) -> bool {
    (8000..=MAX_RATE).contains(&rate) && (1..=MAX_FRAMES as u32).contains(&block)
}
fn span<T>(p: *const T, count: usize) -> Option<(usize, usize)> {
    if p.is_null() || !p.is_aligned() {
        return None;
    }
    let start = p as usize;
    Some((
        start,
        start.checked_add(count.checked_mul(size_of::<T>())?)?,
    ))
}
fn overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.1 && b.0 < a.1
}
fn shape<T>(p: *const T, version: u32, size: u32) -> bool {
    version == 2 && size as usize == size_of::<T>() && span(p, 1).is_some()
}
impl Handle {
    fn owns(&self, other: (usize, usize)) -> bool {
        let start = self as *const Self as usize;
        overlap((start, start + size_of::<Self>()), other)
            || self.channels.iter().any(|c| overlap(c.owned_span(), other))
    }
    fn clear(&mut self, mask: u32, reason: u32) {
        for (c, channel) in self.channels.iter_mut().enumerate() {
            if mask & (1 << c) != 0 {
                channel.clear();
            }
        }
        self.status.reset_reason = reason;
        self.status.reset_count = self.status.reset_count.saturating_add(1);
    }
    fn refresh(&mut self, frame: u64) {
        self.status.remaining_frames = self.channels.each_ref().map(|c| c.remaining());
        self.status.transitioning_mask = self
            .status
            .remaining_frames
            .iter()
            .enumerate()
            .fold(0, |mask, (c, remaining)| {
                mask | (u32::from(*remaining != 0) << c)
            });
        if self.status.transitioning_mask == 0
            && self.status.settled_generation != self.status.applied_generation
        {
            self.status.settled_generation = self.status.applied_generation;
            self.status.settled_source_frame = frame;
        }
    }
    fn fail(&mut self, result: i32, reason: u32) -> i32 {
        self.clear(3, reason);
        self.status.last_process_result = result;
        result
    }
}

/// Allocate maximum delay storage off render; returns null on invalid bounds.
#[unsafe(no_mangle)]
pub extern "C" fn shr_fx_v2_create(sample_rate: u32, max_block: u32) -> *mut c_void {
    if !bounds(sample_rate, max_block) {
        return ptr::null_mut();
    }
    Box::into_raw(Box::new(Handle {
        channels: std::array::from_fn(|_| PreparedWetChannel::new(sample_rate)),
        status: StatusV2 {
            version: 2,
            size: size_of::<StatusV2>() as u32,
            sample_rate,
            max_block_frames: max_block,
            transition_frames: sample_rate.div_ceil(50),
            target: [ChannelV2::default(); 2],
            ..Default::default()
        },
    }))
    .cast()
}

/// Validate/copy both channels and allocate a library-owned token off render.
/// # Safety
/// Config must have a live readable extent; no concurrent mutation of its bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_prepare(
    config: *const ConfigV2,
    sample_rate: u32,
    version: u32,
    size: u32,
) -> *mut c_void {
    if !shape(config, version, size) || !bounds(sample_rate, 1) {
        return ptr::null_mut();
    }
    // SAFETY: shape checked; live readable allocation is a caller obligation.
    let config = unsafe { config.read() };
    if config.version != 2
        || config.size as usize != size_of::<ConfigV2>()
        || config.expected_generation >= config.generation
        || !config.channel.iter().all(|c| c.valid())
    {
        return ptr::null_mut();
    }
    Box::into_raw(Box::new(Prepared {
        config,
        rate: sample_rate,
    }))
    .cast()
}

/// Apply an already validated token at the exact source boundary. Does not consume it.
/// # Safety
/// Both pointers must be live objects from their corresponding v2 constructors;
/// caller serializes handle operations and token retirement.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_commit(
    handle: *mut c_void,
    prepared: *const c_void,
    source_frame: u64,
) -> i32 {
    if span(handle.cast::<Handle>(), 1).is_none() {
        return INVALID_ARGUMENT;
    }
    let Some(token_span) = span(prepared.cast::<Prepared>(), 1) else {
        return INVALID_ARGUMENT;
    };
    // SAFETY: caller supplies exclusive live handle; token has not been accessed.
    let handle = unsafe { &mut *handle.cast::<Handle>() };
    if handle.owns(token_span) {
        return INVALID_ARGUMENT;
    }
    // SAFETY: live token obligation, validated span disjoint from handle.
    let prepared = unsafe { &*prepared.cast::<Prepared>() };
    if prepared.rate != handle.status.sample_rate {
        return INVALID_ARGUMENT;
    }
    if source_frame != handle.status.next_source_frame {
        return TIMELINE;
    }
    if prepared.config.expected_generation != handle.status.applied_generation
        || prepared.config.generation <= handle.status.applied_generation
    {
        return STALE;
    }
    if handle.status.transitioning_mask != 0 {
        return BUSY;
    }
    // Refuse a changed configuration if its finite ramp cannot settle within
    // the source timeline. A no-change commit needs no future frames.
    if prepared.config.channel != handle.status.target
        && source_frame
            .checked_add(handle.status.transition_frames as u64)
            .is_none()
    {
        return TIMELINE;
    }
    for (channel, config) in handle.channels.iter_mut().zip(prepared.config.channel) {
        channel.apply(config);
    }
    handle.status.target = prepared.config.channel;
    handle.status.applied_generation = prepared.config.generation;
    handle.status.applied_source_frame = source_frame;
    handle.refresh(source_frame);
    OK
}

/// Free a prepared token off render, including refused/cancelled tokens. Null no-op.
/// # Safety
/// Non-null token must be live, exclusively owned, and not used again after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_retire(prepared: *mut c_void) {
    if !prepared.is_null() {
        // SAFETY: the token originated from Box::into_raw in prepare.
        drop(unsafe { Box::from_raw(prepared.cast::<Prepared>()) });
    }
}

/// Process one contiguous source block. On any error host must mute the block.
/// # Safety
/// Live exclusive v2 handle and valid readable/writable audio allocations are
/// required. Exact in-place is allowed; other overlaps are rejected before access.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_process(
    handle: *mut c_void,
    input: *const f64,
    output: *mut f64,
    frames: u32,
    source_frame: u64,
) -> i32 {
    if span(handle.cast::<Handle>(), 1).is_none() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: caller supplies live exclusive handle.
    let handle = unsafe { &mut *handle.cast::<Handle>() };
    if source_frame != handle.status.next_source_frame
        || source_frame.checked_add(frames as u64).is_none()
    {
        return TIMELINE;
    }
    if frames > handle.status.max_block_frames {
        return handle.fail(CAPACITY, 2);
    }
    if frames == 0 {
        handle.status.last_process_result = OK;
        return OK;
    }
    let samples = frames as usize * 2;
    let (Some(in_span), Some(out_span)) = (span(input, samples), span(output, samples)) else {
        return handle.fail(INVALID_ARGUMENT, 3);
    };
    if (input != output && overlap(in_span, out_span))
        || handle.owns(in_span)
        || handle.owns(out_span)
    {
        return handle.fail(INVALID_ARGUMENT, 3);
    }
    for i in 0..samples {
        // SAFETY: preflight checked shape and disjointness; caller owns extent.
        let x = unsafe { input.add(i).read() };
        if !x.is_finite() || x.abs() > 16.0 {
            // SAFETY: caller owns complete valid output block, reads have ended.
            unsafe { output.write_bytes(0, samples) };
            return handle.fail(INVALID_SAMPLE, 4);
        }
    }
    for frame in 0..frames as usize {
        // SAFETY: both reads precede writes for exact in-place processing.
        let pair = unsafe { [input.add(frame * 2).read(), input.add(frame * 2 + 1).read()] };
        let wet = std::array::from_fn::<_, 2, _>(|c| handle.channels[c].tick(pair[c]));
        if wet.iter().any(|x| !x.is_finite()) {
            // SAFETY: the whole output allocation belongs to caller.
            unsafe { output.write_bytes(0, samples) };
            return handle.fail(INVALID_SAMPLE, 4);
        }
        // SAFETY: valid output span; in-place input already read.
        unsafe {
            output.add(frame * 2).write(wet[0]);
            output.add(frame * 2 + 1).write(wet[1]);
        }
        handle.refresh(source_frame + frame as u64 + 1);
    }
    handle.status.next_source_frame = source_frame + frames as u64;
    handle.status.last_process_result = OK;
    OK
}

/// Clear selected tail/filter history; future excitation is still permitted.
/// # Safety
/// Handle must be a live exclusively owned v2 object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_panic(handle: *mut c_void, channel_mask: u32) -> i32 {
    if span(handle.cast::<Handle>(), 1).is_none() || !(1..=3).contains(&channel_mask) {
        return INVALID_ARGUMENT;
    }
    // SAFETY: exclusive live v2 handle obligation.
    unsafe { &mut *handle.cast::<Handle>() }.clear(channel_mask, 5);
    OK
}
/// Discontinuity reset snaps accepted controls while host wet is muted.
/// # Safety
/// Handle must be a live exclusively owned v2 object.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_reset(handle: *mut c_void, next_source_frame: u64) -> i32 {
    if span(handle.cast::<Handle>(), 1).is_none() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: exclusive live v2 handle obligation.
    let handle = unsafe { &mut *handle.cast::<Handle>() };
    handle.clear(3, 1);
    for c in &mut handle.channels {
        c.settle();
    }
    handle.status.next_source_frame = next_source_frame;
    handle.status.settled_generation = handle.status.applied_generation;
    handle.status.settled_source_frame = next_source_frame;
    handle.refresh(next_source_frame);
    OK
}
/// Query target/transition/numerical history into caller-owned storage.
/// # Safety
/// Live serialized v2 handle and writable, unaliased output extent required.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_status(
    handle: *mut c_void,
    output: *mut StatusV2,
    version: u32,
    size: u32,
) -> i32 {
    if !shape(output, version, size) || span(handle.cast::<Handle>(), 1).is_none() {
        return INVALID_ARGUMENT;
    }
    let out_span = span(output, 1).expect("validated span");
    // SAFETY: live serialized v2 handle obligation; output not yet accessed.
    let handle = unsafe { &*handle.cast::<Handle>() };
    if handle.owns(out_span) {
        return INVALID_ARGUMENT;
    }
    // SAFETY: valid disjoint output extent is caller-owned.
    unsafe { output.write(handle.status) };
    OK
}
/// Query fixed bounds and truthful identity; not a hardware/rack descriptor.
/// # Safety
/// Output must own a writable, unaliased extent of exactly the declared size.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_capabilities(
    output: *mut CapabilitiesV2,
    version: u32,
    size: u32,
) -> i32 {
    if !shape(output, version, size) {
        return INVALID_ARGUMENT;
    }
    let mut identity = [0; 32];
    let name = b"fx-a/prepared-delay-v2";
    identity[..name.len()].copy_from_slice(name);
    let caps = CapabilitiesV2 {
        version: 2,
        size: size_of::<CapabilitiesV2>() as u32,
        identity,
        min_sample_rate: 8000,
        max_sample_rate: MAX_RATE,
        min_block_frames: 1,
        max_block_frames: MAX_FRAMES as u32,
        channels: 2,
        sample_bits: 64,
        rack_available: 0,
        adapter_buffer_frames: 0,
        min_delay_ms: 1.0,
        max_delay_ms: 500.0,
        max_feedback: 0.85,
        max_damping: 0.99,
        max_wet_gain: 1.0,
        transition_ms: 20,
        max_read_heads_per_channel: 2,
        max_delay_storage_bytes: (MAX_RATE as u64 / 2 + 4) * 16,
    };
    // SAFETY: caller-owned valid unaliased output.
    unsafe { output.write(caps) };
    OK
}
/// Destroy the prepared instance off render. Null no-op.
/// # Safety
/// A non-null handle must be live, exclusively owned, and never used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shr_fx_v2_destroy(handle: *mut c_void) {
    if !handle.is_null() {
        // SAFETY: allocation originated in create, exclusive final ownership.
        drop(unsafe { Box::from_raw(handle.cast::<Handle>()) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_owned_span_rejects_status_audio_and_tokens_before_access() {
        assert_eq!(size_of::<Handle>(), 632);
        assert_eq!(size_of::<Prepared>(), 112);
        let raw = shr_fx_v2_create(48000, 1024);
        // SAFETY: test owns live v2 handle and observes its initialized storage.
        unsafe {
            let handle = &*raw.cast::<Handle>();
            let spans = handle.channels.each_ref().map(|c| c.owned_span());
            for (start, _) in spans {
                let before = std::slice::from_raw_parts(start as *const u64, 21).to_vec();
                assert_eq!(
                    shr_fx_v2_status(raw, start as *mut StatusV2, 2, 168),
                    INVALID_ARGUMENT
                );
                assert_eq!(std::slice::from_raw_parts(start as *const u64, 21), before);
                assert_eq!(
                    shr_fx_v2_commit(raw, start as *const c_void, 0),
                    INVALID_ARGUMENT
                );
                let mut output = [99.0; 2];
                assert_eq!(
                    shr_fx_v2_process(raw, start as *const f64, output.as_mut_ptr(), 1, 0),
                    INVALID_ARGUMENT
                );
                assert_eq!(output, [99.0; 2]);
                assert_eq!(
                    shr_fx_v2_process(raw, output.as_ptr(), start as *mut f64, 1, 0),
                    INVALID_ARGUMENT
                );
                assert_eq!(std::slice::from_raw_parts(start as *const u64, 21), before);
            }
            (*raw.cast::<Handle>()).status.reset_count = u64::MAX;
            assert_eq!(shr_fx_v2_panic(raw, 1), OK);
            assert_eq!((*raw.cast::<Handle>()).status.reset_count, u64::MAX);
            shr_fx_v2_destroy(raw);
        }
    }
}
