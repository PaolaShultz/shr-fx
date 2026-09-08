//! Offline renderer evidence: no JACK, ALSA, files containing user state, or audio.
use ratatui::{Terminal, backend::TestBackend, style::Color};
use shr_fx::{
    midi::{Binding, ControlKind, Message, Target},
    model::{EngineMode, Layout},
    storage::LocalConfig,
    surface::Role,
    ui::{self, App, Command},
};
fn colour(c: Color) -> u8 {
    match c {
        Color::Black => 0,
        Color::Red => 1,
        Color::Green => 2,
        Color::Yellow => 3,
        Color::Blue => 4,
        Color::Magenta => 5,
        Color::Cyan => 6,
        Color::Gray => 7,
        Color::DarkGray => 8,
        Color::LightRed => 9,
        Color::LightGreen => 10,
        Color::LightYellow => 11,
        Color::White => 15,
        _ => 0,
    }
}
fn capture(app: &App, name: &str, screens: &mut Vec<serde_json::Value>) {
    let mut terminal = Terminal::new(TestBackend::new(40, 13)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    let buffer = terminal.backend().buffer();
    let rows = (0..13)
        .map(|y| {
            (0..40)
                .map(|x| buffer.get(x, y).symbol.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    let cells = (0..13).flat_map(|y| (0..40).map(move |x| { let c=buffer.get(x,y); serde_json::json!({"x":x,"y":y,"symbol":c.symbol,"fg":colour(c.fg),"bg":colour(c.bg),"bold":c.modifier.contains(ratatui::style::Modifier::BOLD)}) })).collect::<Vec<_>>();
    screens.push(serde_json::json!({"name":name,"rows":rows,"cells":cells}));
}
fn main() {
    let mut app = App::new(
        std::env::temp_dir().join("shr-fx-gallery-no-user-state"),
        LocalConfig::default(),
        false,
        false,
    );
    let mut screens = Vec::new();
    app.rack.engines[0].mode = EngineMode::MultiFx;
    app.rack.routing.layout = Layout::AStereo;
    for (s, level) in app.rack.engines[0]
        .stages
        .iter_mut()
        .zip([0.32, 0.68, 0.24, 0.18])
    {
        s.level = level;
    }
    capture(&app, "00-rack", &mut screens);
    app.handle(Command::SelectSlot(1));
    capture(&app, "01-vocal", &mut screens);
    app.handle(Command::SelectSlot(0));
    app.handle(Command::Knob(9));
    capture(&app, "02-delay", &mut screens);
    app.handle(Command::ToggleSlot(1));
    app.handle(Command::SelectSlot(1));
    capture(&app, "03-bypass", &mut screens);
    app.rack.engines[0].pieces = 8;
    app.handle(Command::SelectSlot(7));
    capture(&app, "04-slot-eight", &mut screens);
    app.local.bindings.push(Binding {
        channel: 0,
        number: 7,
        kind: ControlKind::AbsoluteCc,
        target: Target::Surface(Role::Sound(10)),
    });
    app.midi_message(
        Message::Cc {
            channel: 0,
            number: 7,
            value: 0,
        },
        0.0,
    );
    capture(&app, "05-pickup-up", &mut screens);
    app.handle(Command::SelectSlot(6));
    app.handle(Command::SelectSlot(7));
    app.midi_message(
        Message::Cc {
            channel: 0,
            number: 7,
            value: 127,
        },
        0.1,
    );
    capture(&app, "06-pickup-down", &mut screens);
    app.handle(Command::More);
    capture(&app, "07-menu", &mut screens);
    app.handle(Command::Controller);
    app.handle(Command::Learn);
    capture(&app, "08-controller-learn", &mut screens);
    app.midi_message(
        Message::Cc {
            channel: 0,
            number: 20,
            value: 10,
        },
        0.2,
    );
    capture(&app, "09-controller-captured", &mut screens);
    app.handle(Command::Cancel);
    app.handle(Command::Routing);
    app.draft.routing.outputs[0] = [0, 0];
    app.handle(Command::Apply);
    capture(&app, "10-routing-repair", &mut screens);
    app.handle(Command::Cancel);
    app.meters.faults = 1;
    app.status = "routine feedback must not hide fault".into();
    capture(&app, "11-fault-offline", &mut screens);
    app.meters.faults = 0;
    app.handle(Command::Panic);
    capture(&app, "12-panic", &mut screens);
    app.handle(Command::Mute);
    app.handle(Command::Effects);
    capture(&app, "13-effects-draft", &mut screens);
    app.handle(Command::Cancel);
    app.handle(Command::Sounds);
    capture(&app, "14-sounds", &mut screens);
    app.handle(Command::Cancel);
    app.handle(Command::SelectSlot(1));
    app.meters.input = [0.08, 0.0];
    app.meters.output = [0.3, 0.0];
    app.status = "Simulated meter values / no audio I/O".into();
    capture(&app, "15-meter-example", &mut screens);
    println!("{}", serde_json::to_string_pretty(&screens).unwrap());
}
