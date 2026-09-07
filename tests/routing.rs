use fx::model::{Availability, Layout, Rack, Routing, Source};
fn route(layout: Layout, a: Source, b: Source) -> Routing {
    let outputs = match layout {
        Layout::DualMono => [[0, 1], [3, 2]],
        Layout::SharedStereo => [[2, 0], [1, 3]],
        Layout::DualStereo => [[0, 2], [3, 1]],
        Layout::MonoStereo => [[2, 0], [3, 1]],
        Layout::StereoMono => [[2, 0], [3, 1]],
        _ => [[2, 0], [3, 1]],
    };
    Routing {
        inputs: [a, b],
        layout,
        outputs,
    }
}
#[test]
fn all_input_pairs_and_return_layouts_keep_channel_identity() {
    let signal = [1.0, 2.0, 4.0, 8.0];
    for a in Source::choices() {
        for b in Source::choices() {
            for layout in Layout::ALL {
                let r = route(layout, a, b);
                r.validate(Availability::ALL).unwrap();
                let wet = [a.read(signal), b.read(signal)];
                let output = r.place(wet);
                let mut expected = [0.0; 4];
                for (e, width) in layout.widths().into_iter().enumerate() {
                    let ports = if layout == Layout::SharedStereo {
                        r.outputs[0]
                    } else {
                        r.outputs[e]
                    };
                    let gain = if layout == Layout::SharedStereo {
                        0.25
                    } else {
                        0.5
                    };
                    match width {
                        1 => expected[ports[0] as usize] += (wet[e][0] + wet[e][1]) / 2.0 * gain,
                        2 => {
                            expected[ports[0] as usize] += wet[e][0] * gain;
                            expected[ports[1] as usize] += wet[e][1] * gain;
                        }
                        _ => {}
                    }
                }
                assert_eq!(output, expected, "{a:?} {b:?} {layout:?}");
            }
        }
    }
}
#[test]
fn mono_fold_cancels_antiphase_and_shared_has_fixed_headroom() {
    let mut r = Routing::default();
    assert_eq!(r.place([[1.0, -1.0], [0.0; 2]]), [0.0; 4]);
    r.layout = Layout::SharedStereo;
    assert_eq!(r.place([[1.0; 2]; 2]), [0.5, 0.5, 0.0, 0.0]);
    assert_eq!(Source::Sum([0, 2]).read([2.0, 3.0, -2.0, 4.0]), [0.0; 2]);
}
#[test]
fn two_inputs_can_feed_four_outputs_and_three_output_cards_work() {
    let mut r = route(Layout::DualStereo, Source::Mono(0), Source::Mono(1));
    r.validate(Availability {
        inputs: 3,
        outputs: 15,
    })
    .unwrap();
    assert!(
        r.validate(Availability {
            inputs: 3,
            outputs: 3
        })
        .is_err()
    );
    r.layout = Layout::MonoStereo;
    r.outputs = [[2, 0], [0, 1]];
    r.validate(Availability {
        inputs: 3,
        outputs: 7,
    })
    .unwrap();
    r.layout = Layout::StereoMono;
    r.outputs = [[0, 2], [1, 0]];
    r.validate(Availability {
        inputs: 3,
        outputs: 7,
    })
    .unwrap();
}
#[test]
fn invalid_slots_duplicate_returns_and_missing_ports_are_rejected() {
    let mut r = Routing::default();
    r.outputs[1][0] = 0;
    assert!(r.validate(Availability::ALL).is_err());
    r.layout = Layout::SharedStereo;
    r.outputs[0] = [1, 1];
    assert!(r.validate(Availability::ALL).is_err());
    r.inputs[0] = Source::Stereo([0, 0]);
    assert!(r.validate(Availability::ALL).is_err());
    r.inputs[0] = Source::Mono(9);
    assert!(r.validate(Availability::ALL).is_err());
    let mut rack = Rack::default();
    rack.engines[0].feedback = f32::NAN;
    assert!(rack.validate(Availability::ALL).is_err());
}
#[test]
fn disabled_engine_does_not_require_missing_input_or_return() {
    for layout in [
        Layout::AMono,
        Layout::AStereo,
        Layout::BMono,
        Layout::BStereo,
    ] {
        let r = route(layout, Source::Mono(0), Source::Mono(1));
        let e = if layout.widths()[0] == 0 { 1 } else { 0 };
        r.validate(Availability {
            inputs: 1 << e,
            outputs: r.output_mask(e),
        })
        .unwrap();
    }
}
