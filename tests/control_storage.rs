use fx::{
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
        text.replacen("\"version\": 1", "\"version\": 99", 1),
        text.replacen("\"version\": 1", "\"unknown\": 1, \"version\": 1", 1),
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
    app.rack.engines[0].time_ms = 921.0;
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
    assert_eq!(app.page, fx::ui::Page::Routing);
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
