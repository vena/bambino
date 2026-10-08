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
    AmsChangeFilamentRequest, AmsControlOp, AmsControlRequest, AmsFilamentDryingRequest,
    AmsFilamentSettingRequest, AmsGetRfidRequest, ChangeTemps, DryingParams, FilamentSpec,
};
pub use control::{
    CalibrationRequest, CleanPrintErrorRequest, HmsActionRequest, IdleIgnoreRequest,
    IdleIgnoreScope, PrintSpeedRequest, SkipObjectsRequest, StandardCommand,
    StandardControlRequest, UiopRequest,
};
pub use gcode::GCodeRequest;
pub use hardware::{
    AirPrintDetectRequest, AirPurificationRequest, AirductMode, AirductRequest,
    AutoRecoveryRequest, BuzzerRequest, DoorOpenCheckRequest, FilamentBackupRequest,
    FilamentTangleDetectRequest, FlashTiming, IdleHeatingProtectionRequest, LedCtrlRequest,
    NozzleBlobDetectRequest, PromptSoundRequest, SmartNozzleBlobDetectRequest,
    StoreSentFilesRequest, XcamControlRequest,
};
pub use print_job::{
    AmsMappingTable, AmsSource, CalibrationMode, NozzleRack, PrintJobConfig, ProjectFileRequest,
    resolve_rack_nozzle_mapping,
};
pub use status::{GetAccessCodeRequest, GetVersionRequest, PushAllRequest};

pub(crate) const TASK_ID_MAX: u64 = i32::MAX as u64;

/// Wraps a 64-bit identifier into `[0, i32::MAX)` by modulo, not saturation.
///
/// Firmware parses task and sequence ids as signed 32-bit integers; an unclamped epoch-millisecond
/// id overflows the motion board's registers, locking the printer in `IDLE` and making it reject
/// every later print dispatch [REF-MQTT-ENV]. Modulo rather than saturation so a counter keeps
/// advancing across the wrap. Reachable only through [`ClampedTaskId`]'s `From<u64>`; see
/// `.claude/rules/task-id-clamping.md`.
pub(crate) fn clamp_task_id(raw_id: u64) -> u32 {
    (raw_id % TASK_ID_MAX) as u32
}

/// A task or sequence id already reduced into the range firmware accepts (below `i32::MAX`).
///
/// Every request constructor takes `impl Into<ClampedTaskId>`; the only way to make one is the
/// clamping `From<u64>`, so an out-of-range id can't reach the wire. Serializes as a decimal
/// string, the form the printer expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClampedTaskId(u32);

impl ClampedTaskId {
    /// The clamped value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl From<u64> for ClampedTaskId {
    fn from(raw_id: u64) -> Self {
        Self(clamp_task_id(raw_id))
    }
}

impl core::fmt::Display for ClampedTaskId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&self.0, f)
    }
}

impl serde::Serialize for ClampedTaskId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

/// Generates one namespace envelope: the single-field wrapper every command is published in.
macro_rules! envelope {
    ($name:ident, $field:ident, $wire:literal) => {
        #[doc = concat!("The `", $wire, "` namespace envelope a command payload is published in.")]
        #[derive(Debug, Clone, serde::Serialize)]
        pub struct $name<P> {
            #[doc = concat!("The payload, serialized under `", $wire, "`.")]
            pub $field: P,
        }
    };
}

envelope!(Print, print, "print");
envelope!(System, system, "system");
envelope!(Pushing, pushing, "pushing");
envelope!(Info, info, "info");
envelope!(Xcam, xcam, "xcam");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::PrinterModel;
    use crate::types::control::LedNode;

    const PLA_SPEC: FilamentSpec<'static> = FilamentSpec {
        preset: "GFA01",
        material: "PLA",
        nozzle_temp_min: 190,
        nozzle_temp_max: 220,
    };

    #[test]
    fn test_command_constructor_clamps_unclamped_sequence_id() {
        // Every command constructor's `sequence_id` must come out clamped even when called
        // directly with a raw `u64` (bypassing PrinterClient::next_sequence_id(), which keeps
        // its own ids in range) — an external consumer of this public API could otherwise pass
        // a raw epoch-millisecond value and reproduce the documented 32-bit overflow firmware
        // lockup. Constructors take `impl Into<ClampedTaskId>`, so this holds by construction;
        // see `.claude/rules/task-id-clamping.md`.
        let req = GCodeRequest::new("G28", u64::MAX);
        let json = serde_json::to_string(&req).unwrap();
        assert!(
            i64::from(req.print.sequence_id.get()) <= i32::MAX as i64,
            "sequence_id {} exceeds i32::MAX in {json}",
            req.print.sequence_id
        );

        // ProjectFileRequest::from_config previously skipped clamp_task_id() too.
        let config = PrintJobConfig::new(
            "job.3mf",
            "Metadata/plate_1.gcode",
            "Test Print",
            u64::MAX,
            "textured",
        );
        let project_req = ProjectFileRequest::from_config(&config, u64::MAX, PrinterModel::P1S);
        assert!(
            i64::from(project_req.print.sequence_id.get()) <= i32::MAX as i64,
            "ProjectFileRequest sequence_id {} exceeds i32::MAX",
            project_req.print.sequence_id
        );
        assert!(
            project_req.print.subtask_id.parse::<i64>().unwrap() <= i32::MAX as i64,
            "ProjectFileRequest subtask_id {} exceeds i32::MAX",
            project_req.print.subtask_id
        );
    }

    #[test]
    fn test_task_id_modulo_math() {
        let raw_epoch: u64 = 1718626458000;
        let clamped = clamp_task_id(raw_epoch);
        assert!(clamped <= i32::MAX as u32);
    }

    #[test]
    fn test_clamp_task_id_wraps_near_max() {
        // tests/client_test.rs's integration test can't seed sequence_counter near
        // TASK_ID_MAX (it's pub(crate), invisible outside this crate) so it never actually
        // exercised wraparound. clamp_task_id() is a free function, so this unit test can seed
        // any raw_id directly.
        assert_eq!(clamp_task_id(TASK_ID_MAX), 0);
        assert_eq!(clamp_task_id(TASK_ID_MAX + 1), 1);
        assert_eq!(clamp_task_id(TASK_ID_MAX - 1), (TASK_ID_MAX - 1) as u32);
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
        let req = ProjectFileRequest::from_config(&config, 5000, PrinterModel::P1S);

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
        let req = ProjectFileRequest::from_config(&config, 5000, PrinterModel::P1S);

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
        let req = ProjectFileRequest::from_config(&config, 5000, PrinterModel::P1S);

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""use_ams":false"#));
        assert!(json.contains(r#""ams_mapping":"""#));
    }

    #[test]
    fn test_with_ams_sanitizes_out_of_range_flat_channel_ids() {
        // issue #56: with_ams's raw i32 path had no equivalent to ams_mapping2's
        // flat_channel_id_for_entry sanitization — 254/255 (external-spool sentinels) and
        // other out-of-range values must fold to -1 (unmapped) instead of reaching the wire.
        let config = PrintJobConfig::new(
            "job.3mf",
            "Metadata/plate_1.gcode",
            "Test Print",
            12345,
            "textured",
        )
        .with_ams(vec![0, 15, 128, 135, 254, 255, 16, -5, -1]);

        assert_eq!(
            config.ams,
            Some(AmsSource::Flat(vec![0, 15, 128, 135, -1, -1, -1, -1, -1]))
        );
    }

    #[test]
    fn test_ams_mapping2_sets_use_ams_true() {
        // Regression test: `.with_ams_mapping2(...)` alone (no `.with_ams(...)`) must
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
        let req = ProjectFileRequest::from_config(&config, 5000, PrinterModel::P1S);

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""use_ams":true"#));
        assert!(json.contains(r#""ams_mapping2""#));
    }

    #[test]
    fn test_ams_mapping2_syncs_flat_ams_mapping() {
        // with_ams_mapping2() alone (no with_ams()) must still populate the flat
        // ams_mapping array in sync with ams_mapping2 — the firmware requires the two arrays
        // to stay index-parallel [REF-AMS-MAP], and with_ams_mapping2() never touches
        // config.ams_mapping directly.
        use crate::ams::mapping::AmsMapping2Entry;

        let config = PrintJobConfig::new(
            "job.3mf",
            "Metadata/plate_1.gcode",
            "Test Print",
            12345,
            "textured",
        )
        .with_ams_mapping2(vec![
            AmsMapping2Entry {
                ams_id: 0,
                slot_id: 1,
            },
            AmsMapping2Entry {
                ams_id: 128,
                slot_id: 0,
            },
        ]);
        let req = ProjectFileRequest::from_config(&config, 5000, PrinterModel::P1S);

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""use_ams":true"#));
        // ams_id 0, slot_id 1 -> flat channel (0 * 4) + 1 = 1; ams_id 128 (AMS-HT) -> 128.
        assert!(json.contains(r#""ams_mapping":[1,128]"#));
    }

    #[test]
    fn test_ams_mapping2_dropped_when_safety_interlock_trips() {
        // Regression test: an all-external-spool `ams_mapping2` on a single-nozzle
        // printer trips `is_external_spool_safety_valid`, forcing `use_ams` to `false` — the
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
        let req = ProjectFileRequest::from_config(&config, 5000, PrinterModel::P1S);

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

        let req = ProjectFileRequest::from_config(&config, 5000, PrinterModel::X2D);
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
        let req = ProjectFileRequest::from_config(&config, 5000, PrinterModel::P1S);
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
        let req = ProjectFileRequest::from_config(&config, 5000, PrinterModel::X2D);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""nozzle_offset_cali":0"#));
    }

    #[test]
    fn test_calibration_mode_auto_wire_encoding() {
        use crate::mqtt::CalibrationMode;

        let config = PrintJobConfig::new(
            "job.3mf",
            "Metadata/plate_1.gcode",
            "Test Print",
            12345,
            "textured",
        )
        .bed_leveling(CalibrationMode::Auto)
        .flow_calibration(CalibrationMode::Auto)
        .nozzle_offset_calibration(CalibrationMode::Auto);
        let req = ProjectFileRequest::from_config(&config, 5000, PrinterModel::X2D);
        let json = serde_json::to_string(&req).unwrap();

        // bed_leveling stays a strict JSON bool (false, since Auto != On) — see
        // reference/03_mqtt_telemetry.md for why this field can never be int-encoded.
        // The tri-state intent is carried by auto_bed_leveling instead.
        assert!(json.contains(r#""bed_leveling":false"#));
        assert!(json.contains(r#""auto_bed_leveling":2"#));
        assert!(json.contains(r#""flow_cali":false"#));
        assert!(json.contains(r#""extrude_cali_flag":2"#));
        assert!(json.contains(r#""nozzle_offset_cali":2"#));
    }

    #[test]
    fn test_ams_change_filament_load_json() {
        let req = AmsChangeFilamentRequest::load(0, 1, ChangeTemps::FIRMWARE, None, 40005);
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
        let req = AmsChangeFilamentRequest::unload(
            0,
            ChangeTemps {
                current: 210,
                target: 210,
            },
            None,
            40008,
        );
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""slot_id":255"#));
        assert!(json.contains(r#""target":255"#));
        assert!(json.contains(r#""curr_temp":210"#));
    }

    #[test]
    fn test_ams_filament_drying_json() {
        // Field names/shapes rewritten to match the real wire protocol.
        let params = DryingParams {
            filament: "PA-CF".into(),
            temp: 55,
            duration_hours: 8,
            rotate_tray: true,
            cooling_temp: 20,
            ..Default::default()
        };
        let req = AmsFilamentDryingRequest::start(128, params, 40004);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"ams_filament_drying"#));
        assert!(json.contains(r#""ams_id":128"#));
        assert!(json.contains(r#""mode":1"#));
        assert!(json.contains(r#""temp":55"#));
        assert!(json.contains(r#""duration":8"#));
        assert!(json.contains(r#""humidity":0"#));
        assert!(json.contains(r#""rotate_tray":true"#));
        assert!(json.contains(r#""cooling_temp":20"#));
        assert!(json.contains(r#""close_power_conflict":false"#));
        assert!(json.contains(r#""filament":"PA-CF""#));

        let stop = serde_json::to_string(&AmsFilamentDryingRequest::stop(128, 40005)).unwrap();
        assert!(stop.contains(r#""mode":0"#));
        assert!(stop.contains(r#""temp":0"#));
        assert!(stop.contains(r#""filament":"""#));
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
    fn test_hms_action_json_sends_err_in_decimal() {
        // 0x0500C010 = 83935248: BambuStudio sends `std::to_string(m_error_code)`.
        let req = HmsActionRequest::ignore(0x0500_C010, Some("4242"), 20011);
        assert_eq!(
            serde_json::to_string(&req).unwrap(),
            r#"{"print":{"command":"ignore","err":"83935248","param":"reserve","job_id":"4242","sequence_id":"20011"}}"#
        );
        let resume = serde_json::to_string(&HmsActionRequest::resume(1, None, 1)).unwrap();
        assert!(resume.contains(r#""command":"resume","err":"1","param":"reserve","job_id":"""#));
        let stop = serde_json::to_string(&HmsActionRequest::stop(1, None, 1)).unwrap();
        assert!(stop.contains(r#""command":"stop""#));
    }

    #[test]
    fn test_idle_ignore_json() {
        let once = serde_json::to_string(&IdleIgnoreRequest::new(
            0x0500_C010,
            IdleIgnoreScope::Once,
            7,
        ))
        .unwrap();
        assert_eq!(
            once,
            r#"{"print":{"command":"idle_ignore","err":"83935248","type":0,"sequence_id":"7"}}"#
        );
        let always =
            serde_json::to_string(&IdleIgnoreRequest::new(1, IdleIgnoreScope::Permanent, 7))
                .unwrap();
        assert!(always.contains(r#""type":1"#));
    }

    #[test]
    fn test_uiop_close_json_sends_err_in_hex() {
        let req = UiopRequest::close_print_error(0x0500_c010, 9);
        assert_eq!(
            serde_json::to_string(&req).unwrap(),
            r#"{"system":{"command":"uiop","sequence_id":"9","name":"print_error","action":"close","source":1,"type":"dialog","err":"0500C010"}}"#
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
    fn test_rack_mapping_sends_physical_ids_not_extruder_indices() {
        // Extruder index 1 is the FIXED hotend (physical id 1); index 0 is the RACK.
        // Upstream had this inverted once and printed a first layer in mid-air, so the
        // polarity is asserted explicitly rather than left implicit in a wire snapshot.
        let wire = resolve_rack_nozzle_mapping(&[0, 1, -1], 18).expect("resolvable");
        assert_eq!(wire[0], 18, "slot 0 uses the rack -> live rack physical id");
        assert_eq!(wire[1], 1, "slot 1 uses the fixed hotend -> physical id 1");
        assert_eq!(wire[2], -1, "slot 2 is not printed");
    }

    #[test]
    fn test_rack_mapping_is_a_padded_32_slot_array() {
        // Not the plate's filament count. Upstream briefly derived the length from a 3-entry
        // capture, then reverted after a real 3-filament dispatch was observed as 32 entries.
        let wire = resolve_rack_nozzle_mapping(&[0], 16).expect("resolvable");
        assert_eq!(wire.len(), 32);
        assert_eq!(wire[0], 16);
        assert!(wire[1..].iter().all(|&v| v == -1), "tail must be -1 padded");
    }

    #[test]
    fn test_rack_mapping_declines_rather_than_guessing() {
        // Every one of these omits nozzle_mapping so firmware picks — never a wrong physical id.
        assert!(
            resolve_rack_nozzle_mapping(&[], 16).is_none(),
            "empty slots"
        );
        assert!(
            resolve_rack_nozzle_mapping(&[0; 33], 16).is_none(),
            "more slots than the wire carries"
        );
        assert!(
            resolve_rack_nozzle_mapping(&[0, 1], 15).is_none(),
            "15 is below the rack id range"
        );
        assert!(
            resolve_rack_nozzle_mapping(&[0, 1], 22).is_none(),
            "22 is above the rack id range"
        );
        assert!(
            resolve_rack_nozzle_mapping(&[1, 1], 16).is_none(),
            "no slot needs the rack — Studio omits the field entirely here"
        );
        assert!(
            resolve_rack_nozzle_mapping(&[0, 2], 16).is_none(),
            "extruder 2 is a carriage an H2C does not have"
        );
    }

    #[test]
    fn test_with_nozzle_rack_rejects_a_non_rack_id_and_ams_builders_replace_each_other() {
        let config = PrintJobConfig::new("j.3mf", "Metadata/plate_1.gcode", "job", 1, "textured");
        assert!(matches!(
            config.clone().with_nozzle_rack(vec![0, 1], 1),
            Err(crate::error::Error::InvalidArgument(_))
        ));

        // #500: the second builder wins outright instead of one silently shadowing the other.
        let entry = crate::ams::AmsMapping2Entry {
            ams_id: 0,
            slot_id: 2,
        };
        let structured_last = config
            .clone()
            .with_ams(vec![1])
            .with_ams_mapping2(vec![entry.clone()]);
        assert_eq!(
            structured_last.ams,
            Some(AmsSource::Structured(vec![entry]))
        );
        let flat_last = structured_last.with_ams(vec![1]);
        assert_eq!(flat_last.ams, Some(AmsSource::Flat(vec![1])));
    }

    #[test]
    fn test_nozzle_mapping_omitted_on_non_rack_models_and_present_on_h2c() {
        let with_rack =
            PrintJobConfig::new("j.3mf", "Metadata/plate_1.gcode", "job", 1, "textured")
                .with_nozzle_rack(vec![0, 1], 17)
                .expect("17 is a rack position");

        let h2c = ProjectFileRequest::from_config(&with_rack, 1, PrinterModel::H2C);
        assert_eq!(h2c.print.nozzle_mapping.as_ref().map(|m| m.len()), Some(32));
        let json = serde_json::to_string(&h2c).unwrap();
        assert!(json.contains("\"nozzle_mapping\""));

        // Same config on a non-rack model must not emit the field at all.
        let p1s = ProjectFileRequest::from_config(&with_rack, 1, PrinterModel::P1S);
        assert!(p1s.print.nozzle_mapping.is_none());
        let json = serde_json::to_string(&p1s).unwrap();
        assert!(!json.contains("nozzle_mapping"));

        // An H2C job that never called with_nozzle_rack also omits it.
        let bare = PrintJobConfig::new("j.3mf", "Metadata/plate_1.gcode", "job", 1, "textured");
        let h2c_bare = ProjectFileRequest::from_config(&bare, 1, PrinterModel::H2C);
        assert!(h2c_bare.print.nozzle_mapping.is_none());
        assert!(
            !serde_json::to_string(&h2c_bare)
                .unwrap()
                .contains("nozzle_mapping")
        );
    }

    #[test]
    fn test_vibration_cali_defaults_off_on_every_model() {
        let config = PrintJobConfig::new("j.3mf", "Metadata/plate_1.gcode", "job", 1, "textured");
        for model in [
            PrinterModel::P1S,
            PrinterModel::X1C,
            PrinterModel::P2S,
            PrinterModel::H2D,
        ] {
            let req = ProjectFileRequest::from_config(&config, 1, model);
            assert!(
                !req.print.vibration_cali,
                "{model:?} should default to false"
            );
        }
    }

    #[test]
    fn test_vibration_cali_explicit_opt_in_honored_on_every_model() {
        let config = PrintJobConfig::new("j.3mf", "Metadata/plate_1.gcode", "job", 1, "textured")
            .vibration_compensation(CalibrationMode::On);
        for model in [PrinterModel::P1S, PrinterModel::P2S] {
            let req = ProjectFileRequest::from_config(&config, 1, model);
            assert!(
                req.print.vibration_cali,
                "{model:?} should honor the opt-in"
            );
        }
    }

    #[test]
    fn test_get_access_code_request_json() {
        let req = GetAccessCodeRequest::new(10002);
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(
            json,
            r#"{"system":{"command":"get_access_code","sequence_id":"10002"}}"#
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
        let req_on = LedCtrlRequest::new(LedNode::Chamber, true, 10005);
        let json = serde_json::to_string(&req_on).unwrap();
        assert!(json.contains(r#""command":"ledctrl"#));
        assert!(json.contains(r#""led_node":"chamber_light""#));
        assert!(json.contains(r#""led_mode":"on""#));

        let req_off = LedCtrlRequest::new(LedNode::Chamber, false, 10006);
        let json_off = serde_json::to_string(&req_off).unwrap();
        assert!(json_off.contains(r#""led_mode":"off""#));
    }

    #[test]
    fn test_led_ctrl_request_new_flashing_json() {
        let req = LedCtrlRequest::new_flashing(
            LedNode::Chamber,
            FlashTiming {
                on_ms: 500,
                off_ms: 500,
                loops: 3,
                interval_ms: 1000,
            },
            10005,
        );
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"ledctrl"#));
        assert!(json.contains(r#""led_node":"chamber_light""#));
        assert!(json.contains(r#""led_mode":"flashing""#));
        assert!(json.contains(r#""led_on_time":500"#));
        assert!(json.contains(r#""led_off_time":500"#));
        assert!(json.contains(r#""loop_times":3"#));
        assert!(json.contains(r#""interval_time":1000"#));
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

    /// Every `print_option` setter, as BambuStudio builds it: one setting field each.
    #[test]
    fn test_print_option_requests_json() {
        use crate::types::control::{AirPurificationMode, NozzleBlobDetectMode};
        let cases = [
            (
                serde_json::to_value(AutoRecoveryRequest::new(true, 1)).unwrap(),
                serde_json::json!({"option": 1, "auto_recovery": true}),
            ),
            (
                serde_json::to_value(AutoRecoveryRequest::new(false, 1)).unwrap(),
                serde_json::json!({"option": 0, "auto_recovery": false}),
            ),
            (
                serde_json::to_value(FilamentBackupRequest::new(true, 1)).unwrap(),
                serde_json::json!({"auto_switch_filament": true}),
            ),
            (
                serde_json::to_value(FilamentTangleDetectRequest::new(false, 1)).unwrap(),
                serde_json::json!({"filament_tangle_detect": false}),
            ),
            (
                serde_json::to_value(NozzleBlobDetectRequest::new(true, 1)).unwrap(),
                serde_json::json!({"nozzle_blob_detect": true}),
            ),
            (
                serde_json::to_value(SmartNozzleBlobDetectRequest::new(
                    NozzleBlobDetectMode::Auto,
                    1,
                ))
                .unwrap(),
                serde_json::json!({"nozzle_blob_detect_v2": 2}),
            ),
            (
                serde_json::to_value(AirPrintDetectRequest::new(true, 1)).unwrap(),
                serde_json::json!({"air_print_detect": true}),
            ),
            (
                serde_json::to_value(AirPurificationRequest::new(AirPurificationMode::Outside, 1))
                    .unwrap(),
                serde_json::json!({"air_purification": 2}),
            ),
        ];
        for (value, setting) in cases {
            let mut expected = serde_json::json!({"command": "print_option", "sequence_id": "1"});
            for (key, field) in setting.as_object().unwrap() {
                expected[key] = field.clone();
            }
            assert_eq!(value["print"], expected);
        }
    }

    /// The settings commands outside `print_option`, as BambuStudio builds them.
    #[test]
    fn test_safety_and_storage_setting_requests_json() {
        use crate::types::control::DoorOpenCheck;
        assert_eq!(
            serde_json::to_value(DoorOpenCheckRequest::new(DoorOpenCheck::PausePrint, 1)).unwrap(),
            serde_json::json!({"system": {"command": "set_door_stat", "sequence_id": "1", "config": 2}})
        );
        assert_eq!(
            serde_json::to_value(IdleHeatingProtectionRequest::new(true, 1)).unwrap(),
            serde_json::json!({"print": {"command": "set_against_continued_heating_mode", "sequence_id": "1", "enable": true}})
        );
        assert_eq!(
            serde_json::to_value(StoreSentFilesRequest::new(false, 1)).unwrap(),
            serde_json::json!({"system": {"command": "print_cache_set", "sequence_id": "1", "config": false}})
        );
    }

    /// `xcam_control_set` as BambuStudio's `command_xcam_control` builds it.
    #[test]
    fn test_xcam_control_request_json() {
        use crate::types::control::{XcamHaltSensitivity, XcamModule};
        assert_eq!(
            serde_json::to_value(XcamControlRequest::new(
                XcamModule::SpaghettiDetector,
                true,
                Some(XcamHaltSensitivity::High),
                1
            ))
            .unwrap(),
            serde_json::json!({"xcam": {
                "command": "xcam_control_set", "sequence_id": "1",
                "module_name": "spaghetti_detector", "control": true, "enable": true,
                "print_halt": true, "halt_print_sensitivity": "high"
            }})
        );
        let value = serde_json::to_value(XcamControlRequest::new(
            XcamModule::FodCheck,
            false,
            None,
            1,
        ))
        .unwrap();
        assert_eq!(value["xcam"]["module_name"], "fod_check");
        assert!(value["xcam"].get("halt_print_sensitivity").is_none());
    }

    #[test]
    fn test_buzzer_request_json() {
        let req = BuzzerRequest::new(crate::types::control::BuzzerMode::Chirp, 10010);
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
        let req = PrintSpeedRequest::new(crate::types::control::PrintSpeed::Sport, 10012);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"print_speed"#));
        assert!(json.contains(r#""param":"3""#));
    }

    #[test]
    fn test_ams_control_request_json() {
        let req = AmsControlRequest::new(AmsControlOp::Resume, 10013);
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
        let req = AmsFilamentSettingRequest::new(0, 1, PLA_SPEC, 10015)
            .with_sub_brands("Bambu PLA Basic")
            .with_color("FF0000FF")
            .unwrap();
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
        let req = AmsFilamentSettingRequest::new(255, 0, PLA_SPEC, 10016);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""tray_sub_brands":"PLA Basic""#));
    }

    #[test]
    fn test_ams_filament_setting_unset_optional_fields() {
        // The description is required; the optional fields go out as an empty color and no
        // setting_id.
        let req = AmsFilamentSettingRequest::new(0, 1, PLA_SPEC, 10026);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""tray_info_idx":"GFA01""#));
        assert!(json.contains(r#""tray_color":"""#));
        assert!(!json.contains("setting_id"));
    }

    #[test]
    fn test_ams_filament_setting_external_spool_derives_tray_id() {
        // BambuStudio's call sites pass slot_id 0 for a virtual tray (`DeviceManager.cpp:4853`,
        // `:4877` — `command_ams_filament_settings(vt_id, 0, ...)`) and the command derives
        // tag_tray_id = VIRTUAL_TRAY_DEPUTY_ID for either external address, never 0. bambuddy
        // sends the same trio for a single external slot: ams 255, slot 0, tray 254.
        for ams_id in [254, 255] {
            let req = AmsFilamentSettingRequest::new(ams_id, 0, PLA_SPEC, 10024);
            let json = serde_json::to_string(&req).unwrap();
            assert!(json.contains(&format!(r#""ams_id":{ams_id}"#)));
            assert!(
                json.contains(r#""slot_id":0"#),
                "slot_id must stay 0: {json}"
            );
            assert!(
                json.contains(r#""tray_id":254"#),
                "tray_id must derive to 254, never 0: {json}"
            );
        }
    }

    #[test]
    fn test_ams_filament_setting_standard_slot_and_tray_coincide() {
        // On a standard AMS the two fields carry the same value, which is why omitting slot_id
        // went unnoticed — it is only the virtual-tray and AMS-HT cases that diverge.
        let req = AmsFilamentSettingRequest::new(1, 3, PLA_SPEC, 10025);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""ams_id":1"#));
        assert!(json.contains(r#""slot_id":3"#));
        assert!(json.contains(r#""tray_id":3"#));
    }

    #[test]
    fn test_ams_filament_setting_uppercases_tray_color() {
        // The firmware parses a lowercase hex letter in tray_color as 0 and stores the
        // corrupted value while acking success (P1S firmware 01.10.00.00). Normalizing inside
        // `with_color` is the single fix point.
        let req = AmsFilamentSettingRequest::new(0, 1, PLA_SPEC, 10019)
            .with_color("09ff00ff")
            .unwrap();
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""tray_color":"09FF00FF""#));
    }

    #[test]
    fn test_ams_filament_setting_strips_color_hash_and_keeps_case_elsewhere() {
        let req = AmsFilamentSettingRequest::new(0, 1, PLA_SPEC, 10020)
            .with_sub_brands("Bambu PLA Basic")
            .with_color("#ff5100ff")
            .unwrap();
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""tray_color":"FF5100FF""#));
        // Case is meaningful in these two and must survive untouched.
        assert!(json.contains(r#""tray_type":"PLA""#));
        assert!(json.contains(r#""tray_sub_brands":"Bambu PLA Basic""#));
    }

    #[test]
    fn test_ams_filament_setting_color_widths() {
        // A 6-digit color gets an opaque alpha; anything but 6 or 8 hex digits is refused rather
        // than sent for the printer to misread.
        let req = AmsFilamentSettingRequest::new(0, 1, PLA_SPEC, 10021)
            .with_color("#ff0000")
            .unwrap();
        assert!(
            serde_json::to_string(&req)
                .unwrap()
                .contains(r#""tray_color":"FF0000FF""#)
        );
        for bad in ["", "FFF", "FF0000F", "GG0000FF", "FF0000FF00"] {
            let result = AmsFilamentSettingRequest::new(0, 1, PLA_SPEC, 1).with_color(bad);
            assert!(
                matches!(result, Err(crate::error::Error::InvalidArgument(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn test_ams_filament_setting_omits_setting_id_by_default() {
        // setting_id is a separate optional wire field; absent, not null, when unset.
        let req = AmsFilamentSettingRequest::new(0, 1, PLA_SPEC, 10022);
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("setting_id"));
    }

    #[test]
    fn test_ams_filament_setting_with_setting_id() {
        // The long preset id belongs here, not in tray_info_idx — a 19-character id in the
        // short field is what an A1 stored as 8 characters while acking success.
        let req = AmsFilamentSettingRequest::new(0, 1, PLA_SPEC, 10023)
            .with_setting_id("PF12345678901234567");
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""tray_info_idx":"GFA01""#));
        assert!(json.contains(r#""setting_id":"PF12345678901234567""#));
    }

    #[test]
    fn test_ams_change_filament_omits_extruder_id_when_none() {
        // Without a Filament Track Switch the payload must be byte-identical to the pre-FTS
        // form: the key is absent, not null.
        let req = AmsChangeFilamentRequest::load(0, 1, ChangeTemps::FIRMWARE, None, 40009);
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("extruder_id"));
    }

    #[test]
    fn test_ams_change_filament_emits_extruder_id_when_set() {
        // On an FTS machine every AMS reports 0xE and a command naming no extruder is
        // discarded in silence, so the key must reach the wire when the caller supplies it.
        let req = AmsChangeFilamentRequest::load(0, 1, ChangeTemps::FIRMWARE, Some(1), 40010);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""extruder_id":1"#));
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
        for (req, cmd) in [
            (StandardControlRequest::pause(10018), "pause"),
            (StandardControlRequest::resume(10018), "resume"),
            (StandardControlRequest::stop(10018), "stop"),
            (
                StandardControlRequest::new(StandardCommand::CloseAirFilter, 10018),
                "close_air_filt",
            ),
        ] {
            let json = serde_json::to_string(&req).unwrap();
            assert!(json.contains(&format!(r#""command":"{}""#, cmd)));
        }
    }
}
