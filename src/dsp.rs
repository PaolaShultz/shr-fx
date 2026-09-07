//! All storage is prepared before activation. Callback paths use scalar state,
//! fixed loops and constant-time logical clears; no heap retirement is needed.
use crate::model::{Algorithm, Availability, EngineConfig, Rack, Routing};

pub const MAX_FRAMES: usize = 8192;
pub const MAX_RATE: u32 = 192_000;
const FAULT_LIMIT: f32 = 16.0;
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

/// A change crossfades read taps for 20 ms. During a fade, new requests are
/// coalesced and start the next fade; a buffer position never jumps audibly.
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
        ring.read(self.from) * (1.0 - self.phase) + ring.read(self.to) * self.phase
    }
}
struct Delay {
    rings: [Ring; 2],
    low: [f32; 2],
    tap: Tap,
}
impl Delay {
    fn new(rate: f32, ms: f32) -> Self {
        Self {
            rings: std::array::from_fn(|_| Ring::new((rate * 2.0) as usize + 4)),
            low: [0.0; 2],
            tap: Tap::new(ms * rate / 1000.0, rate),
        }
    }
    fn clear(&mut self) {
        for ring in &mut self.rings {
            ring.clear();
        }
        self.low = [0.0; 2];
    }
    fn tick(
        &mut self,
        input: [f32; 2],
        time: f32,
        feedback: f32,
        damping: f32,
        ping: bool,
    ) -> [f32; 2] {
        self.tap.advance(time);
        let out = std::array::from_fn(|c| self.tap.read(&self.rings[c]));
        for (c, sample) in out.iter().enumerate() {
            self.low[c] = clean(self.low[c] * damping + sample * (1.0 - damping));
        }
        for (c, sample) in input.into_iter().enumerate() {
            self.rings[c].push(sample + self.low[if ping { 1 - c } else { c }] * feedback);
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
    fn new(rate: f32, ms: f32) -> Self {
        let length = (rate * ms / 1000.0).round();
        Self {
            ring: Ring::new(length as usize + 4),
            length,
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
struct Allpass {
    ring: Ring,
    length: f32,
}
impl Allpass {
    fn new(rate: f32, ms: f32) -> Self {
        let length = (rate * ms / 1000.0).round();
        Self {
            ring: Ring::new(length as usize + 4),
            length,
        }
    }
    fn tick(&mut self, input: f32) -> f32 {
        let delayed = self.ring.read(self.length);
        let out = delayed - input * 0.5;
        self.ring.push(input + out * 0.5);
        clean(out)
    }
}
struct Room {
    predelay: [Ring; 2],
    tap: Tap,
    combs: [[Comb; 4]; 2],
    diffusers: [[Allpass; 2]; 2],
}
impl Room {
    fn new(rate: f32, predelay: f32) -> Self {
        Self {
            predelay: std::array::from_fn(|_| Ring::new((rate * 0.2) as usize + 4)),
            tap: Tap::new((predelay * rate / 1000.0).max(1.0), rate),
            combs: std::array::from_fn(|c| {
                std::array::from_fn(|n| {
                    Comb::new(rate, [29.7, 37.1, 41.1, 43.7][n] + c as f32 * 1.3)
                })
            }),
            diffusers: std::array::from_fn(|c| {
                std::array::from_fn(|n| Allpass::new(rate, [5.0, 1.7][n] + c as f32 * 0.3))
            }),
        }
    }
    fn clear(&mut self) {
        for p in &mut self.predelay {
            p.clear();
        }
        for channel in &mut self.combs {
            for c in channel {
                c.clear();
            }
        }
        for channel in &mut self.diffusers {
            for a in channel {
                a.ring.clear();
            }
        }
    }
    fn tick(&mut self, input: [f32; 2], predelay: f32, feedback: f32, damping: f32) -> [f32; 2] {
        // One sample minimum, reported separately from intentional predelay.
        self.tap.advance(predelay.max(1.0));
        std::array::from_fn(|c| {
            let delayed = self.tap.read(&self.predelay[c]);
            self.predelay[c].push(input[c]);
            let mut out = 0.0;
            for comb in &mut self.combs[c] {
                out += comb.tick(delayed, 0.45 + feedback * 0.5, damping) * 0.25;
            }
            for allpass in &mut self.diffusers[c] {
                out = allpass.tick(out);
            }
            out
        })
    }
}
struct Engine {
    config: EngineConfig,
    algorithm: Algorithm,
    ping: bool,
    delay: Delay,
    room: Room,
    rate: f32,
    transition: f32,
    excitation: f32,
    level: f32,
    feedback: f32,
    damping: f32,
    bypass_samples: u32,
    fault: bool,
    unavailable: bool,
}
impl Engine {
    fn new(rate: u32, config: EngineConfig) -> Self {
        let rate = rate as f32;
        Self {
            config,
            algorithm: config.algorithm,
            ping: config.ping_pong,
            delay: Delay::new(rate, config.delay_ms()),
            room: Room::new(rate, config.predelay_ms),
            rate,
            transition: 0.0,
            excitation: 0.0,
            level: config.level,
            feedback: config.feedback,
            damping: config.damping,
            bypass_samples: 0,
            fault: false,
            unavailable: false,
        }
    }
    fn clear(&mut self) {
        self.delay.clear();
        self.room.clear();
        self.bypass_samples = 0;
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
        self.excitation = 0.0;
        self.transition = 0.0;
    }
    fn tick(&mut self, input: [f32; 2], available: bool, mono: bool) -> [f32; 2] {
        if !available {
            if !self.unavailable {
                self.clear();
                self.transition = 0.0;
                self.excitation = 0.0;
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
        let structural =
            self.algorithm != self.config.algorithm || self.ping != self.config.ping_pong;
        let amount = 1.0 / (self.rate * 0.01);
        slew(
            &mut self.transition,
            if structural { 0.0 } else { 1.0 },
            amount,
        );
        if structural && self.transition == 0.0 {
            self.clear();
            self.algorithm = self.config.algorithm;
            self.ping = self.config.ping_pong;
        }
        slew(
            &mut self.excitation,
            if self.config.bypass { 0.0 } else { 1.0 },
            amount,
        );
        slew(&mut self.level, self.config.level, amount);
        slew(&mut self.feedback, self.config.feedback, amount);
        slew(&mut self.damping, self.config.damping, amount);
        if self.config.bypass {
            self.bypass_samples = self.bypass_samples.saturating_add(1);
            // At 0.9 feedback and a 2 s delay, a 16-peak input falls below
            // -100 dB after 280 s. Fade the final second, then clear to zero.
            if self.bypass_samples >= (self.rate * 281.0) as u32 {
                self.clear();
                self.bypass_samples = (self.rate * 281.0) as u32;
                return [0.0; 2];
            }
        } else {
            self.bypass_samples = 0;
        }
        let mut input = input.map(|x| x * self.excitation);
        // A mono send starts ping-pong on L; feedback crosses to R.
        // Explicit stereo sources retain their independent excitation.
        if mono && self.algorithm == Algorithm::Delay && self.ping {
            input[1] = 0.0;
        }
        let out = match self.algorithm {
            Algorithm::Delay => self.delay.tick(
                input,
                self.config.delay_ms() * self.rate / 1000.0,
                self.feedback,
                self.damping,
                self.ping,
            ),
            Algorithm::Room => self.room.tick(
                input,
                self.config.predelay_ms * self.rate / 1000.0,
                self.feedback,
                self.damping,
            ),
        };
        if out.iter().any(|x| !safe(*x)) {
            self.fault = true;
            self.clear();
            return [0.0; 2];
        }
        let tail_gain =
            ((281.0 * self.rate - self.bypass_samples as f32) / self.rate).clamp(0.0, 1.0);
        out.map(|x| clean(x * self.level * self.transition * tail_gain))
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
