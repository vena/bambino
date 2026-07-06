//! AMS-related MQTT command payloads (filament change, drying, RFID scan, settings).

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use serde::Serialize;

use super::clamp_task_id;

/// Overwrites physical attributes or custom slicer presets assigned to a specific tray.
#[derive(Debug, Clone, Serialize)]
pub struct AmsFilamentSettingPayload {
    /// Wire command name, always `"ams_filament_setting"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
    /// Target AMS unit or external-spool address — see the addressing cheat-sheet on
    /// [`AmsFilamentSettingRequest::new`].
    pub ams_id: i32,
    /// Target tray/slot index — see the addressing cheat-sheet on [`AmsFilamentSettingRequest::new`].
    pub tray_id: i32,
    /// Standard filament preset index code (e.g. "GFL05" / "PF12345678901234567") [REF-AMS-SP_CFG].
    pub tray_info_idx: String,
    /// Material type string (e.g. "PLA", "PETG").
    pub tray_type: String,
    /// Sub-brand label (e.g. "Generic Basic"); defaults to `"{material_type} Basic"` when not given.
    pub tray_sub_brands: String,
    /// Structural hexadecimal color in RRGGBBAA format (e.g., "FFFF00FF").
    pub tray_color: String,
    /// Minimum safe nozzle temperature (°C) for this filament.
    pub nozzle_temp_min: u32,
    /// Maximum safe nozzle temperature (°C) for this filament.
    pub nozzle_temp_max: u32,
}

/// Sets filament properties (type, color, temperature range) on an AMS tray or external spool.
#[derive(Debug, Clone, Serialize)]
pub struct AmsFilamentSettingRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: AmsFilamentSettingPayload,
}

impl AmsFilamentSettingRequest {
    /// Creates a request payload to update slot parameters.
    ///
    /// **Polymorphic Tray Rule [REF-MQTT-LIFECYCLE]:**
    /// For standard physical slots, `ams_id` matches the expansion unit index (0-3).
    /// For the single-nozzle external spool slot, `ams_id` must strictly be set to `255`
    /// and `tray_id` must strictly be set to `254` to prevent command rejection.
    ///
    /// **IDEX External-Spool Addressing Cheat-Sheet [REF-MQTT-LIFECYCLE]:** external-spool
    /// addressing differs by command family — this rule is *not* the same one used by
    /// `extrusion_cali_sel` (K-profile binding, see
    /// [`crate::diagnostics::ExtrusionCaliSelRequest::new`]):
    /// * `ams_filament_setting` (this command) — Single-Nozzle Platforms: `ams_id: 255` /
    ///   `tray_id: 254`. Dual-Nozzle IDEX: Ext-L requires `ams_id: 254` / `tray_id: 0`;
    ///   Ext-R requires `ams_id: 255` / `tray_id: 0`.
    /// * `extrusion_cali_sel` — Single-Nozzle Platforms: `ams_id: 254` / `tray_id: 254`.
    ///   Dual-Nozzle IDEX: Ext-L requires `ams_id: 254` / `tray_id: 254`; Ext-R requires
    ///   `ams_id: 255` / `tray_id: 255`. **Warning:** targeting the wrong address for
    ///   Ext-R on IDEX machines mis-routes the pressure advance profile to the left
    ///   carriage (Ext-L) EEPROM, leaving the primary right carriage completely
    ///   uncalibrated.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ams_id: i32,
        tray_id: i32,
        preset_code: &str,
        material_type: &str,
        sub_brands: Option<&str>,
        color_hex: &str,
        temp_min: u32,
        temp_max: u32,
        sequence_id: u64,
    ) -> Self {
        let tray_sub_brands = match sub_brands {
            Some(s) => String::from(s),
            None => format!("{} Basic", material_type),
        };
        Self {
            print: AmsFilamentSettingPayload {
                command: "ams_filament_setting",
                sequence_id: clamp_task_id(sequence_id).to_string(),
                ams_id,
                tray_id,
                tray_info_idx: String::from(preset_code),
                tray_type: String::from(material_type),
                tray_sub_brands,
                tray_color: String::from(color_hex),
                nozzle_temp_min: temp_min,
                nozzle_temp_max: temp_max,
            },
        }
    }
}

/// Commands standard AMS controllers to resume, pause, or reset physical material feeds.
#[derive(Debug, Clone, Serialize)]
pub struct AmsControlPayload {
    /// Wire command name, always `"ams_control"`.
    pub command: &'static str,
    /// Target physical operation (e.g., "resume", "pause").
    pub param: String,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
}

/// Sends a resume, pause, or reset command to the AMS feed mechanism.
#[derive(Debug, Clone, Serialize)]
pub struct AmsControlRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: AmsControlPayload,
}

impl AmsControlRequest {
    /// Builds an `ams_control` request for the given operation ("resume", "pause", etc.).
    pub fn new(operation: &str, sequence_id: u64) -> Self {
        Self {
            print: AmsControlPayload {
                command: "ams_control",
                param: String::from(operation),
                sequence_id: clamp_task_id(sequence_id).to_string(),
            },
        }
    }
}

/// Triggers physical filament feeder movement to scan proprietary RFID tag properties.
#[derive(Debug, Clone, Serialize)]
pub struct AmsGetRfidPayload {
    /// Wire command name, always `"ams_get_rfid"`.
    pub command: &'static str,
    /// Target AMS unit index.
    pub ams_id: i32,
    /// Target slot index within the AMS unit.
    pub slot_id: i32,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
}

/// Requests an RFID tag scan on a specific AMS slot.
#[derive(Debug, Clone, Serialize)]
pub struct AmsGetRfidRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: AmsGetRfidPayload,
}

impl AmsGetRfidRequest {
    /// Builds an `ams_get_rfid` request.
    pub fn new(ams_id: i32, slot_id: i32, sequence_id: u64) -> Self {
        Self {
            print: AmsGetRfidPayload {
                command: "ams_get_rfid",
                ams_id,
                slot_id,
                sequence_id: clamp_task_id(sequence_id).to_string(),
            },
        }
    }
}

/// Triggers filament load or unload sequences on physical AMS units or virtual external spools [REF-AMS-MAP].
#[derive(Debug, Clone, Serialize)]
pub struct AmsChangeFilamentPayload {
    /// Wire command name, always `"ams_change_filament"`.
    pub command: &'static str,
    /// Target AMS unit index (or external-spool address per the caller's convention).
    pub ams_id: i32,
    /// Target slot index within the AMS unit.
    pub slot_id: i32,
    /// Load/unload destination (1 = toolhead load, 255 = unload/retract).
    pub target: i32,
    /// Current nozzle temperature (-1 = let firmware decide).
    pub curr_temp: i32,
    /// Target nozzle temperature (-1 = let firmware decide).
    pub tar_temp: i32,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
}

/// Loads or unloads filament from an AMS slot or external spool to the toolhead.
#[derive(Debug, Clone, Serialize)]
pub struct AmsChangeFilamentRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: AmsChangeFilamentPayload,
}

impl AmsChangeFilamentRequest {
    /// Builds an `ams_change_filament` request to load or unload filament.
    pub fn new(
        ams_id: i32,
        slot_id: i32,
        target: i32,
        curr_temp: i32,
        tar_temp: i32,
        sequence_id: u64,
    ) -> Self {
        Self {
            print: AmsChangeFilamentPayload {
                command: "ams_change_filament",
                ams_id,
                slot_id,
                target,
                curr_temp,
                tar_temp,
                sequence_id: clamp_task_id(sequence_id).to_string(),
            },
        }
    }
}

/// Initiates or terminates dry-chamber heating cycles on AMS 2 Pro and AMS-HT units [REF-AMS-DRYER].
#[derive(Debug, Clone, Serialize)]
pub struct AmsFilamentDryingPayload {
    /// Wire command name, always `"ams_filament_drying"`.
    pub command: &'static str,
    /// Target AMS unit index.
    pub ams_id: i32,
    /// 1 = start drying, 0 = stop drying.
    pub mode: i32,
    /// Drying temperature (°C).
    pub dry_temp: u32,
    /// Duration in **minutes** (e.g., 8-hour cycle = 480).
    pub dry_time: u32,
    /// Whether to periodically rotate the tray during drying.
    pub rotate_tray: bool,
    /// Filament material type being dried (e.g. "PA-CF").
    pub filament: String,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
}

/// Starts or stops a filament drying cycle on an AMS unit with a built-in heater.
#[derive(Debug, Clone, Serialize)]
pub struct AmsFilamentDryingRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: AmsFilamentDryingPayload,
}

impl AmsFilamentDryingRequest {
    /// Builds an `ams_filament_drying` request.
    pub fn new(
        ams_id: i32,
        mode: i32,
        dry_temp: u32,
        dry_time: u32,
        rotate_tray: bool,
        filament: &str,
        sequence_id: u64,
    ) -> Self {
        Self {
            print: AmsFilamentDryingPayload {
                command: "ams_filament_drying",
                ams_id,
                mode,
                dry_temp,
                dry_time,
                rotate_tray,
                filament: String::from(filament),
                sequence_id: clamp_task_id(sequence_id).to_string(),
            },
        }
    }
}
