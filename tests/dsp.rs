use shr_fx::{
    dsp::{MAX_FRAMES, Processor},
    model::{Algorithm, Availability, EngineConfig, Layout, Rack, Source},
};
fn rack() -> Rack {
    let mut r = Rack::default();
    for engine in &mut r.engines {
        *engine = EngineConfig {
            level: 1.0,
            ..EngineConfig::default()
        };
        engine.stages[0].delay.time_ms = 10.0;
        engine.stages[0].delay.feedback = 0.0;
        engine.stages[0].delay.damping = 0.0;
        engine.stages[0].reverb.decay = 0.0;
        engine.stages[0].reverb.damping = 0.0;
    }
    r
}
fn block(
    p: &mut Processor,
    input: &[[f32; 4]],
    available: Availability,
) -> (Vec<[f32; 4]>, shr_fx::dsp::Meters) {
    let ins: [Vec<f32>; 4] = std::array::from_fn(|c| input.iter().map(|v| v[c]).collect());
    let mut outs: [Vec<f32>; 4] = std::array::from_fn(|_| vec![99.0; input.len()]);
    let meters = p.process(
        ins.each_ref().map(|v| v.as_slice()),
        outs.each_mut().map(|v| v.as_mut_slice()),
        input.len(),
        available,
    );
    (
        (0..input.len())
            .map(|n| std::array::from_fn(|c| outs[c][n]))
            .collect(),
        meters,
    )
}
fn warm(p: &mut Processor) {
    block(p, &[[0.0; 4]; 1024], Availability::ALL);
}
#[test]
fn delay_impulse_has_no_dry_branch_at_every_supported_rate() {
    for rate in [8000, 44_100, 48_000, 96_000, 192_000] {
        let mut p = Processor::new(rate, rack()).unwrap();
        for _ in 0..3 {
            warm(&mut p);
        }
        let delay = (rate / 100) as usize;
        let mut input = vec![[0.0; 4]; delay + 10];
        input[0] = [1.0, 0.5, 0.0, 0.0];
        let (out, _) = block(&mut p, &input, Availability::ALL);
        assert!(out[..delay].iter().all(|v| *v == [0.0; 4]));
        assert_eq!(out[delay], [0.5, 0.25, 0.0, 0.0]);
    }
    assert!(Processor::new(7999, rack()).is_err());
    assert!(Processor::new(192001, rack()).is_err());
}
#[test]
fn room_is_wet_and_deterministic_and_has_a_decaying_tail() {
    let mut r = rack();
    r.engines[0].stages[0].algorithm = Algorithm::Room;
    let mut a = Processor::new(8000, r).unwrap();
    let mut b = Processor::new(8000, r).unwrap();
    warm(&mut a);
    warm(&mut b);
    let mut input = vec![[0.0; 4]; 8000];
    input[0][0] = 1.0;
    let (x, _) = block(&mut a, &input, Availability::ALL);
    let (y, _) = block(&mut b, &input, Availability::ALL);
    assert_eq!(x, y);
    assert_eq!(x[0], [0.0; 4]);
    assert!(x.iter().skip(200).any(|v| v[0].abs() > 0.001));
    assert!(x[6000..].iter().all(|v| v[0].abs() < 0.001));
}
#[test]
fn bypass_drains_then_silences_and_mute_clears_without_dry() {
    let mut r = rack();
    r.engines[0].stages[0].delay.time_ms = 100.0;
    let mut p = Processor::new(8000, r).unwrap();
    warm(&mut p);
    block(&mut p, &[[1.0, 0.0, 0.0, 0.0]], Availability::ALL);
    r.engines[0].bypass = true;
    p.apply(r);
    let (out, _) = block(&mut p, &[[0.0; 4]; 2000], Availability::ALL);
    assert_eq!(out[799][0], 0.5);
    assert!(out[800..].iter().all(|v| v[0] == 0.0));
    let (out, _) = block(&mut p, &[[1.0; 4]; 2000], Availability::ALL);
    assert!(out.iter().all(|v| v[0] == 0.0));
    r.engines[0].bypass = false;
    p.apply(r);
    warm(&mut p);
    block(&mut p, &[[1.0; 4]; 200], Availability::ALL);
    p.panic(1);
    let (out, _) = block(&mut p, &[[1.0; 4]; 1000], Availability::ALL);
    assert!(out.iter().all(|v| v[0] == 0.0));
    assert!(out.iter().any(|v| v[1] != 0.0));
}
#[test]
fn independent_algorithm_edits_preserve_other_engine_exactly() {
    let mut r = rack();
    r.engines[1].stages[0].delay.feedback = 0.8;
    let mut a = Processor::new(8000, r).unwrap();
    let mut b = Processor::new(8000, r).unwrap();
    warm(&mut a);
    warm(&mut b);
    let input = [[0.2, 0.3, 0.0, 0.0]; 512];
    block(&mut a, &input, Availability::ALL);
    block(&mut b, &input, Availability::ALL);
    r.engines[0].stages[0].algorithm = Algorithm::Room;
    a.apply(r);
    for _ in 0..10 {
        let (a, _) = block(&mut a, &input, Availability::ALL);
        let (b, _) = block(&mut b, &input, Availability::ALL);
        assert!(a.iter().zip(b).all(|(a, b)| a[1] == b[1]));
    }
}
#[test]
fn faults_remove_whole_affected_period_including_shared_contribution() {
    let mut r = rack();
    r.routing.layout = Layout::SharedStereo;
    let mut p = Processor::new(8000, r).unwrap();
    warm(&mut p);
    block(&mut p, &[[0.2, 0.4, 0.0, 0.0]; 512], Availability::ALL);
    let mut input = [[0.2, 0.4, 0.0, 0.0]; 128];
    input[127][0] = f32::NAN;
    let (out, meters) = block(&mut p, &input, Availability::ALL);
    assert_eq!(meters.faults, 1);
    assert!(out.iter().all(|v| v[0] == 0.1 && v[1] == 0.1));
    let (out, _) = block(&mut p, &[[0.2, 0.4, 0.0, 0.0]; 128], Availability::ALL);
    assert!(out.iter().all(|v| v[0] == 0.1));
    r.engines[0].mute = true;
    p.apply(r);
    r.engines[0].mute = false;
    p.apply(r);
    let (_, meters) = block(&mut p, &[[0.2, 0.4, 0.0, 0.0]; 512], Availability::ALL);
    assert_eq!(meters.faults, 0);
}
#[test]
fn missing_ports_and_buffers_preserve_healthy_engine() {
    let mut p = Processor::new(8000, rack()).unwrap();
    warm(&mut p);
    block(&mut p, &[[1.0; 4]; 512], Availability::ALL);
    let (out, meters) = block(
        &mut p,
        &[[1.0; 4]; 128],
        Availability {
            inputs: 2,
            outputs: 15,
        },
    );
    assert_eq!(meters.missing, 1);
    assert!(out.iter().all(|v| v[0] == 0.0 && v[1] == 0.5));
    let input = [1.0; 128];
    let mut outputs = [[3.0; 128]; 4];
    let meters = p.process(
        [&[], &input, &input, &input],
        outputs.each_mut().map(|v| &mut v[..]),
        128,
        Availability::ALL,
    );
    assert!(meters.buffer_fault);
    assert!(outputs[0].iter().all(|v| *v == 0.0));
    assert!(outputs[1].iter().all(|v| *v == 0.5));
    let mut outs = [[3.0; 4]; 4];
    let m = p.process(
        [&[]; 4],
        outs.each_mut().map(|v| &mut v[..]),
        MAX_FRAMES + 1,
        Availability::ALL,
    );
    assert!(m.buffer_fault);
    assert_eq!(outs, [[0.0; 4]; 4]);
}
#[test]
fn synthetic_stereo_inputs_feed_distinct_four_output_pairs() {
    let mut r = rack();
    r.routing.layout = Layout::DualStereo;
    r.routing.inputs = [Source::Stereo([0, 2]), Source::Stereo([1, 3])];
    r.routing.outputs = [[2, 0], [3, 1]];
    let mut p = Processor::new(8000, r).unwrap();
    warm(&mut p);
    let mut input = [[0.0; 4]; 100];
    input[0] = [1.0, 2.0, 3.0, 4.0];
    let (out, _) = block(&mut p, &input, Availability::ALL);
    assert_eq!(out[80], [1.5, 2.0, 0.5, 1.0]);
}
#[test]
fn delay_time_changes_crossfade_without_buffer_jumps() {
    let mut r = rack();
    let mut p = Processor::new(8000, r).unwrap();
    warm(&mut p);
    let mut previous = 0.0f32;
    let mut max_jump = 0.0f32;
    for chunk in 0..80 {
        if chunk == 30 {
            r.engines[0].stages[0].delay.time_ms = 73.0;
            p.apply(r);
        }
        let input = (0..128)
            .map(|n| {
                [
                    ((chunk * 128 + n) as f32 * 0.015).sin() * 0.5,
                    0.0,
                    0.0,
                    0.0,
                ]
            })
            .collect::<Vec<_>>();
        let (out, _) = block(&mut p, &input, Availability::ALL);
        for frame in out {
            if chunk > 2 {
                max_jump = max_jump.max((frame[0] - previous).abs());
            }
            previous = frame[0];
        }
    }
    assert!(max_jump < 0.02, "step {max_jump}");
}
#[test]
fn demanding_feedback_and_rapid_controls_remain_finite() {
    let mut r = rack();
    let mut p = Processor::new(8000, r).unwrap();
    for i in 0..100 {
        r.engines[0].stages[0].delay.feedback = 0.9;
        r.engines[1].stages[0].algorithm = if i % 10 < 5 {
            Algorithm::Room
        } else {
            Algorithm::Delay
        };
        r.engines[0].stages[0].delay.time_ms = 1.0 + (i * 17 % 1999) as f32;
        p.apply(r);
        let (out, _) = block(&mut p, &[[0.8, -0.8, 0.0, 0.0]; 256], Availability::ALL);
        assert!(
            out.iter()
                .flatten()
                .all(|v| v.is_finite() && v.abs() <= 16.0)
        );
    }
}

#[test]
fn mono_ping_pong_alternates_between_stereo_returns() {
    let mut r = rack();
    r.engines[0].stages[0].delay.ping_pong = true;
    r.engines[0].stages[0].delay.feedback = 0.5;
    r.routing.layout = Layout::AStereo;
    r.routing.outputs[0] = [0, 1];
    let mut p = Processor::new(8000, r).unwrap();
    warm(&mut p);
    let mut input = [[0.0; 4]; 250];
    input[0][0] = 1.0;
    let (out, _) = block(&mut p, &input, Availability::ALL);
    assert_eq!(out[80], [0.5, 0.0, 0.0, 0.0]);
    assert_eq!(out[160], [0.0, 0.25, 0.0, 0.0]);
    assert_eq!(out[240], [0.125, 0.0, 0.0, 0.0]);
}

#[test]
fn return_reassignment_fades_then_places_only_on_new_owned_slots() {
    let mut r = rack();
    let mut p = Processor::new(8000, r).unwrap();
    warm(&mut p);
    block(&mut p, &[[1.0; 4]; 512], Availability::ALL);
    r.routing.outputs = [[3, 2], [2, 0]];
    p.apply(r);
    let (fading, _) = block(&mut p, &[[1.0; 4]; 128], Availability::ALL);
    assert!(fading.iter().all(|v| v[2] == 0.0 && v[3] == 0.0));
    let (changed, _) = block(&mut p, &[[1.0; 4]; 256], Availability::ALL);
    assert!(changed.iter().all(|v| v[0] == 0.0 && v[1] == 0.0));
    assert_eq!(changed[255], [0.0, 0.0, 0.5, 0.5]);
}

#[test]
fn room_stereo_input_does_not_leak_into_unexcited_channel_or_engine() {
    let mut r = rack();
    r.engines[0].stages[0].algorithm = Algorithm::Room;
    r.engines[1].stages[0].algorithm = Algorithm::Room;
    r.routing.layout = Layout::DualStereo;
    r.routing.inputs = [Source::Stereo([0, 1]), Source::Stereo([2, 3])];
    r.routing.outputs = [[0, 1], [2, 3]];
    let mut p = Processor::new(8000, r).unwrap();
    warm(&mut p);
    let mut input = [[0.0; 4]; 1024];
    input[0][1] = 1.0;
    let (out, _) = block(&mut p, &input, Availability::ALL);
    assert!(
        out.iter()
            .all(|v| v[0] == 0.0 && v[2] == 0.0 && v[3] == 0.0)
    );
    assert!(out.iter().any(|v| v[1].abs() > 0.001));
}
