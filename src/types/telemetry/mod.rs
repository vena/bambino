//! # State Telemetry Payload Schemas
//!
//! Provides structured, allocation-friendly deserialization models for the
//! local MQTTS Port 8883 state telemetry streams [REF-MQTT-ENV].
//!
//! Supports permissive parsing for platform discrepancies (such as the variable
//! types of `sdcard` presence markers) and implements binary unpacking helpers
//! for composite packed temperatures, home/status flags, and door sensors.
//!
//! ## Architectural Alignment
//! * **Quirks Integration:** Raw elements (e.g., `device.airduct.parts` or `ctc.info.temp`)
//!   are fully parsed into clean schemas to allow model-specific behaviors to be evaluated
//!   via the quirks engine.

pub mod ams;
pub(crate) mod bits;
pub mod device;
pub mod diagnostics;
mod loose;
pub(crate) mod merge;
pub mod report;
pub mod stage;
mod temps;
pub mod xcam;

#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

pub use ams::{
    AmsDryFanStatus, AmsDrySetting, AmsDryStatus, AmsDrySubStatus, AmsFilamentStep,
    AmsStatusReport, AmsTray, AmsUnit, AmsUnitModel, DryBlockReason, FilamentSwitchInlet,
    VirtualTray,
};
pub use bits::{FUN2_REMOTE_DRY_BIT, hex_bit};
pub use device::{
    AirductCollection, AirductModeListEntry, AirductPart, BedInfo, BedTelemetry, DeviceTelemetry,
    ExtToolTelemetry, ExtruderCollection, ExtruderInfo, NozzleCollection, NozzleInfo,
};
pub use diagnostics::{CtcInfo, CtcTelemetry, HmsEntry, IpcamTelemetry};
pub(crate) use loose::{
    deserialize_permissive_hms_u32, deserialize_permissive_opt_bool,
    deserialize_permissive_opt_f64, deserialize_permissive_opt_flags,
    deserialize_permissive_opt_int, deserialize_permissive_opt_string,
    deserialize_permissive_string,
};
pub use report::{
    LightReport, NetInfo, PrintPauseList, PrintPausePoint, PrinterTelemetry, SdcardState,
};
pub use stage::PrintStage;
pub use temps::{HeaterTemps, NozzleTemps, unpack_temperature};
pub(crate) use temps::{decode_bed_temperatures, decode_nozzle_temperatures};
pub use xcam::{XcamDetector, XcamSensitivity, XcamTelemetry};

use crate::types::control::{FanTarget, PrintStatus};

/// Unified top-level telemetry report received from the printer's local MQTT broker.
///
/// Under the over-the-wire schema, updates are typically nested within separate
/// top-level domains depending on which micro-system published the frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryReport {
    /// Telemetry parameters representing the physical printer state machine.
    #[serde(default)]
    pub print: Option<PrinterTelemetry>,

    /// Network and hardware board capability descriptors, when sent at the top level.
    ///
    /// Crate-private: pushall frames on H2/P2/X2 nest it inside `print` instead, so reading this
    /// field misses exactly the frames carrying the most data. Use [`device()`](Self::device).
    #[serde(default)]
    pub(crate) device: Option<DeviceTelemetry>,

    /// Developer LAN Mode bitmask field (hex string), when sent at the top level.
    /// Drifts between top-level and `print.fun` depending on firmware version [REF-MQTT-ENV §3.2.1];
    /// use [`fun()`](Self::fun).
    pub(crate) fun: Option<String>,

    /// Second capability bitfield (hex string) — see [`PrinterTelemetry::fun2`]; use
    /// [`fun2()`](Self::fun2).
    ///
    /// Accepted at the top level as well as inside `print` on the same first-found-wins terms as
    /// `fun`. BambuStudio itself reads only `print.fun2`
    /// (`DeviceManager.cpp`); the top-level slot mirrors `fun`'s documented drift rather
    /// than a location observed carrying `fun2`.
    #[serde(default)]
    pub(crate) fun2: Option<String>,
}

impl TelemetryReport {
    /// Returns the bed's temperatures, or `None` when this report carries none.
    ///
    /// Handles the different wire formats across printer generations automatically:
    /// new-gen composite-packed `device.bed`, pushall-nested `print.device.bed`, and
    /// old-gen direct `bed_temper`/`bed_target_temper` fields.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// if let Some(bed) = report.bed_temperatures() {
    ///     println!("Bed: {}°C (target {}°C)", bed.actual, bed.target);
    /// }
    /// ```
    #[must_use]
    pub fn bed_temperatures(&self) -> Option<HeaterTemps> {
        let print = self.print.as_ref();
        decode_bed_temperatures(
            self.device(),
            print.and_then(|p| p.bed_temper),
            print.and_then(|p| p.bed_target_temper),
        )
    }

    /// Returns one entry per nozzle this report carries temperatures for, empty when none.
    ///
    /// Prefers per-extruder `device.extruder.info`; falls back to the flat
    /// `nozzle_temper`/`nozzle_target_temper` fields, including their IDEX routing quirk — see
    /// `PrinterClient::nozzle_temperatures`.
    #[must_use]
    pub fn nozzle_temperatures(&self) -> Vec<NozzleTemps> {
        let print = self.print.as_ref();
        decode_nozzle_temperatures(
            self.device(),
            print.and_then(|p| p.nozzle_temper),
            print.and_then(|p| p.nozzle_target_temper),
        )
    }

    /// Returns `fan`'s speed as a percentage (0-100), or `None` when this report doesn't carry it.
    #[must_use]
    pub fn fan_percent(&self, fan: FanTarget) -> Option<u8> {
        let print = self.print.as_ref();
        decode_fan_percent(
            fan,
            FanStrings {
                part_cooling: print.and_then(|p| p.cooling_fan_speed.as_deref()),
                aux_left: print.and_then(|p| p.big_fan1_speed.as_deref()),
                chamber_exhaust: print.and_then(|p| p.big_fan2_speed.as_deref()),
            },
            self.device(),
        )
    }

    /// Returns the `DeviceTelemetry` sub-object, checking both wire locations it can arrive at.
    ///
    /// Top-level `device` (incremental updates) is checked first, falling back to
    /// pushall-nested `print.device` (H2/P2/X2 models). Returns `None` if neither location is
    /// present. Use this instead of manually checking both locations for
    /// nozzle/extruder/airduct/ctc/ext_tool sub-telemetry.
    pub fn device(&self) -> Option<&DeviceTelemetry> {
        self.device
            .as_ref()
            .or_else(|| self.print.as_ref().and_then(|print| print.device.as_ref()))
    }

    /// Returns the `fun` Developer LAN Mode bitmask, checking both wire locations it can
    /// arrive at.
    ///
    /// Mirrors `device()`'s fallback order — top-level `fun` is checked first,
    /// falling back to `print.fun` [REF-MQTT-ENV §3.2.1].
    pub fn fun(&self) -> Option<&str> {
        self.fun
            .as_deref()
            .or_else(|| self.print.as_ref().and_then(|print| print.fun.as_deref()))
    }

    /// Whether Developer LAN Mode is on, from [`fun()`](Self::fun) — see [`is_developer_mode`].
    #[must_use]
    pub fn is_developer_mode(&self) -> Option<bool> {
        is_developer_mode(self.fun()?)
    }

    /// Returns the `fun2` capability bitfield, checking both wire locations.
    ///
    /// Same first-found-wins order as [`fun`](Self::fun). Prefer [`fun2_bit`](Self::fun2_bit)
    /// over parsing this yourself — see [`hex_bit`] for why the string can't go through
    /// `u64::from_str_radix`.
    #[must_use]
    pub fn fun2(&self) -> Option<&str> {
        self.fun2
            .as_deref()
            .or_else(|| self.print.as_ref().and_then(|print| print.fun2.as_deref()))
    }

    /// Reads a single bit of the `fun2` capability bitfield.
    ///
    /// `None` only when `fun2` is absent or carries no hex digits at all — "the printer didn't
    /// say", which is distinct from a bit that is present and clear. A bit index past the end of
    /// the string reads `false`, matching BambuStudio's extractor, which returns `0` rather than
    /// failing (`DevUtil.cpp`).
    ///
    /// Known bits (`DeviceManager.cpp`): `0` print with eMMC, `3` PA mode,
    /// **`5` remote dry supported** (see [`supports_remote_dry`](Self::supports_remote_dry)),
    /// `6` update-remain hide display, `7` print TPU from left extruder (model-gated),
    /// `8` active arc fitting, `17` model internal storage, `19` check track-switch matches
    /// sliced printer, `21`-`22` AMS preload version, `23` filament manual multi-color.
    #[must_use]
    pub fn fun2_bit(&self, bit: u32) -> Option<bool> {
        hex_bit(self.fun2()?, bit)
    }

    /// Whether the printer reports its own support for remote AMS drying — `fun2` bit 5.
    ///
    /// This is the printer-side half of the drying gate; the attached unit's heater is the other
    /// half (see [`AmsUnitModel::supports_drying`]). BambuStudio requires both
    /// (`Widgets/AMSControl.cpp`).
    ///
    /// `None` means the printer never reported `fun2`, which is not the same as reporting `0` —
    /// older firmware omits the field entirely, and treating that as "unsupported" would refuse
    /// drying on hardware that has always accepted it.
    #[must_use]
    pub fn supports_remote_dry(&self) -> Option<bool> {
        self.fun2_bit(FUN2_REMOTE_DRY_BIT)
    }

    /// Returns the printer's activity classification from `print.gcode_state`.
    #[must_use]
    pub fn print_status(&self) -> Option<PrintStatus> {
        self.print.as_ref()?.print_status()
    }
}

/// The three 0-15 step fan strings `print` carries (`cooling_fan_speed`, `big_fan1_speed`, `big_fan2_speed`).
#[derive(Default, Clone, Copy)]
pub(crate) struct FanStrings<'a> {
    pub(crate) part_cooling: Option<&'a str>,
    pub(crate) aux_left: Option<&'a str>,
    pub(crate) chamber_exhaust: Option<&'a str>,
}

/// Shared fan decode behind [`TelemetryReport::fan_percent`] and `PrinterClient::fan_speed`.
///
/// Three fans report a 0-15 step string in `print`; the second left auxiliary fan reports a
/// direct percentage in `device.airduct.parts` instead.
pub(crate) fn decode_fan_percent(
    fan: FanTarget,
    strings: FanStrings<'_>,
    device: Option<&DeviceTelemetry>,
) -> Option<u8> {
    use crate::quirks::decode_fan_percentage;
    match fan {
        FanTarget::PartCooling => decode_fan_percentage(strings.part_cooling),
        FanTarget::AuxiliaryLeft => decode_fan_percentage(strings.aux_left),
        FanTarget::ChamberExhaust => decode_fan_percentage(strings.chamber_exhaust),
        FanTarget::AuxiliaryLeft2 => {
            let id = fan.airduct_part_id()?;
            device?.airduct.as_ref()?.part_percent(id)
        }
    }
}

/// Evaluates Developer LAN Mode from the `fun` hex string [REF-MQTT-ENV §3.2.1].
///
/// Returns `Some(true)` when developer mode is enabled (MQTT signature NOT required),
/// `Some(false)` when disabled, or `None` if the string carries no hex digits. Bit 29
/// (`0x20000000`) is the `MQTT_SIGNATURE_REQUIRED` flag — when clear, developer mode is on.
/// Read with [`hex_bit`], so a `0x` prefix, whitespace or a string longer than 16 digits all work.
#[must_use]
pub fn is_developer_mode(fun_hex: &str) -> Option<bool> {
    hex_bit(fun_hex, bits::FUN_MQTT_SIGNATURE_REQUIRED_BIT).map(|required| !required)
}

/// A `print` object with more keys than this is a full status report.
///
/// bambuddy's test (`bambu_mqtt.py`, the Filament Backup block, `len(print_data) > 30`): H2D
/// firmware also sends small heartbeat frames whose `home_flag` is partial, so on a printer that
/// sends `cfg`, a `home_flag` read as settings or capability bits is taken only from a full
/// report. P1 and A1 send no `cfg`, and their small diff frames carry a complete `home_flag`.
pub(crate) const FULL_REPORT_MIN_KEYS: usize = 30;

/// Counts the keys of a payload's `print` object without decoding their values.
///
/// `None` when the payload isn't JSON or has no `print` object.
pub(crate) fn print_key_count(payload: &[u8]) -> Option<usize> {
    use serde::de::{Deserializer, IgnoredAny, MapAccess, Visitor};

    struct KeyCount(usize);

    impl<'de> Deserialize<'de> for KeyCount {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Count;
            impl<'de> Visitor<'de> for Count {
                type Value = KeyCount;
                fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    f.write_str("an object")
                }
                fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<KeyCount, A::Error> {
                    let mut keys = 0;
                    while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {
                        keys += 1;
                    }
                    Ok(KeyCount(keys))
                }
            }
            deserializer.deserialize_map(Count)
        }
    }

    #[derive(Deserialize)]
    struct Envelope {
        print: Option<KeyCount>,
    }

    serde_json::from_slice::<Envelope>(payload)
        .ok()?
        .print
        .map(|count| count.0)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
