use ratatui::{Terminal, backend::TestBackend};
use shr_fx::{
    midi::Action,
    storage::LocalConfig,
    ui::{self, App, Command, Page},
};
fn app() -> App {
    App::new(
        std::env::temp_dir().join("fx-unused-ui-test"),
        LocalConfig::default(),
        false,
        false,
    )
}
#[test]
fn every_screen_fits_native_size_and_reserves_status_and_control_rows() {
    let mut app = app();
    let mut terminal = Terminal::new(TestBackend::new(40, 13)).unwrap();
    for command in [
        Command::Main,
        Command::SelectSlot(0),
        Command::More,
        Command::Effects,
        Command::EditSlot(0),
        Command::EffectTime,
        Command::Tempo,
        Command::Routing,
        Command::Ports,
        Command::Sounds,
        Command::Midi,
        Command::Controller,
    ] {
        open(&mut app, command);
        app.status = "SENTINEL STATUS".into();
        terminal.draw(|f| ui::draw(f, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        let status = (0..40)
            .map(|x| buffer.get(x, 12).symbol.as_str())
            .collect::<String>();
        assert!(status.starts_with("SENTINEL STATUS"));
        let hits = app.hits();
        assert!(
            hits.iter()
                .all(|h| h.rect.y < 12 && h.rect.x + h.rect.width <= 40)
        );
        assert_eq!(hits.iter().filter(|h| h.rect.y == 10).count(), 4);
        assert_eq!(hits.iter().filter(|h| h.rect.y == 11).count(), 4);
        assert!(hits.iter().any(|h| h.command == Command::Exit));
        assert!(hits.iter().any(|h| h.command == Command::Panic));
    }
}
#[test]
fn native_touch_targets_reach_engine_edit_apply_cancel_and_exit() {
    let mut app = app();
    let original = app.rack;
    touch_command(&mut app, Command::Engine(1));
    assert_eq!(app.selected, 1);
    touch_command(&mut app, Command::SelectSlot(0));
    touch_command(&mut app, Command::Knob(10));
    app.touch(15, 10); // live decay +
    assert_ne!(
        app.rack.engines[1].stages[0].reverb.decay,
        original.engines[1].stages[0].reverb.decay
    );
    assert_eq!(app.rack.engines[0], original.engines[0]);
    let active = app.rack;
    touch_command(&mut app, Command::Effects);
    touch_command(&mut app, Command::EditSlot(0));
    touch_command(&mut app, Command::Field(1));
    touch_command(&mut app, Command::Plus);
    touch_command(&mut app, Command::Cancel);
    assert_eq!(app.rack, active);
    app.touch(35, 0);
    assert!(app.quit);
    assert!(app.rack.engines.iter().all(|e| e.mute));
}
#[test]
fn midi_navigation_reaches_all_visible_targets_and_applies_same_commands() {
    let mut app = app();
    for page in [
        Command::Main,
        Command::SelectSlot(0),
        Command::More,
        Command::Effects,
        Command::EditSlot(0),
        Command::EffectTime,
        Command::Routing,
        Command::Tempo,
        Command::Ports,
        Command::Sounds,
        Command::Midi,
        Command::Controller,
    ] {
        open(&mut app, page);
        let len = app.hits().len();
        app.focus = 0;
        for expected in 1..=len {
            app.action(Action::Next);
            assert_eq!(app.focus, expected % len);
        }
        app.action(Action::Prev);
        assert_eq!(app.focus, len - 1);
    }
    app.handle(Command::Main);
    app.action(Action::SelectB);
    assert_eq!(app.selected, 1);
    app.action(Action::Mute);
    assert!(app.rack.engines[1].mute);
    assert!(!app.rack.engines[0].mute);
    app.action(Action::Exit);
    assert!(app.quit);
}
#[test]
fn all_eight_physical_slots_are_reachable_without_a_keyboard() {
    let mut app = app();
    app.handle(Command::Ports);
    for i in 0..4 {
        assert!(app.hits().iter().any(|h| h.command == Command::Field(i)));
    }
    let h = app
        .hits()
        .into_iter()
        .find(|h| h.command == Command::PortPage)
        .unwrap();
    app.touch(h.rect.x, h.rect.y);
    for i in 4..8 {
        assert!(app.hits().iter().any(|h| h.command == Command::Field(i)));
    }
    let h = app
        .hits()
        .into_iter()
        .find(|h| h.command == Command::Field(7))
        .unwrap();
    app.touch(h.rect.x, h.rect.y);
    assert_eq!(app.field, 7);
}
#[test]
fn routing_failure_keeps_draft_and_selection_for_repair() {
    let mut app = app();
    app.handle(Command::Routing);
    app.field = 5;
    let original = app.rack;
    app.draft.routing.outputs[1][0] = 0;
    let draft = app.draft;
    app.handle(Command::Apply);
    assert_eq!(app.rack, original);
    assert_eq!(app.draft, draft);
    assert_eq!(app.field, 5);
    assert_eq!(app.page, Page::Routing);
    app.handle(Command::Plus);
    app.handle(Command::Apply);
    assert_eq!(app.page, Page::Main);
}
#[test]
fn tiny_terminal_has_visible_exit_and_no_draw_panic() {
    let app = app();
    let mut terminal = Terminal::new(TestBackend::new(20, 6)).unwrap();
    terminal.draw(|f| ui::draw(f, &app)).unwrap();
    assert_eq!(terminal.backend().buffer().get(0, 0).symbol, "E");
}
#[test]
fn performance_parameters_publish_immediately_and_selection_keeps_live_changes() {
    let mut app = app();
    let original = app.rack;
    app.handle(Command::SelectSlot(0));
    app.handle(Command::Knob(9));
    app.handle(Command::Plus);
    let time = app.rack.engines[0].stages[0].delay.time_ms;
    assert!(time > original.engines[0].stages[0].delay.time_ms);
    app.handle(Command::Wet);
    app.handle(Command::Minus);
    app.handle(Command::Cancel);
    assert_eq!(app.rack.engines[0].stages[0].delay.time_ms, time);
    assert!(app.rack.engines[0].stages[0].level < original.engines[0].stages[0].level);
}

#[test]
fn long_unicode_port_names_can_be_read_in_full_without_losing_characters() {
    use unicode_width::UnicodeWidthStr;
    let name = format!("{}:return-channel-four", "音聲device".repeat(12));
    let mut recovered = String::new();
    let mut offset = 0;
    while offset < name.chars().count() {
        let (part, next) = ui::name_part(&name, offset);
        assert!(part.width() <= 40);
        assert!(next > offset);
        recovered.push_str(&part);
        offset = next;
    }
    assert_eq!(recovered, name);
}

fn touch_command(app: &mut App, command: Command) {
    let hit = app
        .hits()
        .into_iter()
        .find(|h| h.command == command)
        .unwrap_or_else(|| panic!("missing {command:?} on {:?}", app.page));
    app.touch(hit.rect.x + 1, hit.rect.y);
}
#[test]
fn parallel_vocal_slots_are_configurable_by_touch_with_one_cancellable_draft() {
    use shr_fx::model::{DelayKind, EngineMode};
    let mut app = app();
    let original = app.rack;
    touch_command(&mut app, Command::More);
    touch_command(&mut app, Command::Effects);
    touch_command(&mut app, Command::Field(0));
    touch_command(&mut app, Command::Plus);
    touch_command(&mut app, Command::EditSlot(0));
    touch_command(&mut app, Command::Field(1));
    touch_command(&mut app, Command::Plus); // Tape
    touch_command(&mut app, Command::EffectTime);
    touch_command(&mut app, Command::Field(0));
    touch_command(&mut app, Command::Plus); // sync
    touch_command(&mut app, Command::NextSlot);
    assert_eq!(app.effect_slot, 1);
    touch_command(&mut app, Command::Field(5));
    touch_command(&mut app, Command::Minus); // reverb level
    touch_command(&mut app, Command::NextSlot);
    assert_eq!(app.effect_slot, 2);
    touch_command(&mut app, Command::Field(1));
    touch_command(&mut app, Command::Plus); // ensemble
    touch_command(&mut app, Command::Field(6));
    touch_command(&mut app, Command::Plus); // slot bypass
    assert_eq!(app.rack, original);
    let draft = app.draft.engines[0];
    touch_command(&mut app, Command::Apply);
    assert_eq!(app.page, Page::Main);
    assert_eq!(app.rack.engines[0], draft);
    assert_eq!(app.rack.engines[1], original.engines[1]);
    assert_eq!(app.rack.engines[0].mode, EngineMode::MultiFx);
    assert_eq!(app.rack.engines[0].stages[0].delay.kind, DelayKind::Tape);
    assert!(app.rack.engines[0].stages[0].delay.sync);
    assert!(app.rack.engines[0].stages[2].chorus.ensemble);
    assert!(app.rack.engines[0].stages[2].bypass);
    let active = app.rack;
    for i in 0..8 {
        assert!(
            app.hits()
                .iter()
                .any(|h| h.command == Command::SelectSlot(i))
        );
    }
    touch_command(&mut app, Command::SelectSlot(2));
    touch_command(&mut app, Command::Effects);
    touch_command(&mut app, Command::EditSlot(2));
    touch_command(&mut app, Command::Field(2));
    touch_command(&mut app, Command::Plus);
    touch_command(&mut app, Command::PreviousSlot);
    touch_command(&mut app, Command::Cancel);
    assert_eq!(app.rack, active);
}
#[test]
fn effect_apply_preserves_live_controller_changes_tempo_panic_and_other_engine() {
    use shr_fx::midi::{Binding, ControlKind, Message, Parameter, SlotParameter, Target};
    let mut app = app();
    app.local.bindings.push(Binding {
        channel: 0,
        number: 7,
        kind: ControlKind::RelativeCc,
        target: Target::Parameter {
            engine: 0,
            parameter: Parameter::Slot {
                slot: 0,
                control: SlotParameter::DelayFeedback,
            },
        },
    });
    app.handle(Command::Effects);
    app.handle(Command::EditSlot(0));
    app.handle(Command::Field(1));
    app.handle(Command::Plus);
    app.midi_message(
        Message::Cc {
            channel: 0,
            number: 7,
            value: 1,
        },
        1.0,
    );
    let feedback = app.rack.engines[0].stages[0].delay.feedback;
    let tempo = shr_fx::model::Tempo {
        bpm: 133.0,
        source: shr_fx::model::TempoSource::MidiClock,
    };
    app.rack.set_tempo(0, tempo);
    app.handle(Command::Panic);
    let b = app.rack.engines[1];
    app.handle(Command::Apply);
    assert_eq!(app.rack.engines[0].stages[0].delay.feedback, feedback);
    assert_eq!(app.rack.engines[0].tempo, tempo);
    assert!(app.rack.engines[0].mute);
    assert_eq!(app.rack.engines[1], b);
    assert_eq!(
        app.rack.engines[0].stages[0].delay.kind,
        shr_fx::model::DelayKind::Tape
    );
}
#[test]
fn invalid_effect_draft_keeps_selection_and_can_be_repaired_or_cancelled() {
    let mut app = app();
    let original = app.rack;
    app.handle(Command::Effects);
    app.handle(Command::EditSlot(0));
    app.handle(Command::Field(5));
    app.draft.engines[0].stages[0].level = 2.0;
    app.handle(Command::Apply);
    assert_eq!(app.rack, original);
    assert_eq!(app.page, Page::Effect);
    assert_eq!(app.field, 5);
    assert_eq!(app.effect_slot, 0);
    app.handle(Command::Minus);
    app.handle(Command::Apply);
    assert_eq!(app.page, Page::Main);
    app.handle(Command::Effects);
    app.handle(Command::EditSlot(0));
    app.handle(Command::Field(1));
    app.handle(Command::Plus);
    let draft = app.draft;
    app.handle(Command::Engine(1));
    assert_eq!(app.page, Page::Effect);
    assert_eq!(app.selected, 0);
    assert_eq!(app.draft, draft);
    app.handle(Command::Cancel);
    app.handle(Command::Engine(1));
    app.handle(Command::Effects);
    assert_eq!(app.draft, app.rack);
    app.handle(Command::Apply);
    assert_eq!(app.rack.engines[0], original.engines[0]);
}
#[test]
fn all_flavors_and_slot_pages_fit_and_are_reachable_by_controller_navigation() {
    use shr_fx::model::{Algorithm, DelayKind, EngineMode, ReverbKind};
    let mut app = app();
    let mut terminal = Terminal::new(TestBackend::new(40, 13)).unwrap();
    app.rack.engines[0].mode = EngineMode::MultiFx;
    app.rack.engines[0].pieces = shr_fx::model::MAX_STAGES as u8;
    app.action(Action::Effects);
    for slot in 0..shr_fx::model::MAX_STAGES {
        app.handle(Command::EditSlot(slot));
        for algorithm in Algorithm::ALL {
            app.draft.engines[0].stages[slot].algorithm = algorithm;
            for variant in 0..5 {
                let effect = &mut app.draft.engines[0].stages[slot];
                effect.delay.kind = DelayKind::ALL[variant % 4];
                effect.reverb.kind = ReverbKind::ALL[variant];
                effect.chorus.ensemble = variant % 2 == 0;
                terminal.draw(|f| ui::draw(f, &app)).unwrap();
                let hits = app.hits();
                assert_eq!(hits.iter().filter(|h| h.rect.y == 10).count(), 4);
                assert_eq!(hits.iter().filter(|h| h.rect.y == 11).count(), 4);
                for (i, h) in hits.iter().enumerate() {
                    assert!(h.rect.y < 12);
                    assert!(
                        unicode_width::UnicodeWidthStr::width(h.label.as_str())
                            < h.rect.width as usize
                    );
                    assert!(
                        hits[..i]
                            .iter()
                            .all(|old| old.rect.x + old.rect.width <= h.rect.x
                                || h.rect.x + h.rect.width <= old.rect.x
                                || old.rect.y + old.rect.height <= h.rect.y
                                || h.rect.y + h.rect.height <= old.rect.y)
                    );
                }
                app.focus = 0;
                for _ in 0..hits.len() {
                    app.action(Action::Next);
                }
                assert_eq!(app.focus, 0);
            }
        }
    }
    app.focus = app
        .hits()
        .iter()
        .position(|h| h.command == Command::Cancel)
        .unwrap();
    app.action(Action::Confirm);
    assert_eq!(app.page, Page::Main);
}
#[test]
fn selecting_multifx_preserves_the_original_first_slot_family() {
    let mut app = app();
    let original = app.rack.engines[0].stages[0];
    app.handle(Command::Effects);
    app.handle(Command::Field(0));
    app.handle(Command::Plus);
    app.handle(Command::Apply);
    assert_eq!(app.rack.engines[0].mode, shr_fx::model::EngineMode::MultiFx);
    assert_eq!(app.rack.engines[0].stages[0], original);
}

#[test]
fn slot_sync_display_follows_live_clock_while_an_effect_draft_is_open() {
    let mut app = app();
    app.rack.engines[0].stages[0].delay.sync = true;
    app.handle(Command::Effects);
    app.handle(Command::EditSlot(0));
    app.rack.set_tempo(
        0,
        shr_fx::model::Tempo {
            bpm: 60.0,
            source: shr_fx::model::TempoSource::MidiClock,
        },
    );
    assert!(app.hits().iter().any(|h| h.label == "Time: 1000 ms sync"));
    app.handle(Command::Apply);
    assert_eq!(app.rack.engines[0].tempo.bpm, 60.0);
}

#[test]
fn reselecting_current_engine_keeps_effect_draft_and_single_has_no_inactive_count_control() {
    let mut app = app();
    app.handle(Command::Effects);
    assert!(!app.hits().iter().any(|h| h.command == Command::Field(1)));
    app.handle(Command::EditSlot(0));
    app.handle(Command::Field(1));
    app.handle(Command::Plus);
    let draft = app.draft;
    app.action(Action::SelectA);
    assert_eq!(app.page, Page::Effect);
    assert_eq!(app.field, 1);
    assert_eq!(app.draft, draft);
    app.handle(Command::Effects);
    touch_command(&mut app, Command::Engine(0));
    assert_eq!(app.draft, draft);
    touch_command(&mut app, Command::Apply);
    assert_eq!(
        app.rack.engines[0].stages[0].delay.kind,
        shr_fx::model::DelayKind::Tape
    );
}

#[test]
fn eight_slots_page_by_touch_and_midi_with_draft_recovery_and_no_overlap() {
    use shr_fx::model::{Algorithm, MAX_STAGES};
    let mut app = app();
    let original = app.rack;
    touch_command(&mut app, Command::More);
    touch_command(&mut app, Command::Effects);
    touch_command(&mut app, Command::Field(0));
    touch_command(&mut app, Command::Plus);
    touch_command(&mut app, Command::Field(1));
    for _ in 4..MAX_STAGES {
        touch_command(&mut app, Command::Plus);
    }
    assert_eq!(app.draft.engines[0].stage_count(), MAX_STAGES);
    let mut terminal = Terminal::new(TestBackend::new(40, 13)).unwrap();
    for bank in 0..3 {
        terminal.draw(|f| ui::draw(f, &app)).unwrap();
        let hits = app.hits();
        for (index, h) in hits.iter().enumerate() {
            assert!(h.rect.y < 12);
            assert!(
                unicode_width::UnicodeWidthStr::width(h.label.as_str()) < h.rect.width as usize
            );
            for old in &hits[..index] {
                assert!(
                    old.rect.x + old.rect.width <= h.rect.x
                        || h.rect.x + h.rect.width <= old.rect.x
                        || old.rect.y + old.rect.height <= h.rect.y
                        || h.rect.y + h.rect.height <= old.rect.y
                );
            }
        }
        for slot in bank * 3..((bank + 1) * 3).min(MAX_STAGES) {
            touch_command(&mut app, Command::EditSlot(slot));
            assert_eq!(app.effect_slot, slot);
            touch_command(&mut app, Command::Field(5));
            touch_command(&mut app, Command::Minus);
            touch_command(&mut app, Command::Effects);
        }
        if bank < 2 {
            // Navigate to the visible page action using only controller actions.
            for _ in 0..app.hits().len() {
                if app.hits()[app.focus].command == Command::SlotPage {
                    break;
                }
                app.action(Action::Next);
            }
            assert_eq!(app.hits()[app.focus].command, Command::SlotPage);
            app.action(Action::Confirm);
        }
    }
    assert_eq!(app.rack, original);
    touch_command(&mut app, Command::EditSlot(MAX_STAGES - 1));
    assert_eq!(
        app.draft.engines[0].stages[MAX_STAGES - 1].algorithm,
        Algorithm::Exciter
    );
    touch_command(&mut app, Command::Field(1));
    touch_command(&mut app, Command::Plus); // Bright
    let expected = app.draft.engines[0];
    touch_command(&mut app, Command::Apply);
    assert_eq!(app.rack.engines[0], expected);
    assert_eq!(app.rack.engines[1], original.engines[1]);
    // Main exposes all eight slots; selection is separate from draft entry.
    touch_command(&mut app, Command::SelectSlot(MAX_STAGES - 1));
    touch_command(&mut app, Command::Effects);
    touch_command(&mut app, Command::EditSlot(MAX_STAGES - 1));
    touch_command(&mut app, Command::Field(3));
    touch_command(&mut app, Command::Plus);
    touch_command(&mut app, Command::Cancel);
    assert_eq!(app.rack.engines[0], expected);
    touch_command(&mut app, Command::More);
    touch_command(&mut app, Command::Effects);
    touch_command(&mut app, Command::Field(1));
    for _ in 2..MAX_STAGES {
        touch_command(&mut app, Command::Minus);
    }
    touch_command(&mut app, Command::EditSlot(1));
    touch_command(&mut app, Command::NextSlot);
    assert_eq!(app.effect_slot, 0);
    touch_command(&mut app, Command::Cancel);
    assert_eq!(app.rack.engines[0], expected);
    touch_command(&mut app, Command::Exit);
    assert!(app.quit);
}

fn open(app: &mut App, command: Command) {
    app.handle(Command::Cancel);
    app.handle(Command::Rack);
    if command == Command::EffectTime {
        app.handle(Command::Effects);
        app.handle(Command::EditSlot(0));
    }
    app.handle(command);
}
