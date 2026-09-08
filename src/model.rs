use serde::{Deserialize, Serialize};

pub const PORTS: usize = 4;
pub const VERSION: u32 = 3;
pub const LOCAL_VERSION: u32 = 2;
pub const MAX_STAGES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    Mono(u8),
    Sum([u8; 2]),
    Stereo([u8; 2]),
}
impl Source {
    pub fn mask(self) -> u8 {
        match self {
            Self::Mono(a) => bit(a),
            Self::Sum([a, b]) | Self::Stereo([a, b]) => bit(a) | bit(b),
        }
    }
    pub fn validate(self) -> Result<(), String> {
        match self {
            Self::Mono(a) if a < 4 => Ok(()),
            Self::Sum([a, b]) | Self::Stereo([a, b]) if a < 4 && b < 4 && a != b => Ok(()),
            _ => Err("Input requires distinct slots 1-4".into()),
        }
    }
    pub fn read(self, input: [f32; 4]) -> [f32; 2] {
        match self {
            Self::Mono(a) => [input[a as usize]; 2],
            Self::Sum([a, b]) => [(input[a as usize] + input[b as usize]) * 0.5; 2],
            Self::Stereo([a, b]) => [input[a as usize], input[b as usize]],
        }
    }
    pub fn label(self) -> String {
        match self {
            Self::Mono(a) => format!("Mono {}", a + 1),
            Self::Sum([a, b]) => format!("Sum {}+{}", a + 1, b + 1),
            Self::Stereo([a, b]) => format!("Stereo {}/{}", a + 1, b + 1),
        }
    }
    pub fn choices() -> Vec<Self> {
        let mut choices = (0..4).map(Self::Mono).collect::<Vec<_>>();
        for a in 0..4 {
            for b in a + 1..4 {
                choices.push(Self::Sum([a, b]));
            }
        }
        for a in 0..4 {
            for b in 0..4 {
                if a != b {
                    choices.push(Self::Stereo([a, b]));
                }
            }
        }
        choices
    }
}
pub const fn bit(slot: u8) -> u8 {
    if slot < 4 { 1 << slot } else { 0 }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    DualMono,
    SharedStereo,
    DualStereo,
    MonoStereo,
    StereoMono,
    AMono,
    AStereo,
    BMono,
    BStereo,
}
impl Layout {
    pub const ALL: [Self; 9] = [
        Self::DualMono,
        Self::SharedStereo,
        Self::DualStereo,
        Self::MonoStereo,
        Self::StereoMono,
        Self::AMono,
        Self::AStereo,
        Self::BMono,
        Self::BStereo,
    ];
    pub fn widths(self) -> [usize; 2] {
        match self {
            Self::DualMono => [1, 1],
            Self::SharedStereo | Self::DualStereo => [2, 2],
            Self::MonoStereo => [1, 2],
            Self::StereoMono => [2, 1],
            Self::AMono => [1, 0],
            Self::AStereo => [2, 0],
            Self::BMono => [0, 1],
            Self::BStereo => [0, 2],
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::DualMono => "Dual mono",
            Self::SharedStereo => "Shared stereo",
            Self::DualStereo => "Dual stereo",
            Self::MonoStereo => "Mono + stereo",
            Self::StereoMono => "Stereo + mono",
            Self::AMono => "A mono / B off",
            Self::AStereo => "A stereo / B off",
            Self::BMono => "A off / B mono",
            Self::BStereo => "A off / B stereo",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Routing {
    pub inputs: [Source; 2],
    pub layout: Layout,
    pub outputs: [[u8; 2]; 2],
}
impl Default for Routing {
    fn default() -> Self {
        Self {
            inputs: [Source::Mono(0), Source::Mono(1)],
            layout: Layout::DualMono,
            outputs: [[0, 1], [1, 2]],
        }
    }
}
impl Routing {
    pub fn output_mask(&self, engine: usize) -> u8 {
        let width = self.layout.widths()[engine];
        let ports = if self.layout == Layout::SharedStereo {
            self.outputs[0]
        } else {
            self.outputs[engine]
        };
        (0..width).fold(0, |mask, n| mask | bit(ports[n]))
    }
    pub fn engine_available(&self, engine: usize, available: Availability) -> bool {
        self.layout.widths()[engine] > 0
            && self.inputs[engine].mask() & !available.inputs == 0
            && self.output_mask(engine) & !available.outputs == 0
    }
    pub fn validate(&self, available: Availability) -> Result<(), String> {
        for src in self.inputs {
            src.validate()?;
        }
        if self.outputs.iter().flatten().any(|p| *p >= 4) {
            return Err("Output slots must be 1-4".into());
        }
        let widths = self.layout.widths();
        let mut used = 0;
        for (e, width) in widths.into_iter().enumerate() {
            if width == 0 {
                continue;
            }
            let pair = if self.layout == Layout::SharedStereo {
                self.outputs[0]
            } else {
                self.outputs[e]
            };
            if width == 2 && pair[0] == pair[1] {
                return Err("Stereo L/R must be distinct".into());
            }
            let mask = self.output_mask(e);
            if self.layout != Layout::SharedStereo && used & mask != 0 {
                return Err("Duplicate returns: use Shared stereo".into());
            }
            used |= mask;
            if self.inputs[e].mask() & !available.inputs != 0 {
                return Err(format!("{} input missing; draft kept", engine_name(e)));
            }
            if mask & !available.outputs != 0 {
                return Err(format!("{} output missing; draft kept", engine_name(e)));
            }
        }
        Ok(())
    }
    /// Linear mean for mono. Each independent return has 6 dB headroom;
    /// shared stereo has an additional 6 dB per contribution.
    pub fn place(&self, wet: [[f32; 2]; 2]) -> [f32; 4] {
        let mut out = [0.0; 4];
        for (e, width) in self.layout.widths().into_iter().enumerate() {
            let ports = if self.layout == Layout::SharedStereo {
                self.outputs[0]
            } else {
                self.outputs[e]
            };
            let gain = if self.layout == Layout::SharedStereo {
                0.25
            } else {
                0.5
            };
            if width == 1 {
                out[ports[0] as usize] += (wet[e][0] + wet[e][1]) * 0.5 * gain;
            }
            if width == 2 {
                for c in 0..2 {
                    out[ports[c] as usize] += wet[e][c] * gain;
                }
            }
        }
        out
    }
    pub fn return_label(&self, e: usize) -> String {
        let p = if self.layout == Layout::SharedStereo {
            self.outputs[0]
        } else {
            self.outputs[e]
        };
        match self.layout.widths()[e] {
            0 => "Off".into(),
            1 => format!("Mono {}", p[0] + 1),
            _ => format!("L{} R{}", p[0] + 1, p[1] + 1),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Availability {
    pub inputs: u8,
    pub outputs: u8,
}
impl Availability {
    pub const ALL: Self = Self {
        inputs: 15,
        outputs: 15,
    };
}
pub fn engine_name(e: usize) -> &'static str {
    if e == 0 { "A" } else { "B" }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Algorithm {
    #[default]
    Delay,
    Room,
    Chorus,
    Exciter,
}
impl Algorithm {
    pub const ALL: [Self; 4] = [Self::Delay, Self::Room, Self::Chorus, Self::Exciter];
    pub fn label(self) -> &'static str {
        match self {
            Self::Delay => "Delay",
            Self::Room => "Reverb",
            Self::Chorus => "Chorus",
            Self::Exciter => "Exciter",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DelayKind {
    Digital,
    Tape,
    MultiTap,
    Diffused,
}
impl DelayKind {
    pub const ALL: [Self; 4] = [Self::Digital, Self::Tape, Self::MultiTap, Self::Diffused];
    pub fn label(self) -> &'static str {
        match self {
            Self::Digital => "Digital",
            Self::Tape => "Tape echo",
            Self::MultiTap => "Multi-tap",
            Self::Diffused => "Diffused",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReverbKind {
    Room,
    SmallRoom,
    Chamber,
    Plate,
    Hall,
}
impl ReverbKind {
    pub const ALL: [Self; 5] = [
        Self::Room,
        Self::SmallRoom,
        Self::Chamber,
        Self::Plate,
        Self::Hall,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Room => "Room",
            Self::SmallRoom => "Small room",
            Self::Chamber => "Chamber",
            Self::Plate => "Plate",
            Self::Hall => "Hall",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineMode {
    Single,
    MultiFx,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TempoSource {
    Internal,
    MidiClock,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tempo {
    pub bpm: f32,
    pub source: TempoSource,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DelayConfig {
    pub kind: DelayKind,
    pub time_ms: f32,
    pub feedback: f32,
    pub damping: f32,
    pub sync: bool,
    pub division: u8,
    pub ping_pong: bool,
}
impl Default for DelayConfig {
    fn default() -> Self {
        Self {
            kind: DelayKind::Digital,
            time_ms: 375.0,
            feedback: 0.45,
            damping: 0.5,
            sync: false,
            division: 2,
            ping_pong: false,
        }
    }
}
impl DelayConfig {
    pub const DIVISIONS: [f32; 5] = [0.25, 0.5, 1.0, 1.5, 2.0];
    pub const DIVISION_LABELS: [&'static str; 5] = ["1/16", "1/8", "1/4", "1/4 dot", "1/2"];
    pub fn milliseconds(self, tempo: Tempo) -> f32 {
        if self.sync {
            (60_000.0 / tempo.bpm * Self::DIVISIONS[self.division as usize]).clamp(1.0, 2000.0)
        } else {
            self.time_ms
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReverbConfig {
    pub kind: ReverbKind,
    pub predelay_ms: f32,
    pub decay: f32,
    pub damping: f32,
}
impl Default for ReverbConfig {
    fn default() -> Self {
        Self {
            kind: ReverbKind::Room,
            predelay_ms: 0.0,
            decay: 0.45,
            damping: 0.5,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChorusConfig {
    pub rate_hz: f32,
    pub depth_ms: f32,
    pub base_ms: f32,
    pub ensemble: bool,
}
impl Default for ChorusConfig {
    fn default() -> Self {
        Self {
            rate_hz: 0.7,
            depth_ms: 3.0,
            base_ms: 15.0,
            ensemble: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExciterConfig {
    pub tune_hz: f32,
    pub drive: f32,
    pub tone: f32,
    pub bright: bool,
}
impl Default for ExciterConfig {
    fn default() -> Self {
        Self {
            tune_hz: 2500.0,
            drive: 0.3,
            tone: 0.6,
            bright: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectConfig {
    pub level: f32,
    pub bypass: bool,
    pub algorithm: Algorithm,
    pub delay: DelayConfig,
    pub reverb: ReverbConfig,
    pub chorus: ChorusConfig,
    pub exciter: ExciterConfig,
}
impl Default for EffectConfig {
    fn default() -> Self {
        Self {
            level: 1.0,
            bypass: false,
            algorithm: Algorithm::Delay,
            delay: DelayConfig::default(),
            reverb: ReverbConfig::default(),
            chorus: ChorusConfig::default(),
            exciter: ExciterConfig::default(),
        }
    }
}
impl EffectConfig {
    pub fn label(self) -> &'static str {
        match self.algorithm {
            Algorithm::Delay => self.delay.kind.label(),
            Algorithm::Room => self.reverb.kind.label(),
            Algorithm::Chorus => {
                if self.chorus.ensemble {
                    "Ensemble"
                } else {
                    "Chorus"
                }
            }
            Algorithm::Exciter => {
                if self.exciter.bright {
                    "Bright exciter"
                } else {
                    "Warm exciter"
                }
            }
        }
    }
    pub fn validate(self) -> Result<(), String> {
        for (label, value, min, max) in [
            ("Slot level", self.level, 0.0, 1.0),
            ("Time", self.delay.time_ms, 1.0, 2000.0),
            ("Feedback", self.delay.feedback, 0.0, 0.9),
            ("Delay damping", self.delay.damping, 0.0, 0.95),
            ("Predelay", self.reverb.predelay_ms, 0.0, 200.0),
            ("Decay", self.reverb.decay, 0.0, 0.9),
            ("Reverb damping", self.reverb.damping, 0.0, 0.95),
            ("Chorus rate", self.chorus.rate_hz, 0.05, 5.0),
            ("Chorus depth", self.chorus.depth_ms, 0.0, 8.0),
            ("Chorus base", self.chorus.base_ms, 10.0, 30.0),
            ("Exciter tune", self.exciter.tune_hz, 600.0, 6000.0),
            ("Exciter drive", self.exciter.drive, 0.0, 1.0),
            ("Exciter tone", self.exciter.tone, 0.0, 1.0),
        ] {
            bounded(label, value, min, max)?;
        }
        if self.delay.division > 4 {
            return Err("Unknown tempo division".into());
        }
        Ok(())
    }
    /// Only audible structural choices trigger a fade/clear. Inactive family
    /// settings may be edited or recalled without retiring the current tail.
    pub fn same_structure(self, other: Self) -> bool {
        self.algorithm == other.algorithm
            && match self.algorithm {
                Algorithm::Delay => {
                    self.delay.kind == other.delay.kind
                        && self.delay.ping_pong == other.delay.ping_pong
                }
                Algorithm::Room => self.reverb.kind == other.reverb.kind,
                Algorithm::Chorus => self.chorus.ensemble == other.chorus.ensemble,
                Algorithm::Exciter => self.exciter.bright == other.exciter.bright,
            }
    }
}
fn bounded(label: &str, value: f32, min: f32, max: f32) -> Result<(), String> {
    if !value.is_finite() || !(min..=max).contains(&value) {
        Err(format!("{label} outside {min}..{max}"))
    } else {
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineConfig {
    pub mode: EngineMode,
    pub pieces: u8,
    pub stages: [EffectConfig; MAX_STAGES],
    pub level: f32,
    pub bypass: bool,
    pub mute: bool,
    pub tempo: Tempo,
}
impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            mode: EngineMode::Single,
            pieces: 4,
            stages: std::array::from_fn(|i| EffectConfig {
                algorithm: Algorithm::ALL[i % Algorithm::ALL.len()],
                ..EffectConfig::default()
            }),
            level: 0.7,
            bypass: false,
            mute: false,
            tempo: Tempo {
                bpm: 120.0,
                source: TempoSource::Internal,
            },
        }
    }
}
impl EngineConfig {
    /// Preserve v2 two/three-slot gain. Larger mixes reserve equal headroom
    /// per configured slot, independently of bypass or current wet levels.
    pub fn mix_divisor(self) -> usize {
        if self.mode == EngineMode::Single {
            1
        } else {
            self.stage_count().max(3)
        }
    }
    pub fn stage_count(self) -> usize {
        if self.mode == EngineMode::MultiFx {
            self.pieces as usize
        } else {
            1
        }
    }
    pub fn label(self) -> &'static str {
        if self.mode == EngineMode::MultiFx {
            "MultiFX"
        } else {
            self.stages[0].label()
        }
    }
    pub fn path_label(self) -> String {
        if self.mode == EngineMode::Single {
            self.stages[0].label().into()
        } else {
            self.stages[..self.stage_count()]
                .iter()
                .map(|s| s.label())
                .collect::<Vec<_>>()
                .join(" + ")
        }
    }
    pub fn same_structure(self, other: Self) -> bool {
        self.mode == other.mode
            && self.stage_count() == other.stage_count()
            && (0..self.stage_count()).all(|i| self.stages[i].same_structure(other.stages[i]))
    }
    pub fn validate(self) -> Result<(), String> {
        if !(2..=MAX_STAGES as u8).contains(&self.pieces) {
            return Err(format!("MultiFX needs 2–{MAX_STAGES} slots"));
        }
        for stage in self.stages {
            stage.validate()?;
        }
        bounded("Level", self.level, 0.0, 1.0)?;
        bounded("BPM", self.tempo.bpm, 30.0, 300.0)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rack {
    pub version: u32,
    pub engines: [EngineConfig; 2],
    pub routing: Routing,
    pub shared_tempo: bool,
}
impl Default for Rack {
    fn default() -> Self {
        let mut engines = [EngineConfig::default(); 2];
        engines[1].stages[0].algorithm = Algorithm::Room;
        Self {
            version: VERSION,
            engines,
            routing: Routing::default(),
            shared_tempo: true,
        }
    }
}
impl Rack {
    pub fn validate(&self, available: Availability) -> Result<(), String> {
        if self.version != VERSION {
            return Err("Unsupported rack version".into());
        }
        for e in self.engines {
            e.validate()?;
        }
        if self.shared_tempo && self.engines[0].tempo != self.engines[1].tempo {
            return Err("Shared tempo values disagree".into());
        }
        self.routing.validate(available)
    }
    pub fn set_tempo(&mut self, e: usize, tempo: Tempo) {
        self.engines[e].tempo = tempo;
        if self.shared_tempo {
            self.engines[1 - e].tempo = tempo;
        }
    }
}
