//! Hardware control commands (LEDs, fans, airduct mode, buzzer, `print_option` settings).

use serde::Serialize;

use super::ClampedTaskId;
use crate::types::control::{
    AirPurificationMode, BuzzerMode, LedNode, LightMode, NozzleBlobDetectMode,
};

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

/// Bit of `print_option`'s `option` field carrying auto-recovery, BambuStudio's `PRINT_OP_AUTO_RECOVERY` (`DeviceManager.hpp:185`).
pub(crate) const PRINT_OP_AUTO_RECOVERY: u32 = 0;

/// Turns step-loss auto-recovery on or off.
///
/// Carries the setting twice, as BambuStudio's `command_set_printing_option` does
/// (`DeviceManager.cpp:1832-1842`): as bit `PRINT_OP_AUTO_RECOVERY` (0) of `option` and as
/// `auto_recovery`. bambuddy sends only `auto_recovery`.
#[derive(Debug, Clone, Serialize)]
pub struct AutoRecoveryPayload {
    /// Wire command name, always `"print_option"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
    /// The setting as a bit of the legacy option bitmask.
    pub option: u32,
    /// Whether auto-recovery is enabled.
    pub auto_recovery: bool,
}

/// Enables or disables step-loss auto-recovery.
pub type AutoRecoveryRequest = super::Print<AutoRecoveryPayload>;

impl AutoRecoveryRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "print_option";

    /// Builds a `print_option` request enabling or disabling step-loss auto-recovery.
    pub fn new(enable: bool, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: AutoRecoveryPayload {
                command: Self::COMMAND,
                sequence_id: sequence_id.into(),
                option: u32::from(enable) << PRINT_OP_AUTO_RECOVERY,
                auto_recovery: enable,
            },
        }
    }
}

/// Turns AMS Filament Backup (auto-refill from a matching spool) on or off.
#[derive(Debug, Clone, Serialize)]
pub struct FilamentBackupPayload {
    /// Wire command name, always `"print_option"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
    /// Whether Filament Backup is enabled.
    pub auto_switch_filament: bool,
}

/// Enables or disables AMS Filament Backup.
pub type FilamentBackupRequest = super::Print<FilamentBackupPayload>;

impl FilamentBackupRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "print_option";

    /// Builds a `print_option` request enabling or disabling Filament Backup.
    pub fn new(enable: bool, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: FilamentBackupPayload {
                command: Self::COMMAND,
                sequence_id: sequence_id.into(),
                auto_switch_filament: enable,
            },
        }
    }
}

/// Turns filament tangle detection on or off.
#[derive(Debug, Clone, Serialize)]
pub struct FilamentTangleDetectPayload {
    /// Wire command name, always `"print_option"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
    /// Whether tangle detection is enabled.
    pub filament_tangle_detect: bool,
}

/// Enables or disables filament tangle detection.
pub type FilamentTangleDetectRequest = super::Print<FilamentTangleDetectPayload>;

impl FilamentTangleDetectRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "print_option";

    /// Builds a `print_option` request enabling or disabling filament tangle detection.
    pub fn new(enable: bool, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: FilamentTangleDetectPayload {
                command: Self::COMMAND,
                sequence_id: sequence_id.into(),
                filament_tangle_detect: enable,
            },
        }
    }
}

/// Turns nozzle blob detection (the original, on/off form) on or off.
#[derive(Debug, Clone, Serialize)]
pub struct NozzleBlobDetectPayload {
    /// Wire command name, always `"print_option"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
    /// Whether nozzle blob detection is enabled.
    pub nozzle_blob_detect: bool,
}

/// Enables or disables nozzle blob detection.
pub type NozzleBlobDetectRequest = super::Print<NozzleBlobDetectPayload>;

impl NozzleBlobDetectRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "print_option";

    /// Builds a `print_option` request enabling or disabling nozzle blob detection.
    pub fn new(enable: bool, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: NozzleBlobDetectPayload {
                command: Self::COMMAND,
                sequence_id: sequence_id.into(),
                nozzle_blob_detect: enable,
            },
        }
    }
}

/// Sets the smart nozzle blob detection mode (off, on, or auto).
#[derive(Debug, Clone, Serialize)]
pub struct SmartNozzleBlobDetectPayload {
    /// Wire command name, always `"print_option"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
    /// The mode's code — see [`NozzleBlobDetectMode::code`].
    pub nozzle_blob_detect_v2: u8,
}

/// Sets the smart nozzle blob detection mode.
pub type SmartNozzleBlobDetectRequest = super::Print<SmartNozzleBlobDetectPayload>;

impl SmartNozzleBlobDetectRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "print_option";

    /// Builds a `print_option` request setting the smart nozzle blob detection mode.
    pub fn new(mode: NozzleBlobDetectMode, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: SmartNozzleBlobDetectPayload {
                command: Self::COMMAND,
                sequence_id: sequence_id.into(),
                nozzle_blob_detect_v2: mode.code(),
            },
        }
    }
}

/// Turns non-visual air-printing detection on or off.
///
/// Not the camera's AI air-printing detector, which `xcam_control_set` drives.
#[derive(Debug, Clone, Serialize)]
pub struct AirPrintDetectPayload {
    /// Wire command name, always `"print_option"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
    /// Whether air-printing detection is enabled.
    pub air_print_detect: bool,
}

/// Enables or disables non-visual air-printing detection.
pub type AirPrintDetectRequest = super::Print<AirPrintDetectPayload>;

impl AirPrintDetectRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "print_option";

    /// Builds a `print_option` request enabling or disabling air-printing detection.
    pub fn new(enable: bool, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: AirPrintDetectPayload {
                command: Self::COMMAND,
                sequence_id: sequence_id.into(),
                air_print_detect: enable,
            },
        }
    }
}

/// Sets where chamber air is purified at the end of a print.
#[derive(Debug, Clone, Serialize)]
pub struct AirPurificationPayload {
    /// Wire command name, always `"print_option"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
    /// The mode's code — see [`AirPurificationMode::code`].
    pub air_purification: u8,
}

/// Sets the end-of-print air purification mode.
pub type AirPurificationRequest = super::Print<AirPurificationPayload>;

impl AirPurificationRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "print_option";

    /// Builds a `print_option` request setting the end-of-print air purification mode.
    pub fn new(mode: AirPurificationMode, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: AirPurificationPayload {
                command: Self::COMMAND,
                sequence_id: sequence_id.into(),
                air_purification: mode.code(),
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
