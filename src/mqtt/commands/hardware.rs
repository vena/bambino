//! Hardware control commands (LEDs, fans, airduct mode, buzzer, prompt sound).

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use serde::Serialize;

use super::ClampedTaskId;

/// Chamber illumination and toolhead LED control configurations.
#[derive(Debug, Clone, Serialize)]
pub struct LedCtrlPayload {
    /// Wire command name, always `"ledctrl"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
    /// Targets specific physical fixtures (e.g. "chamber_light", "chamber_light2").
    pub led_node: String,
    /// Mode state transitions (e.g., "on", "off", "flashing").
    pub led_mode: String,
    /// On-time per flash cycle (ms); only meaningful in flashing mode.
    pub led_on_time: u32,
    /// Off-time per flash cycle (ms); only meaningful in flashing mode.
    pub led_off_time: u32,
    /// Number of flash loops; only meaningful in flashing mode.
    pub loop_times: u32,
    /// Interval between flash cycles (ms); only meaningful in flashing mode.
    pub interval_time: u32,
}

/// Turns chamber or toolhead LEDs on or off.
#[derive(Debug, Clone, Serialize)]
pub struct LedCtrlRequest {
    /// The `system` namespace envelope required by the wire protocol.
    pub system: LedCtrlPayload,
}

impl LedCtrlRequest {
    /// Builds a simple on/off `ledctrl` request for the given fixture.
    pub fn new(led_node: &str, turn_on: bool, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            system: LedCtrlPayload {
                command: "ledctrl",
                sequence_id: sequence_id.into().to_string(),
                led_node: String::from(led_node),
                led_mode: String::from(if turn_on { "on" } else { "off" }),
                led_on_time: 0,
                led_off_time: 0,
                loop_times: 0,
                interval_time: 0,
            },
        }
    }

    /// Builds a flashing-mode LED command with explicit on/off/loop/interval timing (`led_mode: "flashing"`), per [REF-MQTT-LIFECYCLE].
    pub fn new_flashing(
        led_node: &str,
        on_time: u32,
        off_time: u32,
        loop_times: u32,
        interval_time: u32,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Self {
        Self {
            system: LedCtrlPayload {
                command: "ledctrl",
                sequence_id: sequence_id.into().to_string(),
                led_node: String::from(led_node),
                led_mode: String::from("flashing"),
                led_on_time: on_time,
                led_off_time: off_time,
                loop_times,
                interval_time,
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
    /// Closes internal recirculation dampers, routes hot air out through exhaust.
    Cooling = 0,
    /// Seals enclosure, closes exhaust flaps for heat retention.
    Heating = 1,
    /// Laser engraving module configuration.
    Laser = 2,
}

/// Redirects internal climate airflows using active damper deflection plates.
#[derive(Debug, Clone, Serialize)]
pub struct AirductPayload {
    /// Wire command name, always `"set_airduct"`.
    pub command: &'static str,
    /// Damper mode: 0=cooling (exhaust), 1=heating (sealed), 2=laser [REF-MQTT-LIFECYCLE].
    #[serde(rename = "modeId")]
    pub mode_id: i32,
    /// Damper submode; always `-1` (unused) — [`AirductRequest::new`] never sets it otherwise.
    pub submode: i32,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
}

/// Switches the enclosure airduct damper between cooling, heating, and laser modes.
#[derive(Debug, Clone, Serialize)]
pub struct AirductRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: AirductPayload,
}

impl AirductRequest {
    /// Builds a `set_airduct` request for the given damper mode.
    pub fn new(mode: AirductMode, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: AirductPayload {
                command: "set_airduct",
                mode_id: mode as i32,
                submode: -1,
                sequence_id: sequence_id.into().to_string(),
            },
        }
    }
}

/// Controls structural notification sound output via speakers (Supported on A1, A1 Mini, and A2L only; H2-series buzzer alerts use the separate `buzzer_ctrl` command — see [`BuzzerPayload`]).
#[derive(Debug, Clone, Serialize)]
pub struct PromptSoundPayload {
    /// Wire command name, always `"print_option"`.
    pub command: &'static str,
    /// Whether notification sounds are enabled.
    pub sound_enable: bool,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
}

/// Enables or disables the printer's notification sounds.
#[derive(Debug, Clone, Serialize)]
pub struct PromptSoundRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: PromptSoundPayload,
}

impl PromptSoundRequest {
    /// Builds a `print_option` request enabling or disabling notification sounds.
    pub fn new(enable: bool, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: PromptSoundPayload {
                command: "print_option",
                sound_enable: enable,
                sequence_id: sequence_id.into().to_string(),
            },
        }
    }
}

/// Modifies active alarm or attention chime parameters on the printer cabinet buzzer module.
#[derive(Debug, Clone, Serialize)]
pub struct BuzzerPayload {
    /// Wire command name, always `"buzzer_ctrl"`.
    pub command: &'static str,
    /// Alarm state representation: `0` (Silent), `1` (Alarm), `2` (Chirp/Beep) [REF-MQTT-LIFECYCLE].
    pub mode: i32,
    /// Reason string shown alongside the alarm; always empty in practice, per [`BuzzerRequest::new`].
    pub reason: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
}

/// Controls the printer's buzzer alarm mode (silent, alarm, or chirp).
#[derive(Debug, Clone, Serialize)]
pub struct BuzzerRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: BuzzerPayload,
}

impl BuzzerRequest {
    /// Builds a `buzzer_ctrl` request for the given alarm mode.
    pub fn new(mode_code: i32, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: BuzzerPayload {
                command: "buzzer_ctrl",
                mode: mode_code,
                reason: "",
                sequence_id: sequence_id.into().to_string(),
            },
        }
    }
}
