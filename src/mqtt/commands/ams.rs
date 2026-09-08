//! AMS-related MQTT command payloads (filament change, drying, RFID scan, settings).

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use serde::Serialize;

use super::ClampedTaskId;

/// Normalizes a filament colour to the form the firmware actually stores.
///
/// Strips a leading `#` and uppercases the hex digits. The printer parses lowercase hex
/// letters in `tray_color` as `0` — measured on a P1S running firmware `01.10.00.00`, where
/// `09ff00ff` came back as `09000000` while `090000FF` survived intact — and the corruption is
/// silent, because the `ams_filament_setting` ack echoes what was sent and reports success.
/// An empty string stays empty. Mirrors bambuddy's single normalization point
/// (`bambu_mqtt.py:139`), which applies the same strip-and-uppercase.
fn normalize_tray_color(color_hex: &str) -> String {
    let trimmed = color_hex.trim();
    let trimmed = trimmed.strip_prefix('#').unwrap_or(trimmed);
    trimmed.to_uppercase()
}

/// Overwrites physical attributes or custom slicer presets assigned to a specific tray.
#[derive(Debug, Clone, Serialize)]
pub struct AmsFilamentSettingPayload {
    /// Wire command name, always `"ams_filament_setting"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
    /// Target AMS unit or external-spool address — see the addressing cheat-sheet on [`AmsFilamentSettingRequest::new`].
    pub ams_id: i32,
    /// Slot position within the unit, as supplied by the caller.
    ///
    /// Distinct from [`tray_id`](Self::tray_id), and both are sent: they coincide on a standard
    /// AMS but not on an external spool, where this stays `0` while `tray_id` is `254`.
    pub slot_id: i32,
    /// Derived addressing field — `254` for either external-spool `ams_id` (254/255), otherwise
    /// the slot index.
    ///
    /// Computed by [`AmsFilamentSettingRequest::new`] rather than caller-supplied, matching
    /// BambuStudio's `command_ams_filament_settings` (`DeviceManager.cpp:1707-1715`), so a
    /// caller cannot pair a `slot_id` with a `tray_id` that contradicts it.
    pub tray_id: i32,
    /// **Short-format** filament preset code, e.g. `"GFA01"` or `"GFL05"` [REF-AMS-SP_CFG].
    ///
    /// This is *not* where a long `"PF"`-prefixed preset id belongs — that goes in
    /// [`setting_id`](Self::setting_id), which is a separate wire field. Putting a 19-character
    /// cloud id here is what produced the "truncation" an A1 was measured doing: it stored only
    /// the first 8 characters, uppercased, while acking the command as `"success"`, after which
    /// the slot resolves to Generic and drops out of the calibration table (which is keyed on
    /// this field).
    ///
    /// Both upstreams agree on the split: BambuStudio's `command_ams_filament_settings`
    /// (`DeviceManager.cpp:1723-1724`) assigns `tray_info_idx = filament_id` and
    /// `setting_id = setting_id` as two separate keys, and bambuddy's `ams_set_filament_setting`
    /// documents this parameter as "Filament ID short format (e.g. `GFL05`)" against its own
    /// distinct `setting_id`.
    pub tray_info_idx: String,
    /// Material type string (e.g. "PLA", "PETG").
    pub tray_type: String,
    /// Sub-brand label (e.g. "Generic Basic"); defaults to `"{material_type} Basic"` when not given.
    pub tray_sub_brands: String,
    /// Structural hexadecimal color in RRGGBBAA format (e.g., "FFFF00FF").
    ///
    /// **Must be uppercase.** The firmware parses lowercase hex digits as `0` and stores the
    /// corrupted value silently — [`AmsFilamentSettingRequest::new`] normalizes for you.
    pub tray_color: String,
    /// Minimum safe nozzle temperature (°C) for this filament.
    pub nozzle_temp_min: u32,
    /// Maximum safe nozzle temperature (°C) for this filament.
    pub nozzle_temp_max: u32,
    /// Full preset identifier — the long form, e.g. `"GFSL05_07"` or a `"PF"`-prefixed id.
    ///
    /// Omitted from the wire when `None`, matching both upstreams: BambuStudio always sends the
    /// key, bambuddy includes it only when non-empty, and the firmware accepts its absence.
    /// Supplying it helps the slicer resolve the correct profile for the slot.
    ///
    /// Distinct from [`tray_info_idx`](Self::tray_info_idx), which takes the *short* code — see
    /// that field for what goes wrong when the two are conflated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setting_id: Option<String>,
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
    /// For standard physical slots, `ams_id` matches the expansion unit index (0-3). For an
    /// external spool, pass the virtual `ams_id` (`255` single-nozzle / Ext-R, `254` Ext-L)
    /// with `slot_id: 0`.
    ///
    /// **`slot_id` is what you pass; `tray_id` is derived.** Both reach the wire, and they
    /// differ on a virtual tray: `tray_id` becomes `254` for either external `ams_id` and the
    /// slot index otherwise. Deriving it here rather than accepting it means a caller cannot
    /// send a `slot_id`/`tray_id` pair that contradicts itself — the same reasoning as
    /// `PrinterClient::change_filament()` deriving `target`.
    ///
    /// Confirmed against BambuStudio's `command_ams_filament_settings`
    /// (`DeviceManager.cpp:1707-1722`), whose `tag_tray_id` maps either
    /// `VIRTUAL_TRAY_MAIN_ID`/`VIRTUAL_TRAY_DEPUTY_ID` to `254` and whose own call sites pass
    /// `slot_id: 0` for a virtual tray (`:4853`, `:4877`); and against bambuddy's
    /// `ams_set_filament_setting`, which sends `ams_id: 255`, `tray_id: 254`, `slot_id: 0` for
    /// a single external slot.
    ///
    /// **IDEX External-Spool Addressing Cheat-Sheet [REF-MQTT-LIFECYCLE]:** external-spool
    /// addressing differs by command family — this rule is *not* the same one used by
    /// `extrusion_cali_sel` (K-profile binding, see
    /// [`crate::diagnostics::ExtrusionCaliSelRequest::new`]):
    /// * `ams_filament_setting` (this command) — Single-Nozzle Platforms: `ams_id: 255` /
    ///   `tray_id: 254`. Dual-Nozzle IDEX: both Ext-L (`ams_id: 254`) and Ext-R
    ///   (`ams_id: 255`) require `tray_id: 254` (confirmed against
    ///   `command_ams_filament_settings`, `DeviceManager.cpp:1667-1693` — `tag_ams_id ==
    ///   VIRTUAL_TRAY_MAIN_ID(255) || VIRTUAL_TRAY_DEPUTY_ID(254)` always maps to
    ///   `tag_tray_id = VIRTUAL_TRAY_DEPUTY_ID(254)`, never `0`).
    /// * `extrusion_cali_sel` — Single-Nozzle Platforms: `ams_id: 254` / `tray_id: 254`.
    ///   Dual-Nozzle IDEX: Ext-L requires `ams_id: 254` / `tray_id: 254`; Ext-R requires
    ///   `ams_id: 255` / `tray_id: 255`. **Warning:** targeting the wrong address for
    ///   Ext-R on IDEX machines mis-routes the pressure advance profile to the left
    ///   carriage (Ext-L) EEPROM, leaving the primary right carriage completely
    ///   uncalibrated.
    ///
    /// **`color_hex` is normalized to uppercase**, with a leading `#` stripped. The printer
    /// parses lowercase hex letters in `tray_color` as `0` and the corruption is silent: the
    /// `ams_filament_setting` ack echoes the value that was sent and reports `result:
    /// "success"`, and only the next AMS push status reveals it (measured on a P1S running
    /// firmware `01.10.00.00` — `09ff00ff` stored as `09000000`, `090000FF` intact).
    /// `material_type` and `sub_brands` are deliberately left alone; case is meaningful there.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ams_id: i32,
        slot_id: i32,
        preset_code: &str,
        material_type: &str,
        sub_brands: Option<&str>,
        color_hex: &str,
        temp_min: u32,
        temp_max: u32,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Self {
        let tray_sub_brands = match sub_brands {
            Some(s) => String::from(s),
            None => format!("{} Basic", material_type),
        };
        Self {
            print: AmsFilamentSettingPayload {
                command: "ams_filament_setting",
                sequence_id: sequence_id.into().to_string(),
                ams_id,
                slot_id,
                // `254` for either external-spool address, the slot otherwise — BambuStudio's
                // `tag_tray_id` derivation, which never yields `0` for a virtual tray.
                tray_id: if ams_id == i32::from(crate::ams::parser::AMS_EXTERNAL_SPOOL_MAIN_ID)
                    || ams_id == i32::from(crate::ams::parser::AMS_EXTERNAL_SPOOL_DEPUTY_ID)
                {
                    i32::from(crate::ams::parser::AMS_EXTERNAL_SPOOL_DEPUTY_ID)
                } else {
                    slot_id
                },
                tray_info_idx: String::from(preset_code),
                tray_type: String::from(material_type),
                tray_sub_brands,
                tray_color: normalize_tray_color(color_hex),
                nozzle_temp_min: temp_min,
                nozzle_temp_max: temp_max,
                setting_id: None,
            },
        }
    }

    /// Attaches the full preset identifier, which is a separate wire field from
    /// `tray_info_idx` and is omitted entirely when not set.
    ///
    /// Follows the `with_*` convention [`PrintJobConfig`](super::PrintJobConfig) already uses,
    /// rather than a tenth positional argument on [`new`](Self::new).
    ///
    /// Pass the long form here — `"GFSL05_07"`, or a `"PF"`-prefixed id — and keep the short
    /// code in `tray_info_idx`. See [`AmsFilamentSettingPayload::tray_info_idx`] for what the
    /// printer does when a long id is put in the short field instead.
    #[must_use]
    pub fn with_setting_id(mut self, setting_id: &str) -> Self {
        self.print.setting_id = Some(String::from(setting_id));
        self
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
    pub fn new(operation: &str, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: AmsControlPayload {
                command: "ams_control",
                param: String::from(operation),
                sequence_id: sequence_id.into().to_string(),
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
    pub fn new(ams_id: i32, slot_id: i32, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: AmsGetRfidPayload {
                command: "ams_get_rfid",
                ams_id,
                slot_id,
                sequence_id: sequence_id.into().to_string(),
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
    /// Load/unload destination slot (confirmed against BambuStudio's
    /// `command_ams_change_filament`, `DeviceManager.cpp:1602-1638`): `255` on unload, the
    /// `ams_id` itself for AMS-HT/external-spool units (`ams_id >= 16`), or the flat global
    /// tray ID (`ams_id*4 + slot_id`) for a standard unit. Only coincidentally mirrors
    /// `slot_id` when `ams_id == 0` — see `PrinterClient::change_filament()`, which derives
    /// this field so callers can't misconfigure it.
    pub target: i32,
    /// Current nozzle temperature (-1 = let firmware decide).
    pub curr_temp: i32,
    /// Target nozzle temperature (-1 = let firmware decide).
    pub tar_temp: i32,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
    /// Which hotend to feed — `0` = right/main, `1` = left/deputy. Omitted from the wire when
    /// `None`, matching BambuStudio, whose `DeviceManager::command_ams_change_filament` takes
    /// it as an optional field and leaves it out unless a Filament Track Switch is fitted.
    ///
    /// **Required on a Filament Track Switch machine.** Without a switch each AMS is wired to
    /// exactly one hotend and the firmware derives the target from that binding, so naming it
    /// is redundant. With one fitted the situation inverts: every AMS reports its extruder as
    /// "not fixed" (`0xE`, see [`crate::types::telemetry::ExtruderInfo`]) and is plumbed into
    /// one of the switch's two inlets, from which it can reach either hotend — so a command
    /// naming neither extruder is **discarded in silence**. On an H2C that presents as load
    /// and unload simply doing nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extruder_id: Option<u8>,
}

/// Loads or unloads filament from an AMS slot or external spool to the toolhead.
#[derive(Debug, Clone, Serialize)]
pub struct AmsChangeFilamentRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: AmsChangeFilamentPayload,
}

impl AmsChangeFilamentRequest {
    /// Builds an `ams_change_filament` request to load or unload filament.
    ///
    /// Pass `extruder_id: None` on any printer without a Filament Track Switch — the wire
    /// payload is then byte-identical to the pre-FTS form. See
    /// [`AmsChangeFilamentPayload::extruder_id`] for why an FTS machine requires it.
    pub fn new(
        ams_id: i32,
        slot_id: i32,
        target: i32,
        curr_temp: i32,
        tar_temp: i32,
        extruder_id: Option<u8>,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Self {
        Self {
            print: AmsChangeFilamentPayload {
                command: "ams_change_filament",
                ams_id,
                slot_id,
                target,
                curr_temp,
                tar_temp,
                sequence_id: sequence_id.into().to_string(),
                extruder_id,
            },
        }
    }
}

/// Initiates or terminates dry-chamber heating cycles on AMS 2 Pro and AMS-HT units [REF-AMS-DRYER].
///
/// Field set and shapes rewritten to match the real wire protocol — confirmed
/// against BambuStudio's `DevFilaSystem::CtrlAmsStartDryingHour`/`CtrlAmsStopDrying`
/// (`DevFilaSystemCtrl.cpp:18-53`, the sole outbound `ams_filament_drying` constructor in the
/// tree) and independently corroborated by bambuddy's `send_drying_command`
/// (`bambu_mqtt.py:4141-4171`, whose own comment cites real-hardware silent-rejection
/// incident #1447).
#[derive(Debug, Clone, Serialize)]
pub struct AmsFilamentDryingPayload {
    /// Wire command name, always `"ams_filament_drying"`.
    pub command: &'static str,
    /// Target AMS unit index.
    pub ams_id: i32,
    /// 1 = start drying (`OnTime`), 0 = stop drying (`Off`) — `DevAms::DryCtrlMode`.
    pub mode: i32,
    /// Filament material type being dried (e.g. "PA-CF").
    pub filament: String,
    /// Drying temperature (°C).
    pub temp: u32,
    /// Drying duration in **hours** (e.g., an 8-hour cycle = 8) — the wire field, unlike the
    /// old `dry_time`, is not in minutes.
    pub duration: u32,
    /// Target humidity (0 = firmware default / no target).
    pub humidity: u32,
    /// Whether to periodically rotate the tray during drying.
    pub rotate_tray: bool,
    /// Cooling temperature applied after the drying cycle completes.
    pub cooling_temp: i32,
    /// Whether to override the AMS unit's power-conflict interlock.
    pub close_power_conflict: bool,
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
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ams_id: i32,
        mode: i32,
        filament: &str,
        temp: u32,
        duration_hours: u32,
        humidity: u32,
        rotate_tray: bool,
        cooling_temp: i32,
        close_power_conflict: bool,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Self {
        Self {
            print: AmsFilamentDryingPayload {
                command: "ams_filament_drying",
                ams_id,
                mode,
                filament: String::from(filament),
                temp,
                duration: duration_hours,
                humidity,
                rotate_tray,
                cooling_temp,
                close_power_conflict,
                sequence_id: sequence_id.into().to_string(),
            },
        }
    }
}
