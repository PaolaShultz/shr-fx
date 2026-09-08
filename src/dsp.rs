//! Prepared, bounded wet paths. Each slot owns its delay, reverb, chorus and exciter
//! storage before activation; changes use scalar publication and logical clears.
use crate::model::{
    Algorithm, Availability, DelayKind, EffectConfig, EngineConfig, EngineMode, MAX_STAGES, Rack,
    ReverbKind, Routing, Tempo,
};

pub const MAX_FRAMES: usize = 8192;
pub const MAX_RATE: u32 = 192_000;
pub const MAX_DSP_BYTES: usize = 80 * 1024 * 1024;
const FAULT_LIMIT: f32 = 16.0;
const TAIL_SECONDS: f32 = 281.0;
fn clean(x: f32) -> f32 {
    if x.abs() < 1e-20 { 0.0 } else { x }
}
fn slew(x: &mut f32, target: f32, amount: f32) {
    *x += (target - *x).clamp(-amount, amount);
}
fn safe(x: f32) -> bool {
    x.is_finite() && x.abs() <= FAULT_LIMIT
}

struct Ring {
    data: Vec<f32>,
    pos: usize,
    valid: usize,
}
impl Ring {
    fn new(len: usize) -> Self {
        Self {
            data: vec![0.0; len.max(4)],
            pos: 0,
            valid: 0,
        }
    }
    fn clear(&mut self) {
        self.pos = 0;
        self.valid = 0;
    }
    fn push(&mut self, value: f32) {
        self.data[self.pos] = clean(value);
        self.pos = (self.pos + 1) % self.data.len();
        self.valid = (self.valid + 1).min(self.data.len());
    }
    fn at(&self, delay: usize) -> f32 {
        if delay == 0 || delay > self.valid {
            0.0
        } else {
            self.data[(self.pos + self.data.len() - delay) % self.data.len()]
        }
    }
    fn read(&self, delay: f32) -> f32 {
        let delay = delay.clamp(1.0, (self.data.len() - 2) as f32);
        let n = delay as usize;
        let a = self.at(n);
        a + (self.at(n + 1) - a) * (delay - n as f32)
    }
}
/// Crossfade both read positions for 20 ms; rapid requests coalesce.
struct Tap {
    from: f32,
    to: f32,
    phase: f32,
    step: f32,
}
impl Tap {
    fn new(delay: f32, rate: f32) -> Self {
        Self {
            from: delay,
            to: delay,
            phase: 1.0,
            step: 1.0 / (rate * 0.02),
        }
    }
    fn advance(&mut self, desired: f32) {
        if self.phase >= 1.0 && (desired - self.to).abs() > 0.1 {
            self.from = self.to;
            self.to = desired;
            self.phase = 0.0;
        }
        self.phase = (self.phase + self.step).min(1.0);
    }
    fn read(&self, ring: &Ring) -> f32 {
        self.offset_read(ring, 1.0, 0.0)
    }
    fn offset_read(&self, ring: &Ring, fraction: f32, offset: f32) -> f32 {
        ring.read(self.from * fraction + offset) * (1.0 - self.phase)
            + ring.read(self.to * fraction + offset) * self.phase
    }
}
/// Smooth parabolic oscillator: deterministic phases, no random source or
/// per-sample transcendental work. Every call advances by less than one cycle.
fn oscillator(phase: &mut f32, step: f32) -> f32 {
    *phase += step;
    if *phase >= 1.0 {
        *phase -= 1.0;
    }
    let x = *phase * 2.0 - 1.0;
    4.0 * x * (1.0 - x.abs())
}
struct Allpass {
    ring: Ring,
    length: f32,
}
impl Allpass {
    fn new(rate: f32, ms: f32) -> Self {
        Self {
            ring: Ring::new((rate * 0.016) as usize + 4),
            length: (rate * ms / 1000.0).round(),
        }
    }
    fn tick(&mut self, input: f32) -> f32 {
        let delayed = self.ring.read(self.length);
        let out = delayed - input * 0.5;
        self.ring.push(input + out * 0.5);
        clean(out)
    }
}
struct Delay {
    rings: [Ring; 2],
    low: [f32; 2],
    tap: Tap,
    diffusers: [[Allpass; 2]; 2],
    phases: [f32; 2],
    seed: f32,
}
impl Delay {
    fn new(rate: f32, ms: f32, seed: f32) -> Self {
        Self {
            rings: std::array::from_fn(|_| Ring::new((rate * 2.0) as usize + 4)),
            low: [0.0; 2],
            tap: Tap::new(ms * rate / 1000.0, rate),
            diffusers: std::array::from_fn(|c| {
                std::array::from_fn(|n| Allpass::new(rate, [3.7, 1.3][n] + c as f32 * 0.4))
            }),
            phases: [seed, (seed + 0.31).fract()],
            seed,
        }
    }
    fn clear(&mut self) {
        for ring in &mut self.rings {
            ring.clear();
        }
        for channel in &mut self.diffusers {
            for a in channel {
                a.ring.clear();
            }
        }
        self.low = [0.0; 2];
        self.phases = [self.seed, (self.seed + 0.31).fract()];
    }
    fn tick(
        &mut self,
        mut input: [f32; 2],
        time: f32,
        config: crate::model::DelayConfig,
        rate: f32,
        mono: bool,
    ) -> [f32; 2] {
        self.tap.advance(time);
        if mono && config.ping_pong {
            input[1] = 0.0;
        }
        let tape = config.kind == DelayKind::Tape;
        let offset = if tape {
            (oscillator(&mut self.phases[0], 0.37 / rate) * 0.35
                + oscillator(&mut self.phases[1], 6.1 / rate) * 0.08)
                * rate
                / 1000.0
        } else {
            0.0
        };
        let main: [f32; 2] =
            std::array::from_fn(|c| self.tap.offset_read(&self.rings[c], 1.0, offset));
        let mut out = main;
        if config.kind == DelayKind::MultiTap {
            for (c, sample) in out.iter_mut().enumerate() {
                *sample = main[c] * 0.5
                    + self
                        .tap
                        .offset_read(&self.rings[c], if c == 0 { 0.5 } else { 0.625 }, 0.0)
                        * 0.25
                    + self.tap.offset_read(&self.rings[c], 0.75, 0.0) * 0.25;
            }
        } else if config.kind == DelayKind::Diffused {
            for (c, sample) in out.iter_mut().enumerate() {
                for a in &mut self.diffusers[c] {
                    *sample = a.tick(*sample);
                }
            }
        }
        let damping = if tape {
            config.damping.max(0.35)
        } else {
            config.damping
        };
        for (c, sample) in main.into_iter().enumerate() {
            self.low[c] = clean(self.low[c] * damping + sample * (1.0 - damping));
        }
        for (c, sample) in input.into_iter().enumerate() {
            let value =
                sample + self.low[if config.ping_pong { 1 - c } else { c }] * config.feedback;
            self.rings[c].push(if tape {
                value / (1.0 + 0.12 * value.abs())
            } else {
                value
            });
        }
        out
    }
}
struct Comb {
    ring: Ring,
    length: f32,
    low: f32,
}
impl Comb {
    fn new(rate: f32) -> Self {
        Self {
            ring: Ring::new((rate * 0.12) as usize + 4),
            length: 1.0,
            low: 0.0,
        }
    }
    fn clear(&mut self) {
        self.ring.clear();
        self.low = 0.0;
    }
    fn tick(&mut self, input: f32, feedback: f32, damping: f32) -> f32 {
        let out = self.ring.read(self.length);
        self.low = clean(self.low * damping + out * (1.0 - damping));
        self.ring
            .push(input * (1.0 - feedback) + self.low * feedback);
        out
    }
}
struct Room {
    predelay: [Ring; 2],
    tap: Tap,
    combs: [[Comb; 4]; 2],
    input_diffusers: [[Allpass; 2]; 2],
    diffusers: [[Allpass; 4]; 2],
    kind: ReverbKind,
}
impl Room {
    fn new(rate: f32, predelay: f32, kind: ReverbKind) -> Self {
        let mut room = Self {
            predelay: std::array::from_fn(|_| Ring::new((rate * 0.2) as usize + 4)),
            tap: Tap::new((predelay * rate / 1000.0).max(1.0), rate),
            combs: std::array::from_fn(|_| std::array::from_fn(|_| Comb::new(rate))),
            input_diffusers: std::array::from_fn(|c| {
                std::array::from_fn(|n| Allpass::new(rate, [7.1, 3.3][n] + c as f32 * 0.2))
            }),
            diffusers: std::array::from_fn(|_| std::array::from_fn(|_| Allpass::new(rate, 1.0))),
            kind,
        };
        room.configure(rate, kind);
        room
    }
    fn configure(&mut self, rate: f32, kind: ReverbKind) {
        self.kind = kind;
        let (lengths, diffusion) = match kind {
            ReverbKind::Room => ([29.7, 37.1, 41.1, 43.7], [5.0, 1.7, 1.0, 1.0]),
            ReverbKind::SmallRoom => ([11.3, 13.7, 17.9, 19.3], [3.1, 0.9, 1.0, 1.0]),
            ReverbKind::Chamber => ([31.1, 39.7, 47.3, 53.9], [7.7, 3.1, 1.0, 1.0]),
            ReverbKind::Plate => ([17.3, 23.9, 31.1, 37.7], [9.1, 5.3, 2.7, 1.1]),
            ReverbKind::Hall => ([67.7, 79.3, 97.1, 113.7], [14.7, 9.3, 5.1, 2.3]),
        };
        for c in 0..2 {
            for (n, comb) in self.combs[c].iter_mut().enumerate() {
                comb.length = (rate * (lengths[n] + c as f32 * 1.3) / 1000.0).round();
            }
            for (n, a) in self.diffusers[c].iter_mut().enumerate() {
                a.length = (rate * (diffusion[n] + c as f32 * 0.3) / 1000.0).round();
            }
        }
    }
    fn clear(&mut self) {
        for ring in &mut self.predelay {
            ring.clear();
        }
        for channel in &mut self.combs {
            for comb in channel {
                comb.clear();
            }
        }
        for channel in &mut self.diffusers {
            for a in channel {
                a.ring.clear();
            }
        }
        for channel in &mut self.input_diffusers {
            for a in channel {
                a.ring.clear();
            }
        }
    }
    fn tick(&mut self, input: [f32; 2], predelay: f32, decay: f32, damping: f32) -> [f32; 2] {
        self.tap.advance(predelay.max(1.0));
        let dense = matches!(
            self.kind,
            ReverbKind::Chamber | ReverbKind::Plate | ReverbKind::Hall
        );
        let four = matches!(self.kind, ReverbKind::Plate | ReverbKind::Hall);
        let feedback = match self.kind {
            ReverbKind::Room => 0.45 + decay * 0.5,
            ReverbKind::SmallRoom => 0.32 + decay * 0.5,
            ReverbKind::Chamber => 0.52 + decay * 0.42,
            ReverbKind::Plate => 0.6 + decay * 0.32,
            ReverbKind::Hall => 0.62 + decay * 0.3,
        };
        std::array::from_fn(|c| {
            let mut delayed = self.tap.read(&self.predelay[c]);
            // The original Room and Small room preserve stereo channel isolation.
            self.predelay[c].push(if dense {
                input[c] * 0.85 + input[1 - c] * 0.15
            } else {
                input[c]
            });
            if dense {
                for a in &mut self.input_diffusers[c] {
                    delayed = a.tick(delayed);
                }
            }
            let mut out = 0.0;
            for comb in &mut self.combs[c] {
                out += comb.tick(delayed, feedback, damping) * 0.25;
            }
            for a in &mut self.diffusers[c][..if four { 4 } else { 2 }] {
                out = a.tick(out);
            }
            out * if four { 0.65 } else { 1.0 }
        })
    }
}
struct Chorus {
    rings: [Ring; 2],
    phases: [f32; 3],
    seed: f32,
}
impl Chorus {
    fn new(rate: f32, seed: f32) -> Self {
        let mut chorus = Self {
            rings: std::array::from_fn(|_| Ring::new((rate * 0.04) as usize + 4)),
            phases: [0.0; 3],
            seed,
        };
        chorus.clear();
        chorus
    }
    fn clear(&mut self) {
        for ring in &mut self.rings {
            ring.clear();
        }
        self.phases = std::array::from_fn(|n| (self.seed + n as f32 * 0.27).fract());
    }
    fn tick(&mut self, input: [f32; 2], config: crate::model::ChorusConfig, rate: f32) -> [f32; 2] {
        let voices = if config.ensemble { 3 } else { 1 };
        let mut out = [0.0; 2];
        for voice in 0..voices {
            oscillator(
                &mut self.phases[voice],
                config.rate_hz * (1.0 + voice as f32 * 0.13) / rate,
            );
            for (c, sample) in out.iter_mut().enumerate() {
                let mut phase = (self.phases[voice] + c as f32 * 0.25).fract();
                let modulation = oscillator(&mut phase, 0.0);
                let ms = config.base_ms + config.depth_ms * modulation;
                *sample += self.rings[c].read(ms * rate / 1000.0) / voices as f32;
            }
        }
        for (c, sample) in input.into_iter().enumerate() {
            self.rings[c].push(sample);
        }
        out
    }
}
struct Slot {
    active: EffectConfig,
    smooth: EffectConfig,
    delay: Delay,
    room: Room,
    chorus: Chorus,
    exciter: crate::exciter::Exciter,
    rate: f32,
    transition: f32,
    excitation: f32,
    bypass_samples: u32,
}
impl Slot {
    fn new(rate: f32, config: EffectConfig, tempo: Tempo, seed: f32) -> Self {
        Self {
            active: config,
            smooth: config,
            delay: Delay::new(rate, config.delay.milliseconds(tempo), seed),
            room: Room::new(rate, config.reverb.predelay_ms, config.reverb.kind),
            chorus: Chorus::new(rate, seed),
            exciter: crate::exciter::Exciter::new(rate, config.exciter),
            rate,
            transition: 0.0,
            excitation: 0.0,
            bypass_samples: 0,
        }
    }
    fn clear(&mut self) {
        self.delay.clear();
        self.room.clear();
        self.chorus.clear();
        self.exciter.clear();
        self.bypass_samples = 0;
    }
    fn reset(&mut self) {
        self.clear();
        self.transition = 0.0;
        self.excitation = 0.0;
    }
    fn tick(
        &mut self,
        input: [f32; 2],
        target: EffectConfig,
        tempo: Tempo,
        bypass: bool,
        mono: bool,
    ) -> [f32; 2] {
        let structural = !self.active.same_structure(target);
        let amount = 1.0 / (self.rate * 0.01);
        slew(
            &mut self.transition,
            if structural { 0.0 } else { 1.0 },
            amount,
        );
        if structural && self.transition == 0.0 {
            self.clear();
            self.active = target;
            self.smooth = target;
            self.delay.tap = Tap::new(
                target.delay.milliseconds(tempo) * self.rate / 1000.0,
                self.rate,
            );
            self.room.tap = Tap::new(
                (target.reverb.predelay_ms * self.rate / 1000.0).max(1.0),
                self.rate,
            );
            self.room.configure(self.rate, target.reverb.kind);
        }
        slew(
            &mut self.excitation,
            if bypass || target.bypass { 0.0 } else { 1.0 },
            amount,
        );
        slew(&mut self.smooth.level, target.level, amount);
        if bypass || target.bypass {
            self.bypass_samples = self.bypass_samples.saturating_add(1);
            // Fixed parallel branches have the same maximum drain time as a
            // single delay: final-second fade, then an exact logical clear.
            if self.bypass_samples >= (self.rate * TAIL_SECONDS) as u32 {
                self.clear();
                self.bypass_samples = (self.rate * TAIL_SECONDS) as u32;
                return [0.0; 2];
            }
        } else {
            self.bypass_samples = 0;
        }
        let input = input.map(|x| x * self.excitation);
        let out = match self.active.algorithm {
            Algorithm::Delay => {
                slew(
                    &mut self.smooth.delay.feedback,
                    target.delay.feedback,
                    amount,
                );
                slew(&mut self.smooth.delay.damping, target.delay.damping, amount);
                let config = crate::model::DelayConfig {
                    feedback: self.smooth.delay.feedback,
                    damping: self.smooth.delay.damping,
                    ..self.active.delay
                };
                self.delay.tick(
                    input,
                    target.delay.milliseconds(tempo) * self.rate / 1000.0,
                    config,
                    self.rate,
                    mono,
                )
            }
            Algorithm::Room => {
                slew(&mut self.smooth.reverb.decay, target.reverb.decay, amount);
                slew(
                    &mut self.smooth.reverb.damping,
                    target.reverb.damping,
                    amount,
                );
                self.room.tick(
                    input,
                    target.reverb.predelay_ms * self.rate / 1000.0,
                    self.smooth.reverb.decay,
                    self.smooth.reverb.damping,
                )
            }
            Algorithm::Chorus => {
                slew(
                    &mut self.smooth.chorus.rate_hz,
                    target.chorus.rate_hz,
                    amount * 4.95,
                );
                slew(
                    &mut self.smooth.chorus.depth_ms,
                    target.chorus.depth_ms,
                    amount * 8.0,
                );
                // Bound read-position movement independently of sample rate.
                // 20 ms base changes take >=200 ms (an intentional short glide).
                slew(
                    &mut self.smooth.chorus.base_ms,
                    target.chorus.base_ms,
                    100.0 / self.rate,
                );
                self.smooth.chorus.ensemble = self.active.chorus.ensemble;
                self.chorus.tick(input, self.smooth.chorus, self.rate)
            }
            Algorithm::Exciter => {
                slew(
                    &mut self.smooth.exciter.tune_hz,
                    target.exciter.tune_hz,
                    amount * 5400.0,
                );
                slew(&mut self.smooth.exciter.drive, target.exciter.drive, amount);
                slew(&mut self.smooth.exciter.tone, target.exciter.tone, amount);
                self.smooth.exciter.bright = self.active.exciter.bright;
                self.exciter.tick(input, self.smooth.exciter)
            }
        };
        if out.iter().any(|x| !safe(*x)) {
            return [f32::NAN; 2];
        }
        let tail_gain =
            ((TAIL_SECONDS * self.rate - self.bypass_samples as f32) / self.rate).clamp(0.0, 1.0);
        out.map(|x| clean(x * self.smooth.level * self.transition * tail_gain))
    }
}
struct Engine {
    config: EngineConfig,
    mode: EngineMode,
    pieces: u8,
    slots: [Slot; MAX_STAGES],
    rate: f32,
    transition: f32,
    level: f32,
    fault: bool,
    unavailable: bool,
}
impl Engine {
    fn new(rate: u32, config: EngineConfig) -> Self {
        let rate = rate as f32;
        Self {
            config,
            mode: config.mode,
            pieces: config.pieces,
            slots: std::array::from_fn(|i| {
                Slot::new(
                    rate,
                    config.stages[i],
                    config.tempo,
                    (0.13 + i as f32 * 0.21).fract(),
                )
            }),
            rate,
            transition: 0.0,
            level: config.level,
            fault: false,
            unavailable: false,
        }
    }
    fn clear(&mut self) {
        for slot in &mut self.slots {
            slot.reset();
        }
    }
    fn set(&mut self, config: EngineConfig) {
        if self.config.mute && !config.mute {
            self.fault = false;
            self.clear();
            self.transition = 0.0;
        }
        if config.mute && !self.config.mute {
            self.clear();
        }
        self.config = config;
    }
    fn panic(&mut self) {
        self.clear();
        self.config.mute = true;
        self.fault = false;
        self.transition = 0.0;
    }
    fn tick(&mut self, input: [f32; 2], available: bool, mono: bool) -> [f32; 2] {
        if !available {
            if !self.unavailable {
                self.clear();
                self.transition = 0.0;
            }
            self.unavailable = true;
            return [0.0; 2];
        }
        self.unavailable = false;
        if self.config.mute || self.fault {
            return [0.0; 2];
        }
        if input.iter().any(|x| !safe(*x)) {
            self.fault = true;
            self.clear();
            return [0.0; 2];
        }
        let structural = self.mode != self.config.mode
            || (self.mode == EngineMode::MultiFx && self.pieces != self.config.pieces);
        let amount = 1.0 / (self.rate * 0.01);
        slew(
            &mut self.transition,
            if structural { 0.0 } else { 1.0 },
            amount,
        );
        if structural && self.transition == 0.0 {
            self.clear();
            self.mode = self.config.mode;
            self.pieces = self.config.pieces;
        }
        slew(&mut self.level, self.config.level, amount);
        let count = if self.mode == EngineMode::Single {
            1
        } else {
            self.pieces as usize
        };
        // Reserve 1/max(3, count) per slot; v2 two/three-slot sounds keep gain.
        // Bypass and slot levels never change the gain of another branch.
        let gain = if self.mode == EngineMode::Single {
            1.0
        } else {
            1.0 / count.max(3) as f32
        };
        let mut out = [0.0; 2];
        for i in 0..count {
            let wet = self.slots[i].tick(
                input,
                self.config.stages[i],
                self.config.tempo,
                self.config.bypass,
                mono,
            );
            if wet.iter().any(|x| !safe(*x)) {
                self.fault = true;
                self.clear();
                return [0.0; 2];
            }
            for c in 0..2 {
                out[c] += wet[c] * gain;
            }
        }
        out.map(|x| clean(x * self.level * self.transition))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Meters {
    pub input: [f32; 2],
    pub output: [f32; 2],
    pub faults: u8,
    pub missing: u8,
    pub buffer_fault: bool,
}

pub struct Processor {
    engines: [Engine; 2],
    routing: Routing,
    target: Rack,
    route_gain: f32,
    rate: u32,
    scratch: [Vec<[f32; 2]>; 2],
}
impl Processor {
    pub fn new(rate: u32, rack: Rack) -> Result<Self, String> {
        if !(8000..=MAX_RATE).contains(&rate) {
            return Err("Sample rate must be 8000..192000".into());
        }
        rack.validate(Availability::ALL)?;
        Ok(Self {
            engines: std::array::from_fn(|e| Engine::new(rate, rack.engines[e])),
            routing: rack.routing,
            target: rack,
            route_gain: 1.0,
            rate,
            scratch: std::array::from_fn(|_| vec![[0.0; 2]; MAX_FRAMES]),
        })
    }
    /// Only validated scalar configurations enter the callback queue.
    pub fn apply(&mut self, rack: Rack) {
        self.target = rack;
        for e in 0..2 {
            self.engines[e].set(rack.engines[e]);
        }
    }
    pub fn panic(&mut self, mask: u8) {
        for e in 0..2 {
            if mask & (1 << e) != 0 {
                self.engines[e].panic();
            }
        }
    }
    pub fn process(
        &mut self,
        inputs: [&[f32]; 4],
        mut outputs: [&mut [f32]; 4],
        frames: usize,
        available: Availability,
    ) -> Meters {
        let mut meters = Meters::default();
        for out in &mut outputs {
            out.fill(0.0);
        }
        if frames > MAX_FRAMES {
            for engine in &mut self.engines {
                engine.clear();
            }
            meters.buffer_fault = true;
            return meters;
        }
        let mut available = available;
        for c in 0..4 {
            if inputs[c].len() < frames {
                available.inputs &= !(1 << c);
                meters.buffer_fault = true;
            }
            if outputs[c].len() < frames {
                available.outputs &= !(1 << c);
                meters.buffer_fault = true;
            }
        }
        // Defer routing publication to a later block once the fade reaches
        // silence, so every sample in one block has the same output ownership.
        if self.routing != self.target.routing && self.route_gain == 0.0 {
            for e in 0..2 {
                if self.routing.inputs[e] != self.target.routing.inputs[e]
                    || self.routing.output_mask(e) != self.target.routing.output_mask(e)
                    || self.routing.layout != self.target.routing.layout
                {
                    self.engines[e].clear();
                }
            }
            self.routing = self.target.routing;
        }
        let mut engine_ok = [false; 2];
        for (e, ok) in engine_ok.iter_mut().enumerate() {
            *ok = self.routing.engine_available(e, available);
            if !*ok && self.routing.layout.widths()[e] > 0 {
                meters.missing |= 1 << e;
            }
        }
        for n in 0..frames {
            let input = std::array::from_fn(|c| inputs[c].get(n).copied().unwrap_or(0.0));
            slew(
                &mut self.route_gain,
                if self.routing == self.target.routing {
                    1.0
                } else {
                    0.0
                },
                1.0 / (self.rate as f32 * 0.01),
            );
            for (e, ok) in engine_ok.into_iter().enumerate() {
                let selected = self.routing.inputs[e].read(input);
                for x in selected {
                    if x.is_finite() {
                        meters.input[e] = meters.input[e].max(x.abs());
                    }
                }
                self.scratch[e][n] = self.engines[e]
                    .tick(
                        selected,
                        ok,
                        !matches!(self.routing.inputs[e], crate::model::Source::Stereo(_)),
                    )
                    .map(|x| x * self.route_gain);
            }
        }
        for e in 0..2 {
            if self.engines[e].fault {
                meters.faults |= 1 << e;
            }
        }
        // A late engine fault removes that engine's entire period, including
        // its contribution to shared stereo; the other engine is preserved.
        for n in 0..frames {
            let wet = std::array::from_fn(|e| {
                if meters.faults & (1 << e) != 0 {
                    [0.0; 2]
                } else {
                    let wet = self.scratch[e][n];
                    let gain = if self.routing.layout == crate::model::Layout::SharedStereo {
                        0.25
                    } else {
                        0.5
                    };
                    let peak = if self.routing.layout.widths()[e] == 1 {
                        ((wet[0] + wet[1]) * 0.5).abs()
                    } else {
                        wet[0].abs().max(wet[1].abs())
                    };
                    meters.output[e] = meters.output[e].max(peak * gain);
                    wet
                }
            });
            let out = self.routing.place(wet);
            for c in 0..4 {
                if let Some(sample) = outputs[c].get_mut(n) {
                    *sample = out[c];
                }
            }
        }
        meters
    }
}
