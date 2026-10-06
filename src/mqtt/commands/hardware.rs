//! Hardware control commands (LEDs, fans, airduct mode, buzzer, prompt sound).

use serde::Serialize;

use super::ClampedTaskId;
use crate::types::control::{BuzzerMode, LedNode, LightMode};

/// Chamber illumination and toolhead LED control configurations.
#[derive(Debug, Clone, Serialize)]
pub struct LedCtrlPayload {
    /// Wire command name, always `"ledctrl"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
    /// The fixture addressed — see [`LedNode`].
    pub led_node: &'static str,
    /// The mode set — see [`LightMode`].
    pub led_mode: &'static str,
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
pub type LedCtrlRequest = super::System<LedCtrlPayload>;

/// Flash cycle timing for [`LedCtrlRequest::new_flashing`]; every field is in milliseconds except `loops`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FlashTiming {
    /// Time lit per cycle, in ms.
    pub on_ms: u32,
    /// Time dark per cycle, in ms.
    pub off_ms: u32,
    /// Number of cycles.
    pub loops: u32,
    /// Pause between cycles, in ms.
    pub interval_ms: u32,
}

impl LedCtrlRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "ledctrl";

    /// Builds a simple on/off `ledctrl` request for `node`.
    pub fn new(node: LedNode, turn_on: bool, sequence_id: impl Into<ClampedTaskId>) -> Self {
        let mode = if turn_on {
            LightMode::On
        } else {
            LightMode::Off
        };
        Self::build(node, mode, FlashTiming::default(), sequence_id)
    }

    /// Builds a flashing-mode request (`led_mode: "flashing"`) with explicit timing [REF-MQTT-LIFECYCLE].
    pub fn new_flashing(
        node: LedNode,
        timing: FlashTiming,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Self {
        Self::build(node, LightMode::Flashing, timing, sequence_id)
    }

    fn build(
        node: LedNode,
        mode: LightMode,
        timing: FlashTiming,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Self {
        Self {
            system: LedCtrlPayload {
                command: Self::COMMAND,
                sequence_id: sequence_id.into(),
                led_node: node.as_wire(),
                led_mode: mode.as_wire(),
                led_on_time: timing.on_ms,
                led_off_time: timing.off_ms,
                loop_times: timing.loops,
                interval_time: timing.interval_ms,
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
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
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
    pub sequence_id: ClampedTaskId,
}

/// Switches the enclosure airduct damper between cooling, heating, and laser modes.
pub type AirductRequest = super::Print<AirductPayload>;

impl AirductRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "set_airduct";

    /// Builds a `set_airduct` request for the given damper mode.
    pub fn new(mode: AirductMode, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: AirductPayload {
                command: Self::COMMAND,
                mode_id: mode as i32,
                submode: -1,
                sequence_id: sequence_id.into(),
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
    pub sequence_id: ClampedTaskId,
}

/// Enables or disables the printer's notification sounds.
pub type PromptSoundRequest = super::Print<PromptSoundPayload>;

impl PromptSoundRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "print_option";

    /// Builds a `print_option` request enabling or disabling notification sounds.
    pub fn new(enable: bool, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: PromptSoundPayload {
                command: Self::COMMAND,
                sound_enable: enable,
                sequence_id: sequence_id.into(),
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
    pub sequence_id: ClampedTaskId,
}

/// Controls the printer's buzzer alarm mode (silent, alarm, or chirp).
pub type BuzzerRequest = super::Print<BuzzerPayload>;

impl BuzzerRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "buzzer_ctrl";

    /// Builds a `buzzer_ctrl` request for the given alarm mode.
    pub fn new(mode: BuzzerMode, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: BuzzerPayload {
                command: Self::COMMAND,
                mode: mode.code(),
                reason: "",
                sequence_id: sequence_id.into(),
            },
        }
    }
}
