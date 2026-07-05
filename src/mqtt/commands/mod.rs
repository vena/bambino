//! # MQTT Command Payloads & Serialization Builders
//!
//! Provides the concrete data structures and serialization wrappers required to control
//! physical Bambu Lab printers over MQTTS Port 8883 [REF-MQTT-LIFECYCLE].
//!
//! Handles complex polymorphic rules such as the string-vs-array mapping schemas for the
//! `ams_mapping` parameter, and enforces safety bounds on task identities.
//!
//! ## Architectural Alignment
//! * **Polymorphic Mapping Rules [REF-MQTT-LIFECYCLE]:** Handles conditional typing for
//!   material mappings, where inactive AMS sessions must present as empty strings while active
//!   sessions require integer arrays.
//! * **Task-ID Overflow Prevention [REF-MQTT-ENV]:** Clamps all generated sequence identifiers
//!   to 32-bit signed integer limits to prevent memory allocation overflows on hardware boards.

pub mod ams;
pub mod control;
pub mod gcode;
pub mod hardware;
pub mod print_job;
pub mod status;

pub use ams::{
    AmsChangeFilamentRequest, AmsControlRequest, AmsFilamentDryingRequest,
    AmsFilamentSettingRequest, AmsGetRfidRequest,
};
pub use control::{
    CalibrationRequest, CleanPrintErrorRequest, PrintSpeedRequest, SkipObjectsRequest,
    StandardControlRequest,
};
pub use gcode::GCodeRequest;
pub use hardware::{
    AirductMode, AirductRequest, BuzzerRequest, LedCtrlRequest, PromptSoundRequest,
};
pub use print_job::{AmsMappingTable, PrintJobConfig, ProjectFileRequest};
pub use status::{GetVersionRequest, PushAllRequest};

pub(crate) const TASK_ID_MAX: u64 = i32::MAX as u64;

/// Clamps a 64-bit transaction or tracking identifier (typically standard UNIX epoch
/// milliseconds) within the strict boundary limits of a 32-bit signed integer (`2147483647`).
///
/// **Why this is critical [REF-MQTT-ENV]:**
/// The printer's onboard G-code parsing routine clamps subtask identifiers to standard 32-bit
/// signed integer limits. If a connecting client uses an un-clamped millisecond epoch (13-digit integer),
/// the memory allocation registers on the motion board will overflow. This causes the printer to lock
/// indefinitely in an `IDLE` state and reject all subsequent print dispatches.
pub fn clamp_task_id(raw_id: u64) -> u32 {
    (raw_id % TASK_ID_MAX) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BambuModel;

    #[test]
    fn test_task_id_modulo_math() {
        let raw_epoch: u64 = 1718626458000;
        let clamped = clamp_task_id(raw_epoch);
        assert!(clamped <= i32::MAX as u32);
    }

    #[test]
    fn test_ams_mapping_polymorphism_inactive() {
        let config = PrintJobConfig::new(
            "job.3mf",
            "Metadata/plate_1.gcode",
            "Test Print",
            12345,
            "textured",
        );
        let req = ProjectFileRequest::from_config(&config, 5000, BambuModel::P1S);

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""ams_mapping":"""#));
    }

    #[test]
    fn test_ams_mapping_polymorphism_active() {
        let config = PrintJobConfig::new(
            "job.3mf",
            "Metadata/plate_1.gcode",
            "Test Print",
            12345,
            "textured",
        )
        .with_ams(vec![0, -1, 1]);
        let req = ProjectFileRequest::from_config(&config, 5000, BambuModel::P1S);

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""ams_mapping":[0,-1,1]"#));
    }

    #[test]
    fn test_ams_mapping_all_external_spool_overrides_use_ams_false_single_nozzle() {
        // reference/05_materials_ams.md [REF-AMS-USEAMS]: on single-nozzle printers, dispatching
        // `use_ams: true` when every mapped filament is actually on the external spool
        // (no real physical AMS channel) makes real firmware reject the job with
        // `07FF_8012`. `from_config` must override `use_ams` to `false` in that case.
        let config = PrintJobConfig::new(
            "job.3mf",
            "Metadata/plate_1.gcode",
            "Test Print",
            12345,
            "textured",
        )
        .with_ams(vec![-1, -1]);
        let req = ProjectFileRequest::from_config(&config, 5000, BambuModel::P1S);

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""use_ams":false"#));
        assert!(json.contains(r#""ams_mapping":"""#));
    }

    #[test]
    fn test_ams_mapping2_sets_use_ams_true() {
        // Phase 2.2 regression: `.with_ams_mapping2(...)` alone (no `.with_ams(...)`) must
        // set `use_ams` so the mapping2 array isn't silently dropped by `from_config`'s
        // `use_ams`-gated serialization below.
        use crate::ams::mapping::AmsMapping2Entry;

        let config = PrintJobConfig::new(
            "job.3mf",
            "Metadata/plate_1.gcode",
            "Test Print",
            12345,
            "textured",
        )
        .with_ams_mapping2(vec![AmsMapping2Entry {
            ams_id: 0,
            slot_id: 1,
        }]);
        let req = ProjectFileRequest::from_config(&config, 5000, BambuModel::P1S);

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""use_ams":true"#));
        assert!(json.contains(r#""ams_mapping2""#));
    }

    #[test]
    fn test_ams_mapping2_dropped_when_safety_interlock_trips() {
        // Phase 2.2 regression: an all-external-spool `ams_mapping2` on a single-nozzle
        // printer trips `validate_external_spool_safety`, forcing `use_ams` to `false` — the
        // wire payload must not also carry a populated `ams_mapping2` array in that case
        // (the exact contradictory shape that causes firmware error `0700_8012`).
        use crate::ams::mapping::AmsMapping2Entry;

        let config = PrintJobConfig::new(
            "job.3mf",
            "Metadata/plate_1.gcode",
            "Test Print",
            12345,
            "textured",
        )
        .with_ams_mapping2(vec![AmsMapping2Entry {
            ams_id: 255,
            slot_id: 0,
        }]);
        let req = ProjectFileRequest::from_config(&config, 5000, BambuModel::P1S);

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""use_ams":false"#));
        assert!(!json.contains("ams_mapping2"));
    }

    #[test]
    fn test_nozzle_offset_cali_quirks_default_idex() {
        let config = PrintJobConfig::new(
            "job.3mf",
            "Metadata/plate_1.gcode",
            "Test Print",
            12345,
            "textured",
        );
        assert!(config.nozzle_offset_cali.is_none());

        let req = ProjectFileRequest::from_config(&config, 5000, BambuModel::X2D);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""nozzle_offset_cali":1"#));
    }

    #[test]
    fn test_nozzle_offset_cali_quirks_default_single_nozzle() {
        let config = PrintJobConfig::new(
            "job.3mf",
            "Metadata/plate_1.gcode",
            "Test Print",
            12345,
            "textured",
        );
        let req = ProjectFileRequest::from_config(&config, 5000, BambuModel::P1S);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""nozzle_offset_cali":0"#));
    }

    #[test]
    fn test_nozzle_offset_cali_explicit_override() {
        let config = PrintJobConfig::new(
            "job.3mf",
            "Metadata/plate_1.gcode",
            "Test Print",
            12345,
            "textured",
        )
        .nozzle_offset_calibration(false);
        let req = ProjectFileRequest::from_config(&config, 5000, BambuModel::X2D);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""nozzle_offset_cali":0"#));
    }

    #[test]
    fn test_ams_change_filament_load_json() {
        let req = AmsChangeFilamentRequest::new(0, 1, 1, -1, -1, 40005);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"ams_change_filament"#));
        assert!(json.contains(r#""ams_id":0"#));
        assert!(json.contains(r#""slot_id":1"#));
        assert!(json.contains(r#""target":1"#));
        assert!(json.contains(r#""curr_temp":-1"#));
        assert!(json.contains(r#""tar_temp":-1"#));
    }

    #[test]
    fn test_ams_change_filament_unload_json() {
        let req = AmsChangeFilamentRequest::new(0, 255, 255, 210, 210, 40008);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""slot_id":255"#));
        assert!(json.contains(r#""target":255"#));
        assert!(json.contains(r#""curr_temp":210"#));
    }

    #[test]
    fn test_ams_filament_drying_json() {
        let req = AmsFilamentDryingRequest::new(128, 1, 55, 480, true, "PA-CF", 40004);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"ams_filament_drying"#));
        assert!(json.contains(r#""ams_id":128"#));
        assert!(json.contains(r#""mode":1"#));
        assert!(json.contains(r#""dry_temp":55"#));
        assert!(json.contains(r#""dry_time":480"#));
        assert!(json.contains(r#""rotate_tray":true"#));
        assert!(json.contains(r#""filament":"PA-CF""#));
    }

    #[test]
    fn test_clean_print_error_json() {
        let req = CleanPrintErrorRequest::new(20010);
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(
            json,
            r#"{"print":{"command":"clean_print_error","sequence_id":"20010"}}"#
        );
    }

    #[test]
    fn test_pushall_request_json() {
        let req = PushAllRequest::new(10001);
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(
            json,
            r#"{"pushing":{"command":"pushall","sequence_id":"10001"}}"#
        );
    }

    #[test]
    fn test_get_version_request_json() {
        let req = GetVersionRequest::new(10002);
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(
            json,
            r#"{"info":{"command":"get_version","sequence_id":"10002"}}"#
        );
    }

    #[test]
    fn test_gcode_request_appends_newline() {
        let req = GCodeRequest::new("G28", 10003);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"gcode_line"#));
        assert!(json.contains(r#""param":"G28\n""#));

        let req_with_nl = GCodeRequest::new("G28\n", 10004);
        let json2 = serde_json::to_string(&req_with_nl).unwrap();
        assert!(json2.contains(r#""param":"G28\n""#));
        assert!(!json2.contains(r#""param":"G28\n\n""#));
    }

    #[test]
    fn test_led_ctrl_request_json() {
        let req_on = LedCtrlRequest::new("chamber_light", true, 10005);
        let json = serde_json::to_string(&req_on).unwrap();
        assert!(json.contains(r#""command":"ledctrl"#));
        assert!(json.contains(r#""led_node":"chamber_light""#));
        assert!(json.contains(r#""led_mode":"on""#));

        let req_off = LedCtrlRequest::new("chamber_light", false, 10006);
        let json_off = serde_json::to_string(&req_off).unwrap();
        assert!(json_off.contains(r#""led_mode":"off""#));
    }

    #[test]
    fn test_airduct_request_json() {
        let req = AirductRequest::new(AirductMode::Cooling, 10007);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"set_airduct"#));
        assert!(json.contains(r#""modeId":0"#));

        let req_heat = AirductRequest::new(AirductMode::Heating, 10008);
        let json_heat = serde_json::to_string(&req_heat).unwrap();
        assert!(json_heat.contains(r#""modeId":1"#));

        let req_laser = AirductRequest::new(AirductMode::Laser, 10009);
        let json_laser = serde_json::to_string(&req_laser).unwrap();
        assert!(json_laser.contains(r#""modeId":2"#));
    }

    #[test]
    fn test_prompt_sound_request_json() {
        let req = PromptSoundRequest::new(true, 10009);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"print_option"#));
        assert!(json.contains(r#""sound_enable":true"#));
    }

    #[test]
    fn test_buzzer_request_json() {
        let req = BuzzerRequest::new(2, 10010);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"buzzer_ctrl"#));
        assert!(json.contains(r#""mode":2"#));
        assert!(json.contains(r#""reason":"""#));
    }

    #[test]
    fn test_calibration_request_json() {
        let req = CalibrationRequest::new(6, 10011);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"calibration"#));
        assert!(json.contains(r#""option":6"#));
    }

    #[test]
    fn test_print_speed_request_json() {
        let req = PrintSpeedRequest::new("3", 10012);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"print_speed"#));
        assert!(json.contains(r#""param":"3""#));
    }

    #[test]
    fn test_ams_control_request_json() {
        let req = AmsControlRequest::new("resume", 10013);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"ams_control"#));
        assert!(json.contains(r#""param":"resume""#));
    }

    #[test]
    fn test_ams_get_rfid_request_json() {
        let req = AmsGetRfidRequest::new(0, 2, 10014);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"ams_get_rfid"#));
        assert!(json.contains(r#""ams_id":0"#));
        assert!(json.contains(r#""slot_id":2"#));
    }

    #[test]
    fn test_ams_filament_setting_request_json() {
        let req = AmsFilamentSettingRequest::new(
            0,
            1,
            "GFA01",
            "PLA",
            Some("Bambu PLA Basic"),
            "FF0000FF",
            190,
            220,
            10015,
        );
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"ams_filament_setting"#));
        assert!(json.contains(r#""tray_info_idx":"GFA01""#));
        assert!(json.contains(r#""tray_type":"PLA""#));
        assert!(json.contains(r#""tray_sub_brands":"Bambu PLA Basic""#));
        assert!(json.contains(r#""tray_color":"FF0000FF""#));
        assert!(json.contains(r#""nozzle_temp_min":190"#));
        assert!(json.contains(r#""nozzle_temp_max":220"#));
    }

    #[test]
    fn test_ams_filament_setting_default_sub_brands() {
        let req = AmsFilamentSettingRequest::new(
            255, 254, "GFA01", "PLA", None, "FFFFFFFF", 190, 220, 10016,
        );
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""tray_sub_brands":"PLA Basic""#));
    }

    #[test]
    fn test_skip_objects_request_json() {
        let req = SkipObjectsRequest::new(vec![0, 3, 7], 10017);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"skip_objects"#));
        assert!(json.contains(r#""obj_list":[0,3,7]"#));
    }

    #[test]
    fn test_standard_control_request_json() {
        for cmd in ["pause", "resume", "stop"] {
            let req = StandardControlRequest::new(cmd, 10018);
            let json = serde_json::to_string(&req).unwrap();
            assert!(json.contains(&format!(r#""command":"{}""#, cmd)));
        }
    }
}
