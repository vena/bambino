//! AMS-related MQTT command payloads (filament change, drying, RFID scan, settings).

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use serde::Serialize;

use super::ClampedTaskId;
use crate::error::Error;

/// Normalizes a filament colour to the `RRGGBBAA` form the firmware stores, or rejects it.
///
/// Strips a leading `#`, appends an opaque `FF` alpha to a 6-digit `RRGGBB`, and uppercases the
/// hex digits. The printer parses lowercase hex letters in `tray_color` as `0` — measured on a
/// P1S running firmware `01.10.00.00`, where `09ff00ff` came back as `09000000` while
/// `090000FF` survived intact — and the corruption is silent, because the
/// `ams_filament_setting` ack echoes what was sent and reports success. Mirrors bambuddy's
/// single normalization point (`bambu_mqtt.py`), which applies the same strip-and-uppercase.
///
/// `reference/05_materials_ams.md` defines the field as 8 hex digits; anything else is an
/// [`Error::InvalidArgument`] rather than a value the printer would misread.
fn normalize_tray_color(color_hex: &str) -> Result<String, Error> {
    let trimmed = color_hex.trim();
    let digits = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::InvalidArgument(
            format!("tray color {color_hex:?} is not hex").into(),
        ));
    }
    match digits.len() {
        8 => Ok(digits.to_ascii_uppercase()),
        6 => Ok(format!("{}FF", digits.to_ascii_uppercase())),
        _ => Err(Error::InvalidArgument(
            format!("tray color {color_hex:?} must be RRGGBB or RRGGBBAA").into(),
        )),
    }
}

/// The description an `ams_filament_setting` can't do without.
///
/// Required by [`AmsFilamentSettingRequest::new`]: sending the command without them writes an
/// empty material with a 0–0 °C nozzle window to the tray, and the printer acks it as success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilamentSpec<'a> {
    /// **Short-format** filament preset code, e.g. `"GFA01"` — see
    /// [`AmsFilamentSettingPayload::tray_info_idx`].
    pub preset: &'a str,
    /// Material type, e.g. `"PLA"`.
    pub material: &'a str,
    /// Minimum safe nozzle temperature, °C.
    pub nozzle_temp_min: u32,
    /// Maximum safe nozzle temperature, °C.
    pub nozzle_temp_max: u32,
}

/// Overwrites physical attributes or custom slicer presets assigned to a specific tray.
#[derive(Debug, Clone, Serialize)]
pub struct AmsFilamentSettingPayload {
    /// Wire command name, always `"ams_filament_setting"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
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
    /// BambuStudio's `command_ams_filament_settings` (`DeviceManager.cpp`), so a
    /// caller cannot pair a `slot_id` with a `tray_id` that contradicts it.
    pub tray_id: i32,
    /// **Short-format** filament preset code, e.g. `"GFA01"` or `"GFL05"` [REF-AMS-SP_CFG].
    ///
    /// Set from [`FilamentSpec::preset`].
    ///
    /// This is *not* where a long `"PF"`-prefixed preset id belongs — that goes in
    /// [`setting_id`](Self::setting_id), which is a separate wire field. Putting a 19-character
    /// cloud id here is what produced the "truncation" an A1 was measured doing: it stored only
    /// the first 8 characters, uppercased, while acking the command as `"success"`, after which
    /// the slot resolves to Generic and drops out of the calibration table (which is keyed on
    /// this field).
    ///
    /// Both upstreams agree on the split: BambuStudio's `command_ams_filament_settings`
    /// (`DeviceManager.cpp`) assigns `tray_info_idx = filament_id` and
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
    /// corrupted value silently — [`AmsFilamentSettingRequest::with_color`] normalizes for you.
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
pub type AmsFilamentSettingRequest = super::Print<AmsFilamentSettingPayload>;

impl AmsFilamentSettingRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "ams_filament_setting";

    /// Creates a request payload to update slot parameters.
    ///
    /// **Polymorphic Tray Rule [REF-MQTT-LIFECYCLE]:**
    /// For standard physical slots, `ams_id` matches the expansion unit index (0-3). For an
    /// external spool, pass the virtual `ams_id` (`255` single-nozzle / Ext-R, `254` Ext-L)
    /// with `slot_id: 0`. An A2L-attached AMS Lite takes its physical wire id `16` with a local
    /// `0..=3` slot, not the normalized `6` telemetry reports it as (bambuddy
    /// `ams_set_filament_setting`, matching the firmware's own `ams_mapping2`).
    ///
    /// **`slot_id` is what you pass; `tray_id` is derived.** Both reach the wire, and they
    /// differ on a virtual tray: `tray_id` becomes `254` for either external `ams_id` and the
    /// slot index otherwise. Deriving it here rather than accepting it means a caller cannot
    /// send a `slot_id`/`tray_id` pair that contradicts itself — the same reasoning as
    /// `PrinterClient::change_filament()` deriving `target`.
    ///
    /// Confirmed against BambuStudio's `command_ams_filament_settings`
    /// (`DeviceManager.cpp`), whose `tag_tray_id` maps either
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
    ///   `command_ams_filament_settings`, `DeviceManager.cpp` — `tag_ams_id ==
    ///   VIRTUAL_TRAY_MAIN_ID(255) || VIRTUAL_TRAY_DEPUTY_ID(254)` always maps to
    ///   `tag_tray_id = VIRTUAL_TRAY_DEPUTY_ID(254)`, never `0`).
    /// * `extrusion_cali_sel` — Single-Nozzle Platforms: `ams_id: 254` / `tray_id: 254`.
    ///   Dual-Nozzle IDEX: Ext-L requires `ams_id: 254` / `tray_id: 254`; Ext-R requires
    ///   `ams_id: 255` / `tray_id: 255`. **Warning:** targeting the wrong address for
    ///   Ext-R on IDEX machines mis-routes the pressure advance profile to the left
    ///   carriage (Ext-L) EEPROM, leaving the primary right carriage completely
    ///   uncalibrated.
    ///
    /// The filament's essential description is the named [`FilamentSpec`]; the genuinely
    /// optional fields (color, sub-brand, long setting id) are `with_*` methods. Named fields
    /// rather than positional arguments because the temperature bounds were adjacent `u32`s,
    /// transposable without a compile error on a command whose failures are already silent.
    ///
    /// Unset optional fields: an empty color, a `"{material} Basic"` sub-brand, and no
    /// `setting_id` on the wire.
    pub fn new(
        ams_id: i32,
        slot_id: i32,
        filament: FilamentSpec<'_>,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Self {
        Self {
            print: AmsFilamentSettingPayload {
                command: Self::COMMAND,
                sequence_id: sequence_id.into(),
                ams_id,
                slot_id,
                // `254` for either external-spool address, the slot otherwise — BambuStudio's
                // `tag_tray_id` derivation, which never yields `0` for a virtual tray.
                tray_id: if ams_id == i32::from(crate::ams::ids::AMS_EXTERNAL_SPOOL_MAIN_ID)
                    || ams_id == i32::from(crate::ams::ids::AMS_EXTERNAL_SPOOL_DEPUTY_ID)
                {
                    i32::from(crate::ams::ids::AMS_EXTERNAL_SPOOL_DEPUTY_ID)
                } else {
                    slot_id
                },
                tray_info_idx: String::from(filament.preset),
                tray_type: String::from(filament.material),
                tray_sub_brands: format!("{} Basic", filament.material),
                tray_color: String::new(),
                nozzle_temp_min: filament.nozzle_temp_min,
                nozzle_temp_max: filament.nozzle_temp_max,
                setting_id: None,
            },
        }
    }

    /// Overrides the sub-brand label (default `"{material} Basic"`). Case is meaningful and is left alone.
    #[must_use]
    pub fn with_sub_brands(mut self, sub_brands: &str) -> Self {
        self.print.tray_sub_brands = String::from(sub_brands);
        self
    }

    /// Sets the tray color from `RRGGBB` or `RRGGBBAA` hex, optionally `#`-prefixed.
    ///
    /// Normalized to the 8-digit uppercase form the firmware stores: a 6-digit color gets an
    /// opaque `FF` alpha, and lowercase digits are uppercased. The printer parses lowercase hex
    /// letters in `tray_color` as `0` and the corruption is silent: the `ams_filament_setting`
    /// ack echoes the value that was sent and reports `result: "success"`, and only the next
    /// AMS push status reveals it (measured on a P1S running firmware `01.10.00.00` —
    /// `09ff00ff` stored as `09000000`, `090000FF` intact).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidArgument`] for anything that isn't 6 or 8 hex digits.
    pub fn with_color(mut self, color_hex: &str) -> Result<Self, Error> {
        self.print.tray_color = normalize_tray_color(color_hex)?;
        Ok(self)
    }

    /// Attaches the full preset identifier, which is a separate wire field from
    /// `tray_info_idx` and is omitted entirely when not set.
    ///
    /// Pass the long form here — `"GFSL05_07"`, or a `"PF"`-prefixed id — and keep the short
    /// code in [`FilamentSpec::preset`]. See
    /// [`AmsFilamentSettingPayload::tray_info_idx`] for what the printer does when a long id is
    /// put in the short field instead.
    #[must_use]
    pub fn with_setting_id(mut self, setting_id: &str) -> Self {
        self.print.setting_id = Some(String::from(setting_id));
        self
    }
}

/// An `ams_control` operation on the AMS feed mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AmsControlOp {
    /// Resume feeding (`resume`).
    Resume,
    /// Pause feeding (`pause`).
    Pause,
    /// Reset the feed state (`reset`).
    Reset,
}

impl AmsControlOp {
    /// The wire `param` value.
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            AmsControlOp::Resume => "resume",
            AmsControlOp::Pause => "pause",
            AmsControlOp::Reset => "reset",
        }
    }
}

/// Commands standard AMS controllers to resume, pause, or reset physical material feeds.
#[derive(Debug, Clone, Serialize)]
pub struct AmsControlPayload {
    /// Wire command name, always `"ams_control"`.
    pub command: &'static str,
    /// Target operation — see [`AmsControlOp`].
    pub param: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
}

/// Sends a resume, pause, or reset command to the AMS feed mechanism.
pub type AmsControlRequest = super::Print<AmsControlPayload>;

impl AmsControlRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "ams_control";

    /// Builds an `ams_control` request for `operation`.
    pub fn new(operation: AmsControlOp, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: AmsControlPayload {
                command: Self::COMMAND,
                param: operation.as_wire(),
                sequence_id: sequence_id.into(),
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
    pub sequence_id: ClampedTaskId,
}

/// Requests an RFID tag scan on a specific AMS slot.
pub type AmsGetRfidRequest = super::Print<AmsGetRfidPayload>;

impl AmsGetRfidRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "ams_get_rfid";

    /// Builds an `ams_get_rfid` request.
    pub fn new(ams_id: i32, slot_id: i32, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: AmsGetRfidPayload {
                command: Self::COMMAND,
                ams_id,
                slot_id,
                sequence_id: sequence_id.into(),
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
    /// Load/unload destination slot, derived by [`AmsChangeFilamentRequest::load`]/`unload` —
    /// see there.
    pub target: i32,
    /// Current nozzle temperature (-1 = let firmware decide).
    pub curr_temp: i32,
    /// Target nozzle temperature (-1 = let firmware decide).
    pub tar_temp: i32,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
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
pub type AmsChangeFilamentRequest = super::Print<AmsChangeFilamentPayload>;

impl AmsChangeFilamentRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "ams_change_filament";

    /// Builds a request loading slot `slot_id` of the unit at wire address `ams_id`.
    ///
    /// `target` is derived, never caller-supplied, per BambuStudio's
    /// `command_ams_change_filament` (`DeviceManager.cpp`): the `ams_id` itself for
    /// any unit at wire address 16 or above (an A2L's AMS Lite, AMS-HT, an external spool), or
    /// the flat global tray (`ams_id * 4 + slot_id`) for a standard unit. A caller-supplied
    /// `target` that didn't match was a real hardware misconfiguration risk (`07FF_8012` class);
    /// it mirrored `slot_id` only coincidentally, for `ams_id: 0`.
    ///
    /// `ams_id` is the *wire* address — an A2L's AMS Lite is `16` here, not the `6` telemetry
    /// reports. `PrinterClient::change_filament` validates the address and converts it.
    ///
    /// Pass `extruder_id: None` on any printer without a Filament Track Switch — see
    /// [`AmsChangeFilamentPayload::extruder_id`] for why an FTS machine requires it.
    pub fn load(
        ams_id: u8,
        slot_id: u8,
        temps: ChangeTemps,
        extruder_id: Option<u8>,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Self {
        let target = if ams_id >= crate::ams::ids::AMS_LITE_ON_A2L_PHYSICAL_ID {
            ams_id
        } else {
            ams_id * crate::ams::ids::AMS_SLOTS_PER_UNIT + slot_id
        };
        Self::build(ams_id, slot_id, target, temps, extruder_id, sequence_id)
    }

    /// Builds a request unloading (retracting) the filament fed from the unit at wire address `ams_id`.
    ///
    /// `slot_id` and `target` are both the `255` unload sentinel, as in BambuStudio.
    pub fn unload(
        ams_id: u8,
        temps: ChangeTemps,
        extruder_id: Option<u8>,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Self {
        let unload = crate::ams::ids::SLOT_UNLOAD;
        Self::build(ams_id, unload, unload, temps, extruder_id, sequence_id)
    }

    fn build(
        ams_id: u8,
        slot_id: u8,
        target: u8,
        temps: ChangeTemps,
        extruder_id: Option<u8>,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Self {
        Self {
            print: AmsChangeFilamentPayload {
                command: Self::COMMAND,
                ams_id: i32::from(ams_id),
                slot_id: i32::from(slot_id),
                target: i32::from(target),
                curr_temp: temps.current,
                tar_temp: temps.target,
                sequence_id: sequence_id.into(),
                extruder_id,
            },
        }
    }
}

/// The nozzle temperatures an `ams_change_filament` carries, °C; `-1` lets the firmware decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChangeTemps {
    /// Current nozzle temperature (`curr_temp`).
    pub current: i32,
    /// Target nozzle temperature (`tar_temp`).
    pub target: i32,
}

impl ChangeTemps {
    /// Both temperatures left to the firmware (`-1`).
    pub const FIRMWARE: Self = Self {
        current: -1,
        target: -1,
    };
}

/// Initiates or terminates dry-chamber heating cycles on AMS 2 Pro and AMS-HT units [REF-AMS-DRYER].
///
/// Field set and shapes rewritten to match the real wire protocol — confirmed
/// against BambuStudio's `DevFilaSystem::CtrlAmsStartDryingHour`/`CtrlAmsStopDrying`
/// (`DevFilaSystemCtrl.cpp`, the sole outbound `ams_filament_drying` constructor in the
/// tree) and independently corroborated by bambuddy's `send_drying_command`
/// (`bambu_mqtt.py`, whose own comment cites real-hardware silent-rejection
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
    pub cooling_temp: u32,
    /// Whether to override the AMS unit's power-conflict interlock.
    pub close_power_conflict: bool,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
}

/// Starts or stops a filament drying cycle on an AMS unit with a built-in heater.
pub type AmsFilamentDryingRequest = super::Print<AmsFilamentDryingPayload>;

/// `ams_filament_drying` mode that starts a cycle (`DevAms::DryCtrlMode::OnTime`).
const DRYING_MODE_START: i32 = 1;
/// `ams_filament_drying` mode that stops a cycle (`DevAms::DryCtrlMode::Off`).
const DRYING_MODE_STOP: i32 = 0;

/// Everything a drying-cycle start carries besides the unit and the mode.
///
/// `Default` is the all-zero/empty set BambuStudio sends to stop a cycle; a start needs at
/// least `temp` and `duration_hours`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DryingParams {
    /// Filament material type being dried (e.g. "PA-CF").
    pub filament: String,
    /// Drying temperature (°C).
    pub temp: u32,
    /// Drying duration in whole hours.
    pub duration_hours: u32,
    /// Target humidity (0 = firmware default / no target).
    pub humidity: u32,
    /// Whether to periodically rotate the tray during drying.
    pub rotate_tray: bool,
    /// Cooling temperature applied after the cycle; BambuStudio sends the filament's
    /// softening temperature here.
    pub cooling_temp: u32,
    /// Whether to override the AMS unit's power-conflict interlock.
    pub close_power_conflict: bool,
}

impl AmsFilamentDryingRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "ams_filament_drying";

    /// Builds a request starting a drying cycle on the unit at `ams_id`.
    pub fn start(ams_id: i32, params: DryingParams, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self::build(ams_id, DRYING_MODE_START, params, sequence_id)
    }

    /// Builds a request stopping the drying cycle on the unit at `ams_id`.
    ///
    /// Mirrors BambuStudio's `CtrlAmsStopDrying` (`DevFilaSystemCtrl.cpp`): every field
    /// but the unit and mode zeroed.
    pub fn stop(ams_id: i32, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self::build(
            ams_id,
            DRYING_MODE_STOP,
            DryingParams::default(),
            sequence_id,
        )
    }

    fn build(
        ams_id: i32,
        mode: i32,
        params: DryingParams,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Self {
        Self {
            print: AmsFilamentDryingPayload {
                command: Self::COMMAND,
                ams_id,
                mode,
                filament: params.filament,
                temp: params.temp,
                duration: params.duration_hours,
                humidity: params.humidity,
                rotate_tray: params.rotate_tray,
                cooling_temp: params.cooling_temp,
                close_power_conflict: params.close_power_conflict,
                sequence_id: sequence_id.into(),
            },
        }
    }
}
