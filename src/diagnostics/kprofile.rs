//! # Linear Advance (Pressure Advance / K-Profile) Calibration Database Builders
//!
//! Exposes command serialization schemas and validation checks to manage stored
//! pressure-advance calibration profiles on the printer's onboard EEPROM [REF-DIAG-KPROF].
//!
//! ## Structural Guidelines & Constraints
//! * **Setting ID Validation**: Enforces the 19-character numeric `setting_id` boundary
//!   (`"PF"` followed by exactly 17 decimal digits) to prevent memory table corruption in the local
//!   EEPROM partition database.
//! * **Polymorphic Deletions**: Separates deletion schemas cleanly between standard single-nozzle
//!   platforms (keyed on `setting_id`) and dual-nozzle IDEX platforms (keyed on coordinate/carriage parameters).

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "std"))]
use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::mqtt::commands::ClampedTaskId;

/// Validates whether a provided calibration profile setting ID complies with EEPROM limits.
///
/// **The Calibration Setting ID Boundary Rule [REF-DIAG-KPROF]:**
/// Stored EEPROM K-profiles require standard 19-character numeric formats consisting of a
/// `"PF"` header prefix followed by exactly 17 numeric digits. Standard alphanumeric hashes
/// (e.g. `"PFUS9be9e18f81828a"`) are strictly reserved for slicer-side presets.
/// Transmitting alphanumeric layouts inside direct database operations causes indexing halts
/// or table corruption on the physical mainboard.
#[must_use]
pub fn is_setting_id_valid(setting_id: &str) -> bool {
    if !setting_id.starts_with("PF") {
        return false;
    }
    let digits = &setting_id[2..];
    digits.len() == 17 && digits.chars().all(|c| c.is_ascii_digit())
}

/// Returns `Ok(())` if `setting_id` passes [`is_setting_id_valid`], otherwise the shared
/// `ProtocolViolation` both `ExtrusionCaliSetRequest::new` and `StandardCaliDelRequest::new`
/// construct on rejection.
fn ensure_valid_setting_id(setting_id: &str) -> Result<(), Error> {
    if is_setting_id_valid(setting_id) {
        Ok(())
    } else {
        Err(Error::ProtocolViolation(
            "Setting ID violates the strict 19-character numeric calibration boundary rule".into(),
        ))
    }
}

// ============================================================================
// 1. Database Representation Structs
// ============================================================================

/// Structured representation of a Linear Advance calibration profile entry on the printer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KProfileEntry {
    /// Database index corresponding to the stored slot (-1 indicates a fresh write).
    pub cali_idx: i32,
    /// Preset identifier associated with the base filament category (e.g. `"GFA01"`).
    pub filament_id: String,
    /// Physical orifice size matching the calibrated tool (e.g. `"0.4"`).
    ///
    /// Single-nozzle firmware omits this field per-entry (it only sets it once at the
    /// `ExtrusionCaliGetResponsePayload` envelope level) — callers reading a parsed response
    /// must fall back to the envelope's `nozzle_diameter` when this is `None`.
    ///
    /// `skip_serializing_if` matters on the write side: this same struct is the element type
    /// of `extrusion_cali_set`'s `filaments` array, so round-tripping an entry read back from
    /// single-nozzle firmware would otherwise emit `"nozzle_diameter":null` — a shape neither
    /// the read side nor `reference/07_diagnostics_hms.md` §7.2 ever shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nozzle_diameter: Option<String>,
    /// System designation of the target hotend profile structure (e.g. `"HS00-0.4"`).
    pub nozzle_id: String,
    /// Carriage layout indicator (0 = Right/Primary extruder, 1 = Left/Deputy extruder).
    pub extruder_id: u8,
    /// Custom user-defined name assigned to label the profile slot.
    pub name: String,
    /// Calibrated Linear Advance constant serialized as a float string.
    pub k_value: String,
    /// Extrusion coefficient parameters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n_coef: Option<String>,
    /// Secure 19-character unique setting identifier.
    pub setting_id: String,
    /// Links K-profile to AMS unit (default 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ams_id: Option<i32>,
    /// Links K-profile to AMS tray slot (default -1). At least X1C firmware spuriously
    /// reports `result: "fail"` for `extrusion_cali` writes using `tray_id: -1` even though
    /// the write still applies — don't treat that ack `result` as authoritative for a
    /// `tray_id: -1` write without cross-checking the profile actually landed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tray_id: Option<i32>,
}

// ============================================================================
// 2. Query Calibration Database Commands (extrusion_cali_get)
// ============================================================================

/// Inner payload for [`ExtrusionCaliGetRequest`].
#[derive(Debug, Clone, Serialize)]
pub struct ExtrusionCaliGetPayload {
    /// Wire command name, always `"extrusion_cali_get"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
}

/// JSON request wrapper to trigger a complete dump of the stored calibration database.
///
/// # Firmware Quirk: Priming Required [REF-DIAG-KPROF]
///
/// The firmware ignores the first `extrusion_cali_get` command received after MQTTS
/// connection establishment. A dummy "priming" request must be sent first before the
/// real query will receive a response. `PrinterClient::get_k_profiles()` handles this
/// automatically — use `set_k_profile_primed(true)` to opt out if you manage priming
/// yourself.
#[derive(Debug, Clone, Serialize)]
pub struct ExtrusionCaliGetRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: ExtrusionCaliGetPayload,
}

impl ExtrusionCaliGetRequest {
    /// Builds an `extrusion_cali_get` request.
    /// Callers should prefer `PrinterClient::get_k_profiles()`, which handles the priming quirk
    /// documented above.
    pub fn new(sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: ExtrusionCaliGetPayload {
                command: "extrusion_cali_get",
                sequence_id: sequence_id.into().to_string(),
            },
        }
    }
}

// ============================================================================
// 2b. Query Calibration Database Response (extrusion_cali_get reply)
// ============================================================================

/// Payload envelope returned by the printer in response to `extrusion_cali_get`.
#[derive(Debug, Clone, Deserialize)]
pub struct ExtrusionCaliGetResponsePayload {
    /// Echo of the command name (`"extrusion_cali_get"`).
    pub command: String,
    /// Echo of the original request sequence identifier.
    pub sequence_id: String,
    /// Nozzle diameter filter applied to the returned profile set.
    #[serde(default)]
    pub nozzle_diameter: Option<String>,
    /// Complete array of stored calibration profiles matching the active nozzle.
    #[serde(default)]
    pub filaments: Vec<KProfileEntry>,
}

/// JSON response wrapper containing the printer's stored calibration profile database.
#[derive(Debug, Clone, Deserialize)]
pub struct ExtrusionCaliGetResponse {
    /// The `print` namespace envelope wrapping the returned calibration data.
    pub print: ExtrusionCaliGetResponsePayload,
}

// ============================================================================
// 3. Save or Write Calibration Commands (extrusion_cali_set)
// ============================================================================

/// Inner payload for [`ExtrusionCaliSetRequest`].
#[derive(Debug, Clone, Serialize)]
pub struct ExtrusionCaliSetPayload {
    /// Wire command name, always `"extrusion_cali_set"`.
    pub command: &'static str,
    /// Calibration profile entries to write. Multiple entries support IDEX multi-nozzle writes.
    pub filaments: Vec<KProfileEntry>,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
}

/// JSON request wrapper to create or overwrite calibration profile allocations.
#[derive(Debug, Clone, Serialize)]
pub struct ExtrusionCaliSetRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: ExtrusionCaliSetPayload,
}

impl ExtrusionCaliSetRequest {
    /// Builds a secure write-transaction payload targeting physical EEPROM slots.
    ///
    /// Verifies that all target profiles carry valid setting identifiers to protect local
    /// database health. Supports multi-profile writes for IDEX platforms.
    pub fn new(
        profiles: Vec<KProfileEntry>,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Result<Self, Error> {
        for profile in &profiles {
            ensure_valid_setting_id(&profile.setting_id)?;
        }

        Ok(Self {
            print: ExtrusionCaliSetPayload {
                command: "extrusion_cali_set",
                filaments: profiles,
                sequence_id: sequence_id.into().to_string(),
            },
        })
    }
}

// ============================================================================
// 3b. Bind Calibration Profile to AMS Slot (extrusion_cali_sel)
// ============================================================================

/// Inner payload for [`ExtrusionCaliSelRequest`].
#[derive(Debug, Clone, Serialize)]
pub struct ExtrusionCaliSelPayload {
    /// Wire command name, always `"extrusion_cali_sel"`.
    pub command: &'static str,
    /// Target AMS/external-spool address — see the addressing cheat-sheet on [`ExtrusionCaliSelRequest::new`].
    pub ams_id: i32,
    /// Absolute global tray ID (not local slot index).
    pub tray_id: i32,
    /// Index of the calibration entry within the target's profile database (`KProfileEntry::cali_idx`).
    pub cali_idx: i32,
    /// Filament preset ID this K-profile applies to (`KProfileEntry::filament_id`).
    pub filament_id: String,
    /// Nozzle diameter this K-profile applies to (`KProfileEntry::nozzle_diameter`).
    pub nozzle_diameter: String,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
}

/// JSON request wrapper to bind a stored K-profile calibration entry to an AMS material slot [REF-AMS-MAP].
///
/// The `setting_id` field is intentionally omitted from this payload to prevent
/// database mislinking on the motion board.
#[derive(Debug, Clone, Serialize)]
pub struct ExtrusionCaliSelRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: ExtrusionCaliSelPayload,
}

impl ExtrusionCaliSelRequest {
    /// Creates a request payload to bind a stored K-profile calibration entry to an AMS
    /// material slot.
    ///
    /// **IDEX External-Spool Addressing Cheat-Sheet [REF-MQTT-LIFECYCLE]:** external-spool
    /// addressing differs by command family — this rule is *not* the same one used by
    /// `ams_filament_setting` (filament configuration, see
    /// [`crate::mqtt::AmsFilamentSettingRequest::new`]):
    /// * `extrusion_cali_sel` (this command) — Single-Nozzle Platforms: `ams_id: 254` /
    ///   `tray_id: 254`. Dual-Nozzle IDEX: Ext-L requires `ams_id: 254` / `tray_id: 254`;
    ///   Ext-R requires `ams_id: 255` / `tray_id: 255`. **Warning:** targeting the wrong
    ///   address for Ext-R on IDEX machines mis-routes the pressure advance profile to
    ///   the left carriage (Ext-L) EEPROM, leaving the primary right carriage completely
    ///   uncalibrated.
    /// * `ams_filament_setting` — Single-Nozzle Platforms: `ams_id: 255` / `tray_id: 254`.
    ///   Dual-Nozzle IDEX: both Ext-L (`ams_id: 254`) and Ext-R (`ams_id: 255`) require
    ///   `tray_id: 254`, never `0` (BUG-117 / BambuStudio `DeviceManager.cpp:1667-1693`).
    pub fn new(
        ams_id: i32,
        tray_id: i32,
        cali_idx: i32,
        filament_id: &str,
        nozzle_diameter: &str,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Self {
        Self {
            print: ExtrusionCaliSelPayload {
                command: "extrusion_cali_sel",
                ams_id,
                tray_id,
                cali_idx,
                filament_id: String::from(filament_id),
                nozzle_diameter: String::from(nozzle_diameter),
                sequence_id: sequence_id.into().to_string(),
            },
        }
    }
}

// ============================================================================
// 4. Delete Profile Commands (extrusion_cali_del)
// ============================================================================

/// Deletion data fields utilized by standard single-nozzle databases (Schema A).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StandardCaliDelEntry {
    /// Index of the calibration entry to delete (`KProfileEntry::cali_idx`).
    pub cali_idx: i32,
    /// Filament preset ID of the entry being deleted (`KProfileEntry::filament_id`).
    pub filament_id: String,
    /// Nozzle diameter of the entry being deleted (`KProfileEntry::nozzle_diameter`).
    pub nozzle_diameter: String,
    /// System nozzle profile designation of the entry being deleted (`KProfileEntry::nozzle_id`).
    pub nozzle_id: String,
    /// 19-character setting ID of the entry being deleted, validated by [`is_setting_id_valid`].
    pub setting_id: String,
}

/// Deletion coordinate metrics utilized by dual-nozzle IDEX databases (Schema B).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IdexCaliDelEntry {
    /// Nozzle diameter of the entry being deleted (`KProfileEntry::nozzle_diameter`).
    pub nozzle_diameter: String,
    /// System nozzle profile designation of the entry being deleted (`KProfileEntry::nozzle_id`).
    pub nozzle_id: String,
    /// Carriage index of the entry being deleted (0 = Right/Primary, 1 = Left/Deputy).
    pub extruder_id: u8,
}

/// Inner payload for [`StandardCaliDelRequest`].
#[derive(Debug, Clone, Serialize)]
pub struct StandardCaliDelPayload {
    /// Wire command name, always `"extrusion_cali_del"`.
    pub command: &'static str,
    /// Entries to delete. `StandardCaliDelRequest::new` always sends exactly one.
    pub filaments: Vec<StandardCaliDelEntry>,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
}

/// JSON request wrapper targeting single-nozzle profile deletions (Schema A) [REF-DIAG-KPROF].
#[derive(Debug, Clone, Serialize)]
pub struct StandardCaliDelRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: StandardCaliDelPayload,
}

impl StandardCaliDelRequest {
    /// Builds a single-nozzle deletion transaction keyed on the setting identifier.
    pub fn new(
        target: StandardCaliDelEntry,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Result<Self, Error> {
        ensure_valid_setting_id(&target.setting_id)?;

        Ok(Self {
            print: StandardCaliDelPayload {
                command: "extrusion_cali_del",
                filaments: vec![target],
                sequence_id: sequence_id.into().to_string(),
            },
        })
    }
}

/// Inner payload for [`IdexCaliDelRequest`].
#[derive(Debug, Clone, Serialize)]
pub struct IdexCaliDelPayload {
    /// Wire command name, always `"extrusion_cali_del"`.
    pub command: &'static str,
    /// Entries to delete. `IdexCaliDelRequest::new` always sends exactly one.
    pub filaments: Vec<IdexCaliDelEntry>,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
}

/// JSON request wrapper targeting dual-nozzle IDEX profile deletions (Schema B) [REF-DIAG-KPROF].
#[derive(Debug, Clone, Serialize)]
pub struct IdexCaliDelRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: IdexCaliDelPayload,
}

impl IdexCaliDelRequest {
    /// Builds a dual-nozzle carriage deletion transaction keyed on physical coordinates.
    pub fn new(target: IdexCaliDelEntry, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: IdexCaliDelPayload {
                command: "extrusion_cali_del",
                filaments: vec![target],
                sequence_id: sequence_id.into().to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setting_id_validator() {
        // Correct format: "PF" followed by exactly 17 digits (total length 19)
        assert!(is_setting_id_valid("PF12345678901234567"));

        // Alphanumeric hash used by slicer presets must fail
        assert!(!is_setting_id_valid("PFUS9be9e18f81828a"));

        // Short decimal block
        assert!(!is_setting_id_valid("PF123456"));

        // Missing prefix
        assert!(!is_setting_id_valid("1234567890123456789"));
    }

    #[test]
    fn test_extrusion_cali_get_json() {
        let req = ExtrusionCaliGetRequest::new(50001);
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(
            json,
            r#"{"print":{"command":"extrusion_cali_get","sequence_id":"50001"}}"#
        );
    }

    #[test]
    fn test_extrusion_cali_set_invalid_id() {
        let bad_profile = KProfileEntry {
            cali_idx: -1,
            filament_id: "GFA01".into(),
            nozzle_diameter: Some("0.4".into()),
            nozzle_id: "HS00-0.4".into(),
            extruder_id: 0,
            name: "Faulty ID".into(),
            k_value: "0.022".into(),
            n_coef: None,
            setting_id: "PF_BAD_ALPHANUM_KEY".into(),
            ams_id: None,
            tray_id: None,
        };

        let result = ExtrusionCaliSetRequest::new(vec![bad_profile], 50002);
        assert!(result.is_err());
    }

    #[test]
    fn test_extrusion_cali_set_valid_id() {
        let good_profile = KProfileEntry {
            cali_idx: -1,
            filament_id: "GFA01".into(),
            nozzle_diameter: Some("0.4".into()),
            nozzle_id: "HS00-0.4".into(),
            extruder_id: 0,
            name: "Good ID".into(),
            k_value: "0.022".into(),
            n_coef: None,
            setting_id: "PF12345678901234567".into(),
            ams_id: None,
            tray_id: None,
        };

        let req = ExtrusionCaliSetRequest::new(vec![good_profile], 50002).unwrap();
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("extrusion_cali_set"));
        assert!(json.contains("PF12345678901234567"));
    }

    #[test]
    fn test_extrusion_cali_sel_json() {
        let req = ExtrusionCaliSelRequest::new(0, 1, 4, "GFA01", "0.4", 40003);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"extrusion_cali_sel"#));
        assert!(json.contains(r#""ams_id":0"#));
        assert!(json.contains(r#""tray_id":1"#));
        assert!(json.contains(r#""cali_idx":4"#));
        assert!(json.contains(r#""filament_id":"GFA01""#));
        assert!(json.contains(r#""nozzle_diameter":"0.4""#));
        // setting_id must be absent to prevent database mislinking
        assert!(!json.contains("setting_id"));
    }

    #[test]
    fn test_standard_cali_del_json() {
        let entry = StandardCaliDelEntry {
            cali_idx: 4,
            filament_id: "GFA01".into(),
            nozzle_diameter: "0.4".into(),
            nozzle_id: "HS00-0.4".into(),
            setting_id: "PF12345678901234567".into(),
        };
        let req = StandardCaliDelRequest::new(entry, 50003).unwrap();
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"extrusion_cali_del"#));
        assert!(json.contains(r#""cali_idx":4"#));
        assert!(json.contains(r#""setting_id":"PF12345678901234567""#));
        assert!(json.contains(r#""sequence_id":"50003""#));
    }

    #[test]
    fn test_standard_cali_del_invalid_id() {
        let entry = StandardCaliDelEntry {
            cali_idx: 4,
            filament_id: "GFA01".into(),
            nozzle_diameter: "0.4".into(),
            nozzle_id: "HS00-0.4".into(),
            setting_id: "PFUS9be9e18f81828a".into(),
        };
        assert!(StandardCaliDelRequest::new(entry, 50003).is_err());
    }

    #[test]
    fn test_idex_cali_del_json() {
        let entry = IdexCaliDelEntry {
            nozzle_diameter: "0.4".into(),
            nozzle_id: "HS00-0.4".into(),
            extruder_id: 0,
        };
        let req = IdexCaliDelRequest::new(entry, 50004);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"extrusion_cali_del"#));
        assert!(json.contains(r#""extruder_id":0"#));
        assert!(json.contains(r#""nozzle_id":"HS00-0.4""#));
        assert!(json.contains(r#""sequence_id":"50004""#));
        // IDEX deletion must not contain setting_id
        assert!(!json.contains("setting_id"));
    }

    #[test]
    fn test_nozzle_diameter_none_omitted_from_extrusion_cali_set() {
        // KProfileEntry is both the extrusion_cali_get response element and the
        // extrusion_cali_set request element. Single-nozzle firmware legitimately omits
        // nozzle_diameter per-entry, so a caller round-tripping a profile back into a write
        // (e.g. to edit k_value) used to emit "nozzle_diameter":null — a shape the read side
        // never produces and reference/07_diagnostics_hms.md §7.2 never shows.
        let profile = KProfileEntry {
            cali_idx: -1,
            filament_id: "GFA01".into(),
            nozzle_diameter: None,
            nozzle_id: "HS00-0.4".into(),
            extruder_id: 0,
            name: "Round-tripped".into(),
            k_value: "0.022".into(),
            n_coef: None,
            setting_id: "PF12345678901234567".into(),
            ams_id: None,
            tray_id: None,
        };

        let req = ExtrusionCaliSetRequest::new(vec![profile], 50002).unwrap();
        let json = serde_json::to_string(&req).unwrap();
        assert!(
            !json.contains("nozzle_diameter"),
            "None nozzle_diameter should be omitted, not sent as null: {json}"
        );
    }

    #[test]
    fn test_n_coef_none_omitted_from_json() {
        let profile = KProfileEntry {
            cali_idx: -1,
            filament_id: "GFA01".into(),
            nozzle_diameter: Some("0.4".into()),
            nozzle_id: "HS00-0.4".into(),
            extruder_id: 0,
            name: "Test".into(),
            k_value: "0.022".into(),
            n_coef: None,
            setting_id: "PF12345678901234567".into(),
            ams_id: None,
            tray_id: None,
        };
        let json = serde_json::to_string(&profile).unwrap();
        assert!(
            !json.contains("n_coef"),
            "None n_coef should be omitted, not sent as null"
        );
    }

    #[test]
    fn test_n_coef_some_included_in_json() {
        let profile = KProfileEntry {
            cali_idx: -1,
            filament_id: "GFA01".into(),
            nozzle_diameter: Some("0.4".into()),
            nozzle_id: "HS00-0.4".into(),
            extruder_id: 0,
            name: "Test".into(),
            k_value: "0.022".into(),
            n_coef: Some("0.000000".into()),
            setting_id: "PF12345678901234567".into(),
            ams_id: None,
            tray_id: None,
        };
        let json = serde_json::to_string(&profile).unwrap();
        assert!(json.contains(r#""n_coef":"0.000000""#));
    }

    #[test]
    fn test_extrusion_cali_get_response_deserialization() {
        let json = r#"{
            "print": {
                "command": "extrusion_cali_get",
                "sequence_id": "50001",
                "nozzle_diameter": "0.4",
                "filaments": [{
                    "cali_idx": 4,
                    "filament_id": "GFA01",
                    "nozzle_diameter": "0.4",
                    "nozzle_id": "HS00-0.4",
                    "extruder_id": 0,
                    "name": "My Custom PLA Matte",
                    "k_value": "0.022000",
                    "n_coef": "0.000000",
                    "setting_id": "PF12345678901234567"
                }]
            }
        }"#;
        let resp: ExtrusionCaliGetResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.print.nozzle_diameter, Some("0.4".into()));
        assert_eq!(resp.print.filaments.len(), 1);
        assert_eq!(resp.print.filaments[0].cali_idx, 4);
        assert_eq!(resp.print.filaments[0].k_value, "0.022000");
        assert_eq!(resp.print.filaments[0].n_coef, Some("0.000000".into()));
    }

    #[test]
    fn test_extrusion_cali_get_response_deserializes_entry_missing_nozzle_diameter() {
        // Single-nozzle firmware omits nozzle_diameter per-entry, setting it only at the
        // envelope level — this must deserialize (not fail the whole response) even though
        // the per-entry field is absent. See KProfileEntry::nozzle_diameter's doc comment.
        let json = r#"{
            "print": {
                "command": "extrusion_cali_get",
                "sequence_id": "50001",
                "nozzle_diameter": "0.4",
                "filaments": [{
                    "cali_idx": 4,
                    "filament_id": "GFA01",
                    "nozzle_id": "HS00-0.4",
                    "extruder_id": 0,
                    "name": "My Custom PLA Matte",
                    "k_value": "0.022000",
                    "setting_id": "PF12345678901234567"
                }]
            }
        }"#;
        let resp: ExtrusionCaliGetResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.print.filaments[0].nozzle_diameter, None);
    }

    #[test]
    fn test_all_constructors_clamp_unclamped_sequence_id() {
        // These five constructors must serialize sequence_id through clamp_task_id() —
        // see .claude/rules/task-id-clamping.md.
        let raw = u64::MAX;
        let assert_clamped = |sequence_id: &str, label: &str| {
            assert!(
                sequence_id.parse::<i64>().unwrap() <= i32::MAX as i64,
                "{label} sequence_id {sequence_id} exceeds i32::MAX"
            );
        };

        assert_clamped(
            &ExtrusionCaliGetRequest::new(raw).print.sequence_id,
            "ExtrusionCaliGetRequest",
        );

        let profile = KProfileEntry {
            cali_idx: -1,
            filament_id: "GFA01".into(),
            nozzle_diameter: Some("0.4".into()),
            nozzle_id: "HS00-0.4".into(),
            extruder_id: 0,
            name: "Test".into(),
            k_value: "0.022".into(),
            n_coef: None,
            setting_id: "PF12345678901234567".into(),
            ams_id: None,
            tray_id: None,
        };
        assert_clamped(
            &ExtrusionCaliSetRequest::new(vec![profile], raw)
                .unwrap()
                .print
                .sequence_id,
            "ExtrusionCaliSetRequest",
        );

        assert_clamped(
            &ExtrusionCaliSelRequest::new(0, 1, 4, "GFA01", "0.4", raw)
                .print
                .sequence_id,
            "ExtrusionCaliSelRequest",
        );

        let standard_entry = StandardCaliDelEntry {
            cali_idx: 4,
            filament_id: "GFA01".into(),
            nozzle_diameter: "0.4".into(),
            nozzle_id: "HS00-0.4".into(),
            setting_id: "PF12345678901234567".into(),
        };
        assert_clamped(
            &StandardCaliDelRequest::new(standard_entry, raw)
                .unwrap()
                .print
                .sequence_id,
            "StandardCaliDelRequest",
        );

        let idex_entry = IdexCaliDelEntry {
            nozzle_diameter: "0.4".into(),
            nozzle_id: "HS00-0.4".into(),
            extruder_id: 0,
        };
        assert_clamped(
            &IdexCaliDelRequest::new(idex_entry, raw).print.sequence_id,
            "IdexCaliDelRequest",
        );
    }
}
