use shr_fx::{
    midi::{
        Action, Binding, Clock, Control, ControlKind, Mapper, Message, Parameter, TapTempo, Target,
    },
    model::{Availability, Rack, TempoSource},
    storage::{self, LocalConfig, Ports},
    ui::{App, Command},
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static ID: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "fx-test-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn note(velocity: u8) -> Message {
    Message::Note {
        channel: 0,
        number: 60,
        velocity,
    }
}
fn cc(value: u8) -> Message {
    Message::Cc {
        channel: 0,
        number: 7,
        value,
    }
}
#[test]
fn button_edges_consume_repeats_releases_and_zero_velocity() {
    let mut mapper = Mapper::default();
    let b = [Binding {
        channel: 0,
        number: 60,
        kind: ControlKind::Note,
        target: Target::Action(Action::Tap),
    }];
    assert_eq!(
        mapper.map(note(100), &b, |_, _| 0.0),
        Some(Control::Action(Action::Tap))
    );
    assert_eq!(mapper.map(note(100), &b, |_, _| 0.0), None);
    assert_eq!(mapper.map(note(0), &b, |_, _| 0.0), None);
    assert_eq!(
        mapper.map(note(100), &b, |_, _| 0.0),
        Some(Control::Action(Action::Tap))
    );
    assert_eq!(
        mapper.map(
            Message::Note {
                channel: 1,
                number: 60,
                velocity: 100
            },
            &b,
            |_, _| 0.0
        ),
        None
    );
    let b = [Binding {
        channel: 0,
        number: 7,
        kind: ControlKind::ButtonCc,
        target: Target::Action(Action::Mute),
    }];
    assert_eq!(mapper.map(cc(63), &b, |_, _| 0.0), None);
    assert_eq!(
        mapper.map(cc(64), &b, |_, _| 0.0),
        Some(Control::Action(Action::Mute))
    );
    assert_eq!(mapper.map(cc(127), &b, |_, _| 0.0), None);
}
#[test]
fn absolute_pickup_crosses_target_and_resets_after_recall() {
    let mut mapper = Mapper::default();
    let b = [Binding {
        channel: 0,
        number: 7,
        kind: ControlKind::AbsoluteCc,
        target: Target::Parameter {
            engine: 1,
            parameter: Parameter::Level,
        },
    }];
    assert_eq!(mapper.map(cc(0), &b, |_, _| 0.5), Some(Control::Pickup));
    assert!(matches!(
        mapper.map(cc(100), &b, |_, _| 0.5),
        Some(Control::Absolute { engine: 1, .. })
    ));
    mapper.reset_pickup();
    assert_eq!(mapper.map(cc(0), &b, |_, _| 0.8), Some(Control::Pickup));
}
#[test]
fn relative_encoders_are_bounded_and_neutral_values_do_nothing() {
    let mut mapper = Mapper::default();
    let b = [Binding {
        channel: 0,
        number: 7,
        kind: ControlKind::RelativeCc,
        target: Target::Parameter {
            engine: 0,
            parameter: Parameter::Time,
        },
    }];
    for value in [0, 64] {
        assert_eq!(mapper.map(cc(value), &b, |_, _| 0.5), None);
    }
    for (value, steps) in [(1, 1), (127, -1), (63, 8), (65, -8)] {
        assert_eq!(
            mapper.map(cc(value), &b, |_, _| 0.5),
            Some(Control::Relative {
                engine: 0,
                parameter: Parameter::Time,
                steps
            })
        );
    }
}
#[test]
fn tap_and_external_clock_are_bounded_smoothed_and_hold_on_loss_stop() {
    let mut tap = TapTempo::default();
    assert_eq!(tap.tap(0.0), None);
    assert_eq!(tap.tap(0.5), Some(120.0));
    assert_eq!(tap.tap(5.0), None);
    let mut clock = Clock::default();
    let dt = 0.5 / 24.0;
    for i in 0..200 {
        clock.event(Message::Clock, i as f64 * dt);
    }
    assert!((clock.bpm.unwrap() - 120.0).abs() < 0.01);
    let mut rack = Rack::default();
    rack.engines[0].tempo.source = TempoSource::MidiClock;
    rack.engines[1].tempo.source = TempoSource::MidiClock;
    rack.engines[0].tempo.bpm = 100.0;
    rack.engines[1].tempo.bpm = 100.0;
    assert!(clock.update_rack(&mut rack, 199.0 * dt));
    let retained = rack;
    assert!(clock.lost(9.0));
    assert!(!clock.update_rack(&mut rack, 9.0));
    assert_eq!(rack, retained);
    clock.event(Message::Stop, 9.0);
    assert!(clock.lost(9.0));
    assert_eq!(rack, retained);
}
#[test]
fn strict_snapshots_round_trip_without_physical_identities() {
    let temp = Temp::new();
    let rack = Rack::default();
    storage::save_rack(&temp.0, 0, &rack).unwrap();
    assert_eq!(
        storage::load_rack(&temp.0, 0, Availability::ALL).unwrap(),
        rack
    );
    let path = storage::snapshot_path(&temp.0, 0).unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(!text.contains("midi_source"));
    assert!(!text.contains("ports"));
    assert!(storage::snapshot_path(&temp.0, 16).is_err());
    for invalid in [
        text.replacen("\"version\": 3", "\"version\": 99", 1),
        text.replacen("\"version\": 3", "\"unknown\": 1, \"version\": 3", 1),
        text.replace("\"feedback\": 0.45", "\"feedback\": 2.0"),
    ] {
        fs::write(&path, invalid).unwrap();
        assert!(storage::load_rack(&temp.0, 0, Availability::ALL).is_err());
    }
    fs::write(&path, vec![b' '; 65_537]).unwrap();
    assert!(storage::load_rack(&temp.0, 0, Availability::ALL).is_err());
}
#[test]
fn failed_load_and_cancel_keep_complete_active_rack() {
    let temp = Temp::new();
    let mut app = App::new(temp.0.clone(), LocalConfig::default(), false, false);
    app.rack.engines[0].stages[0].delay.time_ms = 921.0;
    let original = app.rack;
    app.handle(Command::Sounds);
    app.handle(Command::Load);
    assert_eq!(app.rack, original);
    app.handle(Command::Routing);
    app.handle(Command::Field(2));
    app.handle(Command::Plus);
    app.handle(Command::Cancel);
    assert_eq!(app.rack, original);
    app.handle(Command::Field(1));
    app.handle(Command::Plus);
    app.handle(Command::Cancel);
    assert_eq!(app.rack, original);
    app.handle(Command::Routing);
    app.draft.routing.outputs[1][0] = app.draft.routing.outputs[0][0];
    app.handle(Command::Apply);
    assert_eq!(app.rack, original);
    assert_eq!(app.page, shr_fx::ui::Page::Routing);
}
#[test]
fn private_config_rejects_duplicates_and_atomic_save_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let temp = Temp::new();
    let mut config = LocalConfig::default();
    config.ports.outputs[0] = Some("example:return-left".into());
    config.ports.outputs[1] = Some("example:return-left".into());
    assert!(config.validate().is_err());
    config.ports = Ports::default();
    storage::save_local(&temp.0, &config).unwrap();
    assert_eq!(storage::load_local(&temp.0).unwrap(), config);
    assert_eq!(
        fs::metadata(temp.0.join("local.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(
        fs::read_dir(&temp.0).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp"))
    );
    let bad = temp.0.join("not-dir");
    fs::write(&bad, b"original").unwrap();
    assert!(storage::save_local(&bad, &config).is_err());
    assert_eq!(fs::read(&bad).unwrap(), b"original");
}
#[test]
fn save_replacement_requires_visible_second_action_and_cancel_preserves_file() {
    let temp = Temp::new();
    let mut app = App::new(temp.0.clone(), LocalConfig::default(), false, false);
    app.handle(Command::Sounds);
    app.handle(Command::Save);
    let saved = app.rack;
    app.rack.engines[0].level = 0.123;
    app.handle(Command::Save);
    assert!(app.overwrite);
    app.handle(Command::Cancel);
    assert_eq!(
        storage::load_rack(&temp.0, 0, Availability::ALL).unwrap(),
        saved
    );
}

#[test]
fn midi_overflow_recovery_requires_release_before_rearming_actions() {
    let mut mapper = Mapper::default();
    mapper.require_releases();
    let b = [Binding {
        channel: 0,
        number: 60,
        kind: ControlKind::Note,
        target: Target::Action(Action::Exit),
    }];
    assert_eq!(mapper.map(note(100), &b, |_, _| 0.0), None);
    assert_eq!(mapper.map(note(0), &b, |_, _| 0.0), None);
    assert_eq!(
        mapper.map(note(100), &b, |_, _| 0.0),
        Some(Control::Action(Action::Exit))
    );
}

#[test]
fn learned_mapping_is_a_cancellable_draft_and_does_not_fire_capture_gesture() {
    let temp = Temp::new();
    let mut app = App::new(temp.0.clone(), LocalConfig::default(), false, false);
    app.handle(Command::Midi);
    app.learn_target = 15; // Exit
    app.learning = true;
    app.midi_message(note(100), 0.0);
    assert!(!app.quit);
    assert!(!app.learning);
    assert!(app.local.bindings.is_empty());
    assert_eq!(app.midi_draft.bindings.len(), 1);
    app.handle(Command::Apply);
    app.midi_message(note(100), 0.1);
    assert!(!app.quit);
    app.midi_message(note(0), 0.2);
    app.midi_message(note(100), 0.3);
    assert!(app.quit);
}

#[test]
fn version_one_sounds_upgrade_without_rewriting_or_changing_original_settings() {
    use shr_fx::model::{Algorithm, EngineMode, VERSION};
    let temp = Temp::new();
    let path = storage::snapshot_path(&temp.0, 0).unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let legacy = include_str!("fixtures/rack-v1.json");
    fs::write(&path, legacy).unwrap();
    let rack = storage::load_rack(&temp.0, 0, Availability::ALL).unwrap();
    assert_eq!(rack.version, VERSION);
    assert!(rack.engines.iter().all(|e| e.mode == EngineMode::Single));
    let [a, b] = rack.engines;
    assert_eq!(a.stages[0].algorithm, Algorithm::Delay);
    assert_eq!(a.stages[0].delay.time_ms, 743.0);
    assert_eq!(a.stages[0].delay.feedback, 0.63);
    assert_eq!(a.stages[0].reverb.decay, 0.63);
    assert_eq!(a.stages[0].delay.damping, 0.22);
    assert_eq!(a.stages[0].reverb.damping, 0.22);
    assert!(a.stages[0].delay.ping_pong);
    assert_eq!(a.stages[0].delay.division, 3);
    assert_eq!(a.stages[0].level, 1.0);
    assert_eq!(a.level, 0.4);
    assert_eq!(a.tempo.bpm, 147.0);
    assert_eq!(b.stages[0].algorithm, Algorithm::Room);
    assert_eq!(b.stages[0].reverb.predelay_ms, 23.0);
    assert_eq!(b.stages[0].reverb.decay, 0.52);
    assert_eq!(b.stages[0].reverb.damping, 0.61);
    assert!(b.bypass);
    assert_eq!(rack.routing.outputs, [[2, 0], [3, 1]]);
    assert_eq!(fs::read_to_string(&path).unwrap(), legacy);
    storage::save_rack(&temp.0, 1, &rack).unwrap();
    assert_eq!(
        storage::load_rack(&temp.0, 1, Availability::ALL).unwrap(),
        rack
    );
    for invalid in [
        legacy.replace("\"version\": 1", "\"version\": 1, \"version\": 1"),
        legacy.replace("\"delay\"", "\"chorus\""),
        legacy.replace("\"version\": 1", "\"version\": 99"),
        legacy.replace("\"time_ms\": 743.0,", ""),
        legacy.replace("\"feedback\": 0.63", "\"feedback\": 1.0"),
    ] {
        fs::write(&path, invalid).unwrap();
        assert!(storage::load_rack(&temp.0, 0, Availability::ALL).is_err());
    }
}
#[test]
fn multifx_snapshot_is_strict_including_inactive_slots_and_has_no_serial_topology() {
    use shr_fx::model::{EngineMode, MAX_STAGES};
    let temp = Temp::new();
    let mut rack = Rack::default();
    for e in &mut rack.engines {
        e.mode = EngineMode::MultiFx;
    }
    rack.engines[0].stages[2].chorus.ensemble = true;
    rack.engines[0].stages[1].level = 0.37;
    rack.engines[1].stages[2].bypass = true;
    storage::save_rack(&temp.0, 0, &rack).unwrap();
    assert_eq!(
        storage::load_rack(&temp.0, 0, Availability::ALL).unwrap(),
        rack
    );
    let path = storage::snapshot_path(&temp.0, 0).unwrap();
    let value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    for invalid in 0..8 {
        let mut bad = value.clone();
        let engine = &mut bad["engines"][0];
        match invalid {
            0 => engine["pieces"] = (MAX_STAGES + 1).into(),
            1 => engine["pieces"] = 0.into(),
            2 => engine["topology"] = "serial".into(),
            3 => {
                engine["stages"]
                    .as_array_mut()
                    .unwrap()
                    .push(value["engines"][0]["stages"][0].clone());
            }
            4 => {
                engine["stages"][2]["chorus"]
                    .as_object_mut()
                    .unwrap()
                    .remove("rate_hz");
            }
            5 => {
                engine["stages"][0]["delay"]["division"] = 5.into();
            }
            6 => {
                engine["mode"] = "single".into();
                engine["stages"][2]["chorus"]["depth_ms"] = 9.into();
            }
            _ => {
                engine["stages"][0]["level"] = 1.1.into();
            }
        }
        fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        let mut app = App::new(temp.0.clone(), LocalConfig::default(), false, false);
        app.rack = rack;
        app.handle(Command::Sounds);
        app.handle(Command::Load);
        assert_eq!(app.rack, rack);
        assert!(
            app.status.starts_with("Kept rack"),
            "invalid case {invalid}"
        );
    }
    let binding = Binding {
        channel: 0,
        number: 7,
        kind: ControlKind::AbsoluteCc,
        target: Target::Parameter {
            engine: 0,
            parameter: Parameter::Slot {
                slot: MAX_STAGES as u8,
                control: shr_fx::midi::SlotParameter::Level,
            },
        },
    };
    assert!(binding.validate().is_err());
}
#[test]
fn slot_controller_targets_keep_engine_and_slot_identity_and_apply_pickup() {
    use shr_fx::midi::SlotParameter;
    let temp = Temp::new();
    let parameter = Parameter::Slot {
        slot: 2,
        control: SlotParameter::ChorusDepth,
    };
    let local = LocalConfig {
        bindings: vec![Binding {
            channel: 0,
            number: 7,
            kind: ControlKind::AbsoluteCc,
            target: Target::Parameter {
                engine: 1,
                parameter,
            },
        }],
        ..LocalConfig::default()
    };
    storage::save_local(&temp.0, &local).unwrap();
    assert_eq!(storage::load_local(&temp.0).unwrap(), local);
    let mut app = App::new(temp.0.clone(), local, false, false);
    let original = app.rack;
    app.midi_message(cc(0), 0.0);
    assert_eq!(app.rack, original);
    app.midi_message(cc(127), 0.1);
    assert_eq!(app.rack.engines[1].stages[2].chorus.depth_ms, 8.0);
    assert_eq!(app.rack.engines[0], original.engines[0]);
    assert_eq!(
        app.rack.engines[1].stages[..2],
        original.engines[1].stages[..2]
    );
    assert_eq!(app.rack.engines[1].level, original.engines[1].level);
}

#[test]
fn explicit_slot_mapping_rejects_unknown_or_missing_nested_fields() {
    let binding = Binding {
        channel: 0,
        number: 7,
        kind: ControlKind::AbsoluteCc,
        target: Target::Parameter {
            engine: 0,
            parameter: Parameter::Slot {
                slot: 2,
                control: shr_fx::midi::SlotParameter::Level,
            },
        },
    };
    let value = serde_json::to_value(binding).unwrap();
    for unknown in [true, false] {
        let mut bad = value.clone();
        let slot = &mut bad["target"]["parameter"]["parameter"]["slot"];
        if unknown {
            slot["extra"] = 1.into();
        } else {
            slot.as_object_mut().unwrap().remove("control");
        }
        assert!(serde_json::from_value::<Binding>(bad).is_err());
    }
}

#[test]
fn version_two_migration_preserves_every_old_field_and_gain_without_rewriting() {
    let temp = Temp::new();
    let path = storage::snapshot_path(&temp.0, 0).unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let text = include_str!("fixtures/rack-v2.json");
    fs::write(&path, text).unwrap();
    let old: serde_json::Value = serde_json::from_str(text).unwrap();
    let rack = storage::load_rack(&temp.0, 0, Availability::ALL).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), text);
    assert_eq!(rack.version, shr_fx::model::VERSION);
    let new: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&rack).unwrap()).unwrap();
    for e in 0..2 {
        assert_eq!(rack.engines[e].stage_count(), 3);
        assert_eq!(rack.engines[e].mix_divisor(), 3);
        for (name, value) in old["engines"][e].as_object().unwrap() {
            if name == "stages" {
                for i in 0..3 {
                    for (name, value) in value[i].as_object().unwrap() {
                        assert_eq!(&new["engines"][e]["stages"][i][name], value);
                    }
                }
            } else {
                assert_eq!(&new["engines"][e][name], value);
            }
        }
    }
    for case in 0..6 {
        let mut bad = old.clone();
        match case {
            0 => bad["version"] = 3.into(),
            1 => bad["engines"][0]["pieces"] = 4.into(),
            2 => bad["engines"][0]["stages"][2]["algorithm"] = "exciter".into(),
            3 => bad["engines"][0]["stages"][2]["chorus"]["extra"] = true.into(),
            4 => {
                bad["engines"][0]["stages"].as_array_mut().unwrap().pop();
            }
            _ => bad["engines"][0]["stages"][2]["chorus"]["rate_hz"] = 99.into(),
        }
        fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(
            storage::load_rack(&temp.0, 0, Availability::ALL).is_err(),
            "case {case}"
        );
    }
    storage::save_rack(&temp.0, 0, &rack).unwrap();
    assert_eq!(
        storage::load_rack(&temp.0, 0, Availability::ALL).unwrap(),
        rack
    );
}

#[test]
fn last_exciter_slot_round_trips_and_has_strict_validation_and_midi_pickup() {
    use shr_fx::{
        midi::SlotParameter,
        model::{Algorithm, EngineMode, MAX_STAGES},
    };
    let temp = Temp::new();
    let last = MAX_STAGES - 1;
    let mut rack = Rack::default();
    rack.engines[1].mode = EngineMode::MultiFx;
    rack.engines[1].pieces = MAX_STAGES as u8;
    rack.engines[1].stages[last].algorithm = Algorithm::Exciter;
    rack.engines[1].stages[last].exciter.bright = true;
    storage::save_rack(&temp.0, 0, &rack).unwrap();
    assert_eq!(
        storage::load_rack(&temp.0, 0, Availability::ALL).unwrap(),
        rack
    );
    let path = storage::snapshot_path(&temp.0, 0).unwrap();
    let good = serde_json::to_value(rack).unwrap();
    for bad in [
        serde_json::json!({"tune_hz": 0, "drive": 0.3, "tone": 0.6, "bright": false}),
        serde_json::json!({"tune_hz": 2500, "drive": 1.1, "tone": 0.6, "bright": false}),
        serde_json::json!({"tune_hz": 2500, "drive": 0.3, "tone": -0.1, "bright": false}),
        serde_json::json!({"tune_hz": 2500, "drive": 0.3, "tone": 0.6}),
        serde_json::json!({"tune_hz": 2500, "drive": 0.3, "tone": 0.6, "bright": false, "extra": 0}),
    ] {
        let mut value = good.clone();
        value["engines"][1]["mode"] = "single".into(); // inactive still strict
        value["engines"][1]["stages"][last]["exciter"] = bad;
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(storage::load_rack(&temp.0, 0, Availability::ALL).is_err());
    }
    let local = LocalConfig {
        bindings: vec![Binding {
            channel: 0,
            number: 7,
            kind: ControlKind::AbsoluteCc,
            target: Target::Parameter {
                engine: 1,
                parameter: Parameter::Slot {
                    slot: last as u8,
                    control: SlotParameter::ExciterDrive,
                },
            },
        }],
        ..LocalConfig::default()
    };
    storage::save_local(&temp.0, &local).unwrap();
    assert_eq!(storage::load_local(&temp.0).unwrap(), local);
    let mut app = App::new(temp.0.clone(), local, false, false);
    app.rack = rack;
    app.midi_message(cc(0), 0.0);
    assert_eq!(app.rack, rack);
    app.midi_message(cc(127), 0.1);
    rack.engines[1].stages[last].exciter.drive = 1.0;
    assert_eq!(app.rack, rack);
}
