//! Opt-in offline evidence only. This does not open JACK/ALSA or render files.
use fx::{
    dsp::Processor,
    model::{Algorithm, Availability, Rack},
};
use std::time::Instant;
#[test]
#[ignore = "long offline simulation; cargo test --release --test soak -- --ignored --nocapture"]
fn dual_engine_cost_and_recovery_soak() {
    let mut rack = Rack::default();
    for e in &mut rack.engines {
        e.feedback = 0.9;
    }
    let mut processor = Processor::new(48000, rack).unwrap();
    let input = [[0.15; 256]; 4];
    let mut output = [[0.0; 256]; 4];
    let mut times = Vec::with_capacity(11250);
    for block in 0..11250 {
        // one minute of simulated audio
        if block % 94 == 0 {
            rack.engines[0].time_ms = 1.0 + (block % 1999) as f32;
            rack.engines[1].algorithm = if block % 188 == 0 {
                Algorithm::Room
            } else {
                Algorithm::Delay
            };
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
        assert_eq!(meters.faults, 0);
        assert!(output.iter().flatten().all(|x| x.is_finite()));
    }
    times.sort();
    println!(
        "Offline processor ns/block: p50={} p99={} max={}; period={} ns",
        times[times.len() / 2],
        times[times.len() * 99 / 100],
        times[times.len() - 1],
        256_000_000_000u64 / 48000
    );
    println!("No JACK callback, xrun, thermal, latency or listening acceptance established.");
}
