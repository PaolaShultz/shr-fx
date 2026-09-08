use crate::{
    midi::{Binding, MidiSource},
    model::{
        Algorithm, Availability, ChorusConfig, DelayConfig, EffectConfig, EngineConfig, EngineMode,
        LOCAL_VERSION, Rack, ReverbConfig, Routing, Tempo, VERSION,
    },
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ports {
    pub inputs: [Option<String>; 4],
    pub outputs: [Option<String>; 4],
}
impl Ports {
    pub fn validate(&self) -> Result<(), String> {
        for side in [&self.inputs, &self.outputs] {
            for (index, name) in side.iter().enumerate() {
                if let Some(name) = name {
                    if name.is_empty()
                        || name.len() > 255
                        || name.chars().any(char::is_control)
                        || !name.contains(':')
                        || name.starts_with("fx:")
                    {
                        return Err("Use an exact external JACK port name".into());
                    }
                    if side[..index].iter().flatten().any(|n| n == name) {
                        return Err("Duplicate physical port assignment".into());
                    }
                }
            }
        }
        Ok(())
    }
    pub fn assigned(&self) -> Availability {
        Availability {
            inputs: self
                .inputs
                .iter()
                .enumerate()
                .fold(0, |m, (i, n)| m | if n.is_some() { 1 << i } else { 0 }),
            outputs: self
                .outputs
                .iter()
                .enumerate()
                .fold(0, |m, (i, n)| m | if n.is_some() { 1 << i } else { 0 }),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalConfig {
    pub version: u32,
    pub ports: Ports,
    pub midi_source: Option<MidiSource>,
    pub bindings: Vec<Binding>,
}
impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            version: LOCAL_VERSION,
            ports: Ports::default(),
            midi_source: None,
            bindings: Vec::new(),
        }
    }
}
impl LocalConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != LOCAL_VERSION {
            return Err("Unsupported local config version".into());
        }
        self.ports.validate()?;
        if self.bindings.len() > 64 {
            return Err("At most 64 MIDI bindings".into());
        }
        for (i, binding) in self.bindings.iter().enumerate() {
            binding.validate()?;
            if matches!(binding.target, crate::midi::Target::Surface(_))
                && self.bindings[..i]
                    .iter()
                    .any(|b| b.target == binding.target)
            {
                return Err("Surface role has duplicate bindings".into());
            }
            if self.bindings[..i].iter().any(|b| {
                b.channel == binding.channel
                    && b.number == binding.number
                    && (b.kind == crate::midi::ControlKind::Note)
                        == (binding.kind == crate::midi::ControlKind::Note)
            }) {
                return Err("MIDI control has duplicate bindings".into());
            }
        }
        if self.midi_source.as_ref().is_some_and(|s| {
            [&s.client, &s.port]
                .iter()
                .any(|n| n.is_empty() || n.len() > 255 || n.chars().any(char::is_control))
        }) {
            return Err("Invalid MIDI source identity".into());
        }
        Ok(())
    }
}
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(65_537)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 65_536 {
        return Err("File exceeds 64 KiB".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
pub fn atomic_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().ok_or("No parent directory")?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)
        .map_err(|e| e.to_string())?;
    let tmp = parent.join(format!(
        ".fx-{}-{}.tmp",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.write_all(b"\n").map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| e.to_string())?;
        // Atomicity is established by rename. Directory sync is best effort;
        // report success once committed, avoiding a false rollback promise.
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
pub fn load_local(root: &Path) -> Result<LocalConfig, String> {
    let path = root.join("local.json");
    if !path.exists() {
        return Ok(LocalConfig::default());
    }
    let mut config: LocalConfig = read_json(&path)?;
    if config.version == 1 {
        if config
            .bindings
            .iter()
            .any(|b| matches!(b.target, crate::midi::Target::Surface(_)))
        {
            return Err("Surface roles require local config v2".into());
        }
        config.version = LOCAL_VERSION;
    }
    config.validate()?;
    Ok(config)
}
pub fn save_local(root: &Path, config: &LocalConfig) -> Result<(), String> {
    config.validate()?;
    atomic_json(&root.join("local.json"), config)
}
pub fn snapshot_path(root: &Path, slot: usize) -> Result<PathBuf, String> {
    if slot >= 16 {
        return Err("Sound slot must be 1-16".into());
    }
    Ok(root
        .join("sounds")
        .join(format!("sound-{:02}.json", slot + 1)))
}
pub fn save_rack(root: &Path, slot: usize, rack: &Rack) -> Result<(), String> {
    rack.validate(Availability::ALL)?;
    atomic_json(&snapshot_path(root, slot)?, rack)
}
pub fn load_rack(root: &Path, slot: usize, available: Availability) -> Result<Rack, String> {
    let stored: StoredRack = read_json(&snapshot_path(root, slot)?)?;
    let rack = match stored {
        StoredRack::Current(rack) => *rack,
        StoredRack::V2(rack) => rack.upgrade()?,
        StoredRack::Legacy(rack) => rack.upgrade()?,
    };
    rack.validate(available)?;
    Ok(rack)
}

// Typed decoding keeps unknown/missing/duplicate fields invalid in both schemas.
// Migration is read-only; only an explicit Save writes the current format.
#[derive(Deserialize)]
#[serde(untagged)]
enum StoredRack {
    Current(Box<Rack>),
    V2(Box<V2Rack>),
    Legacy(LegacyRack),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V2Rack {
    version: u32,
    engines: [V2Engine; 2],
    routing: Routing,
    shared_tempo: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V2Engine {
    mode: EngineMode,
    pieces: u8,
    stages: [V2Effect; 3],
    level: f32,
    bypass: bool,
    mute: bool,
    tempo: Tempo,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum V2Algorithm {
    Delay,
    Room,
    Chorus,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V2Effect {
    level: f32,
    bypass: bool,
    algorithm: V2Algorithm,
    delay: DelayConfig,
    reverb: ReverbConfig,
    chorus: ChorusConfig,
}
impl V2Rack {
    fn upgrade(self) -> Result<Rack, String> {
        if self.version != 2 || self.engines.iter().any(|e| !(2..=3).contains(&e.pieces)) {
            return Err("Invalid v2 rack version or slot count".into());
        }
        let engines = self.engines.map(|old| {
            let mut engine = EngineConfig {
                mode: old.mode,
                pieces: old.pieces,
                level: old.level,
                bypass: old.bypass,
                mute: old.mute,
                tempo: old.tempo,
                ..EngineConfig::default()
            };
            for (i, effect) in old.stages.into_iter().enumerate() {
                engine.stages[i] = EffectConfig {
                    algorithm: match effect.algorithm {
                        V2Algorithm::Delay => Algorithm::Delay,
                        V2Algorithm::Room => Algorithm::Room,
                        V2Algorithm::Chorus => Algorithm::Chorus,
                    },
                    level: effect.level,
                    bypass: effect.bypass,
                    delay: effect.delay,
                    reverb: effect.reverb,
                    chorus: effect.chorus,
                    ..EffectConfig::default()
                };
            }
            engine
        });
        Ok(Rack {
            version: VERSION,
            engines,
            routing: self.routing,
            shared_tempo: self.shared_tempo,
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyRack {
    version: u32,
    engines: [LegacyEngine; 2],
    routing: Routing,
    shared_tempo: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum LegacyAlgorithm {
    Delay,
    Room,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyEngine {
    algorithm: LegacyAlgorithm,
    time_ms: f32,
    predelay_ms: f32,
    feedback: f32,
    damping: f32,
    level: f32,
    sync: bool,
    division: u8,
    ping_pong: bool,
    bypass: bool,
    mute: bool,
    tempo: Tempo,
}
impl LegacyRack {
    fn upgrade(self) -> Result<Rack, String> {
        if self.version != 1 {
            return Err("Unsupported rack version".into());
        }
        let engines = self.engines.map(|old| {
            let mut engine = EngineConfig {
                level: old.level,
                bypass: old.bypass,
                mute: old.mute,
                tempo: old.tempo,
                ..EngineConfig::default()
            };
            engine.stages[0] = EffectConfig {
                algorithm: match old.algorithm {
                    LegacyAlgorithm::Delay => Algorithm::Delay,
                    LegacyAlgorithm::Room => Algorithm::Room,
                },
                delay: DelayConfig {
                    time_ms: old.time_ms,
                    feedback: old.feedback,
                    damping: old.damping,
                    sync: old.sync,
                    division: old.division,
                    ping_pong: old.ping_pong,
                    ..DelayConfig::default()
                },
                reverb: ReverbConfig {
                    predelay_ms: old.predelay_ms,
                    decay: old.feedback,
                    damping: old.damping,
                    ..ReverbConfig::default()
                },
                ..EffectConfig::default()
            };
            engine
        });
        Ok(Rack {
            version: VERSION,
            engines,
            routing: self.routing,
            shared_tempo: self.shared_tempo,
        })
    }
}
