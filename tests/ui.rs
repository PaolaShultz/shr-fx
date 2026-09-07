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
        Command::More,
        Command::Tempo,
        Command::Routing,
        Command::Ports,
        Command::Sounds,
        Command::Midi,
    ] {
        app.handle(command);
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
    app.touch(2, 3);
    assert_eq!(app.selected, 1);
    app.touch(3, 7);
    app.touch(15, 10);
    assert_eq!(app.rack, original);
    app.touch(35, 10);
    assert_eq!(app.rack, original);
    assert!(app.edit.is_none());
    app.touch(3, 7);
    app.touch(15, 10);
    app.touch(25, 10);
    assert_ne!(app.rack.engines[1].feedback, original.engines[1].feedback);
    assert_eq!(app.rack.engines[0], original.engines[0]);
    app.touch(35, 0);
    assert!(app.quit);
    assert!(app.rack.engines.iter().all(|e| e.mute));
}
#[test]
fn midi_navigation_reaches_all_visible_targets_and_applies_same_commands() {
    let mut app = app();
    for page in [
        Command::Main,
        Command::More,
        Command::Routing,
        Command::Tempo,
        Command::Ports,
        Command::Sounds,
        Command::Midi,
    ] {
        app.handle(page);
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
fn switching_a_parameter_field_cannot_apply_an_unrelated_hidden_draft() {
    let mut app = app();
    let original = app.rack;
    app.handle(Command::Field(1));
    app.handle(Command::Plus);
    app.handle(Command::Field(4));
    app.handle(Command::Plus);
    app.handle(Command::Apply);
    assert_eq!(app.rack.engines[0].time_ms, original.engines[0].time_ms);
    assert!(app.rack.engines[0].level > original.engines[0].level);
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
