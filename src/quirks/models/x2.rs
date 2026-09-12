//! # X2 Series (X2D CoreXY) Quirks
//!
//! Handles parameters unique to the X2D dual-carriage auxiliary-cooling model.
//!
//! Build volumes: Main Nozzle 256×256×260mm, Aux/Dual 235.5×256×256mm.
//! Z-max uses the conservative aux/dual value (256mm).

use crate::camera::CameraProtocol;
use crate::quirks::ModelQuirks;
use crate::types::PrinterTelemetry;

/// Build volume Z depth (mm) — uses the conservative aux/dual-nozzle value, not the main-nozzle value; see module docs.
pub const X2D_Z_MAX: f32 = 256.0;
/// Build volume X width (mm) — conservative aux/dual-nozzle value (235.5mm, smaller than the
/// main-nozzle profile's 256mm); see module docs.
pub const X2D_X_MAX: f32 = 235.5;
/// Build volume Y depth (mm) — 256mm across all nozzle profiles.
pub const X2D_Y_MAX: f32 = 256.0;
/// Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.
pub const X2D_NOZZLE_TEMP_MAX: u16 = 300;
/// Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.
pub const X2D_BED_TEMP_MAX: u16 = 120;
/// Chamber temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Chamber Temperature row.
pub const X2D_CHAMBER_TEMP_MAX: u16 = 65;

/// Firmware release that introduced remote AMS drying, and drying while printing, on the X2D.
///
/// X2D `01.01.00.00` (2026-04-14, <https://wiki.bambulab.com/en/x2d/manual/x2d-firmware-release-history>):
/// "Added support for remote activation of filament drying" and "Added support for 'Print While
/// Drying' feature" (the latter needs the separately sold AMS external power supply). The *Filament
/// drying guide for AMS 2 Pro and AMS HT* gives the same minimum in both lists, and bambuddy's
/// `_DRY_WHILE_PRINTING_MIN_FIRMWARE` agrees; its `_DRYING_MIN_FIRMWARE` omits the X2D.
///
/// This is the earliest published X2D release, so an unread version is inferred supported rather
/// than assumed.
pub const X2D_MIN_REMOTE_DRY_FIRMWARE: &str = "01.01.00.00";

/// Quirks for the X2D dual-carriage, dual-nozzle CoreXY platform.
pub struct X2Quirks;

impl ModelQuirks for X2Quirks {
    fn uses_plaintext_ftps_data_channel(&self) -> bool {
        false
    }

    /// X2D firmware `01.01.00.00` fails the implicit-FTPS handshake on port 990 with `[SSL: WRONG_VERSION_NUMBER]`.
    ///
    /// **Confirmed by symptom; the mechanism this cap was originally justified by has since been
    /// falsified.** The earlier reading — that the error came from the client offering a TLS 1.3
    /// `ClientHello` — does not survive measurement. `bambuddy`'s nine-printer farm probe
    /// (issue #2780) pinned three results: a cleartext `421` banner on the TLS port produces
    /// `[SSL: WRONG_VERSION_NUMBER]`, byte for byte what the field reports; a TLS-1.2-only server
    /// answering a client forced to 1.3 produces `TLSV1_ALERT_PROTOCOL_VERSION` instead; and an
    /// uncapped client reaches a 1.2-only peer unaided. So `WRONG_VERSION_NUMBER` means the
    /// peer's first bytes were **not a TLS record at all**, a version mismatch cannot produce it,
    /// and reaching a TLS-1.2-only peer needs no cap. The leading hypothesis is now an FTP-level
    /// refusal sent in the clear (such as `421 Too many connections`) from a printer out of
    /// connection slots.
    ///
    /// The cap is kept anyway: the original reporter (`@vasmarfas`, bambuddy issue #1638) saw the
    /// symptom clear, and a cap costs nothing on a printer that never offers TLS 1.3. What is
    /// wrong is the recorded reasoning and the confidence it implied, not the setting. bambuddy
    /// marked their own X2D entry RE-TEST WANTED for the same reason. **Re-test on X2D hardware**
    /// — a packet capture of a port-990 connect showing whether the printer's first bytes are a
    /// TLS record or a cleartext FTP reply would settle it, and if it is a cleartext `421` this
    /// cap is unrelated to the fix and should be reconsidered.
    ///
    /// See [REF-FTPS-CONN] in `reference/02_ftps.md` §2.1.
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

    fn physical_nozzle_count(&self) -> u8 {
        2
    }

    fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition {
        crate::ams::AmsPoolComposition::Independent {
            max_standard: 4,
            max_ht: 8,
        }
    }

    fn supports_nozzle_offset_calibration(&self) -> bool {
        true
    }

    /// Firmware-gated from [`X2D_MIN_REMOTE_DRY_FIRMWARE`]; a reported `fun2` bit 5 still wins.
    fn ams_remote_drying_support(
        &self,
        ctx: &crate::quirks::QuirkContext,
    ) -> crate::quirks::Support {
        crate::quirks::remote_dry_reported_or(
            ctx,
            crate::quirks::firmware_gate(
                ctx,
                X2D_MIN_REMOTE_DRY_FIRMWARE,
                crate::quirks::Support::Inferred(true),
            ),
        )
    }

    fn ams_drying_while_printing_support(
        &self,
        ctx: &crate::quirks::QuirkContext,
    ) -> crate::quirks::Support {
        crate::quirks::dry_while_printing_unless_reported_off(
            ctx,
            crate::quirks::firmware_gate(
                ctx,
                X2D_MIN_REMOTE_DRY_FIRMWARE,
                crate::quirks::Support::Inferred(true),
            ),
        )
    }

    fn is_bed_on_z(&self) -> bool {
        true
    }

    fn supports_auxiliary_left2_fan(&self) -> bool {
        true
    }

    fn z_max(&self) -> f32 {
        X2D_Z_MAX
    }

    fn x_max(&self) -> f32 {
        X2D_X_MAX
    }

    fn y_max(&self) -> f32 {
        X2D_Y_MAX
    }

    fn nozzle_temp_max(&self) -> u16 {
        X2D_NOZZLE_TEMP_MAX
    }

    fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16 {
        X2D_BED_TEMP_MAX
    }

    fn active_chamber_heater_max_temp_c(&self) -> Option<u16> {
        Some(X2D_CHAMBER_TEMP_MAX)
    }

    fn supports_airduct_mode(&self) -> bool {
        true
    }

    fn has_chamber_exhaust_fan(&self) -> bool {
        true
    }
}
