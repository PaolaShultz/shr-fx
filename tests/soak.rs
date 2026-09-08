//! Opt-in offline cost evidence. No JACK/ALSA access or audio-file rendering.
use shr_fx::{
    dsp::Processor,
    model::{Algorithm, Availability, DelayKind, EngineMode, Layout, MAX_STAGES, Rack, ReverbKind},
};
use std::time::Instant;
fn measure(label: &str, mut rack: Rack, rapid: bool) {
    let mut processor = Processor::new(48000, rack).unwrap();
    let input: [[f32; 256]; 4] = std::array::from_fn(|c| {
        std::array::from_fn(|n| ((n + c * 17) as f32 * 0.031).sin() * 0.15)
    });
    let mut output = [[0.0; 256]; 4];
    let mut times = Vec::with_capacity(11250);
    for block in 0..11250 {
        // One minute per case, with new excitation and maximum configured feedback.
        if rapid && block % 11 == 0 {
            for (e, engine) in rack.engines.iter_mut().enumerate() {
                for (i, slot) in engine.stages.iter_mut().enumerate() {
                    slot.delay.time_ms = 1.0 + (block * (i + 1) % 1999) as f32;
                    slot.reverb.predelay_ms = (block % 201) as f32;
                    slot.chorus.rate_hz = 0.05 + (block % 496) as f32 / 100.0;
                    slot.chorus.depth_ms = (block % 9) as f32;
                    slot.exciter.drive = (block % 101) as f32 / 100.0;
                    slot.exciter.tune_hz = 600.0 + (block % 55) as f32 * 100.0;
                    slot.exciter.tone = (block % 101) as f32 / 100.0;
                    slot.bypass = block % 121 == 0;
                    if block % 187 == 0 {
                        slot.algorithm =
                            Algorithm::ALL[(block / 187 + i + e) % Algorithm::ALL.len()];
                        slot.delay.kind = DelayKind::ALL[(block / 187) % 4];
                        slot.reverb.kind = ReverbKind::ALL[(block / 187) % 5];
                    }
                }
            }
            processor.apply(rack);
        }
        let start = Instant::now();
        let meters = processor.process(
            input.each_ref().map(|v| &v[..]),
            output.each_mut().map(|v| &mut v[..]),
            256,
            Availability::ALL,
        );
        times.push(start.elapsed().as_nanos());
        assert_eq!(meters.faults, 0, "{label}");
        assert!(output.iter().flatten().all(|x| x.is_finite()));
    }
    times.sort_unstable();
    println!(
        "{label}: ns/block p50={} p95={} p99={} max={}; nominal period={} ns",
        times[times.len() / 2],
        times[times.len() * 95 / 100],
        times[times.len() * 99 / 100],
        times[times.len() - 1],
        256_000_000_000u64 / 48000
    );
}
#[test]
#[ignore = "offline cost matrix; cargo test --release --locked --test soak -- --ignored --nocapture"]
fn dual_engine_cost_and_recovery_soak() {
    let mut vocal = Rack::default();
    for engine in &mut vocal.engines {
        engine.mode = EngineMode::MultiFx;
        engine.stages[0].algorithm = Algorithm::Delay;
        engine.stages[1].algorithm = Algorithm::Room;
        engine.stages[2].algorithm = Algorithm::Chorus;
        for slot in &mut engine.stages {
            slot.delay.time_ms = 2000.0;
            slot.delay.feedback = 0.9;
            slot.delay.kind = DelayKind::Tape;
            slot.reverb.kind = ReverbKind::Hall;
            slot.reverb.decay = 0.9;
            slot.chorus.ensemble = true;
            slot.chorus.rate_hz = 5.0;
            slot.chorus.depth_ms = 8.0;
            slot.exciter.drive = 1.0;
            slot.exciter.tone = 1.0;
            slot.exciter.bright = true;
        }
    }
    let mut single = vocal;
    single.routing.layout = Layout::AStereo;
    measure("one four-effect vocal / stereo return", single, false);
    measure("two four-effect vocal engines", vocal, false);
    let mut full = vocal;
    for engine in &mut full.engines {
        engine.pieces = MAX_STAGES as u8;
    }
    for (label, algorithm, delay, reverb) in [
        (
            "sixteen halls",
            Algorithm::Room,
            DelayKind::Digital,
            ReverbKind::Hall,
        ),
        (
            "sixteen plates",
            Algorithm::Room,
            DelayKind::Digital,
            ReverbKind::Plate,
        ),
        (
            "sixteen tape echoes",
            Algorithm::Delay,
            DelayKind::Tape,
            ReverbKind::Room,
        ),
        (
            "sixteen multi-tap delays",
            Algorithm::Delay,
            DelayKind::MultiTap,
            ReverbKind::Room,
        ),
        (
            "sixteen diffused delays",
            Algorithm::Delay,
            DelayKind::Diffused,
            ReverbKind::Room,
        ),
        (
            "sixteen exciters",
            Algorithm::Exciter,
            DelayKind::Digital,
            ReverbKind::Room,
        ),
        (
            "sixteen ensemble choruses",
            Algorithm::Chorus,
            DelayKind::Digital,
            ReverbKind::Room,
        ),
    ] {
        let mut rack = full;
        for engine in &mut rack.engines {
            for slot in &mut engine.stages {
                slot.algorithm = algorithm;
                slot.delay.kind = delay;
                slot.reverb.kind = reverb;
            }
        }
        measure(label, rack, false);
    }
    measure("sixteen slots under rapid edits", full, true);
    println!(
        "Regular-thread simulations only; no JACK deadline, xrun, thermal, latency or listening acceptance."
    );
}
