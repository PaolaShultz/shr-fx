use shr_fx::{
    audio::{CallbackCore, Command, Shared},
    model::{Algorithm, Availability, DelayKind, EngineMode, Rack, ReverbKind},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    sync::{Arc, atomic::Ordering},
};
thread_local! { static TRACK: Cell<bool> = const { Cell::new(false) }; static ALLOCS: Cell<usize> = const { Cell::new(0) }; static FREES: Cell<usize> = const { Cell::new(0) }; static BYTES: Cell<usize> = const { Cell::new(0) }; }
struct Tracked;
unsafe impl GlobalAlloc for Tracked {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = TRACK.try_with(|t| {
            if t.get() {
                ALLOCS.with(|c| c.set(c.get() + 1));
                BYTES.with(|c| c.set(c.get() + layout.size()));
            }
        });
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _ = TRACK.try_with(|t| {
            if t.get() {
                FREES.with(|c| c.set(c.get() + 1));
            }
        });
        unsafe {
            System.dealloc(ptr, layout);
        }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let _ = TRACK.try_with(|t| {
            if t.get() {
                ALLOCS.with(|c| c.set(c.get() + 1));
                BYTES.with(|c| c.set(c.get() + layout.size()));
                FREES.with(|c| c.set(c.get() + 1));
            }
        });
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: Tracked = Tracked;
fn tracked(f: impl FnOnce()) {
    ALLOCS.with(|c| c.set(0));
    FREES.with(|c| c.set(0));
    TRACK.with(|c| c.set(true));
    f();
    TRACK.with(|c| c.set(false));
    assert_eq!(ALLOCS.with(Cell::get), 0, "callback allocation");
    assert_eq!(FREES.with(Cell::get), 0, "callback deallocation");
}
#[test]
fn complete_callback_is_allocation_free_through_edits_faults_and_panic() {
    let mut rack = Rack::default();
    for engine in &mut rack.engines {
        engine.mode = EngineMode::MultiFx;
        engine.pieces = shr_fx::model::MAX_STAGES as u8;
        for slot in &mut engine.stages {
            slot.delay.feedback = 0.9;
            slot.reverb.decay = 0.9;
            slot.chorus.ensemble = true;
        }
    }
    rack.engines[0].stages[0].delay.time_ms = 1.0;
    let (mut sender, receiver) = rtrb::RingBuffer::new(8);
    let shared = Arc::new(Shared::new(48000));
    shared.publish_availability(shared.audit_generation(), Availability::ALL);
    let mut callback = CallbackCore::new(48000, rack, receiver, shared.clone()).unwrap();
    let mut input = [[0.2; 256]; 4];
    let mut output = [[0.0; 256]; 4];
    for i in 0..100 {
        for (e, engine) in rack.engines.iter_mut().enumerate() {
            for (slot, effect) in engine.stages.iter_mut().enumerate() {
                effect.algorithm =
                    Algorithm::ALL[((i / 5) as usize + slot + e) % Algorithm::ALL.len()];
                effect.delay.time_ms = (i * 17 + 1) as f32;
                effect.delay.kind = DelayKind::ALL[(i / 5) as usize % 4];
                effect.reverb.kind = ReverbKind::ALL[(i / 5) as usize % 5];
                effect.chorus.depth_ms = (i % 9) as f32;
                effect.exciter.drive = (i % 11) as f32 / 10.0;
                effect.exciter.tune_hz = 600.0 + (i % 55) as f32 * 100.0;
                effect.exciter.tone = (i % 11) as f32 / 10.0;
                effect.level = (i % 11) as f32 / 10.0;
                effect.bypass = i % 17 == 0;
            }
        }
        sender.push(Command { rack, sequence: i }).unwrap();
        if i == 40 {
            input[0][128] = f32::NAN;
        }
        if i == 41 {
            input[0][128] = 0.2;
            shared.panic.store(1, Ordering::Release);
        }
        tracked(|| {
            callback.process(
                input.each_ref().map(|v| &v[..]),
                output.each_mut().map(|v| &mut v[..]),
                256,
            )
        });
    }
    assert_eq!(shared.applied.load(Ordering::Acquire), 99);
}
#[test]
fn queue_is_bounded_and_atomic_panic_wins_over_backlog() {
    let mut rack = Rack::default();
    rack.engines[0].stages[0].delay.time_ms = 1.0;
    let (mut sender, receiver) = rtrb::RingBuffer::new(8);
    let shared = Arc::new(Shared::new(8000));
    shared.publish_availability(shared.audit_generation(), Availability::ALL);
    let mut callback = CallbackCore::new(8000, rack, receiver, shared.clone()).unwrap();
    for sequence in 1..=8 {
        assert!(sender.push(Command { rack, sequence }).is_ok());
    }
    assert!(sender.push(Command { rack, sequence: 9 }).is_err());
    shared.panic.store(1, Ordering::Release);
    let input = [[1.0; 512]; 4];
    let mut output = [[3.0; 512]; 4];
    callback.process(
        input.each_ref().map(|v| &v[..]),
        output.each_mut().map(|v| &mut v[..]),
        512,
    );
    assert_eq!(shared.applied.load(Ordering::Acquire), 8);
    assert!(output[0].iter().all(|v| *v == 0.0));
}
#[test]
fn rate_change_mutes_before_any_unprepared_processing_and_fresh_preparation_recovers() {
    let rack = Rack::default();
    let (_, receiver) = rtrb::RingBuffer::new(8);
    let shared = Arc::new(Shared::new(48000));
    shared.publish_availability(shared.audit_generation(), Availability::ALL);
    let mut callback = CallbackCore::new(48000, rack, receiver, shared.clone()).unwrap();
    shared.rate.store(44100, Ordering::Release);
    let input = [0.5; 128];
    let mut output = [[3.0; 128]; 4];
    tracked(|| callback.process([&input; 4], output.each_mut().map(|v| &mut v[..]), 128));
    assert_eq!(output, [[0.0; 128]; 4]);
    assert!(shared.rate_fault.load(Ordering::Acquire));
    let (_, receiver) = rtrb::RingBuffer::new(8);
    let mut fresh = CallbackCore::new(44100, rack, receiver, shared.clone()).unwrap();
    fresh.process([&input; 4], output.each_mut().map(|v| &mut v[..]), 128);
    assert!(!shared.rate_fault.load(Ordering::Acquire));
}
#[test]
fn stale_availability_cannot_replace_newer_graph_inspection() {
    let shared = Shared::new(48000);
    let old = shared.audit_generation();
    assert!(shared.publish_availability(old, Availability::ALL));
    assert!(!shared.publish_availability(old, Availability::default()));
    assert_eq!(shared.availability(), Availability::ALL);
}

#[test]
fn maximum_rate_preparation_stays_within_documented_dsp_budget() {
    BYTES.with(|c| c.set(0));
    TRACK.with(|c| c.set(true));
    let processor = shr_fx::dsp::Processor::new(192000, Rack::default()).unwrap();
    TRACK.with(|c| c.set(false));
    let bytes = BYTES.with(Cell::get);
    assert!(
        bytes < shr_fx::dsp::MAX_DSP_BYTES,
        "prepared DSP allocated {bytes} bytes"
    );
    println!(
        "Maximum-rate prepared DSP: {bytes} bytes; budget {} bytes",
        shr_fx::dsp::MAX_DSP_BYTES
    );
    drop(processor);
}
