//! Decoded heater temperatures, and the one decoder for the wire's composite packing.

#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};

use super::DeviceTelemetry;

/// Raw values above this are composite-packed `(target << 16) | actual` [REF-THER-DECODE].
pub(crate) const TEMP_COMPOSITE_THRESHOLD: u32 = 500;

/// One heater's actual and target temperature, in °C.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct HeaterTemps {
    /// Measured temperature.
    pub actual: u16,
    /// Target temperature; `0` when the heater is off or the wire carries no target.
    pub target: u16,
}

/// One nozzle's temperatures, in °C.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NozzleTemps {
    /// Nozzle id: `0` on single-nozzle models; `0` (right) and `1` (left) on IDEX.
    pub id: u8,
    /// Measured temperature.
    pub actual: u16,
    /// Target temperature.
    pub target: u16,
}

/// Rounds a non-negative wire temperature to the nearest whole degree.
///
/// Rounds rather than truncating, so `27.625` reads `28`. A negative or non-finite value
/// saturates to `0`. Written out by hand because `f64::round` needs `std`.
pub(crate) fn round_temp(value: f64) -> u16 {
    (value + 0.5) as u16
}

/// Resolves a composite-packed temperature into actual and target [REF-THER-DECODE].
///
/// Accepts `f64` because the wire sends both integers and floats depending on model. Values
/// ≤ 500 are direct temperatures (target `0`). Values > 500 are composite-packed: upper 16 bits
/// target, lower 16 bits actual.
///
/// Apply it only to the packed fields — `chamber_temper`, [`ExtruderInfo::temp`],
/// [`BedInfo::temp`] and [`CtcInfo::temp`] — and prefer their `temperatures()` accessors, which
/// do it for you. The flat `bed_temper`/`nozzle_temper` fields are never packed: a legitimate
/// reading above 500 there would decode as nonsense.
///
/// [`ExtruderInfo::temp`]: super::ExtruderInfo::temp
/// [`BedInfo::temp`]: super::BedInfo::temp
/// [`CtcInfo::temp`]: super::CtcInfo::temp
#[must_use]
pub fn unpack_temperature(raw: f64) -> HeaterTemps {
    let value = (raw + 0.5) as u32;
    if value <= TEMP_COMPOSITE_THRESHOLD {
        HeaterTemps {
            actual: value as u16,
            target: 0,
        }
    } else {
        HeaterTemps {
            actual: (value & 0xFFFF) as u16,
            target: (value >> 16) as u16,
        }
    }
}

/// Shared bed-temperature decode behind [`TelemetryReport::bed_temperatures()`](super::TelemetryReport::bed_temperatures) and `PrinterClient::bed_temperatures()`.
///
/// Prefers new-gen composite-packed `device.bed.info.temp`, then the flat old-gen
/// `bed_temper`/`bed_target_temper`. `None` when none of them is present.
pub(crate) fn decode_bed_temperatures(
    device: Option<&DeviceTelemetry>,
    bed_temper: Option<f64>,
    bed_target_temper: Option<f64>,
) -> Option<HeaterTemps> {
    if let Some(temps) = device
        .and_then(|d| d.bed.as_ref())
        .and_then(|bed| bed.info.as_ref())
        .and_then(|info| info.temperatures())
    {
        return Some(temps);
    }
    if bed_temper.is_none() && bed_target_temper.is_none() {
        return None;
    }
    Some(HeaterTemps {
        actual: bed_temper.map_or(0, round_temp),
        target: bed_target_temper.map_or(0, round_temp),
    })
}

/// Shared nozzle-temperature decode behind [`TelemetryReport::nozzle_temperatures()`](super::TelemetryReport::nozzle_temperatures) and `PrinterClient::nozzle_temperatures()`.
///
/// Prefers `device.extruder.info` (composite-packed per-nozzle temperatures, decoded via
/// [`ExtruderInfo::temperatures()`](super::ExtruderInfo::temperatures)). Falls back to the flat
/// `nozzle_temper`/`nozzle_target_temper` fields when absent: one entry for a single-nozzle
/// model, or — for a dual-nozzle (IDEX) model with no live extruder temps yet — the wire's
/// undocumented routing quirk: `nozzle_temper` is nozzle 1 (left)'s actual reading and
/// `nozzle_target_temper` is nozzle 0 (right)'s target, each nozzle only getting half of its
/// own reading from the flat fields. Empty when nothing has been reported.
pub(crate) fn decode_nozzle_temperatures(
    device: Option<&DeviceTelemetry>,
    nozzle_temper: Option<f64>,
    nozzle_target_temper: Option<f64>,
) -> Vec<NozzleTemps> {
    if let Some(info) = device
        .and_then(|d| d.extruder.as_ref())
        .and_then(|extruder| extruder.info.as_deref())
        && !info.is_empty()
    {
        return info
            .iter()
            .filter_map(|entry| {
                let temps = entry.temperatures()?;
                Some(NozzleTemps {
                    id: entry.id,
                    actual: temps.actual,
                    target: temps.target,
                })
            })
            .collect();
    }

    if nozzle_temper.is_none() && nozzle_target_temper.is_none() {
        return Vec::new();
    }

    // Exclude rack-stored spare nozzles before counting — BambuStudio appends them
    // to the same `nozzle.info` array as installed ones, distinguished only by
    // `NozzleInfo::is_rack_stored()`. Without this, an H2C (single hotend + spare-nozzle
    // rack) misclassifies as IDEX.
    let is_idex = device
        .and_then(|d| d.nozzle.as_ref())
        .map(|n| {
            n.info
                .iter()
                .flatten()
                .filter(|nz| !nz.is_rack_stored())
                .count()
                >= 2
        })
        .unwrap_or(false);

    let actual = nozzle_temper.map_or(0, round_temp);
    let target = nozzle_target_temper.map_or(0, round_temp);

    if is_idex {
        vec![
            NozzleTemps {
                id: 0,
                actual: 0,
                target,
            },
            NozzleTemps {
                id: 1,
                actual,
                target: 0,
            },
        ]
    } else {
        vec![NozzleTemps {
            id: 0,
            actual,
            target,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unpack_rounds_instead_of_truncating() {
        assert_eq!(unpack_temperature(27.625).actual, 28);
        assert_eq!(unpack_temperature(27.4).actual, 27);
    }

    #[test]
    fn test_unpack_composite() {
        // 45 << 16 | 38
        let temps = unpack_temperature(f64::from((45u32 << 16) | 38));
        assert_eq!(
            temps,
            HeaterTemps {
                actual: 38,
                target: 45
            }
        );
    }

    #[test]
    fn test_bed_none_when_nothing_reported() {
        assert_eq!(decode_bed_temperatures(None, None, None), None);
        assert!(decode_nozzle_temperatures(None, None, None).is_empty());
    }
}
