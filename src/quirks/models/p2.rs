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

/// Firmware release that introduced remote AMS drying, and drying while printing, on the P2S.
///
/// P2S `01.02.00.00` (2026-04-09, <https://wiki.bambulab.com/en/p2s/manual/p2s-firmware-release-history>):
/// "Added support for remote activation of filament drying" and "Added support for 'Print While
/// Drying' feature". The *Filament drying guide for AMS 2 Pro and AMS HT* gives the same minimum in
/// both lists; bambuddy's two drying tables agree (under `P2S` and its model code `N7`).
pub const P2S_MIN_REMOTE_DRY_FIRMWARE: &str = "01.02.00.00";

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
    /// came back, which `FtpsClient::upload_file` already does
    /// unconditionally (see its doc comment in `src/ftps/client.rs`) — this
    /// quirk alone would not have been a complete fix.
    ///
    /// **The session-ticket mechanism is firmware-version-scoped at best.** It presupposes the
    /// printer negotiates TLS 1.3 in the first place, and `bambuddy`'s later nine-printer probe
    /// (issue #2780) found six P2S units refusing 1.3 outright — correcting their own earlier
    /// claim that "the P2S evidently does offer 1.3". On that firmware the negotiated version
    /// was already 1.2 and this cap changes nothing. Either the firmware moved between the two
    /// reports, or #1401 was fixed by something else in the same change. Kept because a reporter
    /// confirmed the symptom cleared and nobody has hardware to re-test it on; treat it as
    /// confirmed-by-symptom, not confirmed-by-mechanism. This remains the only one of bambino's
    /// two TLS 1.2 caps whose symptom a session-ticket problem could explain at all — see
    /// `X2Quirks::enforces_ftps_tls_1_2`, whose mechanism has been falsified outright.
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

    fn active_chamber_heater_max_temp_c(&self) -> Option<u16> {
        None
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

    /// Firmware-gated from [`P2S_MIN_REMOTE_DRY_FIRMWARE`]; a reported `fun2` bit 5 still wins.
    fn ams_remote_drying_support(
        &self,
        ctx: &crate::quirks::QuirkContext,
    ) -> crate::quirks::Support {
        crate::quirks::remote_dry_from_firmware(ctx, P2S_MIN_REMOTE_DRY_FIRMWARE)
    }

    fn ams_drying_while_printing_support(
        &self,
        ctx: &crate::quirks::QuirkContext,
    ) -> crate::quirks::Support {
        crate::quirks::dry_while_printing_from_firmware(ctx, P2S_MIN_REMOTE_DRY_FIRMWARE)
    }

    fn is_bed_on_z(&self) -> bool {
        true
    }

    fn requires_wallclock_rtsp_timestamps(&self) -> bool {
        true
    }

    fn supports_auxiliary_left2_fan(&self) -> bool {
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

    /// P2S does not run vibration compensation the way the X1/P1 series does, so the flag is
    /// forced off. Rests on bambuddy `be18ebb3` alone and is unverified on hardware here — see
    /// [`crate::quirks::ModelQuirks::supports_vibration_compensation`] and issue #133.
    fn supports_vibration_compensation(&self) -> bool {
        false
    }

    fn supports_airduct_mode(&self) -> bool {
        true
    }
}
