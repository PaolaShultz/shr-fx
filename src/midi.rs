use crate::model::{Rack, Tempo, TempoSource};
use alsa::{
    Direction,
    seq::{
        Addr, ClientIter, EvCtrl, EvNote, EventType, PortCap, PortIter, PortSubscribe, PortType,
        Seq,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    PreviousEffect,
    NextEffect,
    ParameterPage,
    SwitchEngine,
    MenuConfirm,
    SlotBypass(u8),
    Prev,
    Next,
    Confirm,
    Cancel,
    Decrease,
    Increase,
    SelectA,
    SelectB,
    Tap,
    Bypass,
    Mute,
    Panic,
    Route,
    Sounds,
    Midi,
    Exit,
    Save,
    Load,
    Effects,
}
impl Action {
    pub const ALL: [Self; 19] = [
        Self::Prev,
        Self::Next,
        Self::Confirm,
        Self::Cancel,
        Self::Decrease,
        Self::Increase,
        Self::SelectA,
        Self::SelectB,
        Self::Tap,
        Self::Bypass,
        Self::Mute,
        Self::Panic,
        Self::Route,
        Self::Sounds,
        Self::Midi,
        Self::Exit,
        Self::Save,
        Self::Load,
        Self::Effects,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::PreviousEffect => "Previous effect",
            Self::NextEffect => "Next effect",
            Self::ParameterPage => "Parameter page",
            Self::SwitchEngine => "Switch engine",
            Self::MenuConfirm => "Menu/Confirm",
            Self::SlotBypass(_) => "Slot off/on",
            Self::Prev => "Previous",
            Self::Next => "Next",
            Self::Confirm => "Confirm",
            Self::Cancel => "Cancel",
            Self::Decrease => "Decrease",
            Self::Increase => "Increase",
            Self::SelectA => "Select A",
            Self::SelectB => "Select B",
            Self::Tap => "Tap",
            Self::Bypass => "Wet bypass",
            Self::Mute => "Mute",
            Self::Panic => "Panic",
            Self::Route => "Routing",
            Self::Sounds => "Sounds",
            Self::Midi => "MIDI",
            Self::Exit => "Exit",
            Self::Save => "Save",
            Self::Load => "Load",
            Self::Effects => "Effects",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Parameter {
    Slot { slot: u8, control: SlotParameter },
    Time,
    Feedback,
    Damping,
    Level,
    Bpm,
}
impl Parameter {
    pub const ALL: [Self; 5] = [
        Self::Time,
        Self::Feedback,
        Self::Damping,
        Self::Level,
        Self::Bpm,
    ];
    pub fn choices() -> Vec<Self> {
        let mut choices = Self::ALL.to_vec();
        for slot in 0..crate::model::MAX_STAGES as u8 {
            for control in SlotParameter::ALL {
                choices.push(Self::Slot { slot, control });
            }
        }
        choices
    }
    pub fn label(self) -> String {
        match self {
            Self::Time => "Slot 1 time/rate".into(),
            Self::Feedback => "Slot 1 feedback/depth".into(),
            Self::Damping => "Slot 1 damping/base".into(),
            Self::Level => "Return level".into(),
            Self::Bpm => "BPM".into(),
            Self::Slot { slot, control } => format!("Slot {} {}", slot + 1, control.label()),
        }
    }
    fn valid(self) -> bool {
        !matches!(self, Self::Slot { slot, .. } if slot as usize >= crate::model::MAX_STAGES)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotParameter {
    DelaySync,
    DelayDivision,
    DelayTime,
    DelayFeedback,
    DelayDamping,
    Predelay,
    Decay,
    ReverbDamping,
    ChorusRate,
    ChorusDepth,
    ChorusBase,
    Level,
    ExciterTune,
    ExciterDrive,
    ExciterTone,
}
impl SlotParameter {
    pub const ALL: [Self; 15] = [
        Self::DelaySync,
        Self::DelayDivision,
        Self::DelayTime,
        Self::DelayFeedback,
        Self::DelayDamping,
        Self::Predelay,
        Self::Decay,
        Self::ReverbDamping,
        Self::ChorusRate,
        Self::ChorusDepth,
        Self::ChorusBase,
        Self::Level,
        Self::ExciterTune,
        Self::ExciterDrive,
        Self::ExciterTone,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::DelaySync => "delay sync",
            Self::DelayDivision => "delay division",
            Self::DelayTime => "delay time",
            Self::DelayFeedback => "delay feedback",
            Self::DelayDamping => "delay damping",
            Self::Predelay => "predelay",
            Self::Decay => "decay",
            Self::ReverbDamping => "reverb damping",
            Self::ChorusRate => "chorus rate",
            Self::ChorusDepth => "chorus depth",
            Self::ChorusBase => "chorus base",
            Self::Level => "wet level",
            Self::ExciterTune => "exciter tune",
            Self::ExciterDrive => "exciter drive",
            Self::ExciterTone => "exciter tone",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Target {
    Surface(crate::surface::Role),
    Action(Action),
    Parameter { engine: u8, parameter: Parameter },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlKind {
    Note,
    ButtonCc,
    AbsoluteCc,
    RelativeCc,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub channel: u8,
    pub number: u8,
    pub kind: ControlKind,
    pub target: Target,
}
impl Binding {
    pub fn validate(self) -> Result<(), String> {
        if self.channel >= 16 || self.number >= 128 {
            return Err("MIDI channel/number out of range".into());
        }
        if matches!(self.target, Target::Action(Action::SlotBypass(i)) if i >= 8) {
            return Err("Slot out of range".into());
        }
        match self.target {
            Target::Surface(role)
                if role.valid()
                    && role.rotary()
                        == matches!(
                            self.kind,
                            ControlKind::AbsoluteCc | ControlKind::RelativeCc
                        ) =>
            {
                Ok(())
            }
            Target::Action(_) if matches!(self.kind, ControlKind::Note | ControlKind::ButtonCc) => {
                Ok(())
            }
            Target::Parameter { engine, parameter }
                if engine < 2
                    && parameter.valid()
                    && matches!(self.kind, ControlKind::AbsoluteCc | ControlKind::RelativeCc) =>
            {
                Ok(())
            }
            _ => Err("MIDI control kind/target disagree".into()),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MidiSource {
    pub client: String,
    pub port: String,
}
impl MidiSource {
    pub fn label(&self) -> String {
        format!("{}:{}", self.client, self.port)
    }
}
#[derive(Clone, Copy, Debug)]
pub enum Message {
    Note {
        channel: u8,
        number: u8,
        velocity: u8,
    },
    Cc {
        channel: u8,
        number: u8,
        value: u8,
    },
    Clock,
    Start,
    Stop,
    Reset,
}
#[derive(Clone, Copy, Debug)]
pub struct Packet {
    pub message: Message,
    pub time: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Control {
    Surface {
        role: crate::surface::Role,
        value: u8,
        kind: ControlKind,
    },
    Action(Action),
    Absolute {
        engine: usize,
        parameter: Parameter,
        value: f32,
    },
    Relative {
        engine: usize,
        parameter: Parameter,
        steps: i32,
    },
    Pickup,
}

pub struct Mapper {
    notes: [[bool; 128]; 16],
    buttons: [[bool; 128]; 16],
    previous: [Option<f32>; 64],
    picked: [bool; 64],
    targets: [Option<Target>; 64],
    pub pickup: Option<(usize, bool)>,
    pub last_binding: Option<usize>,
    pub press_edge: bool,
}
impl Default for Mapper {
    fn default() -> Self {
        Self {
            notes: [[false; 128]; 16],
            buttons: [[false; 128]; 16],
            previous: [None; 64],
            picked: [false; 64],
            targets: [None; 64],
            pickup: None,
            last_binding: None,
            press_edge: false,
        }
    }
}
impl Mapper {
    pub fn pickup_direction(&self, index: usize, target: f32) -> Option<bool> {
        if self.picked[index] {
            None
        } else {
            self.previous[index].map(|value| value < target)
        }
    }
    pub fn require_releases(&mut self) {
        self.notes = [[true; 128]; 16];
        self.buttons = [[true; 128]; 16];
        self.reset_pickup();
    }
    pub fn invalidate(&mut self, index: usize) {
        self.previous[index] = None;
        self.picked[index] = false;
        if self.pickup.is_some_and(|(i, _)| i == index) {
            self.pickup = None;
        }
    }
    pub fn reset_pickup(&mut self) {
        self.previous = [None; 64];
        self.picked = [false; 64];
        self.pickup = None;
    }
    pub fn map(
        &mut self,
        message: Message,
        bindings: &[Binding],
        current: impl Fn(usize, Parameter) -> f32,
    ) -> Option<Control> {
        self.map_surface(message, bindings, |_| None, current)
    }
    pub fn map_surface(
        &mut self,
        message: Message,
        bindings: &[Binding],
        resolve: impl Fn(crate::surface::Role) -> Option<Target>,
        current: impl Fn(usize, Parameter) -> f32,
    ) -> Option<Control> {
        self.last_binding = None;
        self.press_edge = false;
        let (channel, number, value, note) = match message {
            Message::Note {
                channel,
                number,
                velocity,
            } => (channel, number, velocity, true),
            Message::Cc {
                channel,
                number,
                value,
            } => (channel, number, value, false),
            _ => return None,
        };
        if channel >= 16 || number >= 128 || value >= 128 {
            return None;
        }
        let held = if note {
            &mut self.notes[channel as usize][number as usize]
        } else {
            &mut self.buttons[channel as usize][number as usize]
        };
        let pressed = if note { value > 0 } else { value >= 64 };
        let edge = pressed && !*held;
        *held = pressed;
        self.press_edge = edge;
        for (index, b) in bindings.iter().take(64).enumerate() {
            if b.channel != channel || b.number != number || (b.kind == ControlKind::Note) != note {
                continue;
            }
            self.last_binding = Some(index);
            let resolved = match b.target {
                Target::Surface(role) => resolve(role),
                target => Some(target),
            };
            if self.targets[index] != resolved {
                self.invalidate(index);
                self.targets[index] = resolved;
            }
            return match resolved? {
                Target::Surface(role) => Some(Control::Surface {
                    role,
                    value,
                    kind: b.kind,
                }),
                Target::Action(action) => {
                    if edge {
                        Some(Control::Action(action))
                    } else {
                        None
                    }
                }
                Target::Parameter { engine, parameter } => {
                    let engine = engine as usize;
                    if b.kind == ControlKind::RelativeCc {
                        // MIDI two's complement: 1..63 right, 65..127 left,
                        // 0 and 64 neutral. Bound acceleration to eight steps.
                        let delta = match value {
                            0 | 64 => 0,
                            1..=63 => value as i32,
                            _ => value as i32 - 128,
                        };
                        if delta == 0 {
                            None
                        } else {
                            Some(Control::Relative {
                                engine,
                                parameter,
                                steps: delta.clamp(-8, 8),
                            })
                        }
                    } else {
                        let value = value as f32 / 127.0;
                        let target = current(engine, parameter);
                        let crosses = self.previous[index]
                            .is_some_and(|old| (old - target) * (value - target) <= 0.0);
                        self.picked[index] |= (value - target).abs() <= 0.025 || crosses;
                        self.previous[index] = Some(value);
                        if self.picked[index] {
                            self.pickup = None;
                            Some(Control::Absolute {
                                engine,
                                parameter,
                                value,
                            })
                        } else {
                            self.pickup = Some((index, value < target));
                            Some(Control::Pickup)
                        }
                    }
                }
            };
        }
        None
    }
}
#[derive(Default)]
pub struct Clock {
    last_tick: Option<f64>,
    interval: Option<f64>,
    count: u32,
    stopped: bool,
    pub bpm: Option<f32>,
}
impl Clock {
    pub fn event(&mut self, message: Message, now: f64) {
        match message {
            Message::Stop => {
                self.stopped = true;
                self.last_tick = None;
                self.count = 0;
            }
            Message::Start => {
                self.stopped = false;
                self.last_tick = None;
                self.interval = None;
                self.count = 0;
            }
            Message::Clock => {
                self.stopped = false;
                if let Some(last) = self.last_tick {
                    let dt = now - last;
                    if (60.0 / 300.0 / 24.0 * 0.6..=60.0 / 30.0 / 24.0 * 1.4).contains(&dt) {
                        let old = self.interval.unwrap_or(dt);
                        let smoothed = old * 0.9 + dt * 0.1;
                        self.interval = Some(smoothed);
                        self.count = self.count.saturating_add(1);
                        if self.count >= 24 {
                            self.bpm = Some((60.0 / (smoothed * 24.0)).clamp(30.0, 300.0) as f32);
                        }
                    } else {
                        self.count = 0;
                        self.interval = None;
                    }
                }
                self.last_tick = Some(now);
            }
            _ => {}
        }
    }
    pub fn lost(&self, now: f64) -> bool {
        self.stopped || self.last_tick.is_none_or(|last| now - last > 2.0) || self.count < 24
    }
    pub fn update_rack(&self, rack: &mut Rack, now: f64) -> bool {
        if self.lost(now) {
            return false;
        }
        let Some(bpm) = self.bpm else {
            return false;
        };
        let mut changed = false;
        for e in 0..2 {
            if rack.engines[e].tempo.source == TempoSource::MidiClock
                && (rack.engines[e].tempo.bpm - bpm).abs() >= 0.1
            {
                rack.set_tempo(
                    e,
                    Tempo {
                        bpm,
                        source: TempoSource::MidiClock,
                    },
                );
                changed = true;
            }
        }
        changed
    }
}
#[derive(Default)]
pub struct TapTempo {
    last: Option<f64>,
    intervals: [f64; 4],
    count: usize,
}
impl TapTempo {
    pub fn tap(&mut self, now: f64) -> Option<f32> {
        let last = self.last.replace(now)?;
        let dt = now - last;
        if !(0.2..=2.0).contains(&dt) {
            self.count = 0;
            return None;
        }
        self.intervals[self.count % 4] = dt;
        self.count = self.count.saturating_add(1);
        let n = self.count.min(4);
        Some(
            (60.0 / (self.intervals[..n].iter().sum::<f64>() / n as f64)).clamp(30.0, 300.0) as f32,
        )
    }
}

fn sources(seq: &Seq) -> Vec<(MidiSource, Addr)> {
    let mut result = Vec::new();
    for client in ClientIter::new(seq) {
        if client.get_client() == seq.client_id().unwrap_or(-1) || client.get_client() == 0 {
            continue;
        }
        for port in PortIter::new(seq, client.get_client()) {
            if port
                .get_capability()
                .contains(PortCap::READ | PortCap::SUBS_READ)
                && let (Ok(c), Ok(p)) = (client.get_name(), port.get_name())
            {
                result.push((
                    MidiSource {
                        client: c.into(),
                        port: p.into(),
                    },
                    Addr {
                        client: client.get_client(),
                        port: port.get_port(),
                    },
                ));
            }
        }
    }
    result.sort_by_key(|(s, _)| s.label());
    result
}
pub fn discover() -> Result<Vec<MidiSource>, String> {
    let _errors = alsa::Output::local_error_handler().map_err(|e| e.to_string())?;
    let seq = Seq::open(None, Some(Direction::Capture), true).map_err(|e| e.to_string())?;
    Ok(sources(&seq).into_iter().map(|(s, _)| s).collect())
}
pub struct MidiInput {
    pub receiver: rtrb::Consumer<Packet>,
    pub overflow: Arc<AtomicBool>,
    pub failed: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl MidiInput {
    pub fn start(source: &MidiSource, epoch: Instant) -> Result<Self, String> {
        let _errors = alsa::Output::local_error_handler().map_err(|e| e.to_string())?;
        let seq = Seq::open(None, Some(Direction::Capture), true).map_err(|e| e.to_string())?;
        seq.set_client_name(c"fx").map_err(|e| e.to_string())?;
        let matches = sources(&seq)
            .into_iter()
            .filter(|(s, _)| s == source)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err("MIDI source missing or ambiguous".into());
        }
        let sender = matches[0].1;
        let port = seq
            .create_simple_port(
                c"control",
                PortCap::WRITE | PortCap::SUBS_WRITE,
                PortType::APPLICATION,
            )
            .map_err(|e| e.to_string())?;
        let sub = PortSubscribe::empty().map_err(|e| e.to_string())?;
        sub.set_sender(sender);
        sub.set_dest(Addr {
            client: seq.client_id().map_err(|e| e.to_string())?,
            port,
        });
        seq.subscribe_port(&sub).map_err(|e| e.to_string())?;
        seq.set_client_pool_input(1024).map_err(|e| e.to_string())?;
        let (mut producer, receiver) = rtrb::RingBuffer::new(512);
        let stop = Arc::new(AtomicBool::new(false));
        let overflow = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let (s, o, f) = (stop.clone(), overflow.clone(), failed.clone());
        let identity = source.clone();
        let worker = thread::Builder::new()
            .name("fx-midi".into())
            .spawn(move || {
                let mut input = seq.input();
                let mut inspected = Instant::now();
                while !s.load(Ordering::Relaxed) {
                    if inspected.elapsed() >= Duration::from_millis(500) {
                        let same = seq
                            .get_any_client_info(sender.client)
                            .is_ok_and(|c| c.get_name() == Ok(identity.client.as_str()))
                            && seq
                                .get_any_port_info(sender)
                                .is_ok_and(|p| p.get_name() == Ok(identity.port.as_str()));
                        if !same {
                            f.store(true, Ordering::Release);
                            return;
                        }
                        inspected = Instant::now();
                    }
                    for _ in 0..256 {
                        match input.event_input() {
                            Ok(event) => {
                                if event.get_source() != sender {
                                    continue;
                                }
                                let message = match event.get_type() {
                                    EventType::Noteon | EventType::Noteoff => {
                                        event.get_data::<EvNote>().map(|n| Message::Note {
                                            channel: n.channel,
                                            number: n.note,
                                            velocity: if event.get_type() == EventType::Noteoff {
                                                0
                                            } else {
                                                n.velocity
                                            },
                                        })
                                    }
                                    EventType::Controller => {
                                        event.get_data::<EvCtrl>().and_then(|c| {
                                            if c.param < 128 && (0..128).contains(&c.value) {
                                                Some(Message::Cc {
                                                    channel: c.channel,
                                                    number: c.param as u8,
                                                    value: c.value as u8,
                                                })
                                            } else {
                                                None
                                            }
                                        })
                                    }
                                    EventType::Clock => Some(Message::Clock),
                                    EventType::Start | EventType::Continue => Some(Message::Start),
                                    EventType::Stop => Some(Message::Stop),
                                    EventType::Reset => Some(Message::Reset),
                                    _ => None,
                                };
                                if let Some(message) = message
                                    && producer
                                        .push(Packet {
                                            message,
                                            time: epoch.elapsed().as_secs_f64(),
                                        })
                                        .is_err()
                                {
                                    o.store(true, Ordering::Release);
                                }
                            }
                            Err(e) if e.errno() == 11 => break,
                            Err(e) if e.errno() == 28 => {
                                let _ = input.drop_input();
                                o.store(true, Ordering::Release);
                                break;
                            }
                            Err(_) => {
                                f.store(true, Ordering::Release);
                                return;
                            }
                        }
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            receiver,
            overflow,
            failed,
            stop,
            worker: Some(worker),
        })
    }
}
impl Drop for MidiInput {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
