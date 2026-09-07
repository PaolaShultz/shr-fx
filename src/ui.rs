use crate::{
    audio::Audio,
    dsp::Meters,
    midi::{
        self, Action, Binding, Clock, Control, ControlKind, Mapper, Message, MidiInput, Parameter,
        TapTempo, Target,
    },
    model::{Algorithm, Availability, EngineConfig, Layout, Rack, Tempo, TempoSource, engine_name},
    storage::{self, LocalConfig, Ports},
};
use ratatui::{
    Frame,
    backend::Backend,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::Paragraph,
};
use std::{path::PathBuf, sync::atomic::Ordering, time::Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Main,
    More,
    Tempo,
    Routing,
    Ports,
    Sounds,
    Midi,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Field(usize),
    Engine(usize),
    Minus,
    Plus,
    Apply,
    Cancel,
    Tap,
    Bypass,
    Mute,
    Panic,
    Exit,
    Main,
    More,
    Tempo,
    Routing,
    Ports,
    Sounds,
    Midi,
    Save,
    Load,
    Learn,
    Retry,
    Refresh,
    PortPage,
    NamePart,
}
#[derive(Clone, Debug)]
pub struct Hit {
    pub rect: Rect,
    pub command: Command,
    pub label: String,
}

pub struct App {
    pub rack: Rack,
    pub local: LocalConfig,
    pub page: Page,
    pub selected: usize,
    pub field: usize,
    pub name_offset: usize,
    pub focus: usize,
    pub edit: Option<EngineConfig>,
    pub draft: Rack,
    pub port_draft: Ports,
    pub midi_draft: LocalConfig,
    pub slot: usize,
    pub meters: Meters,
    pub status: String,
    pub quit: bool,
    pub audio: Option<Audio>,
    midi: Option<MidiInput>,
    pub audio_allowed: bool,
    pub midi_allowed: bool,
    pub root: PathBuf,
    epoch: Instant,
    last_graph_poll: Instant,
    mapper: Mapper,
    clock: Clock,
    taps: [TapTempo; 2],
    pub learning: bool,
    pub learn_target: usize,
    pub learn_kind: ControlKind,
    pub port_choices: [Vec<String>; 2],
    pub midi_choices: Vec<midi::MidiSource>,
    pub overwrite: bool,
    pub midi_error: String,
}
impl App {
    pub fn new(root: PathBuf, local: LocalConfig, audio_allowed: bool, midi_allowed: bool) -> Self {
        let rack = Rack::default();
        Self {
            rack,
            draft: rack,
            port_draft: local.ports.clone(),
            midi_draft: local.clone(),
            local,
            page: Page::Main,
            selected: 0,
            field: 1,
            name_offset: 0,
            focus: 5,
            edit: None,
            slot: 0,
            meters: Meters::default(),
            status: "Offline: configure and save sounds".into(),
            quit: false,
            audio: None,
            midi: None,
            audio_allowed,
            midi_allowed,
            root,
            epoch: Instant::now(),
            last_graph_poll: Instant::now() - std::time::Duration::from_secs(1),
            mapper: Mapper::default(),
            clock: Clock::default(),
            taps: std::array::from_fn(|_| TapTempo::default()),
            learning: false,
            learn_target: 0,
            learn_kind: ControlKind::Note,
            port_choices: std::array::from_fn(|_| Vec::new()),
            midi_choices: Vec::new(),
            overwrite: false,
            midi_error: String::new(),
        }
    }
    pub fn start(&mut self) {
        if self.audio_allowed {
            self.retry_audio();
        }
        if self.midi_allowed {
            self.restart_midi();
        }
    }
    pub fn availability(&self) -> Availability {
        self.audio
            .as_ref()
            .map_or(Availability::ALL, |a| a.shared.availability())
    }
    fn publish(&mut self, rack: Rack) -> bool {
        let result = if let Some(audio) = &mut self.audio {
            // Controls unrelated to routing remain usable when a port is lost.
            // Structural routing/recall calls separately require availability.
            rack.validate(Availability::ALL)
                .and_then(|_| audio.submit_controls(rack))
        } else {
            rack.validate(Availability::ALL)
        };
        match result {
            Ok(()) => {
                self.rack = rack;
                true
            }
            Err(e) => {
                self.status = e;
                false
            }
        }
    }
    pub fn retry_audio(&mut self) {
        if !self.audio_allowed {
            self.status = "Audio disabled: launch with --audio".into();
            return;
        }
        drop(self.audio.take());
        match Audio::start(self.rack, self.local.ports.clone()) {
            Ok(audio) => {
                self.audio = Some(audio);
                self.status = "JACK attached; assign ports in More".into();
            }
            Err(e) => {
                self.status = format!("JACK: {e}");
            }
        }
    }
    fn restart_midi(&mut self) {
        drop(self.midi.take());
        self.mapper.reset_pickup();
        self.clock = Clock::default();
        if !self.midi_allowed {
            self.midi_error = "Launch with --midi for MIDI input".into();
            return;
        }
        let Some(source) = &self.local.midi_source else {
            self.midi_error = "Select a MIDI source, then Apply".into();
            return;
        };
        match MidiInput::start(source, self.epoch) {
            Ok(midi) => {
                self.midi = Some(midi);
                self.midi_error.clear();
            }
            Err(e) => {
                self.midi_error = e;
            }
        }
    }
    pub fn poll(&mut self) {
        if let Some(audio) = &self.audio {
            if self.last_graph_poll.elapsed() >= std::time::Duration::from_millis(250) {
                audio.poll();
                self.last_graph_poll = Instant::now();
            }
            let current = audio.shared.meters();
            for e in 0..2 {
                self.meters.input[e] = current.input[e].max(self.meters.input[e] * 0.8);
                self.meters.output[e] = current.output[e].max(self.meters.output[e] * 0.8);
            }
            self.meters.faults = current.faults;
            self.meters.missing = current.missing;
            self.meters.buffer_fault = current.buffer_fault;
        }
        let mut packets = Vec::new();
        if let Some(midi) = &mut self.midi {
            if midi.overflow.swap(false, Ordering::AcqRel) {
                for _ in 0..512 {
                    if midi.receiver.pop().is_err() {
                        break;
                    }
                }
                self.mapper.require_releases();
                self.clock = Clock::default();
                self.status = "MIDI overflow: release and retry".into();
            }
            if midi.failed.load(Ordering::Acquire) {
                self.midi_error = "MIDI input failed; Apply to retry".into();
            }
            for _ in 0..128 {
                if let Ok(packet) = midi.receiver.pop() {
                    packets.push(packet);
                } else {
                    break;
                }
            }
        }
        for packet in packets {
            self.midi_message(packet.message, packet.time);
        }
        let mut rack = self.rack;
        if self
            .clock
            .update_rack(&mut rack, self.epoch.elapsed().as_secs_f64())
        {
            self.publish(rack);
        }
    }
    pub fn midi_message(&mut self, message: Message, time: f64) {
        self.clock.event(message, time);
        if matches!(message, Message::Reset) {
            self.handle(Command::Panic);
            return;
        }
        let rack = self.rack;
        let mapped = self.mapper.map(message, &self.local.bindings, |e, p| {
            normalized(&rack, e, p)
        });
        if self.learning {
            if let Some(Control::Action(action @ (Action::Cancel | Action::Panic | Action::Exit))) =
                mapped
            {
                self.action(action);
                return;
            }
            let source = match (self.learn_kind, message) {
                (
                    ControlKind::Note,
                    Message::Note {
                        channel,
                        number,
                        velocity,
                    },
                ) if velocity > 0 => Some((channel, number)),
                (
                    ControlKind::ButtonCc,
                    Message::Cc {
                        channel,
                        number,
                        value,
                    },
                ) if value >= 64 => Some((channel, number)),
                (
                    ControlKind::AbsoluteCc | ControlKind::RelativeCc,
                    Message::Cc {
                        channel, number, ..
                    },
                ) => Some((channel, number)),
                _ => None,
            };
            if let Some((channel, number)) = source {
                let target = learn_targets()[self.learn_target];
                let binding = Binding {
                    channel,
                    number,
                    kind: self.learn_kind,
                    target,
                };
                if let Err(e) = binding.validate() {
                    self.status = e;
                    return;
                }
                self.midi_draft.bindings.retain(|b| {
                    !(b.target == target
                        || (b.channel == channel
                            && b.number == number
                            && (b.kind == ControlKind::Note)
                                == (binding.kind == ControlKind::Note)))
                });
                if self.midi_draft.bindings.len() >= 64 {
                    self.status = "64 bindings: replace an existing target".into();
                    return;
                }
                self.midi_draft.bindings.push(binding);
                self.learning = false;
                self.status = format!("Captured ch{} #{}; Apply or Cancel", channel + 1, number);
            }
            return;
        }
        match mapped {
            Some(Control::Action(action)) => self.action(action),
            Some(Control::Pickup) => self.status = "Pickup: move knob through shown value".into(),
            Some(Control::Absolute {
                engine,
                parameter,
                value,
            }) => {
                let mut rack = self.rack;
                set_normalized(&mut rack, engine, parameter, value);
                self.publish(rack);
            }
            Some(Control::Relative {
                engine,
                parameter,
                steps,
            }) => {
                let mut rack = self.rack;
                let value =
                    normalized(&rack, engine, parameter) + steps as f32 * parameter_step(parameter);
                set_normalized(&mut rack, engine, parameter, value);
                self.publish(rack);
            }
            None => {}
        }
    }
    pub fn action(&mut self, action: Action) {
        match action {
            Action::Prev | Action::Next => {
                let len = self.hits().len();
                self.focus = wrap(self.focus, if action == Action::Next { 1 } else { -1 }, len);
            }
            Action::Confirm => {
                if let Some(hit) = self.hits().get(self.focus) {
                    self.handle(hit.command);
                }
            }
            Action::Cancel => self.handle(Command::Cancel),
            Action::Decrease => self.handle(Command::Minus),
            Action::Increase => self.handle(Command::Plus),
            Action::SelectA => self.handle(Command::Engine(0)),
            Action::SelectB => self.handle(Command::Engine(1)),
            Action::Tap => self.handle(Command::Tap),
            Action::Bypass => self.handle(Command::Bypass),
            Action::Mute => self.handle(Command::Mute),
            Action::Panic => self.handle(Command::Panic),
            Action::Route => self.handle(Command::Routing),
            Action::Sounds => self.handle(Command::Sounds),
            Action::Midi => self.handle(Command::Midi),
            Action::Exit => self.handle(Command::Exit),
            Action::Save => {
                if self.page == Page::Sounds {
                    self.handle(Command::Save);
                } else {
                    self.handle(Command::Sounds);
                }
            }
            Action::Load => {
                if self.page == Page::Sounds {
                    self.handle(Command::Load);
                } else {
                    self.handle(Command::Sounds);
                }
            }
        }
    }
    pub fn touch(&mut self, x: u16, y: u16) {
        let hits = self.hits();
        if let Some((i, hit)) = hits.iter().enumerate().find(|(_, h)| {
            x >= h.rect.x
                && x < h.rect.x + h.rect.width
                && y >= h.rect.y
                && y < h.rect.y + h.rect.height
        }) {
            self.focus = i;
            self.handle(hit.command);
        }
    }
    fn enter(&mut self, page: Page) {
        self.page = page;
        self.field = 0;
        self.focus = 2;
        self.edit = None;
        self.learning = false;
        self.overwrite = false;
        match page {
            Page::Routing | Page::Tempo => self.draft = self.rack,
            Page::Ports => {
                self.port_draft = self.local.ports.clone();
                self.refresh();
            }
            Page::Midi => {
                self.midi_draft = self.local.clone();
                self.refresh();
            }
            _ => {}
        }
    }
    fn refresh(&mut self) {
        if let Some(audio) = &self.audio {
            self.port_choices = [audio.choices(true), audio.choices(false)];
        }
        if self.page == Page::Midi && self.midi_allowed {
            match midi::discover() {
                Ok(choices) => self.midi_choices = choices,
                Err(e) => self.status = e,
            }
        }
    }
    pub fn handle(&mut self, command: Command) {
        match command {
            Command::Field(field) => {
                if field != self.field {
                    self.edit = None;
                }
                self.name_offset = 0;
                self.field = field;
                if self.page == Page::Main {
                    self.edit.get_or_insert(self.rack.engines[self.selected]);
                }
            }
            Command::Engine(engine) => {
                self.edit = None;
                self.selected = engine;
                self.mapper.reset_pickup();
            }
            Command::Minus | Command::Plus => {
                self.adjust(if command == Command::Plus { 1 } else { -1 })
            }
            Command::Apply => self.apply(),
            Command::Cancel => {
                if self.learning {
                    self.learning = false;
                    self.status = "Learn cancelled; mappings kept".into();
                } else if self.edit.take().is_some() {
                    self.status = "Edit cancelled".into();
                } else {
                    self.enter(Page::Main);
                    self.status = "Draft closed; active rack kept".into();
                }
            }
            Command::Tap => {
                let e = self.selected;
                if self.rack.engines[e].tempo.source == TempoSource::MidiClock {
                    self.status = "External clock: choose Internal to tap".into();
                } else if let Some(bpm) = self.taps[e].tap(self.epoch.elapsed().as_secs_f64()) {
                    let mut rack = self.rack;
                    rack.set_tempo(
                        e,
                        Tempo {
                            bpm,
                            source: TempoSource::Internal,
                        },
                    );
                    if self.publish(rack) {
                        self.mapper.reset_pickup();
                        self.status = format!("{} TAP {:.1} BPM", engine_name(e), bpm);
                    }
                } else {
                    self.status = "Tap again".into();
                }
            }
            Command::Bypass => {
                let mut rack = self.rack;
                rack.engines[self.selected].bypass ^= true;
                if self.publish(rack) {
                    self.status = if rack.engines[self.selected].bypass {
                        "Wet bypass: tail drains"
                    } else {
                        "New excitation enabled"
                    }
                    .into();
                }
            }
            Command::Mute => {
                let e = self.selected;
                let mut rack = self.rack;
                rack.engines[e].mute ^= true;
                if rack.engines[e].mute {
                    if let Some(audio) = &mut self.audio {
                        audio.shared.panic.fetch_or(1 << e, Ordering::Release);
                        let _ = audio.submit_controls(rack);
                    }
                    self.rack = rack;
                    self.status = format!("{} muted; Mute to resume", engine_name(e));
                } else {
                    self.publish(rack);
                }
            }
            Command::Panic => {
                if let Some(audio) = &self.audio {
                    audio.shared.panic.fetch_or(3, Ordering::Release);
                }
                let mut rack = self.rack;
                for e in &mut rack.engines {
                    e.mute = true;
                }
                // Panic's atomic path cannot be blocked by a full command queue.
                self.rack = rack;
                if let Some(audio) = &mut self.audio {
                    let _ = audio.submit_controls(rack);
                }
                self.edit = None;
                self.status = "PANIC: A+B muted; Mute to resume".into();
            }
            Command::Exit => {
                self.handle(Command::Panic);
                self.quit = true;
            }
            Command::Main => self.enter(Page::Main),
            Command::More => self.enter(Page::More),
            Command::Tempo => self.enter(Page::Tempo),
            Command::Routing => self.enter(Page::Routing),
            Command::Ports => self.enter(Page::Ports),
            Command::Sounds => self.enter(Page::Sounds),
            Command::Midi => self.enter(Page::Midi),
            Command::Save => {
                let exists =
                    storage::snapshot_path(&self.root, self.slot).is_ok_and(|p| p.exists());
                if exists && !self.overwrite {
                    self.overwrite = true;
                    self.status = "Replace this sound? Save or Cancel".into();
                } else {
                    match storage::save_rack(&self.root, self.slot, &self.rack) {
                        Ok(()) => {
                            self.status = format!("Saved Sound {:02}", self.slot + 1);
                            self.overwrite = false;
                        }
                        Err(e) => self.status = e,
                    }
                }
            }
            Command::Load => match storage::load_rack(&self.root, self.slot, self.availability()) {
                Ok(rack) => {
                    if self.publish(rack) {
                        self.mapper.reset_pickup();
                        self.edit = None;
                        self.status = format!("Loaded Sound {:02}", self.slot + 1);
                    }
                }
                Err(e) => self.status = format!("Kept rack: {e}"),
            },
            Command::Learn => {
                if self.midi.is_some() {
                    self.learning = true;
                    self.status = "Move chosen control; Cancel to stop".into();
                } else {
                    self.status = "Select source and Apply before Learn".into();
                }
            }
            Command::Retry => self.retry_audio(),
            Command::Refresh => self.refresh(),
            Command::NamePart => {
                let name = if self.field < 4 {
                    &self.port_draft.inputs[self.field]
                } else {
                    &self.port_draft.outputs[self.field - 4]
                };
                if let Some(name) = name {
                    let (_, next) = name_part(name, self.name_offset);
                    self.name_offset = if next >= name.chars().count() {
                        0
                    } else {
                        next
                    };
                }
            }
            Command::PortPage => {
                self.name_offset = 0;
                self.field = if self.field < 4 { 4 } else { 0 };
                self.focus = 2;
            }
        }
    }
    fn adjust(&mut self, delta: i32) {
        self.overwrite = false;
        self.name_offset = 0;
        match self.page {
            Page::Main => {
                let config = self.edit.get_or_insert(self.rack.engines[self.selected]);
                match self.field {
                    0 => {
                        config.algorithm = if config.algorithm == Algorithm::Delay {
                            Algorithm::Room
                        } else {
                            Algorithm::Delay
                        }
                    }
                    1 => {
                        if config.algorithm == Algorithm::Delay {
                            config.time_ms =
                                (config.time_ms + delta as f32 * 5.0).clamp(1.0, 2000.0);
                            config.sync = false;
                        } else {
                            config.predelay_ms =
                                (config.predelay_ms + delta as f32).clamp(0.0, 200.0);
                        }
                    }
                    2 => config.feedback = (config.feedback + delta as f32 * 0.01).clamp(0.0, 0.9),
                    3 => config.damping = (config.damping + delta as f32 * 0.01).clamp(0.0, 0.95),
                    _ => config.level = (config.level + delta as f32 * 0.01).clamp(0.0, 1.0),
                }
            }
            Page::Routing => match self.field {
                0 | 1 => {
                    let choices = crate::model::Source::choices();
                    let i = choices
                        .iter()
                        .position(|s| *s == self.draft.routing.inputs[self.field])
                        .unwrap_or(0);
                    self.draft.routing.inputs[self.field] = choices[wrap(i, delta, choices.len())];
                }
                2 => {
                    let i = Layout::ALL
                        .iter()
                        .position(|l| *l == self.draft.routing.layout)
                        .unwrap_or(0);
                    self.draft.routing.layout = Layout::ALL[wrap(i, delta, 9)];
                }
                3..=6 => {
                    let i = self.field - 3;
                    if !(self.draft.routing.layout == Layout::SharedStereo && i >= 2) {
                        let p = &mut self.draft.routing.outputs[i / 2][i % 2];
                        *p = wrap(*p as usize, delta, 4) as u8;
                    }
                }
                _ => {}
            },
            Page::Ports => {
                let side = self.field / 4;
                let index = self.field % 4;
                let current = if side == 0 {
                    &mut self.port_draft.inputs[index]
                } else {
                    &mut self.port_draft.outputs[index]
                };
                let choices = &self.port_choices[side];
                let i = current
                    .as_ref()
                    .and_then(|n| choices.iter().position(|c| c == n))
                    .map_or(0, |i| i + 1);
                let next = wrap(i, delta, choices.len() + 1);
                *current = if next == 0 {
                    None
                } else {
                    Some(choices[next - 1].clone())
                };
            }
            Page::Tempo => {
                let e = self.selected;
                match self.field {
                    0 => {
                        let mut tempo = self.draft.engines[e].tempo;
                        if tempo.source == TempoSource::Internal {
                            tempo.bpm = (tempo.bpm + delta as f32).clamp(30.0, 300.0);
                            self.draft.set_tempo(e, tempo);
                        } else {
                            self.status = "Choose Internal for manual BPM".into();
                        }
                    }
                    1 => {
                        let mut tempo = self.draft.engines[e].tempo;
                        tempo.source = if tempo.source == TempoSource::Internal {
                            TempoSource::MidiClock
                        } else {
                            TempoSource::Internal
                        };
                        self.draft.set_tempo(e, tempo);
                    }
                    2 => {
                        self.draft.shared_tempo ^= true;
                        let tempo = self.draft.engines[e].tempo;
                        self.draft.set_tempo(e, tempo);
                    }
                    3 => self.draft.engines[e].sync ^= true,
                    4 => {
                        self.draft.engines[e].division =
                            wrap(self.draft.engines[e].division as usize, delta, 5) as u8
                    }
                    5 => self.draft.engines[e].ping_pong ^= true,
                    _ => {}
                }
            }
            Page::Sounds => self.slot = wrap(self.slot, delta, 16),
            Page::Midi => {
                self.learning = false;
                match self.field {
                    0 => {
                        let i = self
                            .midi_draft
                            .midi_source
                            .as_ref()
                            .and_then(|s| self.midi_choices.iter().position(|c| c == s))
                            .map_or(0, |i| i + 1);
                        let next = wrap(i, delta, self.midi_choices.len() + 1);
                        self.midi_draft.midi_source = if next == 0 {
                            None
                        } else {
                            Some(self.midi_choices[next - 1].clone())
                        };
                    }
                    1 => {
                        self.learn_target = wrap(self.learn_target, delta, learn_targets().len());
                        self.learn_kind = if self.learn_target < Action::ALL.len() {
                            ControlKind::Note
                        } else {
                            ControlKind::AbsoluteCc
                        };
                    }
                    2 => {
                        self.learn_kind = match self.learn_kind {
                            ControlKind::Note => ControlKind::ButtonCc,
                            ControlKind::ButtonCc => ControlKind::Note,
                            ControlKind::AbsoluteCc => ControlKind::RelativeCc,
                            ControlKind::RelativeCc => ControlKind::AbsoluteCc,
                        }
                    }
                    _ => {}
                }
            }
            Page::More => {}
        }
    }
    fn apply(&mut self) {
        match self.page {
            Page::Main => {
                if let Some(config) = self.edit {
                    let mut rack = self.rack;
                    let e = &mut rack.engines[self.selected];
                    match self.field {
                        0 => e.algorithm = config.algorithm,
                        1 => {
                            e.time_ms = config.time_ms;
                            e.predelay_ms = config.predelay_ms;
                            e.sync = config.sync;
                        }
                        2 => e.feedback = config.feedback,
                        3 => e.damping = config.damping,
                        _ => e.level = config.level,
                    }
                    if self.publish(rack) {
                        self.edit = None;
                        self.mapper.reset_pickup();
                        self.status = "Applied".into();
                    }
                }
            }
            Page::Routing => {
                let mut rack = self.rack;
                rack.routing = self.draft.routing;
                match rack.validate(self.availability()) {
                    Ok(()) => {
                        if self.publish(rack) {
                            self.enter(Page::Main);
                            self.status = "Routing applied".into();
                        }
                    }
                    Err(e) => self.status = e,
                }
            }
            Page::Tempo => {
                let mut rack = self.rack;
                rack.shared_tempo = self.draft.shared_tempo;
                for e in 0..2 {
                    rack.engines[e].tempo = self.draft.engines[e].tempo;
                }
                let e = self.selected;
                rack.engines[e].sync = self.draft.engines[e].sync;
                rack.engines[e].division = self.draft.engines[e].division;
                rack.engines[e].ping_pong = self.draft.engines[e].ping_pong;
                if self.publish(rack) {
                    self.enter(Page::Main);
                    self.mapper.reset_pickup();
                    self.status = "Tempo applied".into();
                }
            }
            Page::Ports => {
                let mut local = self.local.clone();
                local.ports = self.port_draft.clone();
                let root = self.root.clone();
                let result = if let Some(audio) = &mut self.audio {
                    audio.set_ports(local.ports.clone(), self.rack, || {
                        storage::save_local(&root, &local)
                    })
                } else {
                    local
                        .validate()
                        .and_then(|_| storage::save_local(&root, &local))
                };
                match result {
                    Ok(()) => {
                        self.local = local;
                        self.enter(Page::Main);
                        self.status = "Physical ports saved".into();
                    }
                    Err(e) => self.status = e,
                }
            }
            Page::Midi => match storage::save_local(&self.root, &self.midi_draft) {
                Ok(()) => {
                    let source_changed = self.local.midi_source != self.midi_draft.midi_source;
                    self.local = self.midi_draft.clone();
                    self.learning = false;
                    self.mapper.reset_pickup();
                    if self.midi_allowed
                        && (source_changed || self.midi.is_none() || !self.midi_error.is_empty())
                    {
                        self.restart_midi();
                    }
                    self.status = if self.midi_error.is_empty() {
                        "MIDI mappings saved".into()
                    } else {
                        self.midi_error.clone()
                    };
                }
                Err(e) => self.status = e,
            },
            _ => {}
        }
    }
    pub fn status_line(&self) -> String {
        if let Some(audio) = &self.audio {
            if audio.shared.server_down.load(Ordering::Acquire) {
                return "JACK stopped; More > Retry audio".into();
            }
            if audio.shared.rate_fault.load(Ordering::Acquire) {
                return "Rate changed: muted; More > Retry audio".into();
            }
            if self.meters.buffer_fault {
                return "Buffer missing: affected returns muted".into();
            }
            if self.meters.faults != 0 {
                return format!(
                    "Fault {}: Mute, then unmute to retry",
                    mask_label(self.meters.faults)
                );
            }
            if self.meters.missing != 0 {
                return format!(
                    "{} ports missing; More > Ports",
                    mask_label(self.meters.missing)
                );
            }
            if audio.pending() {
                return "Applying controls...".into();
            }
        }
        if self
            .rack
            .engines
            .iter()
            .any(|e| e.tempo.source == TempoSource::MidiClock)
            && self.clock.lost(self.epoch.elapsed().as_secs_f64())
        {
            return "Clock lost/waiting: holding last BPM".into();
        }
        if self.midi_allowed && self.local.midi_source.is_some() && !self.midi_error.is_empty() {
            return self.midi_error.clone();
        }
        self.status.clone()
    }
    pub fn hits(&self) -> Vec<Hit> {
        let mut hits = vec![
            hit(20, 0, 10, Command::Panic, "PANIC"),
            hit(30, 0, 10, Command::Exit, "Exit"),
        ];
        match self.page {
            Page::Main => {
                hits.push(hit(0, 1, 40, Command::Engine(0), "A"));
                hits.push(hit(0, 3, 40, Command::Engine(1), "B"));
                let config = self.edit.unwrap_or(self.rack.engines[self.selected]);
                let labels = [
                    format!("Algorithm: {}", config.algorithm.label()),
                    if config.algorithm == Algorithm::Delay {
                        format!(
                            "Time: {:.0} ms{}",
                            config.delay_ms(),
                            if config.sync { " sync" } else { " free" }
                        )
                    } else {
                        format!("Predelay: {:.0} ms", config.predelay_ms)
                    },
                    format!("Feedback: {:.0}%", config.feedback * 100.0),
                    format!("Damping: {:.0}%", config.damping * 100.0),
                    format!("Return level: {:.0}%", config.level * 100.0),
                ];
                for (i, label) in labels.into_iter().enumerate() {
                    hits.push(hit(0, i as u16 + 5, 40, Command::Field(i), &label));
                }
                let top = if self.edit.is_some() {
                    [
                        (Command::Minus, "-"),
                        (Command::Plus, "+"),
                        (Command::Apply, "Apply"),
                        (Command::Cancel, "Cancel"),
                    ]
                } else {
                    [
                        (Command::Minus, "-"),
                        (Command::Plus, "+"),
                        (Command::Tap, "TAP"),
                        (Command::Bypass, "Wet bypass"),
                    ]
                };
                footer(
                    &mut hits,
                    top,
                    [
                        (Command::Mute, "Mute"),
                        (Command::Routing, "Routing"),
                        (Command::Sounds, "Sounds"),
                        (Command::More, "More"),
                    ],
                );
            }
            Page::More => {
                for (i, (command, label)) in [
                    (Command::Tempo, "Tempo / delay mode"),
                    (Command::Midi, "MIDI source / Learn"),
                    (Command::Ports, "Physical JACK ports"),
                    (Command::Retry, "Retry audio"),
                    (Command::Main, "Back to engines"),
                ]
                .into_iter()
                .enumerate()
                {
                    hits.push(hit(0, i as u16 + 2, 40, command, label));
                }
                footer(
                    &mut hits,
                    [
                        (Command::Engine(0), "Engine A"),
                        (Command::Engine(1), "Engine B"),
                        (Command::Tap, "TAP"),
                        (Command::Bypass, "Wet bypass"),
                    ],
                    [
                        (Command::Mute, "Mute"),
                        (Command::Routing, "Routing"),
                        (Command::Sounds, "Sounds"),
                        (Command::Main, "Back"),
                    ],
                );
            }
            Page::Routing => {
                let r = &self.draft.routing;
                let mut labels = vec![
                    format!("A input: {}", r.inputs[0].label()),
                    format!("B input: {}", r.inputs[1].label()),
                    format!("Returns: {}", r.layout.label()),
                ];
                for i in 0..4 {
                    labels.push(if r.layout == Layout::SharedStereo && i >= 2 {
                        format!(
                            "B {}: shared A {}",
                            if i % 2 == 0 { "L" } else { "R" },
                            if i % 2 == 0 { "L" } else { "R" }
                        )
                    } else {
                        format!(
                            "{} {} slot: {}",
                            engine_name(i / 2),
                            if i % 2 == 0 { "mono/L" } else { "R" },
                            r.outputs[i / 2][i % 2] + 1
                        )
                    });
                }
                fields(&mut hits, &labels, 0);
                editor_footer(&mut hits, Command::Ports, "Ports");
            }
            Page::Ports => {
                let labels = (0..8)
                    .map(|i| {
                        let name = if i < 4 {
                            &self.port_draft.inputs[i]
                        } else {
                            &self.port_draft.outputs[i - 4]
                        };
                        format!(
                            "{} {}: {}",
                            if i < 4 { "In" } else { "Out" },
                            i % 4 + 1,
                            name.as_deref().unwrap_or("Unassigned")
                        )
                    })
                    .collect::<Vec<_>>();
                let first = if self.field < 4 { 0 } else { 4 };
                fields(&mut hits, &labels[first..first + 4], 0);
                for hit in &mut hits[2..] {
                    if let Command::Field(i) = hit.command {
                        hit.command = Command::Field(i + first);
                    }
                }
                footer(
                    &mut hits,
                    [
                        (Command::Minus, "-"),
                        (Command::Plus, "+"),
                        (Command::Apply, "Apply"),
                        (Command::Cancel, "Cancel"),
                    ],
                    [
                        (
                            Command::PortPage,
                            if first == 0 { "Outputs" } else { "Inputs" },
                        ),
                        (Command::Refresh, "Refresh"),
                        (Command::NamePart, "Name >"),
                        (Command::Main, "Back"),
                    ],
                );
            }
            Page::Tempo => {
                let e = self.draft.engines[self.selected];
                let labels = vec![
                    format!("BPM: {:.1}", e.tempo.bpm),
                    format!("Source: {}", tempo_label(e.tempo.source)),
                    format!(
                        "A/B tempo: {}",
                        if self.draft.shared_tempo {
                            "Shared"
                        } else {
                            "Independent"
                        }
                    ),
                    format!(
                        "Delay time: {}",
                        if e.sync { "Tempo division" } else { "Free ms" }
                    ),
                    format!(
                        "Division: {}",
                        EngineConfig::DIVISION_LABELS[e.division as usize]
                    ),
                    format!(
                        "Delay stereo: {}",
                        if e.ping_pong {
                            "Ping-pong"
                        } else {
                            "Independent L/R"
                        }
                    ),
                ];
                fields(&mut hits, &labels, 0);
                editor_footer(&mut hits, Command::Tap, "TAP");
            }
            Page::Sounds => {
                hits.push(hit(
                    0,
                    3,
                    40,
                    Command::Field(0),
                    &format!("Sound {:02} / 16", self.slot + 1),
                ));
                footer(
                    &mut hits,
                    [
                        (Command::Minus, "Previous"),
                        (Command::Plus, "Next"),
                        (Command::Load, "Load"),
                        (
                            Command::Save,
                            if self.overwrite { "Replace" } else { "Save" },
                        ),
                    ],
                    [
                        (Command::Tap, "TAP"),
                        (Command::Bypass, "Wet bypass"),
                        (Command::Mute, "Mute"),
                        (Command::Cancel, "Cancel"),
                    ],
                );
            }
            Page::Midi => {
                let target = learn_targets()[self.learn_target];
                let labels = vec![
                    format!(
                        "Source: {}",
                        self.midi_draft
                            .midi_source
                            .as_ref()
                            .map_or("None".into(), |s| s.label())
                    ),
                    format!("Target: {}", target_label(target)),
                    format!("Kind: {}", kind_label(self.learn_kind)),
                ];
                fields(&mut hits, &labels, 0);
                footer(
                    &mut hits,
                    [
                        (Command::Minus, "-"),
                        (Command::Plus, "+"),
                        (Command::Learn, "Learn"),
                        (Command::Apply, "Apply"),
                    ],
                    [
                        (Command::Refresh, "Refresh"),
                        (Command::Tap, "TAP"),
                        (Command::Mute, "Mute"),
                        (Command::Cancel, "Cancel"),
                    ],
                );
            }
        }
        hits
    }
}
fn wrap(index: usize, delta: i32, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        (index.min(len - 1) as i32 + delta).rem_euclid(len as i32) as usize
    }
}
fn hit(x: u16, y: u16, width: u16, command: Command, label: &str) -> Hit {
    Hit {
        rect: Rect::new(x, y, width, 1),
        command,
        label: label.into(),
    }
}
fn footer(hits: &mut Vec<Hit>, top: [(Command, &str); 4], bottom: [(Command, &str); 4]) {
    for (row, items) in [top, bottom].into_iter().enumerate() {
        for (col, (command, label)) in items.into_iter().enumerate() {
            hits.push(hit(col as u16 * 10, row as u16 + 10, 10, command, label));
        }
    }
}
fn fields(hits: &mut Vec<Hit>, labels: &[String], first: usize) {
    for (i, label) in labels.iter().enumerate().skip(first).take(7) {
        hits.push(hit(0, (i - first) as u16 + 3, 40, Command::Field(i), label));
    }
}
fn editor_footer(hits: &mut Vec<Hit>, extra: Command, label: &str) {
    footer(
        hits,
        [
            (Command::Minus, "-"),
            (Command::Plus, "+"),
            (Command::Apply, "Apply"),
            (Command::Cancel, "Cancel"),
        ],
        [
            (Command::Engine(0), "Engine A"),
            (Command::Engine(1), "Engine B"),
            (extra, label),
            (Command::Main, "Back"),
        ],
    );
}
fn tempo_label(source: TempoSource) -> &'static str {
    if source == TempoSource::Internal {
        "Internal"
    } else {
        "MIDI clock"
    }
}
fn mask_label(mask: u8) -> &'static str {
    match mask & 3 {
        1 => "A",
        2 => "B",
        _ => "A+B",
    }
}
fn learn_targets() -> Vec<Target> {
    let mut targets = Action::ALL
        .into_iter()
        .map(Target::Action)
        .collect::<Vec<_>>();
    for engine in 0..2 {
        for parameter in Parameter::ALL {
            targets.push(Target::Parameter { engine, parameter });
        }
    }
    targets
}
fn target_label(target: Target) -> String {
    match target {
        Target::Action(action) => action.label().into(),
        Target::Parameter { engine, parameter } => {
            format!("{} {}", engine_name(engine as usize), parameter.label())
        }
    }
}
fn kind_label(kind: ControlKind) -> &'static str {
    match kind {
        ControlKind::Note => "Note button",
        ControlKind::ButtonCc => "CC button",
        ControlKind::AbsoluteCc => "CC absolute / pickup",
        ControlKind::RelativeCc => "CC relative 2s complement",
    }
}
fn parameter_step(parameter: Parameter) -> f32 {
    match parameter {
        Parameter::Time => 5.0 / 1999.0,
        Parameter::Bpm => 1.0 / 270.0,
        _ => 0.01,
    }
}
pub fn normalized(rack: &Rack, engine: usize, parameter: Parameter) -> f32 {
    let e = rack.engines[engine];
    match parameter {
        Parameter::Time => {
            if e.algorithm == Algorithm::Delay {
                (e.time_ms - 1.0) / 1999.0
            } else {
                e.predelay_ms / 200.0
            }
        }
        Parameter::Feedback => e.feedback / 0.9,
        Parameter::Damping => e.damping / 0.95,
        Parameter::Level => e.level,
        Parameter::Bpm => (e.tempo.bpm - 30.0) / 270.0,
    }
}
pub fn set_normalized(rack: &mut Rack, engine: usize, parameter: Parameter, value: f32) {
    let value = value.clamp(0.0, 1.0);
    let e = &mut rack.engines[engine];
    match parameter {
        Parameter::Time => {
            if e.algorithm == Algorithm::Delay {
                e.time_ms = 1.0 + value * 1999.0;
                e.sync = false;
            } else {
                e.predelay_ms = value * 200.0;
            }
        }
        Parameter::Feedback => e.feedback = value * 0.9,
        Parameter::Damping => e.damping = value * 0.95,
        Parameter::Level => e.level = value,
        Parameter::Bpm => {
            if e.tempo.source == TempoSource::Internal {
                rack.set_tempo(
                    engine,
                    Tempo {
                        bpm: 30.0 + value * 270.0,
                        source: TempoSource::Internal,
                    },
                );
            }
        }
    }
}
fn text<B: Backend>(
    frame: &mut Frame<B>,
    x: u16,
    y: u16,
    width: u16,
    value: impl Into<String>,
    style: Style,
) {
    frame.render_widget(
        Paragraph::new(value.into()).style(style),
        Rect::new(x, y, width, 1),
    );
}
fn leds(peak: f32) -> Vec<Span<'static>> {
    [-48.0f32, -30.0, -18.0, -6.0, -1.0]
        .into_iter()
        .map(|db| {
            let lit = peak >= 10.0f32.powf(db / 20.0);
            Span::styled(
                "●",
                Style::default().fg(if !lit {
                    Color::DarkGray
                } else if db >= -1.0 {
                    Color::Red
                } else if db >= -6.0 {
                    Color::Yellow
                } else {
                    Color::Green
                }),
            )
        })
        .collect()
}
pub fn draw<B: Backend>(frame: &mut Frame<B>, app: &App) {
    let size = frame.size();
    if size.width < 40 || size.height < 13 {
        frame.render_widget(Paragraph::new("Exit (q)\nNeeds 40 columns x 13 rows"), size);
        return;
    }
    let white = Style::default().fg(Color::White);
    let gray = Style::default().fg(Color::Gray);
    text(
        frame,
        0,
        0,
        20,
        if app.audio.is_some() {
            "fx  WET ONLY"
        } else {
            "fx WET ONLY OFFLINE"
        },
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    match app.page {
        Page::Main => {
            for e in 0..2 {
                let config = app.rack.engines[e];
                let y = (1 + e * 2) as u16;
                let state = if app.meters.faults & (1 << e) != 0 {
                    "FAULT"
                } else if config.mute {
                    "MUTED"
                } else if config.bypass {
                    "TAIL"
                } else {
                    ""
                };
                text(
                    frame,
                    0,
                    y,
                    14,
                    format!(
                        "{} {} {}",
                        if app.selected == e { ">" } else { " " },
                        engine_name(e),
                        config.algorithm.label()
                    ),
                    if app.selected == e {
                        Style::default().fg(Color::Cyan)
                    } else {
                        white
                    },
                );
                let mut spans = vec![Span::raw("In ")];
                spans.extend(leds(app.meters.input[e]));
                spans.push(Span::raw(" Out "));
                spans.extend(leds(app.meters.output[e]));
                spans.push(Span::raw(format!(" {state}")));
                frame.render_widget(Paragraph::new(Spans::from(spans)), Rect::new(14, y, 26, 1));
                text(
                    frame,
                    0,
                    y + 1,
                    40,
                    format!(
                        " {} > {} {:3.0} {}",
                        app.rack.routing.inputs[e].label(),
                        app.rack.routing.return_label(e),
                        config.tempo.bpm,
                        if config.tempo.source == TempoSource::Internal {
                            "Int"
                        } else {
                            "Clk"
                        }
                    ),
                    gray,
                );
            }
        }
        Page::More => {
            text(
                frame,
                0,
                1,
                40,
                format!("Engine {} settings", engine_name(app.selected)),
                white,
            );
            if let Some(audio) = &app.audio {
                text(
                    frame,
                    0,
                    7,
                    40,
                    format!(
                        "JACK {:.1}%  xruns {}",
                        audio.cpu_load(),
                        audio.shared.xruns.load(Ordering::Relaxed)
                    ),
                    gray,
                );
                text(
                    frame,
                    0,
                    8,
                    40,
                    format!(
                        "{} Hz / {} frames",
                        audio.shared.rate.load(Ordering::Relaxed),
                        audio.shared.frames.load(Ordering::Relaxed)
                    ),
                    gray,
                );
                text(frame, 0, 9, 40, "JACK load is whole-server load", gray);
            } else {
                text(frame, 0, 8, 40, "Offline: no audio I/O", gray);
            }
        }
        Page::Routing => {
            text(frame, 0, 1, 40, "Routing draft (physical slots 1-4)", white);
            text(
                frame,
                0,
                2,
                40,
                if app.audio.is_some() {
                    let available = app.availability();
                    let slots = |mask: u8| {
                        (0..4)
                            .map(|i| {
                                if mask & (1 << i) != 0 {
                                    (i + 1).to_string()
                                } else {
                                    "-".into()
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(" ")
                    };
                    format!(
                        "Inputs {}  Outputs {}",
                        slots(available.inputs),
                        slots(available.outputs)
                    )
                } else {
                    "Offline: slots 1-4 for planning".into()
                },
                gray,
            );
        }
        Page::Ports => {
            text(frame, 0, 1, 40, "Physical ports draft", white);
            text(
                frame,
                0,
                2,
                40,
                if app.audio.is_some() {
                    "Choose exact names; +/- cycles"
                } else {
                    "Start --audio to discover JACK ports"
                },
                gray,
            );
            let name = if app.field < 4 {
                &app.port_draft.inputs[app.field]
            } else {
                &app.port_draft.outputs[app.field - 4]
            };
            if let Some(name) = name {
                text(frame, 0, 7, 40, "Selected name (Name > continues)", gray);
                text(frame, 0, 8, 40, name_part(name, app.name_offset).0, white);
                if !app.port_choices[app.field / 4].contains(name) {
                    text(
                        frame,
                        0,
                        9,
                        40,
                        "MISSING: Refresh or choose another",
                        Style::default().fg(Color::Yellow),
                    );
                }
            }
        }
        Page::Tempo => {
            text(
                frame,
                0,
                1,
                40,
                format!("Engine {} tempo draft", engine_name(app.selected)),
                white,
            );
            text(
                frame,
                0,
                2,
                40,
                "Division caps at 2000 ms; room is free",
                gray,
            );
        }
        Page::Sounds => {
            text(
                frame,
                0,
                1,
                40,
                "Rack sounds: A+B, tempo and routing",
                white,
            );
            let exists = storage::snapshot_path(&app.root, app.slot).is_ok_and(|p| p.exists());
            text(
                frame,
                0,
                5,
                40,
                if exists {
                    "Saved sound: Load or replace with Save"
                } else {
                    "Empty slot: Save current rack"
                },
                gray,
            );
            text(
                frame,
                0,
                7,
                40,
                "Load validates before changing sound",
                gray,
            );
            text(
                frame,
                0,
                8,
                40,
                "Physical ports stay in local settings",
                gray,
            );
        }
        Page::Midi => {
            text(frame, 0, 1, 40, "MIDI input / control learn", white);
            text(
                frame,
                0,
                2,
                40,
                if app.midi.is_some() {
                    "Input open; no MIDI output"
                } else {
                    &app.midi_error
                },
                gray,
            );
            text(
                frame,
                0,
                6,
                40,
                format!("{} bindings in draft", app.midi_draft.bindings.len()),
                gray,
            );
            text(
                frame,
                0,
                7,
                40,
                if app.learning {
                    "LEARNING: move control or Cancel"
                } else {
                    "Choose target + kind, Learn, Apply"
                },
                Style::default().fg(Color::Yellow),
            );
            text(frame, 0, 8, 40, "Relative: 1..63 + / 65..127 -", gray);
            text(frame, 0, 9, 40, "Absolute: pickup after edits/recall", gray);
        }
    }
    for (i, hit) in app.hits().iter().enumerate() {
        let focused = i == app.focus;
        let is_engine = matches!(hit.command, Command::Engine(_)) && app.page == Page::Main;
        if is_engine {
            if focused {
                text(
                    frame,
                    0,
                    hit.rect.y,
                    1,
                    ">",
                    Style::default().fg(Color::Yellow),
                );
            }
            continue;
        }
        let field = matches!(hit.command, Command::Field(n) if n == app.field);
        let style = if focused {
            Style::default().fg(Color::Black).bg(Color::Yellow)
        } else if field {
            Style::default().fg(Color::Cyan)
        } else if hit.command == Command::Panic {
            Style::default().fg(Color::Red)
        } else {
            white
        };
        let label = if hit.rect.y >= 10 || hit.rect.y == 0 {
            format!("{:^width$}", hit.label, width = hit.rect.width as usize)
        } else {
            format!("{}{}", if field { ">" } else { " " }, hit.label)
        };
        text(frame, hit.rect.x, hit.rect.y, hit.rect.width, label, style);
    }
    text(
        frame,
        0,
        12,
        40,
        app.status_line(),
        Style::default().fg(Color::White).bg(Color::DarkGray),
    );
}

/// Page long port identities without dropping wide Unicode characters.
pub fn name_part(name: &str, start: usize) -> (String, usize) {
    use unicode_width::UnicodeWidthChar;
    let mut used = 0;
    let mut next = start;
    let mut part = String::new();
    for character in name.chars().skip(start) {
        let width = character.width().unwrap_or(0);
        if used + width > 40 {
            break;
        }
        used += width;
        next += 1;
        part.push(character);
    }
    (part, next)
}
