use ratatui::{Terminal, backend::TestBackend};
use shr_fx::{
    midi::{Binding, ControlKind, Message, Target},
    model::{EngineMode, Layout},
    storage::LocalConfig,
    surface::Role,
    ui::{self, App, Command, Page},
};
use std::sync::atomic::{AtomicU64, Ordering};
struct Rig(App);
impl Rig {
    fn new() -> Self {
        static ID: AtomicU64 = AtomicU64::new(0);
        let mut app = App::new(
            std::env::temp_dir().join(format!(
                "fx-two-clicks-{}-{}",
                std::process::id(),
                ID.fetch_add(1, Ordering::Relaxed)
            )),
            LocalConfig::default(),
            false,
            false,
        );
        app.local.bindings = (0..Role::SETUP_COUNT)
            .map(|i| Binding {
                channel: 0,
                number: if i < 16 { i as u8 } else { 40 + i as u8 },
                kind: if matches!(i, 0 | 8) {
                    ControlKind::RelativeCc
                } else if i < 16 {
                    ControlKind::AbsoluteCc
                } else {
                    ControlKind::Note
                },
                target: Target::Surface(Role::at(i)),
            })
            .collect();
        app.rack.engines[0].stages[0].level = 0.5;
        Self(app)
    }
}
impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0.root);
    }
}
fn turn(app: &mut App, knob: u8, value: u8) {
    app.midi_message(
        Message::Cc {
            channel: 0,
            number: knob - 1,
            value,
        },
        0.0,
    );
}
fn edge(app: &mut App, step: u8, velocity: u8) {
    app.midi_message(
        Message::Note {
            channel: 0,
            number: 40 + step,
            velocity,
        },
        0.0,
    );
}
fn click(app: &mut App, knob: u8) {
    let step = if knob == 1 { 24 } else { 25 };
    edge(app, step, 100);
    edge(app, step, 0);
}
fn point(app: &mut App, command: Command) {
    for _ in 0..app.hits().len() {
        if app.hits()[app.focus].command == command {
            return;
        }
        turn(app, 1, 1);
    }
    panic!("Cannot reach {command:?} on {:?}", app.page);
}
fn choose(app: &mut App, command: Command) {
    point(app, command);
    click(app, 1);
}
#[test]
fn only_eight_pads_and_two_clicks_build_and_operate_the_vocal_rack() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    assert_eq!(app.local.bindings.len(), 26);
    app.local.validate().unwrap();
    choose(app, Command::Effects);
    point(app, Command::Field(0));
    turn(app, 9, 1);
    choose(app, Command::Apply);
    assert_eq!(app.rack.engines[0].mode, EngineMode::MultiFx);
    assert_eq!(app.rack.engines[0].stage_count(), 4);
    choose(app, Command::Routing);
    point(app, Command::Field(2));
    turn(app, 9, 6);
    choose(app, Command::Apply);
    assert_eq!(app.rack.routing.layout, Layout::AStereo);
    choose(app, Command::SelectSlot(1));
    assert_eq!(app.page, Page::Play);
    assert_eq!(app.context().slot, 1);
    choose(app, Command::Wet);
    turn(app, 9, 127);
    let wet = app.rack.engines[0].stages[1].level;
    assert!(wet < 1.0);
    let focus = app.focus;
    edge(app, 17, 100);
    edge(app, 17, 0);
    assert!(app.rack.engines[0].stages[1].bypass);
    assert_eq!(app.context().slot, 1);
    assert_eq!(app.focus, focus);
    choose(app, Command::More);
    choose(app, Command::Sounds);
    choose(app, Command::Save);
    let saved = app.rack;
    edge(app, 17, 100);
    edge(app, 17, 0);
    choose(app, Command::Load);
    assert_eq!(app.rack, saved);
    click(app, 9);
    assert_eq!(app.page, Page::Play);
    assert_eq!(app.context().slot, 1);
    click(app, 9);
    assert_eq!(app.page, Page::Main);
    for slot in 0..8 {
        choose(app, Command::SelectSlot(slot));
        assert_eq!(app.page, Page::Play);
        assert_eq!(app.context().slot, slot);
        click(app, 9);
        assert_eq!(app.page, Page::Main);
    }
    choose(app, Command::Panic);
    assert!(app.rack.engines.iter().all(|e| e.mute));
    choose(app, Command::Exit);
    assert!(app.quit);
}
#[test]
fn ninth_rotary_pickup_follows_selected_value_and_touch_rearms_it() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    app.local.bindings[8].kind = ControlKind::AbsoluteCc;
    choose(app, Command::SelectSlot(0));
    turn(app, 9, 0);
    assert_eq!(app.rack.engines[0].stages[0].level, 0.5);
    assert!(app.status.contains("UP"));
    let mut terminal = Terminal::new(TestBackend::new(40, 13)).unwrap();
    terminal.draw(|frame| ui::draw(frame, app)).unwrap();
    assert_eq!(terminal.backend().buffer().get(4, 7).symbol, "^");
    turn(app, 9, 100);
    assert_eq!(app.rack.engines[0].stages[0].level, 100.0 / 127.0);
    app.handle(Command::Minus);
    let wet = app.rack.engines[0].stages[0].level;
    turn(app, 9, 0);
    assert_eq!(app.rack.engines[0].stages[0].level, wet);
    point(app, Command::Knob(10));
    let feedback = app.rack.engines[0].stages[0].delay.feedback;
    turn(app, 9, 0);
    assert_eq!(app.rack.engines[0].stages[0].delay.feedback, feedback);
    turn(app, 9, 127);
    assert_eq!(app.rack.engines[0].stages[0].delay.feedback, 0.9);
    assert_eq!(app.rack.engines[0].stages[0].level, wet);
    choose(app, Command::SwitchEngine);
    let b = app.rack.engines[1];
    turn(app, 9, 127);
    assert_eq!(app.rack.engines[1], b);
}
#[test]
fn absolute_ninth_rotary_edits_drafts_with_pickup_and_no_live_jump() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    app.local.bindings[8].kind = ControlKind::AbsoluteCc;
    let original = app.rack;
    choose(app, Command::Effects);
    point(app, Command::Field(0));
    turn(app, 9, 127);
    assert_eq!(app.draft.engines[0].mode, EngineMode::Single); // first position cannot cross old context
    turn(app, 9, 0);
    turn(app, 9, 127);
    assert_eq!(app.draft.engines[0].mode, EngineMode::MultiFx);
    assert_eq!(app.rack, original);
    point(app, Command::Field(1));
    turn(app, 9, 127);
    assert_eq!(app.draft.engines[0].pieces, 4);
    turn(app, 9, 0);
    turn(app, 9, 127);
    assert_eq!(app.draft.engines[0].pieces, 8);
    click(app, 9);
    assert_eq!(app.rack, original);
    assert_eq!(app.page, Page::Main);
    choose(app, Command::Effects);
    point(app, Command::Field(0));
    turn(app, 9, 0); // acquire the single-mode draft value
    click(app, 9);
    choose(app, Command::Effects);
    turn(app, 9, 127); // reopening the same editor still needs new pickup
    assert_eq!(app.draft.engines[0].mode, EngineMode::Single);
    turn(app, 9, 0);
    app.midi_lost();
    turn(app, 9, 127); // input recovery must also reset draft pickup
    assert_eq!(app.draft.engines[0].mode, EngineMode::Single);
    assert_eq!(app.rack, original);
}
#[test]
fn held_clicks_do_not_repeat_on_new_screens_and_loss_keeps_the_rack() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    point(app, Command::SelectSlot(0));
    edge(app, 24, 100);
    assert_eq!(app.page, Page::Play);
    point(app, Command::More);
    edge(app, 24, 100);
    assert_eq!(app.page, Page::Play);
    edge(app, 24, 0);
    edge(app, 24, 100);
    assert_eq!(app.page, Page::More);
    edge(app, 25, 100);
    assert_eq!(app.page, Page::Play);
    edge(app, 25, 100);
    assert_eq!(app.page, Page::Play);
    edge(app, 25, 0);
    edge(app, 25, 100);
    assert_eq!(app.page, Page::Main);
    let rack = app.rack;
    app.midi_lost();
    edge(app, 16, 100);
    assert_eq!(app.rack, rack);
    edge(app, 16, 0);
    edge(app, 16, 100);
    assert!(app.rack.engines[0].stages[0].bypass);
}
#[test]
fn actual_render_places_two_rows_of_eight_with_only_first_rotaries_clickable() {
    let mut rig = Rig::new();
    let app = &mut rig.0;
    choose(app, Command::SelectSlot(0));
    let mut terminal = Terminal::new(TestBackend::new(40, 13)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    for row in 0..2 {
        let y = 4 + row * 3;
        for col in 0..8 {
            let label = (0..4)
                .map(|i| {
                    terminal
                        .backend()
                        .buffer()
                        .get(col * 5 + i, y)
                        .symbol
                        .as_str()
                })
                .collect::<String>();
            let number = row * 8 + col + 1;
            if col == 0 {
                assert_eq!(label, format!("[{number:02}]"));
            } else {
                assert!(label.contains(&format!("{number:02}")));
                assert!(!label.contains('['));
            }
        }
    }
    let hits = app.hits();
    for (i, h) in hits.iter().enumerate() {
        assert!(h.rect.x + h.rect.width <= 40 && h.rect.y + h.rect.height <= 12);
        for old in &hits[..i] {
            assert!(
                old.rect.x + old.rect.width <= h.rect.x
                    || h.rect.x + h.rect.width <= old.rect.x
                    || old.rect.y + old.rect.height <= h.rect.y
                    || h.rect.y + h.rect.height <= old.rect.y
            );
        }
    }
    assert!(Role::Press(0).valid());
    assert!(Role::Press(8).valid());
    assert!(!Role::Press(1).valid());
    assert_eq!(
        (0..26)
            .map(Role::at)
            .filter(|r| matches!(r, Role::Pad(_)))
            .count(),
        8
    );
    assert_eq!(
        (0..26)
            .map(Role::at)
            .filter(|r| matches!(r, Role::Button(_)))
            .count(),
        0
    );
}
