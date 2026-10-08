//! # P2 Series (P2S CoreXY) Quirks
//!
//! Configures transport parameters, thermal layouts, and camera corrections for the P2S platform.

use crate::ams::AmsPoolComposition;
use crate::camera::CameraProtocol;
use crate::quirks::{BedMax, BuildVolume, DoorSensor, DryRule, ModelQuirks, SafetyLimits};

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

/// P2S CoreXY platform.
///
/// ## FTPS TLS 1.2 cap
///
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
/// the `X2D` row, whose mechanism has been falsified outright.
pub(crate) const P2S: ModelQuirks = ModelQuirks {
    ftps_tls_1_2: true,
    door: DoorSensor::Stat,
    chamber_temperature_sensor: true,
    wallclock_rtsp_timestamps: true,
    auxiliary_left2_fan: true,
    airduct_mode: true,
    store_sent_files: true,
    ai_monitoring: true,
    ..ModelQuirks::new(
        SafetyLimits {
            volume: BuildVolume::cube(P2S_Z_MAX),
            nozzle_temp_max: P2S_NOZZLE_TEMP_MAX,
            bed_temp_max: BedMax::Flat(P2S_BED_TEMP_MAX),
            chamber_heater_temp_max: None,
            bed_on_z: true,
        },
        CameraProtocol::Rtsps,
        AmsPoolComposition::Independent {
            max_standard: 4,
            max_ht: 4,
        },
        DryRule::Firmware {
            min: P2S_MIN_REMOTE_DRY_FIRMWARE,
            in_first_release: false,
        },
    )
};
