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

use crate::error::BambuError;

/// Validates whether a provided calibration profile setting ID complies with EEPROM limits.
///
/// **The Calibration Setting ID Boundary Rule [REF-DIAG-KPROF]:**
/// Stored EEPROM K-profiles require standard 19-character numeric formats consisting of a
/// `"PF"` header prefix followed by exactly 17 numeric digits. Standard alphanumeric hashes
/// (e.g. `"PFUS9be9e18f81828a"`) are strictly reserved for slicer-side presets.
/// Transmitting alphanumeric layouts inside direct database operations causes indexing halts
/// or table corruption on the physical mainboard.
pub fn validate_setting_id(setting_id: &str) -> bool {
    if !setting_id.starts_with("PF") {
        return false;
    }
    let digits = &setting_id[2..];
    digits.len() == 17 && digits.chars().all(|c| c.is_ascii_digit())
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
    pub nozzle_diameter: String,
    /// System designation of the target hotend profile structure (e.g. `"HS00-0.4"`).
    pub nozzle_id: String,
    /// Carriage layout indicator (0 = Right/Primary extruder, 1 = Left/Deputy extruder).
    pub extruder_id: u8,
    /// Custom user-defined name assigned to label the profile slot.
    pub name: String,
    /// Calibrated Linear Advance constant serialized as a float string.
    pub k_value: String,
    /// Extrusion coefficient parameters.
    #[serde(default)]
    pub n_coef: Option<String>,
    /// Secure 19-character unique setting identifier.
    pub setting_id: String,
}

// ============================================================================
// 2. Query Calibration Database Commands (extrusion_cali_get)
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct ExtrusionCaliGetPayload {
    pub command: &'static str,
    pub sequence_id: String,
}

/// JSON request wrapper to trigger a complete dump of the stored calibration database.
#[derive(Debug, Clone, Serialize)]
pub struct ExtrusionCaliGetRequest {
    pub print: ExtrusionCaliGetPayload,
}

impl ExtrusionCaliGetRequest {
    pub fn new(sequence_id: u64) -> Self {
        Self {
            print: ExtrusionCaliGetPayload {
                command: "extrusion_cali_get",
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}

// ============================================================================
// 3. Save or Write Calibration Commands (extrusion_cali_set)
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct ExtrusionCaliSetPayload {
    pub command: &'static str,
    pub filaments: Vec<KProfileEntry>,
    pub sequence_id: String,
}

/// JSON request wrapper to create or overwrite calibration profile allocations.
#[derive(Debug, Clone, Serialize)]
pub struct ExtrusionCaliSetRequest {
    pub print: ExtrusionCaliSetPayload,
}

impl ExtrusionCaliSetRequest {
    /// Builds a secure write-transaction payload targeting physical EEPROM slots.
    ///
    /// Verifies that all target profiles carry valid setting identifiers to protect local
    /// database health. Supports multi-profile writes for IDEX platforms.
    pub fn new(profiles: Vec<KProfileEntry>, sequence_id: u64) -> Result<Self, BambuError> {
        for profile in &profiles {
            if !validate_setting_id(&profile.setting_id) {
                return Err(BambuError::ProtocolViolation(
                    "Setting ID violates the strict 19-character numeric calibration boundary rule".into(),
                    ));
            }
        }

        Ok(Self {
            print: ExtrusionCaliSetPayload {
                command: "extrusion_cali_set",
                filaments: profiles,
                sequence_id: sequence_id.to_string(),
            },
        })
    }
}

// ============================================================================
// 3b. Bind Calibration Profile to AMS Slot (extrusion_cali_sel)
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct ExtrusionCaliSelPayload {
    pub command: &'static str,
    pub ams_id: i32,
    /// Absolute global tray ID (not local slot index).
    pub tray_id: i32,
    pub cali_idx: i32,
    pub filament_id: String,
    pub nozzle_diameter: String,
    pub sequence_id: String,
}

/// JSON request wrapper to bind a stored K-profile calibration entry to an AMS material slot [REF-AMS-MAP].
///
/// The `setting_id` field is intentionally omitted from this payload to prevent
/// database mislinking on the motion board.
#[derive(Debug, Clone, Serialize)]
pub struct ExtrusionCaliSelRequest {
    pub print: ExtrusionCaliSelPayload,
}

impl ExtrusionCaliSelRequest {
    pub fn new(
        ams_id: i32,
        tray_id: i32,
        cali_idx: i32,
        filament_id: &str,
        nozzle_diameter: &str,
        sequence_id: u64,
    ) -> Self {
        Self {
            print: ExtrusionCaliSelPayload {
                command: "extrusion_cali_sel",
                ams_id,
                tray_id,
                cali_idx,
                filament_id: String::from(filament_id),
                nozzle_diameter: String::from(nozzle_diameter),
                sequence_id: sequence_id.to_string(),
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
    pub cali_idx: i32,
    pub filament_id: String,
    pub nozzle_diameter: String,
    pub nozzle_id: String,
    pub setting_id: String,
}

/// Deletion coordinate metrics utilized by dual-nozzle IDEX databases (Schema B).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IdexCaliDelEntry {
    pub nozzle_diameter: String,
    pub nozzle_id: String,
    pub extruder_id: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct StandardCaliDelPayload {
    pub command: &'static str,
    pub filaments: Vec<StandardCaliDelEntry>,
    pub sequence_id: String,
}

/// JSON request wrapper targeting single-nozzle profile deletions (Schema A) [REF-DIAG-KPROF].
#[derive(Debug, Clone, Serialize)]
pub struct StandardCaliDelRequest {
    pub print: StandardCaliDelPayload,
}

impl StandardCaliDelRequest {
    /// Builds a single-nozzle deletion transaction keyed on the setting identifier.
    pub fn new(target: StandardCaliDelEntry, sequence_id: u64) -> Result<Self, BambuError> {
        if !validate_setting_id(&target.setting_id) {
            return Err(BambuError::ProtocolViolation(
                "Setting ID violates the strict 19-character numeric calibration boundary rule".into(),
                ));
        }

        Ok(Self {
            print: StandardCaliDelPayload {
                command: "extrusion_cali_del",
                filaments: vec![target],
                sequence_id: sequence_id.to_string(),
            },
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct IdexCaliDelPayload {
    pub command: &'static str,
    pub filaments: Vec<IdexCaliDelEntry>,
    pub sequence_id: String,
}

/// JSON request wrapper targeting dual-nozzle IDEX profile deletions (Schema B) [REF-DIAG-KPROF].
#[derive(Debug, Clone, Serialize)]
pub struct IdexCaliDelRequest {
    pub print: IdexCaliDelPayload,
}

impl IdexCaliDelRequest {
    /// Builds a dual-nozzle carriage deletion transaction keyed on physical coordinates.
    pub fn new(target: IdexCaliDelEntry, sequence_id: u64) -> Self {
        Self {
            print: IdexCaliDelPayload {
                command: "extrusion_cali_del",
                filaments: vec![target],
                sequence_id: sequence_id.to_string(),
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
        assert!(validate_setting_id("PF12345678901234567"));

        // Alphanumeric hash used by slicer presets must fail
        assert!(!validate_setting_id("PFUS9be9e18f81828a"));

        // Short decimal block
        assert!(!validate_setting_id("PF123456"));

        // Missing prefix
        assert!(!validate_setting_id("1234567890123456789"));
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
            nozzle_diameter: "0.4".into(),
            nozzle_id: "HS00-0.4".into(),
            extruder_id: 0,
            name: "Faulty ID".into(),
            k_value: "0.022".into(),
            n_coef: None,
            setting_id: "PF_BAD_ALPHANUM_KEY".into(), // Violated boundary
        };

        let result = ExtrusionCaliSetRequest::new(vec![bad_profile], 50002);
        assert!(result.is_err());
    }

    #[test]
    fn test_extrusion_cali_set_valid_id() {
        let good_profile = KProfileEntry {
            cali_idx: -1,
            filament_id: "GFA01".into(),
            nozzle_diameter: "0.4".into(),
            nozzle_id: "HS00-0.4".into(),
            extruder_id: 0,
            name: "Good ID".into(),
            k_value: "0.022".into(),
            n_coef: None,
            setting_id: "PF12345678901234567".into(),
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
}
