use ratatui::{Terminal, backend::TestBackend};
use shr_fx::{
    midi::{Binding, ControlKind, Message, Parameter, Target},
    model::{Algorithm, EngineMode, Layout},
    storage::{self, LocalConfig},
    surface::Role,
    ui::{self, App, Command, Page},
};
use std::sync::atomic::{AtomicU64, Ordering};
struct Rig(App);
impl Rig {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        Self(App::new(
            std::env::temp_dir().join(format!(
                "fx-performance-{}-{}",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            )),
            LocalConfig::default(),
            false,
            false,
        ))
    }
}
impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0.root);
    }
}
fn touch(app: &mut App, command: Command) {
    let hit = app
        .hits()
        .into_iter()
        .find(|h| h.command == command)
        .unwrap_or_else(|| panic!("{command:?} missing on {:?}", app.page));
    app.touch(hit.rect.x + 1, hit.rect.y);
}
// Compatibility fixture for profiles saved by the preceding interface.
// The current hardware profile is exercised in surface_workflow.rs.
fn bindings(app: &mut App) {
    app.local.bindings = (0..32)
        .map(|i| Binding {
            channel: 0,
            number: if i < 16 { i as u8 } else { (40 + i) as u8 },
            kind: if i < 16 {
                ControlKind::AbsoluteCc
            } else {
                ControlKind::Note
            },
            target: Target::Surface(match i {
                0..=15 => Role::Knob(i as u8),
                16..=23 => Role::Pad((i - 16) as u8),
                _ => Role::Button((i - 24) as u8),
            }),
        })
        .collect();
}
fn knob(app: &mut App, number: u8, value: u8) {
    app.midi_message(
        Message::Cc {
            channel: 0,
            number,
            value,
        },
        0.0,
    );
}
fn button_event(app: &mut App, step: u8, velocity: u8) {
    app.midi_message(
        Message::Note {
            channel: 0,
            number: 40 + step,
            velocity,
        },
        0.0,
    );
}
fn press(app: &mut App, step: u8) {
    button_event(app, step, 100);
    button_event(app, step, 0);
}
fn choose(app: &mut App, command: Command) {
    for _ in 0..app.hits().len() {
        if app.hits()[app.focus].command == command {
            press(app, 30);
            return;
        }
        press(app, 25);
    }
    panic!("Controller cannot reach {command:?} on {:?}", app.page);
}
fn vocal(app: &mut App) {
    touch(app, Command::More);
    touch(app, Command::Effects);
    touch(app, Command::Field(0));
    touch(app, Command::Plus);
    touch(app, Command::Apply);
    touch(app, Command::More);
    touch(app, Command::Routing);
    touch(app, Command::Field(2));
    for _ in 0..6 {
        touch(app, Command::Plus);
    }
    touch(app, Command::Apply);
    assert_eq!(app.rack.routing.layout, Layout::AStereo);
    assert_eq!(app.rack.engines[0].stage_count(), 4);
    assert_eq!(
        app.rack.engines[0].stages[..4]
            .iter()
            .map(|s| s.algorithm)
            .collect::<Vec<_>>(),
        Algorithm::ALL
    );
}
#[test]
fn legacy_actions_keep_save_recall_routing_panic_exit_compatibility() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    vocal(app);
    bindings(app);
    let original = app.rack;
    press(app, 25); // next effect
    assert_eq!(app.context().slot, 1);
    press(app, 26); // page
    assert_eq!(app.context().page, 1);
    let context = app.context();
    press(app, 30); // menu
    choose(app, Command::Sounds);
    choose(app, Command::Save);
    assert!(storage::snapshot_path(&app.root, 0).unwrap().exists());
    press(app, 16); // pad stays live in menus
    assert!(app.rack.engines[0].stages[0].bypass);
    choose(app, Command::Load);
    assert_eq!(app.rack, original);
    press(app, 29); // cancel/back
    assert_eq!(app.context(), context);
    press(app, 30);
    choose(app, Command::Routing);
    choose(app, Command::Cancel);
    assert_eq!(app.context(), context);
    press(app, 31);
    assert!(app.rack.engines.iter().all(|e| e.mute));
    press(app, 30);
    choose(app, Command::Exit);
    assert!(app.quit);
}
#[test]
fn pads_are_independent_of_selection_page_count_gain_and_empty_slots() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    vocal(app);
    bindings(app);
    app.handle(Command::SelectSlot(2));
    app.handle(Command::ParameterPage);
    let context = app.context();
    let focus = app.focus;
    let original = app.rack;
    for slot in 0..8 {
        press(app, 16 + slot);
        let mut expected = original;
        if slot < 4 {
            expected.engines[0].stages[slot as usize].bypass = true;
        }
        assert_eq!(app.rack, expected);
        assert_eq!(app.context(), context);
        assert_eq!(app.focus, focus);
        assert_eq!(app.rack.engines[0].mix_divisor(), 4);
        press(app, 16 + slot);
        assert_eq!(app.rack, original);
    }
    for slot in 0..8 {
        app.handle(Command::SelectSlot(slot));
        assert_eq!(app.context().slot, slot);
        app.handle(Command::More);
        app.handle(Command::Cancel);
        assert_eq!(app.context().slot, slot);
    }
}
#[test]
fn absolute_pickup_rearms_changed_targets_and_preserves_levels_and_explicit_targets() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    vocal(app);
    bindings(app);
    app.rack.engines[0].stages[0].level = 0.6;
    app.rack.engines[1].stages[0].level = 0.2;
    knob(app, 0, 76);
    assert_eq!(app.rack.engines[0].stages[0].level, 76.0 / 127.0);
    knob(app, 9, 63); // delay feedback pickup near half
    let old_feedback = app.rack.engines[0].stages[0].delay.feedback;
    press(app, 25); // reverb
    knob(app, 0, 90); // still owns same wet target, no pickup needed
    assert_eq!(app.rack.engines[0].stages[0].level, 90.0 / 127.0);
    let decay = app.rack.engines[0].stages[1].reverb.decay;
    knob(app, 9, 100);
    assert_eq!(app.rack.engines[0].stages[1].reverb.decay, decay);
    assert!(app.status.contains("DOWN"));
    knob(app, 9, 0);
    assert_eq!(app.rack.engines[0].stages[1].reverb.decay, 0.0); // crossed target
    assert_eq!(app.rack.engines[0].stages[0].delay.feedback, old_feedback);
    press(app, 26); // page 2: knob 10 is return
    let level = app.rack.engines[0].level;
    knob(app, 9, 0);
    assert_eq!(app.rack.engines[0].level, level);
    press(app, 27); // B; absolute wet knob must not jump
    knob(app, 0, 90);
    assert_eq!(app.rack.engines[1].stages[0].level, 0.2);
    knob(app, 0, 0);
    assert_eq!(app.rack.engines[1].stages[0].level, 0.0);
    // Legacy explicit target does not change with selected engine.
    app.local.bindings.push(Binding {
        channel: 1,
        number: 7,
        kind: ControlKind::AbsoluteCc,
        target: Target::Parameter {
            engine: 0,
            parameter: Parameter::Level,
        },
    });
    app.midi_message(
        Message::Cc {
            channel: 1,
            number: 7,
            value: 89,
        },
        0.0,
    );
    app.handle(Command::Engine(0));
    app.midi_message(
        Message::Cc {
            channel: 1,
            number: 7,
            value: 110,
        },
        0.0,
    );
    assert_eq!(app.rack.engines[0].level, 110.0 / 127.0);
}
#[test]
fn held_pads_and_buttons_do_not_retrigger_after_context_changes_or_midi_loss() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    bindings(app);
    button_event(app, 16, 100);
    assert!(app.rack.engines[0].stages[0].bypass);
    press(app, 27); // engine B
    button_event(app, 16, 100);
    assert!(!app.rack.engines[1].stages[0].bypass);
    button_event(app, 16, 0);
    button_event(app, 16, 100);
    assert!(app.rack.engines[1].stages[0].bypass);
    let before = app.rack;
    app.midi_lost();
    button_event(app, 31, 100);
    assert_eq!(app.rack, before);
    button_event(app, 16, 100);
    assert_eq!(app.rack, before);
    button_event(app, 16, 0);
    button_event(app, 16, 100);
    assert!(!app.rack.engines[1].stages[0].bypass);
    button_event(app, 31, 0);
    button_event(app, 31, 100);
    assert!(app.rack.engines.iter().all(|e| e.mute));
}
#[test]
fn guided_learning_reuses_roles_rejects_collisions_and_persists_privately() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    touch(app, Command::More);
    touch(app, Command::Controller);
    let rack = app.rack;
    for i in 0..Role::SETUP_COUNT {
        assert_eq!(app.setup_step, i);
        touch(app, Command::Learn);
        if i < 16 {
            knob(app, i as u8, 10);
        } else {
            button_event(app, i as u8, 100);
            button_event(app, i as u8, 0);
        }
        assert!(!app.learning);
        assert_eq!(app.rack, rack); // capture gestures never perform
        assert_eq!(app.midi_draft.bindings.len(), i + 1);
        touch(app, Command::SetupNext);
    }
    assert!(app.local.bindings.is_empty());
    touch(app, Command::Apply);
    assert_eq!(storage::load_local(&app.root).unwrap(), app.local);
    assert_eq!(app.local.bindings.len(), 26);
    // Current step 1, attempting a duplicate physical knob must retain all mappings.
    touch(app, Command::Learn);
    knob(app, 1, 20);
    assert!(app.learning);
    assert!(app.status.contains("Already mapped"));
    touch(app, Command::Cancel); // cancel capture, then entire draft
    touch(app, Command::Cancel);
    press(app, 24);
    assert_eq!(app.page, Page::Play);
    app.handle(Command::Controller);
    app.setup_step = 16;
    app.learn_kind = ControlKind::Note;
    touch(app, Command::Learn);
    button_event(app, 16, 100);
    assert!(!app.learning);
    touch(app, Command::SetupNext);
    touch(app, Command::Learn);
    button_event(app, 16, 100);
    assert!(app.learning); // held across next learn
    button_event(app, 16, 0);
    button_event(app, 17, 100);
    assert!(!app.learning);
}
#[test]
fn concurrent_live_fields_conflict_explicitly_and_keep_live_preserves_other_draft_edits() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    bindings(app);
    app.local.bindings[9].kind = ControlKind::RelativeCc;
    app.handle(Command::Effects);
    app.handle(Command::EditSlot(0));
    app.handle(Command::Field(1));
    app.handle(Command::Plus); // Tape structure
    app.handle(Command::Field(3));
    app.handle(Command::Plus); // feedback draft
    knob(app, 9, 4);
    let live = app.rack.engines[0].stages[0].delay.feedback;
    let draft = app.draft;
    app.handle(Command::Panic);
    app.handle(Command::Apply);
    assert_eq!(app.page, Page::Effect);
    assert_eq!(app.draft, draft);
    assert!(app.status.contains("conflict"));
    touch(app, Command::KeepLive);
    touch(app, Command::Apply);
    assert_eq!(app.rack.engines[0].stages[0].delay.feedback, live);
    assert_eq!(
        app.rack.engines[0].stages[0].delay.kind,
        shr_fx::model::DelayKind::Tape
    );
    assert!(app.rack.engines.iter().all(|e| e.mute));
    app.handle(Command::Effects);
    let draft = app.draft;
    app.handle(Command::Routing);
    assert_eq!(app.page, Page::Effects);
    assert_eq!(app.draft, draft);
    app.handle(Command::Engine(1));
    assert_eq!(app.selected, 0);
    app.handle(Command::Cancel);
    assert_eq!(app.rack.engines[0].stages[0].delay.feedback, live);
}
#[test]
fn tempo_draft_keeps_live_bpm_and_conflicts_are_repairable() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    bindings(app);
    app.handle(Command::SelectSlot(0));
    app.handle(Command::ParameterPage);
    app.local.bindings[8].kind = ControlKind::RelativeCc;
    app.handle(Command::More);
    app.handle(Command::Tempo);
    app.handle(Command::Field(4));
    app.handle(Command::Plus); // division
    knob(app, 8, 4);
    let bpm = app.rack.engines[0].tempo.bpm;
    app.handle(Command::Apply);
    assert_eq!(app.rack.engines[0].tempo.bpm, bpm);
    app.handle(Command::Tempo);
    app.handle(Command::Field(0));
    app.handle(Command::Plus);
    knob(app, 8, 4);
    let bpm = app.rack.engines[0].tempo.bpm;
    app.handle(Command::Apply);
    assert_eq!(app.page, Page::Tempo);
    touch(app, Command::KeepLive);
    touch(app, Command::Apply);
    assert_eq!(app.rack.engines[0].tempo.bpm, bpm);
}
#[test]
fn routing_ports_roundtrip_and_failed_recall_retain_work_and_context() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    vocal(app);
    bindings(app);
    app.handle(Command::SelectSlot(3));
    app.handle(Command::ParameterPage);
    let context = app.context();
    app.handle(Command::More);
    app.handle(Command::Routing);
    app.handle(Command::Field(0));
    app.handle(Command::Plus);
    let draft = app.draft;
    touch(app, Command::Ports);
    touch(app, Command::Cancel);
    assert_eq!(app.page, Page::Routing);
    assert_eq!(app.draft, draft);
    touch(app, Command::Apply);
    assert_eq!(app.rack.routing, draft.routing);
    assert_eq!(app.context(), context);
    app.handle(Command::Sounds);
    let rack = app.rack;
    app.handle(Command::Load);
    assert_eq!(app.rack, rack);
    assert_eq!(app.page, Page::Sounds);
    app.handle(Command::Cancel);
    assert_eq!(app.context(), context);
}
#[test]
fn native_renderer_bounds_labels_targets_and_fault_priority_are_real() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    let mut terminal = Terminal::new(TestBackend::new(40, 13)).unwrap();
    for count in [1, 4, 8] {
        app.rack.engines[0].mode = if count == 1 {
            EngineMode::Single
        } else {
            EngineMode::MultiFx
        };
        app.rack.engines[0].pieces = count.max(2);
        for slot in 0..8 {
            app.handle(Command::SelectSlot(slot));
            for _ in 0..2 {
                terminal.draw(|f| ui::draw(f, app)).unwrap();
                let hits = app.hits();
                for (i, h) in hits.iter().enumerate() {
                    assert!(h.rect.x + h.rect.width <= 40 && h.rect.y + h.rect.height <= 12);
                    assert!(
                        unicode_width::UnicodeWidthStr::width(h.label.as_str())
                            < h.rect.width as usize,
                        "clipped {:?}: {}",
                        h.command,
                        h.label
                    );
                    for old in &hits[..i] {
                        assert!(
                            old.rect.x + old.rect.width <= h.rect.x
                                || h.rect.x + h.rect.width <= old.rect.x
                                || old.rect.y + old.rect.height <= h.rect.y
                                || h.rect.y + h.rect.height <= old.rect.y
                        );
                    }
                }
                assert_eq!(app.page, Page::Play);
                app.handle(Command::Rack);
                assert_eq!(
                    app.hits()
                        .iter()
                        .filter(
                            |h| matches!(h.command, Command::SelectSlot(_)) && h.rect.height == 2
                        )
                        .count(),
                    8
                );
                app.handle(Command::SelectSlot(slot));
                app.handle(Command::ParameterPage);
            }
        }
    }
    app.meters.faults = 1;
    app.handle(Command::ToggleSlot(0));
    assert!(app.status_line().starts_with("Fault A:"));
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    let row = (0..40)
        .map(|x| terminal.backend().buffer().get(x, 12).symbol.clone())
        .collect::<String>();
    assert!(row.starts_with("Fault A:"));
}
#[test]
fn local_v1_migration_is_read_only_and_surface_validation_is_strict() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    let mut old = LocalConfig {
        version: 1,
        ..LocalConfig::default()
    };
    old.bindings.push(Binding {
        channel: 0,
        number: 1,
        kind: ControlKind::Note,
        target: Target::Action(shr_fx::midi::Action::Tap),
    });
    storage::atomic_json(&app.root.join("local.json"), &old).unwrap();
    let bytes = std::fs::read(app.root.join("local.json")).unwrap();
    let migrated = storage::load_local(&app.root).unwrap();
    assert_eq!(migrated.version, 2);
    assert_eq!(migrated.bindings, old.bindings);
    assert_eq!(std::fs::read(app.root.join("local.json")).unwrap(), bytes);
    bindings(app);
    let mut invalid = app.local.clone();
    invalid.bindings[0].target = Target::Surface(Role::Knob(16));
    assert!(invalid.validate().is_err());
    let mut invalid = app.local.clone();
    invalid.bindings[1].target = invalid.bindings[0].target;
    assert!(invalid.validate().is_err());
    let mut invalid = app.local.clone();
    invalid.bindings[0].kind = ControlKind::Note;
    assert!(invalid.validate().is_err());
    let text = serde_json::to_string(&app.local).unwrap();
    assert!(
        serde_json::from_str::<LocalConfig>(&text.replacen("\"surface\":", "\"unknown\":", 1))
            .is_err()
    );
}

#[test]
fn recall_and_touch_rearm_pickup_while_menu_return_restores_page_and_focus() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    vocal(app);
    bindings(app);
    touch(app, Command::SelectSlot(1));
    touch(app, Command::Knob(10));
    let focus = app.focus;
    let context = app.context();
    touch(app, Command::More);
    touch(app, Command::Sounds);
    touch(app, Command::Cancel);
    assert_eq!(app.focus, focus);
    assert_eq!(app.context(), context);
    knob(app, 9, 63);
    let value = app.rack.engines[0].stages[1].reverb.decay;
    touch(app, Command::Plus);
    let touched = app.rack.engines[0].stages[1].reverb.decay;
    assert!(touched > value);
    knob(app, 9, 100);
    assert_eq!(app.rack.engines[0].stages[1].reverb.decay, touched);
    knob(app, 9, 0); // crosses; then save at low value
    app.handle(Command::Sounds);
    app.handle(Command::Save);
    knob(app, 9, 100);
    assert!(app.rack.engines[0].stages[1].reverb.decay > 0.0);
    app.handle(Command::Load);
    knob(app, 9, 110);
    assert_eq!(app.rack.engines[0].stages[1].reverb.decay, 0.0);
    app.handle(Command::Cancel);
    assert_eq!(app.context(), context);
    app.handle(Command::ParameterPage);
    let a = app.context();
    app.handle(Command::Engine(1));
    app.handle(Command::SelectSlot(7));
    app.handle(Command::Engine(0));
    assert_eq!(app.context(), a);
}
#[test]
fn learning_does_not_acquire_live_pickup_on_a_suppressed_gesture() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    bindings(app);
    app.rack.engines[0].stages[0].level = 0.5;
    app.handle(Command::Controller);
    app.handle(Command::Learn);
    knob(app, 0, 63); // capture passes pickup zone but must not acquire live ownership
    knob(app, 0, 110); // continuing the setup gesture cannot perform after capture
    assert_eq!(app.rack.engines[0].stages[0].level, 0.5);
    app.handle(Command::Cancel);
    knob(app, 0, 110);
    assert_eq!(app.rack.engines[0].stages[0].level, 0.5);
}
#[test]
fn every_pad_preserves_other_sounding_slots_and_engine_tails_sample_for_sample() {
    use shr_fx::{dsp::Processor, model::Availability};
    fn process(p: &mut Processor, level: f32) -> [[f32; 256]; 4] {
        let input = [[level; 256]; 4];
        let mut output = [[0.0; 256]; 4];
        p.process(
            input.each_ref().map(|v| &v[..]),
            output.each_mut().map(|v| &mut v[..]),
            256,
            Availability::ALL,
        );
        output
    }
    for slot in 0..8 {
        let mut rig = Rig::new();
        let app = &mut rig.0;
        bindings(app);
        app.rack.engines[0].mode = EngineMode::MultiFx;
        app.rack.engines[0].pieces = 8;
        app.rack.routing.layout = Layout::DualStereo;
        app.rack.routing.outputs = [[0, 1], [2, 3]];
        for e in &mut app.rack.engines {
            for s in &mut e.stages {
                s.algorithm = Algorithm::Delay;
                s.delay.time_ms = 20.0;
                s.delay.feedback = 0.8;
            }
        }
        // Exclude only the toggled contribution, so equality proves untouched tails/gain.
        app.rack.engines[0].stages[slot].level = 0.0;
        let mut changed = Processor::new(8000, app.rack).unwrap();
        let mut reference = Processor::new(8000, app.rack).unwrap();
        for _ in 0..4 {
            process(&mut changed, 0.2);
            process(&mut reference, 0.2);
        }
        press(app, 16 + slot as u8);
        changed.apply(app.rack);
        for _ in 0..5 {
            let x = process(&mut changed, 0.0);
            let y = process(&mut reference, 0.0);
            assert_eq!(x, y);
            assert!(y.iter().flatten().any(|v| *v != 0.0));
        }
    }
}

#[test]
fn guided_adoption_of_an_explicit_control_requires_visible_role_choice() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    let binding = Binding {
        channel: 0,
        number: 0,
        kind: ControlKind::AbsoluteCc,
        target: Target::Parameter {
            engine: 1,
            parameter: Parameter::Level,
        },
    };
    app.local.bindings.push(binding);
    app.handle(Command::Controller);
    app.handle(Command::Learn);
    knob(app, 0, 20);
    assert_eq!(app.midi_draft.bindings, vec![binding]);
    assert!(app.learn_conflict.is_some());
    touch(app, Command::Cancel);
    assert_eq!(app.midi_draft.bindings, vec![binding]);
    app.handle(Command::Learn);
    knob(app, 0, 30);
    touch(app, Command::UseRole);
    assert_eq!(app.local.bindings, vec![binding]);
    assert_eq!(
        app.midi_draft.bindings[0].target,
        Target::Surface(Role::Navigate)
    );
    touch(app, Command::Apply);
    assert_eq!(
        app.local.bindings[0].target,
        Target::Surface(Role::Navigate)
    );
}
#[test]
fn failed_controller_save_keeps_active_mappings_and_complete_editable_draft() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    bindings(app);
    let root = app.root.clone();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("blocked"), b"not a directory").unwrap();
    app.root = root.join("blocked");
    app.handle(Command::Controller);
    app.setup_step = 1;
    touch(app, Command::ClearRole);
    let (local, draft, rack) = (app.local.clone(), app.midi_draft.clone(), app.rack);
    touch(app, Command::Apply);
    assert_eq!(app.local, local);
    assert_eq!(app.midi_draft, draft);
    assert_eq!(app.rack, rack);
    assert_eq!(app.page, Page::Controller);
    app.root = root;
    touch(app, Command::Apply);
    assert_eq!(app.local, draft);
}
