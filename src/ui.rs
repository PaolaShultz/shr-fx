use crate::{
    audio::Audio,
    dsp::Meters,
    midi::{
        self, Action, Binding, Clock, Control, ControlKind, Mapper, Message, MidiInput, Parameter,
        SlotParameter, TapTempo, Target,
    },
    model::{
        Algorithm, Availability, DelayConfig, DelayKind, EffectConfig, EngineConfig, EngineMode,
        Layout, MAX_STAGES, Rack, ReverbKind, Tempo, TempoSource, engine_name,
    },
    storage::{self, LocalConfig, Ports},
    surface::{Context, Role},
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
    Play,
    Controller,
    Effects,
    Effect,
    EffectTime,
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
    Rack,
    Wet,
    SwitchEngine,
    Controller,
    KeepLive,
    UseRole,
    SetupNext,
    SetupPrevious,
    ClearRole,
    SelectSlot(usize),
    ToggleSlot(usize),
    ParameterPage,
    Knob(usize),
    Effects,
    EditSlot(usize),
    PreviousSlot,
    SlotPage,
    NextSlot,
    EffectTime,
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
    pub effect_slot: usize,
    slot_page: [usize; 2],
    pub selected_slots: [usize; 2],
    pub parameter_pages: [[usize; 8]; 2],
    home_focus: usize,
    home_field: usize,
    home_page: Page,
    value_position: Option<u8>,
    value_context: Option<(Page, usize)>,
    value_picked: bool,
    value_expected: Option<f32>,
    pub setup_step: usize,
    pub feedback: String,
    midi_driving: Option<usize>,
    tempo_base: Rack,
    parent_routing: Option<Rack>,
    effects_base: EngineConfig,
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
    pub learn_conflict: Option<Binding>,
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
            field: 8,
            name_offset: 0,
            focus: 2,
            edit: None,
            effect_slot: 0,
            slot_page: [0; 2],
            selected_slots: [0; 2],
            parameter_pages: [[0; 8]; 2],
            home_focus: 2,
            home_field: 8,
            home_page: Page::Main,
            value_position: None,
            value_context: None,
            value_picked: false,
            value_expected: None,
            setup_step: 0,
            feedback: "Select slot; pads only off/on".into(),
            midi_driving: None,
            tempo_base: rack,
            parent_routing: None,
            effects_base: rack.engines[0],
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
            learn_conflict: None,
            learn_target: 0,
            learn_kind: ControlKind::Note,
            port_choices: std::array::from_fn(|_| Vec::new()),
            midi_choices: Vec::new(),
            overwrite: false,
            midi_error: String::new(),
        }
    }
    pub fn context(&self) -> Context {
        let slot = self.selected_slots[self.selected];
        Context {
            engine: self.selected,
            slot,
            page: self.parameter_pages[self.selected][slot],
        }
    }
    fn value_target(&self) -> Option<Target> {
        if self.page == Page::Main {
            if let Some(Hit {
                command: Command::SelectSlot(slot),
                ..
            }) = self.hits().get(self.focus)
            {
                return (*slot < self.rack.engines[self.selected].stage_count()).then_some(
                    Target::Parameter {
                        engine: self.selected as u8,
                        parameter: Parameter::Slot {
                            slot: *slot as u8,
                            control: SlotParameter::Level,
                        },
                    },
                );
            }
            return None;
        }
        if self.page != Page::Play {
            return None;
        }
        if self.field == 16 {
            return (self.context().slot < self.rack.engines[self.selected].stage_count())
                .then_some(Target::Parameter {
                    engine: self.selected as u8,
                    parameter: Parameter::Slot {
                        slot: self.context().slot as u8,
                        control: SlotParameter::Level,
                    },
                });
        }
        self.context()
            .resolve(&self.rack, Role::at(self.field))
            .filter(|t| matches!(t, Target::Parameter { .. }))
    }
    fn surface_control(&mut self, role: Role, value: u8, kind: ControlKind) {
        let relative = kind == ControlKind::RelativeCc;
        let steps = if relative {
            match value {
                0 | 64 => 0,
                1..=63 => value as i32,
                _ => value as i32 - 128,
            }
            .clamp(-8, 8)
        } else {
            0
        };
        if role == Role::Navigate {
            let len = self.hits().len();
            self.focus = if relative {
                wrap(self.focus, steps, len)
            } else {
                (value as usize * len / 128).min(len - 1)
            };
            if let Some(hit) = self.hits().get(self.focus) {
                match hit.command {
                    Command::Knob(i) | Command::Field(i) => self.field = i,
                    Command::Wet => self.field = 16,
                    _ => {}
                }
            }
        } else if role == Role::Value {
            if relative {
                if steps != 0 {
                    self.adjust(steps);
                }
                return;
            }
            let Some((current, step)) = self.edit_scale() else {
                return;
            };
            let context = (self.page, self.field);
            if self.value_context != Some(context)
                || self
                    .value_expected
                    .is_some_and(|old| (old - current).abs() > 0.0001)
            {
                self.value_context = Some(context);
                self.value_position = None;
                self.value_picked = false;
            }
            let position = value as f32 / 127.0;
            self.value_picked |= (position - current).abs() <= 0.025
                || self.value_position.is_some_and(|old| {
                    (old as f32 / 127.0 - current) * (position - current) <= 0.0
                });
            self.value_position = Some(value);
            if self.value_picked {
                let delta = (position / step).round() as i32 - (current / step).round() as i32;
                if delta != 0 {
                    self.adjust(delta);
                }
            } else {
                self.status = format!(
                    "Rotary 9 pickup: move {}",
                    if position < current { "UP" } else { "DOWN" }
                );
            }
            self.value_expected = self.edit_scale().map(|(value, _)| value);
        }
    }
    fn edit_scale(&self) -> Option<(f32, f32)> {
        let e = self.draft.engines[self.selected];
        let choice = |index: usize, len: usize| {
            let max = len.saturating_sub(1).max(1) as f32;
            (index as f32 / max, 1.0 / max)
        };
        let boolean = |v: bool| (if v { 1.0 } else { 0.0 }, 1.0);
        Some(match self.page {
            Page::Effects => match self.field {
                0 => boolean(e.mode == EngineMode::MultiFx),
                1 => choice(e.pieces.saturating_sub(2) as usize, 7),
                _ => return None,
            },
            Page::Effect => {
                let s = e.stages[self.effect_slot];
                match self.field {
                    0 => choice(
                        Algorithm::ALL
                            .iter()
                            .position(|a| *a == s.algorithm)
                            .unwrap_or(0),
                        4,
                    ),
                    1 => match s.algorithm {
                        Algorithm::Delay => choice(
                            DelayKind::ALL
                                .iter()
                                .position(|k| *k == s.delay.kind)
                                .unwrap_or(0),
                            4,
                        ),
                        Algorithm::Room => choice(
                            ReverbKind::ALL
                                .iter()
                                .position(|k| *k == s.reverb.kind)
                                .unwrap_or(0),
                            5,
                        ),
                        Algorithm::Chorus => boolean(s.chorus.ensemble),
                        Algorithm::Exciter => boolean(s.exciter.bright),
                    },
                    2..=4 => {
                        let c = primary_control(s, self.field - 1);
                        (slot_normalized(s, c), slot_step(c))
                    }
                    5 => (s.level, 0.01),
                    6 => boolean(s.bypass),
                    _ => return None,
                }
            }
            Page::EffectTime => {
                let d = e.stages[self.effect_slot].delay;
                match self.field {
                    0 => boolean(d.sync),
                    1 => choice(d.division as usize, 5),
                    2 => boolean(d.ping_pong),
                    _ => return None,
                }
            }
            Page::Tempo => match self.field {
                0 => ((e.tempo.bpm - 30.0) / 270.0, 1.0 / 270.0),
                1 => boolean(e.tempo.source == TempoSource::MidiClock),
                2 => boolean(self.draft.shared_tempo),
                3 => boolean(e.stages[0].delay.sync),
                4 => choice(e.stages[0].delay.division as usize, 5),
                5 => boolean(e.stages[0].delay.ping_pong),
                _ => return None,
            },
            Page::Routing => match self.field {
                0 | 1 => {
                    let list = crate::model::Source::choices();
                    choice(
                        list.iter()
                            .position(|v| *v == self.draft.routing.inputs[self.field])
                            .unwrap_or(0),
                        list.len(),
                    )
                }
                2 => choice(
                    Layout::ALL
                        .iter()
                        .position(|v| *v == self.draft.routing.layout)
                        .unwrap_or(0),
                    Layout::ALL.len(),
                ),
                3..=6 => choice(
                    self.draft.routing.outputs[(self.field - 3) / 2][(self.field - 3) % 2] as usize,
                    4,
                ),
                _ => return None,
            },
            Page::Sounds => choice(self.slot, 16),
            Page::Ports => {
                let side = self.field / 4;
                let name = if side == 0 {
                    &self.port_draft.inputs[self.field % 4]
                } else {
                    &self.port_draft.outputs[self.field % 4]
                };
                choice(
                    name.as_ref()
                        .and_then(|n| self.port_choices[side].iter().position(|p| p == n))
                        .map_or(0, |i| i + 1),
                    self.port_choices[side].len() + 1,
                )
            }
            Page::Midi => match self.field {
                0 => choice(
                    self.midi_draft
                        .midi_source
                        .as_ref()
                        .and_then(|s| self.midi_choices.iter().position(|c| c == s))
                        .map_or(0, |i| i + 1),
                    self.midi_choices.len() + 1,
                ),
                1 => choice(self.learn_target, learn_targets().len()),
                2 => boolean(matches!(
                    self.learn_kind,
                    ControlKind::ButtonCc | ControlKind::RelativeCc
                )),
                _ => return None,
            },
            Page::Controller => boolean(matches!(
                self.learn_kind,
                ControlKind::ButtonCc | ControlKind::RelativeCc
            )),
            _ => return None,
        })
    }
    fn pickup_mark(&self, knob: u8) -> &'static str {
        if let Some((i, _)) = self
            .local
            .bindings
            .iter()
            .enumerate()
            .find(|(_, b)| b.target == Target::Surface(Role::at(knob as usize)))
            && let Some(Target::Parameter { engine, parameter }) = if knob == 8 {
                self.value_target()
            } else {
                self.context().resolve(&self.rack, Role::at(knob as usize))
            }
        {
            return match self
                .mapper
                .pickup_direction(i, normalized(&self.rack, engine as usize, parameter))
            {
                Some(true) => "^",
                Some(false) => "v",
                None => " ",
            };
        }
        " "
    }
    fn rearm_surface(&mut self, levels: bool) {
        for (i, b) in self.local.bindings.iter().enumerate() {
            if matches!(b.target, Target::Surface(Role::Sound(_) | Role::Value))
                || matches!(b.target, Target::Surface(Role::Knob(k)) if levels || k >= 8)
            {
                self.mapper.invalidate(i);
            }
        }
        let c = self.context();
        self.control_feedback(format!(
            "{}{} {} / 1: Pick/Open  9: Value/Back",
            engine_name(c.engine),
            c.slot + 1,
            self.slot_state(c.slot)
        ));
    }
    fn rearm_changes(&mut self, old: Rack, new: Rack) {
        let context = self.context();
        let value_target = self.value_target();
        for (i, binding) in self.local.bindings.iter().enumerate() {
            if Some(i) == self.midi_driving {
                continue;
            }
            let resolve = |rack: &Rack| match binding.target {
                Target::Surface(Role::Value) => value_target,
                Target::Surface(role) => context.resolve(rack, role),
                t => Some(t),
            };
            let (before, after) = (resolve(&old), resolve(&new));
            let changed = if let Some(Target::Parameter { engine, parameter }) = after {
                let e = engine as usize;
                let structure_changed = match parameter {
                    Parameter::Slot {
                        slot,
                        control: SlotParameter::Level,
                    } => {
                        old.engines[e].stage_count() != new.engines[e].stage_count()
                            && slot as usize
                                >= old.engines[e]
                                    .stage_count()
                                    .min(new.engines[e].stage_count())
                    }
                    Parameter::Slot { slot, .. } => !old.engines[e].stages[slot as usize]
                        .same_structure(new.engines[e].stages[slot as usize]),
                    Parameter::Time | Parameter::Feedback | Parameter::Damping => {
                        !old.engines[e].stages[0].same_structure(new.engines[e].stages[0])
                    }
                    Parameter::Bpm => old.engines[e].tempo.source != new.engines[e].tempo.source,
                    Parameter::Level => false,
                };
                structure_changed
                    || normalized(&old, e, parameter) != normalized(&new, e, parameter)
            } else {
                false
            };
            if before != after || changed {
                self.mapper.invalidate(i);
            }
        }
    }
    fn control_feedback(&mut self, value: String) {
        self.status = value.clone();
        self.feedback = value;
    }
    fn parameter_feedback(&mut self, engine: usize, parameter: Parameter) {
        self.control_feedback(format!(
            "{} {}",
            engine_name(engine),
            parameter_value(&self.rack, engine, parameter)
        ));
    }
    pub fn slot_state(&self, slot: usize) -> &'static str {
        let e = self.rack.engines[self.selected];
        if slot >= e.stage_count() {
            "EMPTY"
        } else if self.meters.faults & (1 << self.selected) != 0 {
            "FAULT"
        } else if self.meters.buffer_fault || self.meters.missing & (1 << self.selected) != 0 {
            "BLOCK"
        } else if e.mute {
            "MUTED"
        } else if self.rack.routing.layout.widths()[self.selected] == 0 {
            "NO OUT"
        } else if e.bypass || e.stages[slot].bypass {
            "OFF"
        } else {
            "ON"
        }
    }
    pub fn midi_lost(&mut self) {
        self.mapper.require_releases();
        self.value_context = None;
        self.clock = Clock::default();
        self.midi_error = "MIDI lost: Menu > MIDI > Apply to retry".into();
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
                self.rearm_changes(self.rack, rack);
                self.rack = rack;
                true
            }
            Err(e) => {
                if let Some(index) = self.midi_driving {
                    self.mapper.invalidate(index);
                }
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
                self.status = "JACK attached; assign ports in Menu".into();
            }
            Err(e) => {
                self.status = format!("JACK: {e}");
            }
        }
    }
    fn restart_midi(&mut self) {
        drop(self.midi.take());
        self.mapper.require_releases();
        self.value_context = None;
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
                self.value_context = None;
                self.clock = Clock::default();
                self.status = "MIDI overflow: release and retry".into();
            }
            if midi.failed.load(Ordering::Acquire) {
                self.mapper.require_releases();
                self.value_context = None;
                self.clock = Clock::default();
                self.midi_error = "MIDI lost: Menu > MIDI > Apply to retry".into();
                for _ in 0..512 {
                    if midi.receiver.pop().is_err() {
                        break;
                    }
                }
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
        let context = self.context();
        let value_target = self.value_target();
        let performance = matches!(self.page, Page::Main | Page::Play);
        let mapped = self.mapper.map_surface(
            message,
            &self.local.bindings,
            |role| {
                if role == Role::Value && performance {
                    value_target
                } else {
                    context.resolve(&rack, role)
                }
            },
            |e, p| normalized(&rack, e, p),
        );
        if self.page == Page::Controller
            && !self.learning
            && matches!(
                mapped,
                Some(Control::Absolute { .. } | Control::Relative { .. } | Control::Pickup)
            )
        {
            if let Some(index) = self.mapper.last_binding {
                self.mapper.invalidate(index);
            }
            return;
        }
        if self.learning {
            if matches!(
                mapped,
                Some(Control::Absolute { .. } | Control::Relative { .. } | Control::Pickup)
            ) && let Some(index) = self.mapper.last_binding
            {
                self.mapper.invalidate(index);
            }
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
                ) if velocity > 0 && self.mapper.press_edge => Some((channel, number)),
                (
                    ControlKind::ButtonCc,
                    Message::Cc {
                        channel,
                        number,
                        value,
                    },
                ) if value >= 64 && self.mapper.press_edge => Some((channel, number)),
                (
                    ControlKind::AbsoluteCc | ControlKind::RelativeCc,
                    Message::Cc {
                        channel, number, ..
                    },
                ) => Some((channel, number)),
                _ => None,
            };
            if let Some((channel, number)) = source {
                let target = if self.page == Page::Controller {
                    Target::Surface(Role::at(self.setup_step))
                } else {
                    learn_targets()[self.learn_target]
                };
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
                if self.page == Page::Controller
                    && let Some(old) = self.midi_draft.bindings.iter().find(|b| {
                        b.target != target
                            && b.channel == channel
                            && b.number == number
                            && (b.kind == ControlKind::Note) == (binding.kind == ControlKind::Note)
                    })
                {
                    if matches!(old.target, Target::Surface(_)) {
                        self.status = "Already mapped; use a unique note/CC".into();
                    } else {
                        self.learn_conflict = Some(binding);
                        self.learning = false;
                        self.status = "Explicit mapping: Use role or Cancel".into();
                    }
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
            Some(Control::Surface { role, value, kind }) => self.surface_control(role, value, kind),
            Some(Control::Action(action)) => self.action(action),
            Some(Control::Pickup) => {
                if let Some((index, up)) = self.mapper.pickup {
                    let binding = self.local.bindings[index];
                    let label = match binding.target {
                        Target::Surface(Role::Knob(i) | Role::Sound(i)) => format!("K{:02}", i + 1),
                        Target::Surface(Role::Value) => "K09".into(),
                        _ => format!("CC{}", binding.number),
                    };
                    let target = match binding.target {
                        Target::Surface(Role::Value) => value_target,
                        Target::Surface(role) => context.resolve(&rack, role),
                        target => Some(target),
                    };
                    if let Some(Target::Parameter { engine, parameter }) = target {
                        self.control_feedback(format!(
                            "{} {} > {} {}",
                            label,
                            if up { "UP" } else { "DOWN" },
                            engine_name(engine as usize),
                            parameter_value(&rack, engine as usize, parameter)
                        ));
                    }
                }
            }
            Some(Control::Absolute {
                engine,
                parameter,
                value,
            }) => {
                let mut rack = self.rack;
                set_normalized(&mut rack, engine, parameter, value);
                self.midi_driving = self.mapper.last_binding;
                if self.publish(rack) {
                    self.parameter_feedback(engine, parameter);
                }
                self.midi_driving = None;
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
                self.midi_driving = self.mapper.last_binding;
                if self.publish(rack) {
                    self.parameter_feedback(engine, parameter);
                }
                self.midi_driving = None;
            }
            None => {}
        }
    }
    pub fn action(&mut self, action: Action) {
        match action {
            Action::SlotBypass(slot) => self.handle(Command::ToggleSlot(slot as usize)),
            Action::PreviousEffect | Action::NextEffect => {
                if matches!(self.page, Page::Main | Page::Play) {
                    let slot = wrap(
                        self.selected_slots[self.selected],
                        if action == Action::NextEffect { 1 } else { -1 },
                        8,
                    );
                    self.handle(Command::SelectSlot(slot));
                } else {
                    self.action(if action == Action::NextEffect {
                        Action::Next
                    } else {
                        Action::Prev
                    });
                }
            }
            Action::ParameterPage => self.handle(Command::ParameterPage),
            Action::SwitchEngine => self.handle(Command::Engine(1 - self.selected)),
            Action::MenuConfirm => {
                if matches!(self.page, Page::Main | Page::Play) {
                    self.handle(Command::More);
                } else {
                    self.action(Action::Confirm);
                }
            }
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
            Action::Effects => self.handle(Command::Effects),
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
            let old_page = self.page;
            let old_focus = self.focus;
            self.focus = i;
            self.handle(hit.command);
            if matches!(old_page, Page::Main | Page::Play)
                && !matches!(self.page, Page::Main | Page::Play)
            {
                self.home_focus = old_focus;
            }
        }
    }
    fn enter(&mut self, page: Page) {
        self.value_context = None;
        let previous_page = self.page;
        if matches!(previous_page, Page::Main | Page::Play)
            && !matches!(page, Page::Main | Page::Play)
        {
            self.home_page = previous_page;
            self.home_focus = self.focus;
            self.home_field = self.field;
        }
        if effect_page(page) && !effect_page(self.page) {
            self.draft = self.rack;
            self.effects_base = self.rack.engines[self.selected];
            self.effect_slot = self.selected_slots[self.selected]
                .min(self.rack.engines[self.selected].stage_count() - 1);
            self.slot_page[self.selected] = self.effect_slot / 3;
        }
        self.page = page;
        self.field = 0;
        self.focus = 2;
        self.edit = None;
        self.learning = false;
        self.learn_conflict = None;
        self.overwrite = false;
        match page {
            Page::Routing | Page::Tempo => {
                self.draft = self.rack;
                self.tempo_base = self.rack;
            }
            Page::Ports => {
                self.port_draft = self.local.ports.clone();
                self.refresh();
            }
            Page::Midi | Page::Controller => {
                if !matches!(previous_page, Page::Midi | Page::Controller) {
                    self.midi_draft = self.local.clone();
                }
                self.refresh();
            }
            _ => {}
        }
        if matches!(page, Page::Main | Page::Play) {
            self.focus = self.home_focus.min(self.hits().len() - 1);
            self.field = self.home_field;
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
        // A shortcut must not silently replace an open structural draft.
        let leaving_draft = matches!(
            command,
            Command::More
                | Command::Routing
                | Command::Sounds
                | Command::Midi
                | Command::Controller
                | Command::Tempo
                | Command::Effects
                | Command::Ports
        );
        let within_effects = effect_page(self.page) && command == Command::Effects;
        let to_ports = self.page == Page::Routing && command == Command::Ports;
        if leaving_draft
            && (effect_page(self.page) || matches!(self.page, Page::Routing | Page::Tempo))
            && !within_effects
            && !to_ports
        {
            self.status = "Apply or Cancel draft before opening menu".into();
            return;
        }
        match command {
            Command::Wet => {
                self.field = 16;
                self.status = "Selected effect wet level / rotary 9".into();
            }
            Command::Rack => {
                self.enter(Page::Main);
                self.focus = 2 + self.context().slot;
            }
            Command::SwitchEngine => self.handle(Command::Engine(1 - self.selected)),
            Command::UseRole => {
                if let Some(binding) = self.learn_conflict.take() {
                    self.midi_draft.bindings.retain(|b| {
                        b.target != binding.target
                            && !(b.channel == binding.channel
                                && b.number == binding.number
                                && (b.kind == ControlKind::Note)
                                    == (binding.kind == ControlKind::Note))
                    });
                    self.midi_draft.bindings.push(binding);
                    self.status = "Role replaces explicit mapping in draft".into();
                }
            }
            Command::KeepLive => {
                if effect_page(self.page) {
                    let active = self.rack.engines[self.selected];
                    keep_live_effects(
                        &mut self.draft.engines[self.selected],
                        self.effects_base,
                        active,
                    );
                    self.effects_base = active;
                } else if self.page == Page::Tempo {
                    let e = self.selected;
                    let (base, active) = (self.tempo_base, self.rack);
                    if active.engines[e].tempo.bpm != base.engines[e].tempo.bpm {
                        self.draft.engines[e].tempo.bpm = active.engines[e].tempo.bpm;
                    }
                    if active.engines[e].tempo.source != base.engines[e].tempo.source {
                        self.draft.engines[e].tempo.source = active.engines[e].tempo.source;
                    }
                    keep_live_effects(
                        &mut self.draft.engines[e],
                        base.engines[e],
                        active.engines[e],
                    );
                    self.tempo_base = active;
                }
                self.status = "Live fields kept; review then Apply".into();
            }
            Command::Controller => {
                self.enter(Page::Controller);
                self.learn_kind = if Role::at(self.setup_step).rotary() {
                    ControlKind::AbsoluteCc
                } else {
                    ControlKind::Note
                };
            }
            Command::SetupNext | Command::SetupPrevious => {
                self.learn_conflict = None;
                self.setup_step = wrap(
                    self.setup_step,
                    if command == Command::SetupNext { 1 } else { -1 },
                    Role::SETUP_COUNT,
                );
                self.learning = false;
                self.learn_kind = self
                    .midi_draft
                    .bindings
                    .iter()
                    .find(|b| b.target == Target::Surface(Role::at(self.setup_step)))
                    .map_or(
                        if Role::at(self.setup_step).rotary() {
                            ControlKind::AbsoluteCc
                        } else {
                            ControlKind::Note
                        },
                        |b| b.kind,
                    );
            }
            Command::ClearRole => {
                self.midi_draft
                    .bindings
                    .retain(|b| b.target != Target::Surface(Role::at(self.setup_step)));
                self.learning = false;
                self.status = "Role cleared in draft; Apply or Cancel".into();
            }
            Command::SelectSlot(slot) => {
                if slot < 8 && matches!(self.page, Page::Main | Page::Play) {
                    self.selected_slots[self.selected] = slot;
                    self.page = Page::Play;
                    self.field = 16;
                    self.focus = 2;
                    self.rearm_surface(false);
                }
            }
            Command::ToggleSlot(slot) => {
                let mut rack = self.rack;
                let e = &mut rack.engines[self.selected];
                if slot >= e.stage_count() {
                    self.control_feedback(format!("Slot {} empty; use Effects", slot + 1));
                } else {
                    e.stages[slot].bypass ^= true;
                    let off = e.stages[slot].bypass;
                    if self.publish(rack) {
                        self.control_feedback(format!(
                            "{}{} {}",
                            engine_name(self.selected),
                            slot + 1,
                            if off {
                                "OFF: tail allowed"
                            } else {
                                "ON: processing"
                            }
                        ));
                    }
                }
            }
            Command::ParameterPage => {
                if matches!(self.page, Page::Main | Page::Play) {
                    let slot = self.selected_slots[self.selected];
                    self.parameter_pages[self.selected][slot] ^= 1;
                    self.field = 16;
                    self.rearm_surface(false);
                } else {
                    self.status = "Page selects parameters on main screen".into();
                }
            }
            Command::Knob(knob) => {
                self.field = knob;
                if let Some(Target::Parameter { engine, parameter }) =
                    self.context().resolve(&self.rack, Role::at(knob))
                {
                    self.control_feedback(parameter_range(parameter).into());
                    let _ = engine;
                }
            }
            Command::Field(field) => {
                if field != self.field {
                    self.edit = None;
                }
                self.name_offset = 0;
                self.field = field;
            }
            Command::Engine(engine) => {
                if engine == self.selected || engine >= self.rack.engines.len() {
                    return;
                }
                if effect_page(self.page) || self.page == Page::Tempo {
                    self.status = "Apply or Cancel draft before engine swap".into();
                    return;
                }
                self.edit = None;
                self.selected = engine;
                self.rearm_surface(true);
            }
            Command::Minus | Command::Plus => {
                self.adjust(if command == Command::Plus { 1 } else { -1 })
            }
            Command::Apply => self.apply(),
            Command::Cancel => {
                if self.learn_conflict.take().is_some() {
                    self.status = "Explicit mapping kept".into();
                } else if self.learning {
                    self.learning = false;
                    self.status = "Learn cancelled; mappings kept".into();
                } else if self.edit.take().is_some() {
                    self.status = "Edit cancelled".into();
                } else if self.page == Page::Ports && self.parent_routing.is_some() {
                    let draft = self.parent_routing.take().unwrap();
                    self.enter(Page::Routing);
                    self.draft = draft;
                } else if self.page == Page::Play {
                    self.handle(Command::Rack);
                } else {
                    self.enter(self.home_page);
                    self.status = "Back; live sound kept".into();
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
                } else if self.publish(rack) {
                    self.status = format!("{} resumed", engine_name(e));
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
                self.status = "PANIC: A+B muted; Resume each engine".into();
            }
            Command::Exit => {
                self.handle(Command::Panic);
                self.quit = true;
            }
            Command::Effects => self.enter(Page::Effects),
            Command::EditSlot(slot) => {
                if !effect_page(self.page) {
                    self.enter(Page::Effects);
                }
                if slot < self.draft.engines[self.selected].stage_count() {
                    self.effect_slot = slot;
                    self.slot_page[self.selected] = slot / 3;
                    self.enter(Page::Effect);
                }
            }
            Command::SlotPage => {
                let engine = if effect_page(self.page) {
                    self.draft.engines[self.selected]
                } else {
                    self.edit.unwrap_or(self.rack.engines[self.selected])
                };
                self.slot_page[self.selected] =
                    (self.slot_page[self.selected] + 1) % engine.stage_count().div_ceil(3);
            }
            Command::PreviousSlot | Command::NextSlot => {
                let count = self.draft.engines[self.selected].stage_count();
                self.effect_slot = wrap(
                    self.effect_slot,
                    if command == Command::NextSlot { 1 } else { -1 },
                    count,
                );
                self.slot_page[self.selected] = self.effect_slot / 3;
                self.enter(Page::Effect);
            }
            Command::EffectTime => {
                if effect_page(self.page) {
                    self.enter(Page::EffectTime);
                }
            }
            Command::Main => self.enter(self.home_page),
            Command::More => self.enter(Page::More),
            Command::Tempo => self.enter(Page::Tempo),
            Command::Routing => self.enter(Page::Routing),
            Command::Ports => {
                if self.page == Page::Routing {
                    self.parent_routing = Some(self.draft);
                }
                self.enter(Page::Ports);
            }
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
                        self.value_context = None;
                        self.edit = None;
                        self.status = format!("Loaded Sound {:02}", self.slot + 1);
                    }
                }
                Err(e) => self.status = format!("Kept rack: {e}"),
            },
            Command::Learn => {
                self.learn_conflict = None;
                if self.midi.is_some() || !self.midi_allowed {
                    self.learning = true;
                    self.status = "Move chosen control; Cancel to stop".into();
                } else {
                    self.status = "Select source and Apply before Learn".into();
                }
            }
            Command::Retry => self.retry_audio(),
            Command::Refresh => self.refresh(),
            Command::NamePart => {
                if self.page == Page::Midi {
                    let name = self
                        .midi_draft
                        .midi_source
                        .as_ref()
                        .map_or("None".into(), |s| s.label());
                    let (_, next) = name_part(&name, self.name_offset);
                    self.name_offset = if next >= name.chars().count() {
                        0
                    } else {
                        next
                    };
                    return;
                }
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
            Page::Main | Page::Play => {
                if let Some(Target::Parameter { engine, parameter }) = self.value_target() {
                    let mut rack = self.rack;
                    let value = normalized(&rack, engine as usize, parameter)
                        + delta as f32 * parameter_step(parameter);
                    set_normalized(&mut rack, engine as usize, parameter, value);
                    if self.publish(rack) {
                        self.parameter_feedback(engine as usize, parameter);
                    }
                } else {
                    self.control_feedback("Unassigned rotary".into());
                }
            }
            Page::Effects => {
                let engine = &mut self.draft.engines[self.selected];
                match self.field {
                    0 => {
                        engine.mode = if engine.mode == EngineMode::Single {
                            EngineMode::MultiFx
                        } else {
                            EngineMode::Single
                        }
                    }
                    1 if engine.mode == EngineMode::MultiFx => {
                        engine.pieces =
                            (engine.pieces as i32 + delta).clamp(2, MAX_STAGES as i32) as u8;
                        self.effect_slot = self.effect_slot.min(engine.stage_count() - 1);
                        self.slot_page[self.selected] =
                            self.slot_page[self.selected].min((engine.stage_count() - 1) / 3);
                    }
                    _ => {}
                }
            }
            Page::Effect => {
                let effect = &mut self.draft.engines[self.selected].stages[self.effect_slot];
                match self.field {
                    0 => effect.algorithm = cycle(&Algorithm::ALL, effect.algorithm, delta),
                    1 => match effect.algorithm {
                        Algorithm::Delay => {
                            effect.delay.kind = cycle(&DelayKind::ALL, effect.delay.kind, delta)
                        }
                        Algorithm::Room => {
                            effect.reverb.kind = cycle(&ReverbKind::ALL, effect.reverb.kind, delta)
                        }
                        Algorithm::Chorus => effect.chorus.ensemble ^= true,
                        Algorithm::Exciter => effect.exciter.bright ^= true,
                    },
                    2..=4 => adjust_primary(effect, self.field - 1, delta),
                    5 => effect.level = (effect.level + delta as f32 * 0.01).clamp(0.0, 1.0),
                    6 => effect.bypass ^= true,
                    _ => {}
                }
            }
            Page::EffectTime => {
                let delay = &mut self.draft.engines[self.selected].stages[self.effect_slot].delay;
                match self.field {
                    0 => delay.sync ^= true,
                    1 => delay.division = wrap(delay.division as usize, delta, 5) as u8,
                    2 => delay.ping_pong ^= true,
                    _ => {}
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
                    3 => self.draft.engines[e].stages[0].delay.sync ^= true,
                    4 => {
                        self.draft.engines[e].stages[0].delay.division = wrap(
                            self.draft.engines[e].stages[0].delay.division as usize,
                            delta,
                            5,
                        )
                            as u8
                    }
                    5 => self.draft.engines[e].stages[0].delay.ping_pong ^= true,
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
            Page::Controller => {
                self.learning = false;
                self.learn_kind = match self.learn_kind {
                    ControlKind::Note => ControlKind::ButtonCc,
                    ControlKind::ButtonCc => ControlKind::Note,
                    ControlKind::AbsoluteCc => ControlKind::RelativeCc,
                    ControlKind::RelativeCc => ControlKind::AbsoluteCc,
                };
            }
            Page::More => {}
        }
    }
    fn apply(&mut self) {
        match self.page {
            Page::Effects | Page::Effect | Page::EffectTime => {
                if effects_conflict(
                    self.rack.engines[self.selected],
                    self.effects_base,
                    self.draft.engines[self.selected],
                ) {
                    self.status = "Live conflict: Keep live, or Cancel".into();
                    return;
                }
                let mut rack = self.rack;
                merge_effects(
                    &mut rack.engines[self.selected],
                    self.effects_base,
                    self.draft.engines[self.selected],
                );
                if self.publish(rack) {
                    self.enter(self.home_page);
                    self.status = "Applied draft fields; live others kept".into();
                }
            }
            Page::Main | Page::Play => {}
            Page::Routing => {
                let mut rack = self.rack;
                rack.routing = self.draft.routing;
                match rack.validate(self.availability()) {
                    Ok(()) => {
                        if self.publish(rack) {
                            self.enter(self.home_page);
                            self.status = "Routing applied".into();
                        }
                    }
                    Err(e) => self.status = e,
                }
            }
            Page::Tempo => {
                let (a, b, d) = (
                    self.rack.engines[self.selected],
                    self.tempo_base.engines[self.selected],
                    self.draft.engines[self.selected],
                );
                if conflict(a.tempo.bpm, b.tempo.bpm, d.tempo.bpm)
                    || conflict(a.tempo.source, b.tempo.source, d.tempo.source)
                    || effects_conflict(a, b, d)
                {
                    self.status = "Live conflict: Keep live, or Cancel".into();
                    return;
                }
                let mut rack = self.rack;
                let base = self.tempo_base;
                let draft = self.draft;
                if base.shared_tempo != draft.shared_tempo {
                    rack.shared_tempo = draft.shared_tempo;
                }
                let e = self.selected;
                let mut tempo = rack.engines[e].tempo;
                if base.engines[e].tempo.bpm != draft.engines[e].tempo.bpm {
                    tempo.bpm = draft.engines[e].tempo.bpm;
                }
                if base.engines[e].tempo.source != draft.engines[e].tempo.source {
                    tempo.source = draft.engines[e].tempo.source;
                }
                rack.set_tempo(e, tempo);
                merge_effects(&mut rack.engines[e], base.engines[e], draft.engines[e]);
                if self.publish(rack) {
                    self.enter(self.home_page);
                    self.status = "Tempo draft applied; live others kept".into();
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
                        if let Some(draft) = self.parent_routing.take() {
                            self.enter(Page::Routing);
                            self.draft = draft;
                        } else {
                            self.enter(self.home_page);
                        }
                        self.status = "Physical ports saved".into();
                    }
                    Err(e) => self.status = e,
                }
            }
            Page::Midi | Page::Controller => self.apply_midi(),
            _ => {}
        }
    }
    fn apply_midi(&mut self) {
        if let Err(error) = self.midi_draft.validate() {
            self.status = error;
            return;
        }
        let replace_input = self.midi_allowed
            && (self.local.midi_source != self.midi_draft.midi_source
                || self.midi.is_none()
                || !self.midi_error.is_empty());
        // Prepare a replacement before retiring the working subscription. A
        // failed open/save retains the old source, bindings, and editable draft.
        let prepared = if replace_input {
            match self
                .midi_draft
                .midi_source
                .as_ref()
                .map(|source| MidiInput::start(source, self.epoch))
                .transpose()
            {
                Ok(input) => input,
                Err(error) => {
                    self.status = format!("Kept MIDI: {error}");
                    return;
                }
            }
        } else {
            None
        };
        if let Err(error) = storage::save_local(&self.root, &self.midi_draft) {
            self.status = error;
            return;
        }
        self.local = self.midi_draft.clone();
        self.learning = false;
        self.mapper.reset_pickup();
        self.value_context = None;
        if replace_input {
            self.midi = prepared;
            self.mapper.require_releases();
            self.value_context = None;
            self.clock = Clock::default();
            self.midi_error.clear();
        }
        self.status = "MIDI mappings saved; release controls".into();
    }
    pub fn status_line(&self) -> String {
        if let Some(audio) = &self.audio {
            if audio.shared.server_down.load(Ordering::Acquire) {
                return "JACK stopped; Menu > Retry audio".into();
            }
            if audio.shared.rate_fault.load(Ordering::Acquire) {
                return "Rate changed: muted; Menu > Retry audio".into();
            }
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
                "{} ports missing; Menu > Ports",
                mask_label(self.meters.missing)
            );
        }
        if self.audio.as_ref().is_some_and(|audio| audio.pending()) {
            return "Applying controls...".into();
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
    fn visible_slots(&self, engine: EngineConfig) -> std::ops::Range<usize> {
        let count = engine.stage_count();
        let start = self.slot_page[self.selected].min((count - 1) / 3) * 3;
        start..(start + 3).min(count)
    }
    pub fn hits(&self) -> Vec<Hit> {
        let mut hits = vec![
            hit(20, 0, 10, Command::Panic, "PANIC"),
            hit(30, 0, 10, Command::Exit, "Exit"),
        ];
        match self.page {
            Page::Main => {
                for slot in 0..8 {
                    let e = self.rack.engines[self.selected];
                    let label = if slot < e.stage_count() {
                        format!("{} {}", slot + 1, e.stages[slot].label())
                    } else {
                        format!("{} Empty", slot + 1)
                    };
                    let mut card = hit(
                        (slot % 2) as u16 * 20,
                        2 + (slot / 2) as u16 * 2,
                        20,
                        Command::SelectSlot(slot),
                        &label,
                    );
                    card.rect.height = 2;
                    hits.push(card);
                }
                footer(
                    &mut hits,
                    [
                        (Command::Engine(0), "Engine A"),
                        (Command::Engine(1), "Engine B"),
                        (Command::Tap, "TAP"),
                        (Command::More, "Menu"),
                    ],
                    [
                        (Command::SelectSlot(self.context().slot), "Open"),
                        (Command::Effects, "Configure"),
                        (Command::Routing, "Routing"),
                        (Command::Sounds, "Sounds"),
                    ],
                );
            }
            Page::Play => {
                hits.push(hit(0, 2, 13, Command::Wet, "Wet"));
                for knob in 0..16 {
                    if matches!(
                        self.context().resolve(&self.rack, Role::at(knob as usize)),
                        Some(Target::Parameter { .. })
                    ) {
                        let mut control = hit(
                            (knob % 8) as u16 * 5,
                            if knob < 8 { 4 } else { 7 },
                            5,
                            Command::Knob(knob as usize),
                            &format!("{knob}"),
                        );
                        control.rect.height = 3;
                        hits.push(control);
                    }
                }
                footer(
                    &mut hits,
                    [
                        (Command::Minus, "-"),
                        (Command::Plus, "+"),
                        (Command::Tap, "TAP"),
                        (
                            Command::ToggleSlot(self.context().slot),
                            if self.rack.engines[self.selected].stages[self.context().slot].bypass {
                                "Slot ON"
                            } else {
                                "Slot OFF"
                            },
                        ),
                    ],
                    [
                        (Command::Rack, "Rack"),
                        (Command::SwitchEngine, "A / B"),
                        (Command::More, "Menu"),
                        (Command::Effects, "Configure"),
                    ],
                );
            }
            Page::More => {
                for (i, (command, label)) in [
                    (Command::Effects, "Effects / parallel slots"),
                    (Command::Tempo, "Tempo / slot 1 delay"),
                    (Command::Midi, "MIDI source / explicit mappings"),
                    (Command::Controller, "Controller / 16 knobs + 8 pads"),
                    (Command::Ports, "Physical JACK ports"),
                    (Command::Retry, "Retry audio"),
                    (Command::Main, "Back to performance"),
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
            Page::Effects => {
                let engine = self.draft.engines[self.selected];
                let mut labels = vec![format!(
                    "Mode: {}",
                    if engine.mode == EngineMode::MultiFx {
                        "MultiFX / parallel"
                    } else {
                        "Single effect"
                    }
                )];
                if engine.mode == EngineMode::MultiFx {
                    labels.push(format!(
                        "Active slots: {} (2–{MAX_STAGES})",
                        engine.stage_count()
                    ));
                }
                fields(&mut hits, &labels, 0);
                let visible = self.visible_slots(engine);
                if engine.stage_count() > 3 {
                    hits.push(hit(
                        0,
                        8,
                        40,
                        Command::SlotPage,
                        &format!(
                            "Slots {}-{} of {} / next page",
                            visible.start + 1,
                            visible.end,
                            engine.stage_count()
                        ),
                    ));
                }
                for (row, i) in visible.enumerate() {
                    hits.push(hit(
                        0,
                        5 + row as u16,
                        40,
                        Command::EditSlot(i),
                        &format!("Edit slot {}: {}", i + 1, engine.stages[i].label()),
                    ));
                }
                editor_footer(&mut hits, Command::Tap, "TAP");
                if let Some(h) = hits.iter_mut().find(|h| h.command == Command::Engine(1)) {
                    h.command = Command::KeepLive;
                    h.label = "Keep live".into();
                }
            }
            Page::Effect => {
                let engine = self.draft.engines[self.selected];
                let effect = engine.stages[self.effect_slot];
                let mut labels = vec![
                    format!("Effect: {}", effect.algorithm.label()),
                    format!("Type: {}", effect.label()),
                ];
                labels.extend(primary_labels(
                    effect,
                    self.rack.engines[self.selected].tempo,
                ));
                labels.push(format!("Slot wet level: {:.0}%", effect.level * 100.0));
                labels.push(format!(
                    "Slot bypass: {}",
                    if effect.bypass {
                        "OFF / tail allowed"
                    } else {
                        "ON / processing"
                    }
                ));
                fields(&mut hits, &labels, 0);
                footer(
                    &mut hits,
                    [
                        (Command::Minus, "-"),
                        (Command::Plus, "+"),
                        (Command::Apply, "Apply"),
                        (Command::Cancel, "Cancel"),
                    ],
                    [
                        (Command::PreviousSlot, "Prev slot"),
                        (Command::NextSlot, "Next slot"),
                        if effect.algorithm == Algorithm::Delay {
                            (Command::EffectTime, "Timing")
                        } else {
                            (Command::Tap, "TAP")
                        },
                        (Command::Effects, "Slots"),
                    ],
                );
            }
            Page::EffectTime => {
                let delay = self.draft.engines[self.selected].stages[self.effect_slot].delay;
                fields(
                    &mut hits,
                    &[
                        format!(
                            "Time source: {}",
                            if delay.sync {
                                "Tempo division"
                            } else {
                                "Free ms"
                            }
                        ),
                        format!(
                            "Division: {}",
                            DelayConfig::DIVISION_LABELS[delay.division as usize]
                        ),
                        format!(
                            "Feedback path: {}",
                            if delay.ping_pong {
                                "Ping-pong"
                            } else {
                                "Independent L/R"
                            }
                        ),
                    ],
                    0,
                );
                footer(
                    &mut hits,
                    [
                        (Command::Minus, "-"),
                        (Command::Plus, "+"),
                        (Command::Apply, "Apply"),
                        (Command::Cancel, "Cancel"),
                    ],
                    [
                        (Command::PreviousSlot, "Prev slot"),
                        (Command::NextSlot, "Next slot"),
                        (Command::Tap, "TAP"),
                        (Command::EditSlot(self.effect_slot), "Back"),
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
                        if e.stages[0].delay.sync {
                            "Tempo division"
                        } else {
                            "Free ms"
                        }
                    ),
                    format!(
                        "Division: {}",
                        DelayConfig::DIVISION_LABELS[e.stages[0].delay.division as usize]
                    ),
                    format!(
                        "Delay stereo: {}",
                        if e.stages[0].delay.ping_pong {
                            "Ping-pong"
                        } else {
                            "Independent L/R"
                        }
                    ),
                ];
                fields(&mut hits, &labels, 0);
                editor_footer(&mut hits, Command::Tap, "TAP");
                if let Some(h) = hits.iter_mut().find(|h| h.command == Command::Engine(1)) {
                    h.command = Command::KeepLive;
                    h.label = "Keep live".into();
                }
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
            Page::Controller => {
                hits.push(hit(
                    0,
                    4,
                    40,
                    Command::Field(0),
                    &format!("Kind: {}", kind_label(self.learn_kind)),
                ));
                hits.push(hit(
                    0,
                    8,
                    20,
                    if self.learn_conflict.is_some() {
                        Command::UseRole
                    } else {
                        Command::ClearRole
                    },
                    if self.learn_conflict.is_some() {
                        "Use role"
                    } else {
                        "Clear this role"
                    },
                ));
                hits.push(hit(20, 8, 20, Command::Midi, "Source / explicit"));
                footer(
                    &mut hits,
                    [
                        (Command::Minus, "Kind <"),
                        (Command::Plus, "Kind >"),
                        (Command::Learn, "Learn"),
                        (Command::Apply, "Apply"),
                    ],
                    [
                        (Command::SetupPrevious, "Role <"),
                        (Command::SetupNext, "Role >"),
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
                        (Command::Controller, "Guided"),
                        (Command::NamePart, "Name >"),
                        (Command::Cancel, "Cancel"),
                    ],
                );
            }
        }
        if (effect_page(self.page)
            && effects_conflict(
                self.rack.engines[self.selected],
                self.effects_base,
                self.draft.engines[self.selected],
            ))
            && !hits.iter().any(|h| h.command == Command::KeepLive)
            && let Some(h) = hits.iter_mut().find(|h| h.command == Command::PreviousSlot)
        {
            h.command = Command::KeepLive;
            h.label = "Keep live".into();
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
        for parameter in Parameter::choices() {
            targets.push(Target::Parameter { engine, parameter });
        }
    }
    targets
}
fn target_label(target: Target) -> String {
    match target {
        Target::Surface(role) => role.label(),
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
fn effect_page(page: Page) -> bool {
    matches!(page, Page::Effects | Page::Effect | Page::EffectTime)
}
fn cycle<T: Copy + PartialEq>(choices: &[T], current: T, delta: i32) -> T {
    choices[wrap(
        choices.iter().position(|v| *v == current).unwrap_or(0),
        delta,
        choices.len(),
    )]
}
fn primary_control(effect: EffectConfig, field: usize) -> SlotParameter {
    match (effect.algorithm, field) {
        (Algorithm::Delay, 1) => SlotParameter::DelayTime,
        (Algorithm::Delay, 2) => SlotParameter::DelayFeedback,
        (Algorithm::Delay, _) => SlotParameter::DelayDamping,
        (Algorithm::Room, 1) => SlotParameter::Predelay,
        (Algorithm::Room, 2) => SlotParameter::Decay,
        (Algorithm::Room, _) => SlotParameter::ReverbDamping,
        (Algorithm::Chorus, 1) => SlotParameter::ChorusRate,
        (Algorithm::Chorus, 2) => SlotParameter::ChorusDepth,
        (Algorithm::Chorus, _) => SlotParameter::ChorusBase,
        (Algorithm::Exciter, 1) => SlotParameter::ExciterTune,
        (Algorithm::Exciter, 2) => SlotParameter::ExciterDrive,
        (Algorithm::Exciter, _) => SlotParameter::ExciterTone,
    }
}
fn primary_labels(effect: EffectConfig, tempo: Tempo) -> [String; 3] {
    match effect.algorithm {
        Algorithm::Delay => [
            format!(
                "Time: {:.0} ms{}",
                effect.delay.milliseconds(tempo),
                if effect.delay.sync { " sync" } else { " free" }
            ),
            format!("Feedback: {:.0}%", effect.delay.feedback * 100.0),
            format!("Damping: {:.0}%", effect.delay.damping * 100.0),
        ],
        Algorithm::Room => [
            format!("Predelay: {:.0} ms", effect.reverb.predelay_ms),
            format!("Decay: {:.0}%", effect.reverb.decay * 100.0),
            format!("Damping: {:.0}%", effect.reverb.damping * 100.0),
        ],
        Algorithm::Chorus => [
            format!("Rate: {:.2} Hz", effect.chorus.rate_hz),
            format!("Depth: {:.1} ms", effect.chorus.depth_ms),
            format!("Base delay: {:.1} ms", effect.chorus.base_ms),
        ],
        Algorithm::Exciter => [
            format!("Tune: {:.0} Hz", effect.exciter.tune_hz),
            format!("Drive: {:.0}%", effect.exciter.drive * 100.0),
            format!("Tone: {:.0}%", effect.exciter.tone * 100.0),
        ],
    }
}
fn slot_step(control: SlotParameter) -> f32 {
    match control {
        SlotParameter::DelaySync => 1.0,
        SlotParameter::DelayDivision => 0.25,
        SlotParameter::DelayTime => 5.0 / 1999.0,
        SlotParameter::Predelay => 1.0 / 200.0,
        SlotParameter::ChorusRate => 0.05 / 4.95,
        SlotParameter::ChorusDepth => 0.1 / 8.0,
        SlotParameter::ChorusBase => 0.5 / 20.0,
        SlotParameter::DelayFeedback | SlotParameter::Decay => 0.01 / 0.9,
        SlotParameter::DelayDamping | SlotParameter::ReverbDamping => 0.01 / 0.95,
        SlotParameter::ExciterTune => 100.0 / 5400.0,
        SlotParameter::Level | SlotParameter::ExciterDrive | SlotParameter::ExciterTone => 0.01,
    }
}
fn slot_normalized(effect: EffectConfig, control: SlotParameter) -> f32 {
    match control {
        SlotParameter::DelaySync => {
            if effect.delay.sync {
                1.0
            } else {
                0.0
            }
        }
        SlotParameter::DelayDivision => effect.delay.division as f32 / 4.0,
        SlotParameter::DelayTime => (effect.delay.time_ms - 1.0) / 1999.0,
        SlotParameter::DelayFeedback => effect.delay.feedback / 0.9,
        SlotParameter::DelayDamping => effect.delay.damping / 0.95,
        SlotParameter::Predelay => effect.reverb.predelay_ms / 200.0,
        SlotParameter::Decay => effect.reverb.decay / 0.9,
        SlotParameter::ReverbDamping => effect.reverb.damping / 0.95,
        SlotParameter::ChorusRate => (effect.chorus.rate_hz - 0.05) / 4.95,
        SlotParameter::ChorusDepth => effect.chorus.depth_ms / 8.0,
        SlotParameter::ChorusBase => (effect.chorus.base_ms - 10.0) / 20.0,
        SlotParameter::Level => effect.level,
        SlotParameter::ExciterTune => (effect.exciter.tune_hz - 600.0) / 5400.0,
        SlotParameter::ExciterDrive => effect.exciter.drive,
        SlotParameter::ExciterTone => effect.exciter.tone,
    }
}
fn set_slot_normalized(effect: &mut EffectConfig, control: SlotParameter, value: f32) {
    let value = value.clamp(0.0, 1.0);
    match control {
        SlotParameter::DelaySync => effect.delay.sync = value >= 0.5,
        SlotParameter::DelayDivision => effect.delay.division = (value * 4.0).round() as u8,
        SlotParameter::DelayTime => {
            effect.delay.time_ms = 1.0 + value * 1999.0;
            effect.delay.sync = false;
        }
        SlotParameter::DelayFeedback => effect.delay.feedback = value * 0.9,
        SlotParameter::DelayDamping => effect.delay.damping = value * 0.95,
        SlotParameter::Predelay => effect.reverb.predelay_ms = value * 200.0,
        SlotParameter::Decay => effect.reverb.decay = value * 0.9,
        SlotParameter::ReverbDamping => effect.reverb.damping = value * 0.95,
        SlotParameter::ChorusRate => effect.chorus.rate_hz = 0.05 + value * 4.95,
        SlotParameter::ChorusDepth => effect.chorus.depth_ms = value * 8.0,
        SlotParameter::ChorusBase => effect.chorus.base_ms = 10.0 + value * 20.0,
        SlotParameter::Level => effect.level = value,
        SlotParameter::ExciterTune => effect.exciter.tune_hz = 600.0 + value * 5400.0,
        SlotParameter::ExciterDrive => effect.exciter.drive = value,
        SlotParameter::ExciterTone => effect.exciter.tone = value,
    }
}
fn adjust_primary(effect: &mut EffectConfig, field: usize, delta: i32) {
    let control = primary_control(*effect, field);
    set_slot_normalized(
        effect,
        control,
        slot_normalized(*effect, control) + delta as f32 * slot_step(control),
    );
}
/// Commit only deliberate draft differences. Clock, Panic, live CCs on other
/// controls and the other engine remain authoritative during a long edit.
fn merge_effects(active: &mut EngineConfig, base: EngineConfig, draft: EngineConfig) {
    if base.mode != draft.mode {
        active.mode = draft.mode;
    }
    if base.pieces != draft.pieces {
        active.pieces = draft.pieces;
    }
    for i in 0..MAX_STAGES {
        let (a, b, d) = (&mut active.stages[i], base.stages[i], draft.stages[i]);
        macro_rules! merge { ($($field:ident).+) => { if b.$($field).+ != d.$($field).+ { a.$($field).+ = d.$($field).+; } }; }
        merge!(algorithm);
        merge!(level);
        merge!(bypass);
        merge!(delay.kind);
        merge!(delay.time_ms);
        merge!(delay.feedback);
        merge!(delay.damping);
        merge!(delay.sync);
        merge!(delay.division);
        merge!(delay.ping_pong);
        merge!(reverb.kind);
        merge!(reverb.predelay_ms);
        merge!(reverb.decay);
        merge!(reverb.damping);
        merge!(chorus.rate_hz);
        merge!(chorus.depth_ms);
        merge!(chorus.base_ms);
        merge!(chorus.ensemble);
        merge!(exciter.tune_hz);
        merge!(exciter.drive);
        merge!(exciter.tone);
        merge!(exciter.bright);
    }
}
fn parameter_step(parameter: Parameter) -> f32 {
    match parameter {
        Parameter::Time => 5.0 / 1999.0,
        Parameter::Bpm => 1.0 / 270.0,
        Parameter::Slot { control, .. } => slot_step(control),
        _ => 0.01,
    }
}
pub fn normalized(rack: &Rack, engine: usize, parameter: Parameter) -> f32 {
    let e = rack.engines[engine];
    match parameter {
        Parameter::Slot { slot, control } => e
            .stages
            .get(slot as usize)
            .map_or(0.0, |s| slot_normalized(*s, control)),
        Parameter::Time => slot_normalized(e.stages[0], primary_control(e.stages[0], 1)),
        Parameter::Feedback => slot_normalized(e.stages[0], primary_control(e.stages[0], 2)),
        Parameter::Damping => slot_normalized(e.stages[0], primary_control(e.stages[0], 3)),
        Parameter::Level => e.level,
        Parameter::Bpm => (e.tempo.bpm - 30.0) / 270.0,
    }
}
pub fn set_normalized(rack: &mut Rack, engine: usize, parameter: Parameter, value: f32) {
    let value = value.clamp(0.0, 1.0);
    let e = &mut rack.engines[engine];
    match parameter {
        Parameter::Slot { slot, control } => {
            if let Some(s) = e.stages.get_mut(slot as usize) {
                set_slot_normalized(s, control, value);
            }
        }
        Parameter::Time | Parameter::Feedback | Parameter::Damping => {
            let field = match parameter {
                Parameter::Time => 1,
                Parameter::Feedback => 2,
                _ => 3,
            };
            let control = primary_control(e.stages[0], field);
            set_slot_normalized(&mut e.stages[0], control, value);
        }
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
                if lit { "O" } else { "o" },
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
    let white = Style::default().fg(Color::White).bg(Color::Black);
    let gray = Style::default().fg(Color::Gray).bg(Color::Black);
    frame.render_widget(Paragraph::new("").style(white), Rect::new(0, 0, 40, 13));
    text(
        frame,
        0,
        0,
        20,
        if app.audio.is_some() {
            "shr-fx / WET"
        } else {
            "shr-fx / OFFLINE"
        },
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );
    match app.page {
        Page::Main => {
            let e = app.rack.engines[app.selected];
            text(
                frame,
                0,
                1,
                40,
                format!(
                    "[{}]  {} > {}    {:.0} {}",
                    engine_name(app.selected),
                    app.rack.routing.inputs[app.selected].label(),
                    app.rack.routing.return_label(app.selected),
                    e.tempo.bpm,
                    if e.tempo.source == TempoSource::Internal {
                        "Int"
                    } else {
                        "Clk"
                    }
                ),
                gray,
            );
        }
        Page::Play => {
            let c = app.context();
            let e = app.rack.engines[c.engine];
            let effect = e.stages[c.slot];
            text(
                frame,
                0,
                1,
                40,
                format!(
                    "{} / {}  {}   {}",
                    engine_name(c.engine),
                    c.slot + 1,
                    if c.slot < e.stage_count() {
                        effect.label()
                    } else {
                        "Empty"
                    },
                    app.slot_state(c.slot)
                ),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );
            let detail = if let Some(Target::Parameter { engine, parameter }) = app.value_target() {
                let full = match parameter {
                    Parameter::Slot { control, .. } => control.label().to_owned(),
                    _ => parameter.label(),
                };
                let value = parameter_value(&app.rack, engine as usize, parameter);
                let value = value.split_once(' ').map_or(value.as_str(), |(_, v)| v);
                let unit = match parameter {
                    Parameter::Slot {
                        control: SlotParameter::DelayTime,
                        ..
                    } if !value.ends_with("ms") => "ms",
                    Parameter::Slot {
                        control: SlotParameter::ChorusRate | SlotParameter::ExciterTune,
                        ..
                    } => "Hz",
                    _ => "",
                };
                format!("{full} {value}{unit}")
            } else if c.slot >= e.stage_count() {
                "Empty / Configure to add".into()
            } else {
                "Unassigned / choose a value".into()
            };
            text(frame, 13, 2, 27, detail, white);
            let mut spans = vec![Span::raw("I ")];
            spans.extend(leds(app.meters.input[c.engine]));
            spans.push(Span::raw("  O "));
            spans.extend(leds(app.meters.output[c.engine]));
            spans.push(Span::raw(format!(
                "  {:.0} {} / {}",
                e.tempo.bpm,
                if e.tempo.source == TempoSource::Internal {
                    "Int"
                } else {
                    "Clk"
                },
                app.rack.routing.return_label(c.engine)
            )));
            frame.render_widget(Paragraph::new(Spans::from(spans)), Rect::new(0, 3, 40, 1));
            for knob in 0..16 {
                draw_rotary(frame, app, knob);
            }
        }
        Page::More => {
            text(
                frame,
                0,
                1,
                40,
                format!(
                    "MENU {} / 1 Pick + click / 9 Back",
                    engine_name(app.selected)
                ),
                white,
            );
            text(
                frame,
                0,
                9,
                40,
                if let Some(audio) = &app.audio {
                    format!(
                        "Server {:.1}% xruns {}",
                        audio.cpu_load(),
                        audio.shared.xruns.load(Ordering::Relaxed)
                    )
                } else {
                    "Offline: no audio I/O".into()
                },
                gray,
            );
        }
        Page::Effects | Page::Effect | Page::EffectTime => {
            if app.page == Page::Effects
                && app.draft.engines[app.selected].mode == EngineMode::Single
            {
                text(frame, 0, 4, 40, "Single mode uses slot 1", gray);
            }
            if app.page == Page::Effects {
                text(
                    frame,
                    0,
                    9,
                    40,
                    format!(
                        "Mix headroom: 1/{} per slot",
                        app.draft.engines[app.selected].mix_divisor()
                    ),
                    gray,
                );
            }
            text(
                frame,
                0,
                1,
                40,
                if app.page == Page::Effects {
                    format!("Engine {} effects draft", engine_name(app.selected))
                } else {
                    format!(
                        "Engine {} / slot {} draft",
                        engine_name(app.selected),
                        app.effect_slot + 1
                    )
                },
                white,
            );
            text(
                frame,
                0,
                2,
                40,
                if app.page == Page::EffectTime {
                    "Sync max 2000ms; +/- edits this draft".into()
                } else {
                    format!(
                        "MIDI live {}{}; 1 Pick / 9 Edit",
                        engine_name(app.selected),
                        app.context().slot + 1
                    )
                },
                gray,
            );
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
                "Live edits kept; conflicts use Keep live",
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
        Page::Controller => {
            let role = Role::at(app.setup_step);
            text(
                frame,
                0,
                1,
                40,
                format!("CONTROLLER / step {:02} of 26", app.setup_step + 1),
                Style::default().fg(Color::Yellow),
            );
            text(
                frame,
                0,
                2,
                40,
                "Setup rotaries learn without performing",
                gray,
            );
            text(frame, 0, 3, 40, role.label(), white);
            let binding = app
                .midi_draft
                .bindings
                .iter()
                .find(|b| b.target == Target::Surface(role));
            text(
                frame,
                0,
                5,
                40,
                binding.map_or("Unassigned / Role > skips".into(), |b| {
                    format!(
                        "Captured: ch{} #{} / {}",
                        b.channel + 1,
                        b.number,
                        if b.kind == ControlKind::Note {
                            "note"
                        } else {
                            "CC"
                        }
                    )
                }),
                gray,
            );
            text(
                frame,
                0,
                6,
                40,
                if app.learning {
                    "LEARN: move / press, then release"
                } else {
                    "Choose Kind, Learn, gesture, Role >"
                },
                Style::default().fg(Color::Yellow),
            );
            text(
                frame,
                0,
                7,
                40,
                if role.rotary() {
                    "Relative: 2s complement; never guessed"
                } else {
                    "Press once; release before next action"
                },
                gray,
            );
            text(
                frame,
                0,
                9,
                40,
                if app.midi.is_some() {
                    "Apply saves all captured roles privately"
                } else {
                    "No MIDI input; Source needs --midi"
                },
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
            text(frame, 0, 8, 40, "Source name / Name > continues:", gray);
            text(
                frame,
                0,
                9,
                40,
                name_part(
                    &app.midi_draft
                        .midi_source
                        .as_ref()
                        .map_or("None".into(), |s| s.label()),
                    app.name_offset,
                )
                .0,
                gray,
            );
        }
    }
    for (i, hit) in app.hits().iter().enumerate() {
        let focused = i == app.focus;
        if app.page == Page::Play && matches!(hit.command, Command::Knob(_)) {
            continue;
        }
        if app.page == Page::Play && hit.command == Command::Wet {
            text(
                frame,
                0,
                2,
                13,
                format!(
                    "{}Wet {:3.0}%",
                    if app.field == 16 { ">" } else { " " },
                    app.rack.engines[app.selected].stages[app.context().slot].level * 100.0
                ),
                if app.field == 16 {
                    Style::default().fg(Color::Yellow)
                } else {
                    white
                },
            );
            continue;
        }
        if app.page == Page::Main
            && let Command::SelectSlot(slot) = hit.command
            && hit.rect.height == 2
        {
            let style = if focused {
                Style::default().fg(Color::Black).bg(Color::Yellow)
            } else {
                white
            };
            text(
                frame,
                hit.rect.x,
                hit.rect.y,
                20,
                format!("{}{}", if focused { ">" } else { " " }, hit.label),
                style,
            );
            let e = app.rack.engines[app.selected];
            text(
                frame,
                hit.rect.x,
                hit.rect.y + 1,
                20,
                if slot < e.stage_count() {
                    format!(
                        "  {:3.0}%   {}",
                        e.stages[slot].level * 100.0,
                        app.slot_state(slot)
                    )
                } else {
                    "  --".into()
                },
                if focused { style } else { gray },
            );
            continue;
        }
        let field = if matches!(app.page, Page::Main | Page::Play) {
            matches!(hit.command, Command::Knob(n) if n == app.field)
                || matches!(hit.command, Command::SelectSlot(n) if n == app.context().slot)
        } else {
            matches!(hit.command, Command::Field(n) if n == app.field)
        };
        let style = if focused {
            Style::default().fg(Color::Black).bg(Color::Yellow)
        } else if field {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else if hit.command == Command::Panic {
            Style::default().fg(Color::Red)
        } else {
            white
        };
        let label = if hit.rect.y >= 10 || hit.rect.y == 0 {
            format!("{:^width$}", hit.label, width = hit.rect.width as usize)
        } else {
            format!("{}{}", if field || focused { ">" } else { " " }, hit.label)
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

fn parameter_range(parameter: Parameter) -> &'static str {
    match parameter {
        Parameter::Bpm => "BPM 30-300; internal clock only",
        Parameter::Level => "Engine return 0-100%",
        Parameter::Slot { control, .. } => match control {
            SlotParameter::DelaySync => "Sync: free ms / tempo division",
            SlotParameter::DelayDivision => "Division: 1/16 1/8 1/4 1/4. 1/2",
            SlotParameter::DelayTime => "Time 1-2000ms; moving selects free ms",
            SlotParameter::DelayFeedback => "Feedback 0-90%",
            SlotParameter::DelayDamping | SlotParameter::ReverbDamping => "Damping 0-95%",
            SlotParameter::Predelay => "Predelay 0-200ms; added effect delay",
            SlotParameter::Decay => "Decay 0-90%; relative, not RT60",
            SlotParameter::ChorusRate => "Rate 0.05-5Hz",
            SlotParameter::ChorusDepth => "Depth 0-8ms",
            SlotParameter::ChorusBase => "Base delay 10-30ms",
            SlotParameter::ExciterTune => "Tune 600-6000Hz",
            SlotParameter::ExciterDrive => "Drive 0-100%",
            SlotParameter::ExciterTone => "Tone 0-100%",
            SlotParameter::Level => "Slot wet 0-100%; no mix renormalizing",
        },
        _ => "Legacy slot 1 control",
    }
}
fn parameter_value(rack: &Rack, engine: usize, parameter: Parameter) -> String {
    match parameter {
        Parameter::Bpm => format!("BPM {:.0}", rack.engines[engine].tempo.bpm),
        Parameter::Level => format!("Rtn {:.0}%", rack.engines[engine].level * 100.0),
        Parameter::Slot { slot, control } => {
            let s = rack.engines[engine].stages[slot as usize];
            match control {
                SlotParameter::DelaySync => {
                    format!("Sync {}", if s.delay.sync { "ON" } else { "OFF" })
                }
                SlotParameter::DelayDivision => format!(
                    "Div {}",
                    DelayConfig::DIVISION_LABELS[s.delay.division as usize]
                ),
                SlotParameter::DelayTime => {
                    if s.delay.sync {
                        format!(
                            "Sync{:.0}ms",
                            s.delay.milliseconds(rack.engines[engine].tempo)
                        )
                    } else {
                        format!("Ms {:.0}", s.delay.time_ms)
                    }
                }
                SlotParameter::DelayFeedback => format!("Fbk {:.0}%", s.delay.feedback * 100.0),
                SlotParameter::DelayDamping => format!("Dmp {:.0}%", s.delay.damping * 100.0),
                SlotParameter::ReverbDamping => format!("Dmp {:.0}%", s.reverb.damping * 100.0),
                SlotParameter::Predelay => format!("Pre {:.0}ms", s.reverb.predelay_ms),
                SlotParameter::Decay => format!("Dcy {:.0}%", s.reverb.decay * 100.0),
                SlotParameter::ChorusRate => format!("Hz {:.2}", s.chorus.rate_hz),
                SlotParameter::ChorusDepth => format!("Dep {:.1}ms", s.chorus.depth_ms),
                SlotParameter::ChorusBase => format!("Base {:.0}ms", s.chorus.base_ms),
                SlotParameter::ExciterTune => format!("Hz {:.0}", s.exciter.tune_hz),
                SlotParameter::ExciterDrive => format!("Drv {:.0}%", s.exciter.drive * 100.0),
                SlotParameter::ExciterTone => format!("Tone {:.0}%", s.exciter.tone * 100.0),
                SlotParameter::Level => format!("Wet {:.0}%", s.level * 100.0),
            }
        }
        _ => format!(
            "{} {:.0}%",
            parameter.label(),
            normalized(rack, engine, parameter) * 100.0
        ),
    }
}

fn conflict<T: PartialEq>(active: T, base: T, draft: T) -> bool {
    draft != base && active != base && active != draft
}
fn effects_conflict(active: EngineConfig, base: EngineConfig, draft: EngineConfig) -> bool {
    let mut merged = active;
    merge_effects(&mut merged, base, draft);
    let mut live_first = draft;
    keep_live_effects(&mut live_first, base, active);
    let mut live_merged = active;
    merge_effects(&mut live_merged, base, live_first);
    merged != live_merged
}
fn keep_live_effects(draft: &mut EngineConfig, base: EngineConfig, active: EngineConfig) {
    // Only fields changed since entry take live values. Other draft work survives.
    merge_effects(draft, base, active);
}

fn draw_rotary<B: Backend>(frame: &mut Frame<B>, app: &App, knob: u8) {
    let x = (knob % 8) as u16 * 5;
    let y = if knob < 8 { 4 } else { 7 };
    let target = if knob == 8 {
        app.value_target()
    } else {
        app.context().resolve(&app.rack, Role::at(knob as usize))
    };
    let selected = app.field == knob as usize;
    let style = if selected {
        Style::default().fg(Color::Black).bg(Color::Yellow)
    } else {
        Style::default().fg(Color::Gray)
    };
    let mark = app.pickup_mark(knob);
    let number = if matches!(knob, 0 | 8) {
        format!("[{:02}]{}", knob + 1, mark)
    } else {
        format!(" {:02}{}", knob + 1, mark)
    };
    text(frame, x, y, 5, number, style);
    let (label, value) = match target {
        Some(Target::Parameter { engine, parameter }) => {
            let value = if let Parameter::Slot {
                slot,
                control: SlotParameter::DelayTime,
            } = parameter
            {
                format!(
                    "ms {:.0}",
                    app.rack.engines[engine as usize].stages[slot as usize]
                        .delay
                        .milliseconds(app.rack.engines[engine as usize].tempo)
                )
            } else {
                parameter_value(&app.rack, engine as usize, parameter)
            };
            let (name, value) = value.split_once(' ').unwrap_or((&value, ""));
            let name = if knob < 8 {
                format!("Wet{}", knob + 1)
            } else {
                name.to_owned()
            };
            (name, value.to_owned())
        }
        _ => ("--".into(), "".into()),
    };
    text(
        frame,
        x,
        y + 1,
        5,
        if knob == 0 {
            "Pick"
        } else if knob == 8 {
            "Edit"
        } else {
            &label
        },
        style,
    );
    text(
        frame,
        x,
        y + 2,
        5,
        if knob == 0 {
            "Open"
        } else if knob == 8 {
            "Back"
        } else {
            &value
        },
        style,
    );
}
