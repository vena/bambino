//! # P2 Series (P2S CoreXY) Quirks
//!
//! Configures transport parameters, thermal layouts, and camera corrections for the P2S platform.

use crate::camera::CameraProtocol;
use crate::quirks::ModelQuirks;
use crate::types::PrinterTelemetry;

/// Build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row.
pub const P2S_Z_MAX: f32 = 256.0;
/// Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.
pub const P2S_NOZZLE_TEMP_MAX: u16 = 300;
/// Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.
pub const P2S_BED_TEMP_MAX: u16 = 110;

/// Quirks for the P2S CoreXY platform.
pub struct P2Quirks;

impl ModelQuirks for P2Quirks {
    fn uses_plaintext_ftps_data_channel(&self) -> bool {
        false
    }

    /// P2S firmware `01.02.00.00`'s embedded vsFTPd can't process TLS 1.3's asynchronous session-ticket model on the FTPS data channel — transfers truncate mid-stream with `426 "Failure reading network stream"`.
    /// This is a firmware bug, not a real TLS-1.3 incompatibility: independently confirmed by the
    /// `bambuddy` project (reporter `@iitazz`, upstream issue #1401), which hit the identical symptom
    /// only after its own client started defaulting to TLS 1.3. See [REF-FTPS-CONN] in
    /// `reference/02_ftps.md` §2.1.
    ///
    /// The cap narrows the race, it doesn't close it: `bambuddy`'s own
    /// follow-up (issue #1417) found P2S can still return a transient `426` on
    /// the final post-upload response even under TLS 1.2 — the data-channel
    /// close still occasionally races the `226` confirmation, just later and
    /// less often than the pre-cap mid-stream truncation. What actually closes
    /// it is verifying the transfer via `SIZE` regardless of which reply code
    /// came back, which `BambuFtpsClient::upload_file` already does
    /// unconditionally (see its doc comment in `src/ftps/client.rs`) — this
    /// quirk alone would not have been a complete fix.
    fn enforces_ftps_tls_1_2(&self) -> bool {
        true
    }

    fn is_door_open(&self, telemetry: &PrinterTelemetry) -> bool {
        telemetry.is_door_open_from_stat()
    }

    fn has_door_sensor_field(&self, telemetry: &PrinterTelemetry) -> bool {
        telemetry.stat.is_some()
    }

    fn has_door_sensor(&self) -> bool {
        true
    }

    fn camera_protocol(&self) -> CameraProtocol {
        CameraProtocol::Rtsps
    }

    fn ignores_chamber_temperature(&self) -> bool {
        false
    }

    fn has_stg_cur_idle_bug(&self) -> bool {
        false
    }

    fn has_active_chamber_heater(&self) -> bool {
        false
    }

    fn physical_nozzle_count(&self) -> u8 {
        1
    }

    fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition {
        crate::ams::AmsPoolComposition::Independent {
            max_standard: 4,
            max_ht: 4,
        }
    }

    fn supports_nozzle_offset_calibration(&self) -> bool {
        false
    }

    fn is_bed_on_z(&self) -> bool {
        true
    }

    fn requires_wallclock_rtsp_timestamps(&self) -> bool {
        true
    }

    fn supports_auxiliary_right_fan(&self) -> bool {
        true
    }

    fn reports_auxiliary_fan_percentage(&self) -> bool {
        true
    }

    fn z_max(&self) -> f32 {
        P2S_Z_MAX
    }

    fn x_max(&self) -> f32 {
        P2S_Z_MAX
    }

    fn y_max(&self) -> f32 {
        P2S_Z_MAX
    }

    fn nozzle_temp_max(&self) -> u16 {
        P2S_NOZZLE_TEMP_MAX
    }

    fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16 {
        P2S_BED_TEMP_MAX
    }

    fn supports_airduct_mode(&self) -> bool {
        true
    }
}
