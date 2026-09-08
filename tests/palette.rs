use shr_fx::{
    dsp::Processor,
    model::{
        Algorithm, Availability, DelayKind, EffectConfig, EngineMode, Layout, Rack, ReverbKind,
    },
};

fn rack() -> Rack {
    let mut r = Rack::default();
    r.routing.layout = Layout::AStereo;
    r.engines[0].level = 1.0;
    r.engines[0].mode = EngineMode::MultiFx;
    r.engines[0].pieces = 3;
    for (i, s) in r.engines[0].stages.iter_mut().enumerate() {
        s.algorithm = Algorithm::Delay;
        s.delay.time_ms = (i + 1) as f32 * 10.0;
        s.delay.feedback = 0.0;
        s.delay.damping = 0.0;
    }
    r
}
fn block(p: &mut Processor, input: &[[f32; 256]; 4]) -> ([[f32; 256]; 4], shr_fx::dsp::Meters) {
    let mut output = [[0.0; 256]; 4];
    let meters = p.process(
        input.each_ref().map(|v| &v[..]),
        output.each_mut().map(|v| &mut v[..]),
        256,
        Availability::ALL,
    );
    (output, meters)
}
fn warm(p: &mut Processor, rate: u32) {
    for _ in 0..(rate / 256 / 20 + 1) {
        block(p, &[[0.0; 256]; 4]);
    }
}
fn impulse(rate: u32, r: Rack, frames: usize) -> Vec<[f32; 2]> {
    let mut p = Processor::new(rate, r).unwrap();
    warm(&mut p, rate);
    let mut samples = Vec::new();
    for n in 0..frames.div_ceil(256) {
        let mut input = [[0.0; 256]; 4];
        if n == 0 {
            input[0][0] = 1.0;
        }
        let (out, meters) = block(&mut p, &input);
        assert_eq!(meters.faults, 0);
        samples.extend((0..256).map(|n| [out[0][n], out[1][n]]));
    }
    samples.truncate(frames);
    samples
}
#[test]
fn three_independent_delays_are_parallel_and_slot_count_does_not_renormalize() {
    let mut r = rack();
    for (slot, level) in r.engines[0].stages.iter_mut().zip([1.0, 0.6, 0.3]) {
        slot.level = level;
    }
    let out = impulse(8000, r, 320);
    assert!(out[..80].iter().all(|s| *s == [0.0; 2]));
    for (frame, expected) in [(80, 1.0 / 6.0), (160, 0.1), (240, 0.05)] {
        for sample in out[frame] {
            assert!((sample - expected).abs() < 1e-6);
        }
    }
    assert_eq!(out.iter().filter(|s| **s != [0.0; 2]).count(), 3);
    r.engines[0].pieces = 2;
    let two = impulse(8000, r, 320);
    assert_eq!(two[80], out[80]);
    assert_eq!(two[160], out[160]);
    assert_eq!(two[240], [0.0; 2]);
    r.engines[0].stages[0].bypass = true;
    let bypass = impulse(8000, r, 320);
    assert_eq!(bypass[80], [0.0; 2]);
    assert_eq!(bypass[160], out[160]);
}
#[test]
fn vocal_mix_is_the_sum_of_its_independent_wet_branches() {
    let mut r = rack();
    r.engines[0].stages[0].delay.kind = DelayKind::Tape;
    r.engines[0].stages[1].algorithm = Algorithm::Room;
    r.engines[0].stages[1].reverb.kind = ReverbKind::Chamber;
    r.engines[0].stages[2].algorithm = Algorithm::Chorus;
    r.engines[0].stages[2].chorus.ensemble = true;
    r.engines[0].pieces = 4;
    r.engines[0].stages[3].algorithm = Algorithm::Exciter;
    let combined = impulse(8000, r, 4096);
    let mut sum = vec![[0.0; 2]; combined.len()];
    for slot in 0..4 {
        let mut solo = r;
        for i in 0..4 {
            solo.engines[0].stages[i].level = if i == slot { 1.0 } else { 0.0 };
        }
        let branch = impulse(8000, solo, sum.len());
        assert!(branch.iter().flatten().any(|v| v.abs() > 1e-4));
        for (total, value) in sum.iter_mut().zip(branch) {
            for c in 0..2 {
                total[c] += value[c];
            }
        }
    }
    for (a, b) in combined.iter().zip(sum) {
        for c in 0..2 {
            assert!((a[c] - b[c]).abs() < 1e-6);
        }
    }
}
fn palette() -> Vec<EffectConfig> {
    let mut result = Vec::new();
    for kind in ReverbKind::ALL {
        let mut s = EffectConfig {
            algorithm: Algorithm::Room,
            ..EffectConfig::default()
        };
        s.reverb.kind = kind;
        result.push(s);
    }
    for kind in DelayKind::ALL {
        let mut s = EffectConfig::default();
        s.delay.kind = kind;
        s.delay.time_ms = 30.0;
        result.push(s);
    }
    for ensemble in [false, true] {
        let mut s = EffectConfig {
            algorithm: Algorithm::Chorus,
            ..EffectConfig::default()
        };
        s.chorus.ensemble = ensemble;
        result.push(s);
    }
    for bright in [false, true] {
        let mut s = EffectConfig {
            algorithm: Algorithm::Exciter,
            ..EffectConfig::default()
        };
        s.exciter.bright = bright;
        result.push(s);
    }
    result
}
#[test]
fn every_flavor_is_wet_finite_and_deterministic_at_low_standard_and_maximum_rates() {
    for rate in [8000, 48000, 192000] {
        for effect in palette() {
            let mut r = rack();
            r.engines[0].mode = EngineMode::Single;
            r.engines[0].stages[0] = effect;
            let a = impulse(rate, r, rate as usize / 6);
            let b = impulse(rate, r, a.len());
            assert_eq!(a, b, "{} at {rate}", effect.label());
            // Exciter is an immediate nonlinear return; its dry exclusion is
            // covered by zero-drive and small-signal spectral regressions.
            if effect.algorithm != Algorithm::Exciter {
                assert_eq!(a[0], [0.0; 2], "dry leak: {}", effect.label());
            }
            assert!(a.iter().flatten().all(|x| x.is_finite() && x.abs() <= 16.0));
            assert!(
                a.iter().flatten().any(|x| x.abs() > 1e-5),
                "silent {}",
                effect.label()
            );
        }
    }
}
#[test]
fn reverb_and_delay_choices_produce_distinct_responses_and_reverbs_decay() {
    let mut responses = Vec::new();
    for effect in palette() {
        let mut r = rack();
        r.engines[0].mode = EngineMode::Single;
        r.engines[0].stages[0] = effect;
        let out = impulse(8000, r, 16000);
        assert!(
            responses.iter().all(|previous| *previous != out),
            "duplicate sound {}",
            effect.label()
        );
        if effect.algorithm == Algorithm::Room {
            let energy = |slice: &[[f32; 2]]| slice.iter().flatten().map(|x| x * x).sum::<f32>();
            assert!(
                energy(&out[8000..]) < energy(&out[..8000]) * 0.01,
                "tail did not decay: {}",
                effect.label()
            );
        }
        responses.push(out);
    }
}
#[test]
fn slot_edits_and_slot_bypass_preserve_other_slots_and_other_engine_exactly() {
    let mut r = rack();
    r.routing.layout = Layout::DualStereo;
    r.routing.outputs = [[0, 1], [2, 3]];
    r.engines[0].stages[0].level = 0.0;
    r.engines[0].stages[1].delay.feedback = 0.8;
    r.engines[0].stages[2].delay.feedback = 0.7;
    let mut a = Processor::new(8000, r).unwrap();
    let mut b = Processor::new(8000, r).unwrap();
    warm(&mut a, 8000);
    warm(&mut b, 8000);
    block(&mut a, &[[0.2; 256]; 4]);
    block(&mut b, &[[0.2; 256]; 4]);
    for effect in palette() {
        r.engines[0].stages[0] = EffectConfig {
            level: 0.0,
            ..effect
        };
        r.engines[0].stages[0].bypass = !r.engines[0].stages[0].bypass;
        a.apply(r);
        for _ in 0..3 {
            let (x, _) = block(&mut a, &[[0.0; 256]; 4]);
            let (y, _) = block(&mut b, &[[0.0; 256]; 4]);
            assert_eq!(x, y);
        }
    }
}
#[test]
fn chorus_has_no_dry_branch_and_bypass_drains_its_short_wet_delays() {
    let mut r = rack();
    r.engines[0].mode = EngineMode::Single;
    let s = &mut r.engines[0].stages[0];
    s.algorithm = Algorithm::Chorus;
    s.chorus.depth_ms = 0.0;
    s.chorus.base_ms = 15.0;
    let out = impulse(8000, r, 256);
    assert!(out[..120].iter().all(|x| *x == [0.0; 2]));
    assert_eq!(out[120], [0.5; 2]);
    let mut p = Processor::new(8000, r).unwrap();
    warm(&mut p, 8000);
    block(&mut p, &[[0.5; 256]; 4]);
    r.engines[0].stages[0].bypass = true;
    p.apply(r);
    let (tail, _) = block(&mut p, &[[0.5; 256]; 4]);
    assert!(tail[0].iter().any(|x| *x != 0.0));
    let (silence, _) = block(&mut p, &[[0.5; 256]; 4]);
    assert!(silence[0].iter().all(|x| *x == 0.0));
}
#[test]
fn chorus_continuous_changes_are_bounded_and_stereo_modulation_is_audible_in_output() {
    let mut r = rack();
    r.engines[0].mode = EngineMode::Single;
    r.engines[0].stages[0].algorithm = Algorithm::Chorus;
    let mut p = Processor::new(8000, r).unwrap();
    warm(&mut p, 8000);
    let mut previous = 0.0f32;
    let mut jump = 0.0f32;
    let mut stereo_difference = 0.0f32;
    for chunk in 0..80 {
        if chunk % 10 == 0 {
            let c = &mut r.engines[0].stages[0].chorus;
            c.rate_hz = if chunk % 20 == 0 { 5.0 } else { 0.05 };
            c.depth_ms = if chunk % 20 == 0 { 8.0 } else { 0.0 };
            c.base_ms = if chunk % 20 == 0 { 30.0 } else { 10.0 };
            p.apply(r);
        }
        let input = std::array::from_fn(|_| {
            std::array::from_fn(|n| ((chunk * 256 + n) as f32 * 0.04).sin() * 0.5)
        });
        let (out, meters) = block(&mut p, &input);
        assert_eq!(meters.faults, 0);
        for (left, right) in out[0].iter().zip(out[1]) {
            if chunk > 2 {
                jump = jump.max((*left - previous).abs());
            }
            previous = *left;
            stereo_difference += (*left - right).abs();
        }
    }
    assert!(jump < 0.1, "chorus jump {jump}");
    assert!(stereo_difference > 1.0);
}
#[test]
fn late_multifx_fault_removes_only_that_engines_shared_contribution_and_recovers() {
    let mut r = rack();
    r.engines[0].pieces = shr_fx::model::MAX_STAGES as u8;
    r.routing.layout = Layout::SharedStereo;
    r.engines[1] = r.engines[0];
    let mut reference = r;
    reference.engines[0].level = 0.0;
    let mut a = Processor::new(8000, r).unwrap();
    let mut b = Processor::new(8000, reference).unwrap();
    warm(&mut a, 8000);
    warm(&mut b, 8000);
    block(&mut a, &[[0.2; 256]; 4]);
    block(&mut b, &[[0.2; 256]; 4]);
    let mut poison = [[0.2; 256]; 4];
    poison[0][255] = f32::NAN;
    let (actual, meters) = block(&mut a, &poison);
    let (expected, _) = block(&mut b, &[[0.2; 256]; 4]);
    assert_eq!(meters.faults, 1);
    assert_eq!(actual, expected);
    r.engines[0].mute = true;
    a.apply(r);
    r.engines[0].mute = false;
    a.apply(r);
    let (_, meters) = block(&mut a, &[[0.2; 256]; 4]);
    assert_eq!(meters.faults, 0);
}

#[test]
fn every_slot_count_has_distinct_parallel_echoes_and_stable_bypass_gain() {
    for count in 2..=shr_fx::model::MAX_STAGES {
        let mut r = rack();
        r.engines[0].pieces = count as u8;
        let out = impulse(8000, r, 800);
        let gain = 0.5 / count.max(3) as f32;
        assert_eq!(out.iter().filter(|s| **s != [0.0; 2]).count(), count);
        for i in 0..count {
            assert!((out[(i + 1) * 80][0] - gain).abs() < 1e-6);
        }
        r.engines[0].stages[count - 1].bypass = true;
        let bypassed = impulse(8000, r, 800);
        assert_eq!(bypassed[count * 80], [0.0; 2]);
        assert_eq!(bypassed[80], out[80]);
    }
}

#[test]
fn four_effect_vocal_uses_one_input_and_only_one_physical_stereo_return() {
    let mut r = Rack::default();
    r.routing.layout = Layout::AStereo;
    r.engines[0].mode = EngineMode::MultiFx;
    assert_eq!(r.engines[0].stage_count(), 4);
    let available = Availability {
        inputs: 1,
        outputs: 3,
    };
    r.validate(available).unwrap();
    let mut processor = Processor::new(48000, r).unwrap();
    let mut energy = 0.0;
    for chunk in 0..100 {
        let input: [f32; 256] =
            std::array::from_fn(|n| ((chunk * 256 + n) as f32 * 0.19).sin() * 0.3);
        let mut left = [0.0; 256];
        let mut right = [0.0; 256];
        // JACK still supplies four logical port buffers on a two-output card;
        // availability describes only the assigned physical channels.
        let silence = [0.0; 256];
        let mut unused_left = [1.0; 256];
        let mut unused_right = [1.0; 256];
        let meters = processor.process(
            [&input, &silence, &silence, &silence],
            [&mut left, &mut right, &mut unused_left, &mut unused_right],
            256,
            available,
        );
        assert_eq!(meters.faults | meters.missing, 0);
        assert!(!meters.buffer_fault);
        assert_eq!(unused_left, silence);
        assert_eq!(unused_right, silence);
        energy += left.iter().chain(&right).map(|x| x * x).sum::<f32>();
    }
    assert!(energy > 0.1);
}

#[test]
fn exciter_zero_drive_bypass_and_mute_are_wet_only_with_click_free_controls() {
    for rate in [8000, 48000, 192000] {
        let mut r = rack();
        r.engines[0].mode = EngineMode::Single;
        r.engines[0].stages[0].algorithm = Algorithm::Exciter;
        r.engines[0].stages[0].exciter.drive = 0.0;
        let mut p = Processor::new(rate, r).unwrap();
        warm(&mut p, rate);
        let signal = |chunk: usize| {
            std::array::from_fn(|_| {
                std::array::from_fn(|n| ((chunk * 256 + n) as f32 * 0.1).sin() * 0.4)
            })
        };
        for chunk in 0..4 {
            assert_eq!(block(&mut p, &signal(chunk)).0, [[0.0; 256]; 4]);
        }
        let mut previous = 0.0f32;
        let mut max_jump = 0.0f32;
        let mut peak = 0.0f32;
        for chunk in 4..80 {
            let config = &mut r.engines[0].stages[0].exciter;
            if chunk % 10 == 4 {
                config.drive = if chunk % 20 == 4 { 1.0 } else { 0.0 };
                config.tune_hz = if chunk % 20 == 4 { 600.0 } else { 6000.0 };
                config.tone = config.drive;
                p.apply(r);
            }
            let (out, meters) = block(&mut p, &signal(chunk));
            assert_eq!(meters.faults, 0);
            for sample in out[0] {
                max_jump = max_jump.max((sample - previous).abs());
                peak = peak.max(sample.abs());
                previous = sample;
            }
        }
        assert!(peak > 1e-5, "silent at {rate}");
        assert!(max_jump < 0.03, "jump {max_jump} at {rate}");
        r.engines[0].stages[0].bypass = true;
        p.apply(r);
        for _ in 0..rate / 256 {
            block(&mut p, &signal(80));
        }
        assert_eq!(block(&mut p, &signal(81)).0, [[0.0; 256]; 4]);
        p.panic(1);
        assert_eq!(block(&mut p, &signal(82)).0, [[0.0; 256]; 4]);
    }
}

#[test]
fn expanding_and_shrinking_one_engine_preserves_the_other_tail() {
    let mut r = rack();
    r.routing.layout = Layout::DualStereo;
    r.routing.outputs = [[0, 1], [2, 3]];
    r.engines[1] = r.engines[0];
    r.engines[1].stages[0].delay.feedback = 0.85;
    let mut a = Processor::new(8000, r).unwrap();
    let mut b = Processor::new(8000, r).unwrap();
    warm(&mut a, 8000);
    warm(&mut b, 8000);
    block(&mut a, &[[0.2; 256]; 4]);
    block(&mut b, &[[0.2; 256]; 4]);
    for count in [8, 4, 2, 8, 3] {
        r.engines[0].pieces = count;
        a.apply(r);
        for _ in 0..4 {
            let (x, mx) = block(&mut a, &[[0.0; 256]; 4]);
            let (y, _) = block(&mut b, &[[0.0; 256]; 4]);
            assert_eq!(mx.faults, 0);
            assert_eq!(x[2..], y[2..]);
        }
    }
}
