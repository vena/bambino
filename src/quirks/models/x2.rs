//! # X2 Series (X2D CoreXY) Quirks
//!
//! Handles parameters unique to the X2D dual-carriage auxiliary-cooling model.
//!
//! Build volumes: Main Nozzle 256×256×260mm, Aux/Dual 235.5×256×256mm.
//! Z-max uses the conservative aux/dual value (256mm).

use crate::ams::AmsPoolComposition;
use crate::camera::CameraProtocol;
use crate::quirks::{
    BedMax, BuildVolume, DoorSensor, DryRule, ModelQuirks, NozzleLayout, SafetyLimits,
};

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

/// X2D dual-carriage, dual-nozzle CoreXY platform.
///
/// ## FTPS TLS 1.2 cap
///
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
/// **Unverified on X2D, and no upstream can settle it — don't re-check them expecting an
/// answer.** All three were searched (2026-09-17): bambuddy still says outright that nobody
/// there has an X2D, so their entry stays RE-TEST WANTED; ha-bambulab caps
/// `maximum_version` to TLS 1.2 unconditionally for every model, so it never reaches the
/// question and its silence is not evidence; and BambuStudio has no implicit-FTPS client in
/// its open tree at all (its LAN file transfer is in the closed BambuNetworking library),
/// so the vendor source cannot speak to this either. What bambuddy *did* add is the
/// instrument rather than the answer: on `WRONG_VERSION_NUMBER` their client now opens one
/// plain connection to :990 and logs the printer's own reply, because the TLS layer eats
/// those bytes before the error surfaces. This needs *an* X2D, not a re-reading of upstream
/// — one owner running `openssl s_client -connect <ip>:990` against it is enough.
///
/// See [REF-FTPS-CONN] in `reference/02_ftps.md` §2.1.
pub(crate) const X2D: ModelQuirks = ModelQuirks {
    ftps_tls_1_2: true,
    door: DoorSensor::Stat,
    chamber_temperature_sensor: true,
    nozzles: NozzleLayout::Dual,
    auxiliary_left2_fan: true,
    airduct_mode: true,
    chamber_exhaust_fan: true,
    ..ModelQuirks::new(
        SafetyLimits {
            volume: BuildVolume {
                x: X2D_X_MAX,
                y: X2D_Y_MAX,
                z: X2D_Z_MAX,
            },
            nozzle_temp_max: X2D_NOZZLE_TEMP_MAX,
            bed_temp_max: BedMax::Flat(X2D_BED_TEMP_MAX),
            chamber_heater_temp_max: Some(X2D_CHAMBER_TEMP_MAX),
            bed_on_z: true,
        },
        CameraProtocol::Rtsps,
        AmsPoolComposition::Independent {
            max_standard: 4,
            max_ht: 8,
        },
        DryRule::Firmware {
            min: X2D_MIN_REMOTE_DRY_FIRMWARE,
            in_first_release: true,
        },
    )
};
