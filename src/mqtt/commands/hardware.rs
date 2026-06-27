//! Hardware control commands (LEDs, fans, airduct mode, buzzer, prompt sound).

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use serde::Serialize;

/// Chamber illumination and toolhead LED control configurations.
#[derive(Debug, Clone, Serialize)]
pub struct LedCtrlPayload {
    pub command: &'static str,
    pub sequence_id: String,
    /// Targets specific physical fixtures (e.g. "chamber_light", "chamber_light2").
    pub led_node: String,
    /// Mode state transitions (e.g., "on", "off", "flashing").
    pub led_mode: String,
    pub led_on_time: u32,
    pub led_off_time: u32,
    pub loop_times: u32,
    pub interval_time: u32,
}

/// Turns chamber or toolhead LEDs on or off.
#[derive(Debug, Clone, Serialize)]
pub struct LedCtrlRequest {
    pub system: LedCtrlPayload,
}

impl LedCtrlRequest {
    pub fn new(led_node: &str, turn_on: bool, sequence_id: u64) -> Self {
        Self {
            system: LedCtrlPayload {
                command: "ledctrl",
                sequence_id: sequence_id.to_string(),
                led_node: String::from(led_node),
                led_mode: String::from(if turn_on { "on" } else { "off" }),
                led_on_time: 0,
                led_off_time: 0,
                loop_times: 0,
                interval_time: 0,
            },
        }
    }
}

/// Airduct damper operating mode [REF-MQTT-LIFECYCLE].
///
/// `Cooling` (0): closes internal recirculation dampers, routes hot air out through exhaust.
/// `Heating` (1): closes exhaust flaps, seals enclosure for heat retention.
/// `Laser` (2): configuration for laser engraving module operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AirductMode {
    Cooling = 0,
    Heating = 1,
    Laser = 2,
}

/// Redirects internal climate airflows using active damper deflection plates.
#[derive(Debug, Clone, Serialize)]
pub struct AirductPayload {
    pub command: &'static str,
    /// Damper mode: 0=cooling (exhaust), 1=heating (sealed), 2=laser [REF-MQTT-LIFECYCLE].
    #[serde(rename = "modeId")]
    pub mode_id: i32,
    pub submode: i32,
    pub sequence_id: String,
}

/// Switches the enclosure airduct damper between cooling, heating, and laser modes.
#[derive(Debug, Clone, Serialize)]
pub struct AirductRequest {
    pub print: AirductPayload,
}

impl AirductRequest {
    pub fn new(mode: AirductMode, sequence_id: u64) -> Self {
        Self {
            print: AirductPayload {
                command: "set_airduct",
                mode_id: mode as i32,
                submode: -1,
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}

/// Controls structural notification sound output via speakers (Supported on A1 and H2D series only).
#[derive(Debug, Clone, Serialize)]
pub struct PromptSoundPayload {
    pub command: &'static str,
    pub sound_enable: bool,
    pub sequence_id: String,
}

/// Enables or disables the printer's notification sounds.
#[derive(Debug, Clone, Serialize)]
pub struct PromptSoundRequest {
    pub print: PromptSoundPayload,
}

impl PromptSoundRequest {
    pub fn new(enable: bool, sequence_id: u64) -> Self {
        Self {
            print: PromptSoundPayload {
                command: "print_option",
                sound_enable: enable,
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}

/// Modifies active alarm or attention chime parameters on the printer cabinet buzzer module.
#[derive(Debug, Clone, Serialize)]
pub struct BuzzerPayload {
    pub command: &'static str,
    /// Alarm state representation: `0` (Silent), `1` (Alarm), `2` (Chirp/Beep) [REF-MQTT-LIFECYCLE].
    pub mode: i32,
    pub reason: &'static str,
    pub sequence_id: String,
}

/// Controls the printer's buzzer alarm mode (silent, alarm, or chirp).
#[derive(Debug, Clone, Serialize)]
pub struct BuzzerRequest {
    pub print: BuzzerPayload,
}

impl BuzzerRequest {
    pub fn new(mode_code: i32, sequence_id: u64) -> Self {
        Self {
            print: BuzzerPayload {
                command: "buzzer_ctrl",
                mode: mode_code,
                reason: "",
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}
