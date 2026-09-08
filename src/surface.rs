//! Stable physical sound roles; navigation and value roles resolve in the UI.
use crate::{
    midi::{Action, Parameter, SlotParameter, Target},
    model::{Algorithm, Rack},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Role {
    Navigate,
    Value,
    Sound(u8),
    Knob(u8),
    Pad(u8),
    Button(u8), // Legacy explicit role; no extra physical buttons assumed.
    Press(u8),
}
impl Role {
    pub const SETUP_COUNT: usize = 26;
    pub fn valid(self) -> bool {
        match self {
            Self::Navigate | Self::Value => true,
            Self::Sound(i) => (9..16).contains(&i),
            Self::Knob(i) => i < 16,
            Self::Pad(i) | Self::Button(i) => i < 8,
            Self::Press(i) => matches!(i, 0 | 8),
        }
    }
    pub fn rotary(self) -> bool {
        matches!(
            self,
            Self::Knob(_) | Self::Sound(_) | Self::Navigate | Self::Value
        )
    }
    pub fn at(step: usize) -> Self {
        match step {
            0 => Self::Navigate,
            8 => Self::Value,
            9..=15 => Self::Sound(step as u8),
            1..=7 => Self::Knob(step as u8),
            16..=23 => Self::Pad((step - 16) as u8),
            24 => Self::Press(0),
            _ => Self::Press(8),
        }
    }
    pub fn label(self) -> String {
        match self {
            Self::Navigate => "Rotary 1 / Browse".into(),
            Self::Value => "Rotary 9 / Adjust selected value".into(),
            Self::Sound(i) => format!(
                "Rotary {} / {}",
                i + 1,
                [
                    "Time / rate / tune",
                    "Feedback / depth / drive",
                    "Damping / base / tone",
                    "BPM",
                    "Engine return",
                    "Delay sync",
                    "Delay division"
                ][(i - 9) as usize]
            ),
            Self::Press(i) => format!(
                "Click rotary {} / {}",
                i + 1,
                if i == 0 {
                    "Open / Confirm"
                } else {
                    "Back / Cancel"
                }
            ),
            Self::Knob(i) => format!(
                "Knob {} / {}",
                i + 1,
                if i < 8 {
                    "slot wet"
                } else {
                    "effect parameter"
                }
            ),
            Self::Pad(i) => format!("Pad {} / slot {} off/on", i + 1, i + 1),
            Self::Button(i) => format!("Button {} / {}", i + 1, Self::BUTTON_LABELS[i as usize]),
        }
    }
    pub const BUTTON_LABELS: [&'static str; 8] = [
        "Previous effect",
        "Next effect",
        "Parameter page",
        "Engine A/B",
        "TAP",
        "Back/Cancel",
        "Menu/Confirm",
        "PANIC",
    ];
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Context {
    pub engine: usize,
    pub slot: usize,
    pub page: usize,
}
impl Context {
    pub fn resolve(self, rack: &Rack, role: Role) -> Option<Target> {
        if !role.valid() {
            return None;
        }
        let parameter = match role {
            Role::Navigate | Role::Value => return Some(Target::Surface(role)),
            Role::Sound(i) => {
                if self.slot >= rack.engines[self.engine].stage_count() {
                    return None;
                }
                match i {
                    9..=11 => return Self { page: 0, ..self }.resolve(rack, Role::Knob(i - 1)),
                    12 if rack.engines[self.engine].tempo.source
                        == crate::model::TempoSource::Internal =>
                    {
                        Parameter::Bpm
                    }
                    12 => return None,
                    13 => Parameter::Level,
                    14..=15
                        if rack.engines[self.engine].stages[self.slot].algorithm
                            == Algorithm::Delay =>
                    {
                        Parameter::Slot {
                            slot: self.slot as u8,
                            control: if i == 14 {
                                SlotParameter::DelaySync
                            } else {
                                SlotParameter::DelayDivision
                            },
                        }
                    }
                    _ => return None,
                }
            }
            Role::Press(i) => {
                return Some(Target::Action(if i == 0 {
                    Action::Confirm
                } else {
                    Action::Cancel
                }));
            }
            Role::Pad(slot) => return Some(Target::Action(Action::SlotBypass(slot))),
            Role::Button(i) => {
                return Some(Target::Action(
                    [
                        Action::PreviousEffect,
                        Action::NextEffect,
                        Action::ParameterPage,
                        Action::SwitchEngine,
                        Action::Tap,
                        Action::Cancel,
                        Action::MenuConfirm,
                        Action::Panic,
                    ][i as usize],
                ));
            }
            Role::Knob(i) if i < 8 => {
                if i as usize >= rack.engines[self.engine].stage_count() {
                    return None;
                }
                Parameter::Slot {
                    slot: i,
                    control: SlotParameter::Level,
                }
            }
            Role::Knob(i) => {
                if self.slot >= rack.engines[self.engine].stage_count() {
                    return None;
                }
                let effect = rack.engines[self.engine].stages[self.slot];
                let control = if self.page == 0 {
                    match (effect.algorithm, i) {
                        (Algorithm::Delay, 8) => SlotParameter::DelayTime,
                        (Algorithm::Delay, 9) => SlotParameter::DelayFeedback,
                        (Algorithm::Delay, 10) => SlotParameter::DelayDamping,
                        (Algorithm::Room, 8) => SlotParameter::Predelay,
                        (Algorithm::Room, 9) => SlotParameter::Decay,
                        (Algorithm::Room, 10) => SlotParameter::ReverbDamping,
                        (Algorithm::Chorus, 8) => SlotParameter::ChorusRate,
                        (Algorithm::Chorus, 9) => SlotParameter::ChorusDepth,
                        (Algorithm::Chorus, 10) => SlotParameter::ChorusBase,
                        (Algorithm::Exciter, 8) => SlotParameter::ExciterTune,
                        (Algorithm::Exciter, 9) => SlotParameter::ExciterDrive,
                        (Algorithm::Exciter, 10) => SlotParameter::ExciterTone,
                        _ => return None,
                    }
                } else {
                    match i {
                        8 => {
                            return (rack.engines[self.engine].tempo.source
                                == crate::model::TempoSource::Internal)
                                .then_some(Target::Parameter {
                                    engine: self.engine as u8,
                                    parameter: Parameter::Bpm,
                                });
                        }
                        9 => {
                            return Some(Target::Parameter {
                                engine: self.engine as u8,
                                parameter: Parameter::Level,
                            });
                        }
                        10 if effect.algorithm == Algorithm::Delay => SlotParameter::DelaySync,
                        11 if effect.algorithm == Algorithm::Delay => SlotParameter::DelayDivision,
                        _ => return None,
                    }
                };
                Parameter::Slot {
                    slot: self.slot as u8,
                    control,
                }
            }
        };
        Some(Target::Parameter {
            engine: self.engine as u8,
            parameter,
        })
    }
}
