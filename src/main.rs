use crossterm::{
    cursor::{Hide, Show},
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        MouseButton, MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use shr_fx::{
    midi::Action,
    storage,
    ui::{self, App, Command},
};
use std::{
    io,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const HELP: &str = "shr-fx 0.3.0 — standalone wet-only send effects rack\n\nUsage: shr-fx [--audio] [--midi] [--data-root DIR]\n       shr-fx --help | --version\n\nDefault: offline 40x13 touch interface; no JACK or MIDI access.\n--audio          Attach to existing JACK only; never start a server.\n--midi           Enable ALSA Sequencer input, using an explicit source.\n--data-root DIR  Private sounds/settings (also FX_DATA_ROOT).\n\nTouch: open an effect; -/+ performs live; Rack returns to all slots.\nKeyboard: Tab/Up/Down focus, Enter activate, Left/Right adjust,\nEsc cancel, a/b engine, t tap, w wet bypass, m mute, Space panic,\nr routing, s sounds, q exit. Ctrl-C/SIGTERM stop and restore terminal.\n\n[ / ] effect, p legacy mapping page, e A/B, g Menu/Confirm, x slot off/on.\nMenu > Effects edits up to eight parallel wet slots per engine.\nMenu > Ports selects exact JACK names. Menu > MIDI selects an input\nand Guided learns 16 rotations, 8 pads and clicks 1/9. No MIDI output.\n";
struct Args {
    root: PathBuf,
    audio: bool,
    midi: bool,
}
fn args() -> Result<Option<Args>, String> {
    let mut args = std::env::args().skip(1);
    let mut root = std::env::var_os("FX_DATA_ROOT").map(PathBuf::from);
    let mut audio = false;
    let mut midi = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print!("{HELP}");
                return Ok(None);
            }
            "--version" | "-V" => {
                println!("shr-fx {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--audio" => audio = true,
            "--midi" => midi = true,
            "--data-root" => {
                root = Some(PathBuf::from(
                    args.next().ok_or("--data-root requires a directory")?,
                ))
            }
            _ => return Err(format!("Unknown argument {arg}; use --help")),
        }
    }
    let root = root
        .or_else(|| std::env::var_os("XDG_DATA_HOME").map(|p| PathBuf::from(p).join("fx")))
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share/fx")))
        .ok_or("Set --data-root or FX_DATA_ROOT")?;
    if root.as_os_str().is_empty() {
        return Err("Data root cannot be empty".into());
    }
    Ok(Some(Args { root, audio, midi }))
}
struct Screen;
impl Screen {
    fn enter() -> io::Result<Self> {
        let guard = Self;
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture, Hide)?;
        Ok(guard)
    }
    fn restore() {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            Show,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
    }
}
impl Drop for Screen {
    fn drop(&mut self) {
        Self::restore();
    }
}
fn run(args: Args) -> Result<(), String> {
    let local = storage::load_local(&args.root)?;
    let stop = Arc::new(AtomicBool::new(false));
    for signal in [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
    ] {
        signal_hook::flag::register(signal, stop.clone()).map_err(|e| e.to_string())?;
    }
    // Declare terminal guard before App: audio shuts down before terminal
    // restoration on both ordinary return and handled error unwinding.
    let _screen = Screen::enter().map_err(|e| e.to_string())?;
    let mut terminal =
        Terminal::new(CrosstermBackend::new(io::stdout())).map_err(|e| e.to_string())?;
    let mut app = App::new(args.root, local, args.audio, args.midi);
    app.start();
    let mut drawn = Instant::now() - Duration::from_secs(1);
    let mut polled = Instant::now() - Duration::from_secs(1);
    while !app.quit && !stop.load(Ordering::Acquire) {
        if polled.elapsed() >= Duration::from_millis(10) {
            app.poll();
            polled = Instant::now();
        }
        if drawn.elapsed() >= Duration::from_millis(33) {
            terminal
                .draw(|frame| ui::draw(frame, &app))
                .map_err(|e| e.to_string())?;
            drawn = Instant::now();
        }
        if event::poll(Duration::from_millis(5)).map_err(|e| e.to_string())? {
            match event::read().map_err(|e| e.to_string())? {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    let repeatable = matches!(
                        key.code,
                        KeyCode::Up
                            | KeyCode::Down
                            | KeyCode::Left
                            | KeyCode::Right
                            | KeyCode::Tab
                            | KeyCode::BackTab
                    );
                    if key.kind == KeyEventKind::Repeat && !repeatable {
                        continue;
                    }
                    let action = match key.code {
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            Some(Action::Exit)
                        }
                        KeyCode::Tab | KeyCode::Down => Some(Action::Next),
                        KeyCode::BackTab | KeyCode::Up => Some(Action::Prev),
                        KeyCode::Left => Some(Action::Decrease),
                        KeyCode::Right => Some(Action::Increase),
                        KeyCode::Enter => Some(Action::Confirm),
                        KeyCode::Esc => Some(Action::Cancel),
                        KeyCode::Char('a') => Some(Action::SelectA),
                        KeyCode::Char('b') => Some(Action::SelectB),
                        KeyCode::Char('[') => Some(Action::PreviousEffect),
                        KeyCode::Char(']') => Some(Action::NextEffect),
                        KeyCode::Char('p') => Some(Action::ParameterPage),
                        KeyCode::Char('e') => Some(Action::SwitchEngine),
                        KeyCode::Char('g') => Some(Action::MenuConfirm),
                        KeyCode::Char('x') => Some(Action::SlotBypass(app.context().slot as u8)),
                        KeyCode::Char('t') => Some(Action::Tap),
                        KeyCode::Char('w') => Some(Action::Bypass),
                        KeyCode::Char('m') => Some(Action::Mute),
                        KeyCode::Char(' ') => Some(Action::Panic),
                        KeyCode::Char('r') => Some(Action::Route),
                        KeyCode::Char('s') => Some(Action::Sounds),
                        KeyCode::Char('q') => Some(Action::Exit),
                        _ => None,
                    };
                    if let Some(action) = action {
                        app.action(action);
                    }
                }
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        let size = terminal.size().map_err(|e| e.to_string())?;
                        if size.width < 40 || size.height < 13 {
                            if mouse.row == 0 {
                                app.handle(Command::Exit);
                            }
                        } else {
                            app.touch(mouse.column, mouse.row);
                        }
                    }
                    MouseEventKind::ScrollDown => app.action(Action::Next),
                    MouseEventKind::ScrollUp => app.action(Action::Prev),
                    _ => {}
                },
                _ => {}
            }
        }
    }
    app.handle(Command::Panic);
    Ok(())
}
fn main() {
    let result = args().and_then(|args| args.map_or(Ok(()), run));
    if let Err(error) = result {
        eprintln!("shr-fx: {error}");
        std::process::exit(1);
    }
}
