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
/// Compile-time sample precision for the shared delay primitives. The rack keeps
/// its f32 specialization; the integration adapter uses f64 storage/arithmetic.
trait DelaySample:
    Copy
    + PartialOrd
    + std::ops::Add<Output = Self>
    + std::ops::Sub<Output = Self>
    + std::ops::Mul<Output = Self>
    + std::ops::Div<Output = Self>
    + std::ops::AddAssign
    + std::ops::SubAssign
{
    const ZERO: Self;
    const ONE: Self;
    fn from_f64(value: f64) -> Self;
    fn as_usize(self) -> usize;
    fn abs(self) -> Self;
    fn round(self) -> Self;
    fn fract(self) -> Self;
    fn min(self, other: Self) -> Self;
    fn max(self, other: Self) -> Self;
    fn clamp(self, min: Self, max: Self) -> Self;
}
macro_rules! delay_sample {
    ($sample:ty) => {
        impl DelaySample for $sample {
            const ZERO: Self = 0.0;
            const ONE: Self = 1.0;
            fn from_f64(value: f64) -> Self {
                value as Self
            }
            fn as_usize(self) -> usize {
                self as usize
            }
            fn abs(self) -> Self {
                <$sample>::abs(self)
            }
            fn round(self) -> Self {
                <$sample>::round(self)
            }
            fn fract(self) -> Self {
                <$sample>::fract(self)
            }
            fn min(self, other: Self) -> Self {
                <$sample>::min(self, other)
            }
            fn max(self, other: Self) -> Self {
                <$sample>::max(self, other)
            }
            fn clamp(self, min: Self, max: Self) -> Self {
                <$sample>::clamp(self, min, max)
            }
        }
    };
}
delay_sample!(f32);
delay_sample!(f64);

fn clean<S: DelaySample>(x: S) -> S {
    if x.abs() < S::from_f64(1e-20) {
        S::ZERO
    } else {
        x
    }
}
fn slew(x: &mut f32, target: f32, amount: f32) {
    *x += (target - *x).clamp(-amount, amount);
}
fn safe(x: f32) -> bool {
    x.is_finite() && x.abs() <= FAULT_LIMIT
}

struct Ring<S: DelaySample = f32> {
    data: Vec<S>,
    pos: usize,
    valid: usize,
}
impl<S: DelaySample> Ring<S> {
    fn new(len: usize) -> Self {
        Self {
            data: vec![S::ZERO; len.max(4)],
            pos: 0,
            valid: 0,
        }
    }
    fn clear(&mut self) {
        self.pos = 0;
        self.valid = 0;
    }
    fn push(&mut self, value: S) {
        self.data[self.pos] = clean(value);
        self.pos = (self.pos + 1) % self.data.len();
        self.valid = (self.valid + 1).min(self.data.len());
    }
    fn at(&self, delay: usize) -> S {
        if delay == 0 || delay > self.valid {
            S::ZERO
        } else {
            self.data[(self.pos + self.data.len() - delay) % self.data.len()]
        }
    }
    fn read(&self, delay: S) -> S {
        let delay = delay.clamp(S::ONE, S::from_f64((self.data.len() - 2) as f64));
        let n = delay.as_usize();
        let a = self.at(n);
        a + (self.at(n + 1) - a) * (delay - S::from_f64(n as f64))
    }
}
/// Crossfade both read positions for 20 ms; rapid requests coalesce.
struct Tap<S: DelaySample = f32> {
    from: S,
    to: S,
    phase: S,
    step: S,
}
impl<S: DelaySample> Tap<S> {
    fn new(delay: S, rate: S) -> Self {
        Self {
            from: delay,
            to: delay,
            phase: S::ONE,
            step: S::ONE / (rate * S::from_f64(0.02)),
        }
    }
    fn advance(&mut self, desired: S) {
        if self.phase >= S::ONE && (desired - self.to).abs() > S::from_f64(0.1) {
            self.from = self.to;
            self.to = desired;
            self.phase = S::ZERO;
        }
        self.phase = (self.phase + self.step).min(S::ONE);
    }
    fn read(&self, ring: &Ring<S>) -> S {
        self.offset_read(ring, S::ONE, S::ZERO)
    }
    fn offset_read(&self, ring: &Ring<S>, fraction: S, offset: S) -> S {
        ring.read(self.from * fraction + offset) * (S::ONE - self.phase)
            + ring.read(self.to * fraction + offset) * self.phase
    }
}
/// Smooth parabolic oscillator: deterministic phases, no random source or
/// per-sample transcendental work. Every call advances by less than one cycle.
fn oscillator<S: DelaySample>(phase: &mut S, step: S) -> S {
    *phase += step;
    if *phase >= S::ONE {
        *phase -= S::ONE;
    }
    let x = *phase * S::from_f64(2.0) - S::ONE;
    S::from_f64(4.0) * x * (S::ONE - x.abs())
}
struct Allpass<S: DelaySample = f32> {
    ring: Ring<S>,
    length: S,
}
impl<S: DelaySample> Allpass<S> {
    fn new(rate: S, ms: S) -> Self {
        Self {
            ring: Ring::new((rate * S::from_f64(0.016)).as_usize() + 4),
            length: (rate * ms / S::from_f64(1000.0)).round(),
        }
    }
    fn tick(&mut self, input: S) -> S {
        let delayed = self.ring.read(self.length);
        let out = delayed - input * S::from_f64(0.5);
        self.ring.push(input + out * S::from_f64(0.5));
        clean(out)
    }
}
struct DelayParameters<S: DelaySample> {
    kind: DelayKind,
    feedback: S,
    damping: S,
    ping_pong: bool,
}
impl From<crate::model::DelayConfig> for DelayParameters<f32> {
    fn from(config: crate::model::DelayConfig) -> Self {
        Self {
            kind: config.kind,
            feedback: config.feedback,
            damping: config.damping,
            ping_pong: config.ping_pong,
        }
    }
}
struct Delay<S: DelaySample = f32> {
    rings: [Ring<S>; 2],
    low: [S; 2],
    tap: Tap<S>,
    diffusers: [[Allpass<S>; 2]; 2],
    phases: [S; 2],
    seed: S,
}
impl<S: DelaySample> Delay<S> {
    fn new(rate: S, ms: S, seed: S) -> Self {
        Self {
            rings: std::array::from_fn(|_| Ring::new((rate * S::from_f64(2.0)).as_usize() + 4)),
            low: [S::ZERO; 2],
            tap: Tap::new(ms * rate / S::from_f64(1000.0), rate),
            diffusers: std::array::from_fn(|c| {
                std::array::from_fn(|n| {
                    Allpass::new(
                        rate,
                        S::from_f64([3.7, 1.3][n]) + S::from_f64(c as f64) * S::from_f64(0.4),
                    )
                })
            }),
            phases: [seed, (seed + S::from_f64(0.31)).fract()],
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
        self.low = [S::ZERO; 2];
        self.phases = [self.seed, (self.seed + S::from_f64(0.31)).fract()];
    }
    fn tick(
        &mut self,
        mut input: [S; 2],
        time: S,
        config: DelayParameters<S>,
        rate: S,
        mono: bool,
    ) -> [S; 2] {
        self.tap.advance(time);
        if mono && config.ping_pong {
            input[1] = S::ZERO;
        }
        let tape = config.kind == DelayKind::Tape;
        let offset = if tape {
            (oscillator(&mut self.phases[0], S::from_f64(0.37) / rate) * S::from_f64(0.35)
                + oscillator(&mut self.phases[1], S::from_f64(6.1) / rate) * S::from_f64(0.08))
                * rate
                / S::from_f64(1000.0)
        } else {
            S::ZERO
        };
        let main: [S; 2] =
            std::array::from_fn(|c| self.tap.offset_read(&self.rings[c], S::ONE, offset));
        let mut out = main;
        if config.kind == DelayKind::MultiTap {
            for (c, sample) in out.iter_mut().enumerate() {
                *sample = main[c] * S::from_f64(0.5)
                    + self.tap.offset_read(
                        &self.rings[c],
                        S::from_f64(if c == 0 { 0.5 } else { 0.625 }),
                        S::ZERO,
                    ) * S::from_f64(0.25)
                    + self
                        .tap
                        .offset_read(&self.rings[c], S::from_f64(0.75), S::ZERO)
                        * S::from_f64(0.25);
            }
        } else if config.kind == DelayKind::Diffused {
            for (c, sample) in out.iter_mut().enumerate() {
                for a in &mut self.diffusers[c] {
                    *sample = a.tick(*sample);
                }
            }
        }
        let damping = if tape {
            config.damping.max(S::from_f64(0.35))
        } else {
            config.damping
        };
        for (c, sample) in main.into_iter().enumerate() {
            self.low[c] = clean(self.low[c] * damping + sample * (S::ONE - damping));
        }
        for (c, sample) in input.into_iter().enumerate() {
            let value =
                sample + self.low[if config.ping_pong { 1 - c } else { c }] * config.feedback;
            self.rings[c].push(if tape {
                value / (S::ONE + S::from_f64(0.12) * value.abs())
            } else {
                value
            });
        }
        out
    }
}
/// Fixed integration preset using the rack's existing digital-delay algorithm.
/// The first tap is exactly the rounded 20 ms frame count, including at rates
/// where 20 ms is not an integral number of frames. No additional block delay.
pub(crate) struct IntegrationDelay {
    delay: Delay<f64>,
    rate: f64,
    frames: u32,
}
impl IntegrationDelay {
    pub(crate) fn new(rate: u32) -> Self {
        let frames = (rate + 25) / 50;
        let mut delay = Delay::new(f64::from(rate), 20.0, 0.13);
        delay.tap = Tap::new(f64::from(frames), f64::from(rate));
        Self {
            delay,
            rate: f64::from(rate),
            frames,
        }
    }
    /// Raw owned allocation spans for ABI output-overlap preflight only.
    /// Does not expose samples or change the delay algorithm.
    pub(crate) fn owned_spans(&self) -> [(usize, usize); 6] {
        let span = |ring: &Ring<f64>| {
            let start = ring.data.as_ptr() as usize;
            (start, start + ring.data.capacity() * size_of::<f64>())
        };
        [
            span(&self.delay.rings[0]),
            span(&self.delay.rings[1]),
            span(&self.delay.diffusers[0][0].ring),
            span(&self.delay.diffusers[0][1].ring),
            span(&self.delay.diffusers[1][0].ring),
            span(&self.delay.diffusers[1][1].ring),
        ]
    }
    pub(crate) fn frames(&self) -> u32 {
        self.frames
    }
    pub(crate) fn clear(&mut self) {
        self.delay.clear();
    }
    pub(crate) fn tick(&mut self, input: [f64; 2]) -> [f64; 2] {
        self.delay
            .tick(
                input,
                f64::from(self.frames),
                DelayParameters {
                    kind: DelayKind::Digital,
                    feedback: 0.25,
                    damping: 0.35,
                    ping_pong: false,
                },
                self.rate,
                false,
            )
            .map(|sample| clean(sample * 0.5))
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
                    config.into(),
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

#[cfg(test)]
mod precision_tests {
    use super::*;

    #[test]
    fn shared_f32_delay_retains_prior_sample_bits() {
        // Captured from the original 88a28ac release ABI, whose internal delay
        // was f32. Protect the rack specialization through shared-core changes.
        let expected = [
            (960, [0x3e800000, 0xbe000000]),
            (1920, [0x3d266666, 0xbca66666]),
            (1921, [0x3c68f5c2, 0xbbe8f5c2]),
            (1922, [0x3ba3126e, 0xbb23126e]),
            (1923, [0x3ae44d00, 0xba644d00]),
            (2880, [0x3bd851ea, 0xbb5851ea]),
            (2881, [0x3b976c8a, 0xbb176c8a]),
            (2882, [0x3b1efec4, 0xba9efec4]),
            (3840, [0x3a8c9ba5, 0xba0c9ba5]),
        ];
        let mut delay: Delay<f32> = Delay::new(48000.0, 20.0, 0.13);
        let config = crate::model::DelayConfig {
            feedback: 0.25,
            damping: 0.35,
            ..Default::default()
        };
        let mut check = 0;
        for frame in 0..=3840 {
            let input = if frame == 0 { [0.5, -0.25] } else { [0.0; 2] };
            let wet = delay.tick(input, 960.0, config.into(), 48000.0, false);
            if frame == expected[check].0 {
                assert_eq!(wet.map(|x| (x * 0.5).to_bits()), expected[check].1);
                check += 1;
                if check == expected.len() {
                    break;
                }
            }
        }
        assert_eq!(check, expected.len());
    }
}
