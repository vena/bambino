//! Closed sets of control values shared by the command builders, the telemetry decoders and `PrinterClient`.
//!
//! These live below both `mqtt::commands` and `client` so a request constructor can take the
//! typed value instead of the raw wire string or integer, without the command layer depending on
//! the client. `crate::client` re-exports every one of them.

use crate::error::Error;

/// Generates `ALL`, `as_wire`, `Display` and a case-sensitive wire-name `FromStr` for a fieldless enum.
macro_rules! wire_enum {
    ($ty:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        impl $ty {
            /// Every variant, in declaration order.
            pub const ALL: &'static [$ty] = &[$($ty::$variant),+];

            /// The value's wire spelling.
            #[must_use]
            pub const fn as_wire(self) -> &'static str {
                match self {
                    $($ty::$variant => $wire),+
                }
            }
        }

        impl core::fmt::Display for $ty {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str(self.as_wire())
            }
        }

        impl core::str::FromStr for $ty {
            type Err = Error;

            fn from_str(s: &str) -> Result<Self, Error> {
                match s {
                    $($wire => Ok($ty::$variant),)+
                    _ => Err(Error::InvalidArgument(
                        alloc_format!(concat!("unknown ", stringify!($ty), " '{}'"), s).into(),
                    )),
                }
            }
        }
    };
}

#[cfg(not(feature = "std"))]
use alloc::format as alloc_format;
#[cfg(feature = "std")]
use std::format as alloc_format;

/// Target onboard cooling fans [REF-CLIM-FANS].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum FanTarget {
    /// Primary part cooling fan (Port 1).
    #[cfg_attr(feature = "cli", value(name = "part"))]
    PartCooling,
    /// Primary left-side auxiliary fan (Port 2).
    #[cfg_attr(feature = "cli", value(name = "aux"))]
    AuxiliaryLeft,
    /// Chamber exhaust/filtration fan (Port 3).
    #[cfg_attr(feature = "cli", value(name = "exhaust"))]
    ChamberExhaust,
    /// Secondary left-side auxiliary fan (Port 10, supported on X2D and P2S) [REF-CLIM-FANS].
    ///
    /// Despite the wire port number (M106 `P10`) and read-side airduct id (160) suggesting a
    /// "right" fan, BambuStudio's `DevFan.h` names decoded id 10 `FAN_REMOTE_COOLING_1_IDX` —
    /// a second left-side auxiliary fan, distinct from [`AuxiliaryLeft`](Self::AuxiliaryLeft)'s
    /// primary port-2 fan (`FAN_REMOTE_COOLING_0_IDX`, mirrored into `big_fan1_speed`).
    /// Confirmed against bambuddy's test suite, which titles this fan "P2S/X2D left auxiliary
    /// part cooling fan" throughout (issue #60).
    #[cfg_attr(feature = "cli", value(name = "left2"))]
    AuxiliaryLeft2,
}

impl FanTarget {
    /// Every fan, in declaration order.
    pub const ALL: &'static [FanTarget] = &[
        FanTarget::PartCooling,
        FanTarget::AuxiliaryLeft,
        FanTarget::ChamberExhaust,
        FanTarget::AuxiliaryLeft2,
    ];

    /// The M106 `P` port that drives this fan.
    #[must_use]
    pub const fn write_port(self) -> u16 {
        match self {
            FanTarget::PartCooling => 1,
            FanTarget::AuxiliaryLeft => 2,
            FanTarget::ChamberExhaust => 3,
            FanTarget::AuxiliaryLeft2 => 10,
        }
    }

    /// The `device.airduct.parts[].id` this fan reports under, for the one fan read from there.
    ///
    /// A different address space from [`write_port`](Self::write_port): the three other fans
    /// report through `print.*_fan_speed` strings instead and return `None`.
    #[must_use]
    pub const fn airduct_part_id(self) -> Option<u32> {
        match self {
            FanTarget::AuxiliaryLeft2 => Some(160),
            FanTarget::PartCooling | FanTarget::AuxiliaryLeft | FanTarget::ChamberExhaust => None,
        }
    }

    /// Whether `quirks` says this model has the fan.
    #[must_use]
    pub fn is_supported_by(self, quirks: &crate::quirks::ModelQuirks) -> bool {
        match self {
            FanTarget::PartCooling => true,
            FanTarget::AuxiliaryLeft => quirks.has_auxiliary_left_fan(),
            FanTarget::ChamberExhaust => quirks.has_chamber_exhaust_fan(),
            FanTarget::AuxiliaryLeft2 => quirks.has_auxiliary_left2_fan(),
        }
    }
}

/// Buzzer alarm/attention chime mode [REF-MQTT-LIFECYCLE]; supported on models with a physical fire alarm buzzer (H2 series).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum BuzzerMode {
    /// Silent/disarmed.
    Silent,
    /// Alarm triggered.
    Alarm,
    /// Beeping attention chime.
    Chirp,
}

impl BuzzerMode {
    /// The `buzzer_ctrl` `mode` code: `0` silent, `1` alarm, `2` chirp.
    #[must_use]
    pub const fn code(self) -> i32 {
        match self {
            BuzzerMode::Silent => 0,
            BuzzerMode::Alarm => 1,
            BuzzerMode::Chirp => 2,
        }
    }
}

/// Smart nozzle blob detection mode, `print_option`'s `nozzle_blob_detect_v2` and `print.cfg` bits 43-44 [REF-MQTT-TELEMETRY].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum NozzleBlobDetectMode {
    /// Detection off.
    Off,
    /// Detection on.
    On,
    /// The printer decides per print.
    Auto,
}

impl NozzleBlobDetectMode {
    /// The wire code: `0` off, `1` on, `2` auto.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            NozzleBlobDetectMode::Off => 0,
            NozzleBlobDetectMode::On => 1,
            NozzleBlobDetectMode::Auto => 2,
        }
    }

    /// Decodes a wire code; `None` for any value outside `0..=2`.
    #[must_use]
    pub const fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(NozzleBlobDetectMode::Off),
            1 => Some(NozzleBlobDetectMode::On),
            2 => Some(NozzleBlobDetectMode::Auto),
            _ => None,
        }
    }
}

/// Where the chamber air is purified at the end of a print, `print_option`'s `air_purification` and `print.cfg` bits 36-37 [REF-MQTT-TELEMETRY].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum AirPurificationMode {
    /// No purification at print end.
    Disabled,
    /// Recirculate through the internal filter.
    Inside,
    /// Exhaust to the outside.
    Outside,
}

impl AirPurificationMode {
    /// The wire code: `0` disabled, `1` inside, `2` outside.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            AirPurificationMode::Disabled => 0,
            AirPurificationMode::Inside => 1,
            AirPurificationMode::Outside => 2,
        }
    }

    /// Decodes a wire code; `None` for any value outside `0..=2`.
    #[must_use]
    pub const fn from_code(code: u32) -> Option<Self> {
        match code {
            0 => Some(AirPurificationMode::Disabled),
            1 => Some(AirPurificationMode::Inside),
            2 => Some(AirPurificationMode::Outside),
            _ => None,
        }
    }
}

/// Velocity and acceleration scaling presets for active print jobs [REF-MQTT-LIFECYCLE].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum PrintSpeed {
    /// 50% max acceleration and feedrate limits.
    Silent = 1,
    /// 100% nominal feedrate limit.
    Standard = 2,
    /// 124% nominal feedrate limit.
    Sport = 3,
    /// 166% nominal feedrate limit.
    Ludicrous = 4,
}

impl PrintSpeed {
    /// Every level, slowest first.
    pub const ALL: &'static [PrintSpeed] = &[
        PrintSpeed::Silent,
        PrintSpeed::Standard,
        PrintSpeed::Sport,
        PrintSpeed::Ludicrous,
    ];

    /// Classifies a raw `spd_lvl` telemetry value (`1`-`4`, the same values `print_speed` sends); `None` for an out-of-range level.
    #[must_use]
    pub fn from_level(level: u8) -> Option<Self> {
        match level {
            1 => Some(PrintSpeed::Silent),
            2 => Some(PrintSpeed::Standard),
            3 => Some(PrintSpeed::Sport),
            4 => Some(PrintSpeed::Ludicrous),
            _ => None,
        }
    }

    /// The wire level, `1`-`4` — the inverse of [`from_level`](Self::from_level).
    #[must_use]
    pub const fn level(self) -> u8 {
        self as u8
    }
}

/// Decoded classification of the printer's high-level `gcode_state` telemetry field.
///
/// `Unknown` covers an unrecognized wire value; callers needing to tell that apart from a known
/// state should inspect the raw `gcode_state` string directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrintStatus {
    /// No print job active or loaded (wire: `"IDLE"`).
    Idle,
    /// Print preparing to start — homing, bed leveling, or priming, physical
    /// motion in progress (wire: `"PREPARE"`).
    Preparing,
    /// Printer is slicing a job on-device, before any physical motion (wire: `"SLICING"`).
    ///
    /// Distinct from [`Preparing`](Self::Preparing): nothing is moving yet. It is still a
    /// busy state — a job is in flight — so treat it like the other active states when
    /// deciding whether the printer can accept new work.
    Slicing,
    /// Print job actively executing (wire: `"RUNNING"`).
    Running,
    /// Print job paused, resumable (wire: `"PAUSE"`).
    Paused,
    /// Print job completed successfully (wire: `"FINISH"`).
    Finished,
    /// Print job aborted by an error condition (wire: `"FAILED"`).
    Failed,
    /// Unrecognized wire value — see the enum's doc comment.
    Unknown,
}

impl PrintStatus {
    /// Classifies a raw `gcode_state` wire value (firmware casing: `"IDLE"`, `"PREPARE"`, `"SLICING"`, `"RUNNING"`, `"PAUSE"`, `"FINISH"`, `"FAILED"` [REF-MQTT-IDLEBUG]).
    #[must_use]
    pub fn from_gcode_state(state: &str) -> Self {
        match state {
            "IDLE" => PrintStatus::Idle,
            "PREPARE" => PrintStatus::Preparing,
            "SLICING" => PrintStatus::Slicing,
            "RUNNING" => PrintStatus::Running,
            "PAUSE" => PrintStatus::Paused,
            "FINISH" => PrintStatus::Finished,
            "FAILED" => PrintStatus::Failed,
            _ => PrintStatus::Unknown,
        }
    }

    /// The `gcode_state` wire value for this status; `None` for [`Unknown`](Self::Unknown).
    #[must_use]
    pub const fn as_str(self) -> Option<&'static str> {
        match self {
            PrintStatus::Idle => Some("IDLE"),
            PrintStatus::Preparing => Some("PREPARE"),
            PrintStatus::Slicing => Some("SLICING"),
            PrintStatus::Running => Some("RUNNING"),
            PrintStatus::Paused => Some("PAUSE"),
            PrintStatus::Finished => Some("FINISH"),
            PrintStatus::Failed => Some("FAILED"),
            PrintStatus::Unknown => None,
        }
    }

    /// True while a job is in flight — preparing, slicing, running or paused — so the printer
    /// shouldn't be given new work or motion that could collide with a part.
    ///
    /// `Unknown` is not busy, so a caller gating on safety must treat a missing status
    /// (`PrinterClient::print_status() == None`) or `Unknown` as "can't confirm idle" itself.
    #[must_use]
    pub fn is_busy(self) -> bool {
        matches!(
            self,
            PrintStatus::Preparing
                | PrintStatus::Slicing
                | PrintStatus::Running
                | PrintStatus::Paused
        )
    }
}

/// A printer LED fixture addressed by `ledctrl` and reported in `lights_report`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum LedNode {
    /// The chamber light (`chamber_light`).
    #[cfg_attr(feature = "cli", value(name = "chamber"))]
    Chamber,
    /// The second chamber light on models with two (`chamber_light2`).
    #[cfg_attr(feature = "cli", value(name = "chamber2"))]
    Chamber2,
    /// The work light (`work_light`).
    #[cfg_attr(feature = "cli", value(name = "work"))]
    Work,
}

wire_enum!(LedNode {
    Chamber => "chamber_light",
    Chamber2 => "chamber_light2",
    Work => "work_light",
});

/// An LED fixture's mode, as sent in `ledctrl` and reported in `lights_report`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LightMode {
    /// Lit.
    On,
    /// Dark.
    Off,
    /// Cycling on a flash timing.
    Flashing,
}

wire_enum!(LightMode {
    On => "on",
    Off => "off",
    Flashing => "flashing",
});

/// Bitmask flags for selecting hardware calibration routines [REF-MQTT-LIFECYCLE].
///
/// Combine flags with `|` (or collect an iterator of them) to trigger several routines at once,
/// e.g. `CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION`. Only the
/// named constants can be built, so a value never carries bits no routine owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CalibrationOption(pub(crate) u32);

impl CalibrationOption {
    /// Automatic bed mesh leveling.
    pub const BED_LEVELING: Self = Self(2);
    /// Input shaper vibration compensation tuning.
    pub const VIBRATION_COMPENSATION: Self = Self(4);
    /// Motor noise cancellation profiling.
    pub const MOTOR_NOISE_CANCELLATION: Self = Self(8);
    /// First-layer nozzle height calibration.
    pub const NOZZLE_HEIGHT: Self = Self(16);
    /// Heated bed thermal compensation mapping.
    pub const HEATBED_THERMAL: Self = Self(32);

    /// No routines.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// The wire `option` bitmask.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Whether every routine in `other` is also in `self`.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether no routine is selected.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The routines in both `self` and `other`.
    #[must_use]
    pub(crate) const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// The routines in `self` but not in `other`.
    #[must_use]
    pub(crate) const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

impl core::ops::BitOr for CalibrationOption {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl core::ops::BitOrAssign for CalibrationOption {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl core::iter::FromIterator<CalibrationOption> for CalibrationOption {
    fn from_iter<I: IntoIterator<Item = CalibrationOption>>(iter: I) -> Self {
        iter.into_iter().fold(Self::empty(), |acc, opt| acc | opt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::str::FromStr;

    #[test]
    fn test_print_status_is_busy() {
        use PrintStatus::*;
        for status in [Preparing, Slicing, Running, Paused] {
            assert!(status.is_busy(), "{status:?}");
        }
        for status in [Idle, Finished, Failed, Unknown] {
            assert!(!status.is_busy(), "{status:?}");
        }
    }

    #[test]
    fn test_print_status_round_trips_its_wire_value() {
        use PrintStatus::*;
        for status in [Idle, Preparing, Slicing, Running, Paused, Finished, Failed] {
            let wire = status.as_str().expect("known status has a wire value");
            assert_eq!(PrintStatus::from_gcode_state(wire), status);
        }
        assert_eq!(PrintStatus::from_gcode_state(""), Unknown);
        assert_eq!(Unknown.as_str(), None);
    }

    #[test]
    fn test_print_speed_level_round_trips() {
        for &speed in PrintSpeed::ALL {
            assert_eq!(PrintSpeed::from_level(speed.level()), Some(speed));
        }
        assert_eq!(PrintSpeed::from_level(9), None);
    }

    #[test]
    fn test_wire_enums_parse_their_display_form() {
        for &node in LedNode::ALL {
            assert_eq!(LedNode::from_str(&node.to_string()).unwrap(), node);
        }
        assert!(LedNode::from_str("chamber_ligt").is_err());
        assert_eq!(BuzzerMode::Chirp.code(), 2);
        assert_eq!(FanTarget::AuxiliaryLeft2.write_port(), 10);
        assert_eq!(FanTarget::AuxiliaryLeft2.airduct_part_id(), Some(160));
    }

    #[test]
    fn test_calibration_option_collects_and_contains() {
        let opts: CalibrationOption = [
            CalibrationOption::BED_LEVELING,
            CalibrationOption::NOZZLE_HEIGHT,
        ]
        .into_iter()
        .collect();
        assert!(opts.contains(CalibrationOption::BED_LEVELING));
        assert!(!opts.contains(CalibrationOption::HEATBED_THERMAL));
        assert_eq!(opts.bits(), 2 | 16);
        let mut more = CalibrationOption::empty();
        more |= CalibrationOption::HEATBED_THERMAL;
        assert_eq!(more.bits(), 32);
    }
}
