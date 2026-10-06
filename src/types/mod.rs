//! # Types & Telemetry Schemas
//!
//! Shared data types used across the crate — most importantly [`PrinterTelemetry`],
//! the deserialized form of the JSON state reports the printer pushes over MQTT.
//! Also includes [`VersionInfo`] for firmware version queries and AMS/device
//! sub-structures like [`AmsTray`], [`DeviceTelemetry`], and [`ExtruderInfo`].

pub mod control;
pub mod drying;
pub mod telemetry;
pub mod version;

pub use drying::{DEFAULT_COMMAND_COOLING_TEMP, DryingMaterial};

pub use control::{
    BuzzerMode, CalibrationOption, FanTarget, LedNode, LightMode, PrintSpeed, PrintStatus,
};
pub use telemetry::{
    AirductCollection, AirductModeListEntry, AirductPart, AmsDryFanStatus, AmsDrySetting,
    AmsDryStatus, AmsDrySubStatus, AmsFilamentStep, AmsStatusReport, AmsTray, AmsUnit,
    AmsUnitModel, BedInfo, BedTelemetry, CtcInfo, CtcTelemetry, DeviceTelemetry, ExtToolTelemetry,
    ExtruderCollection, ExtruderInfo, HmsEntry, IpcamTelemetry, LightReport, NetInfo,
    NozzleCollection, NozzleInfo, PrinterTelemetry, SdcardState, TelemetryReport, VirtualTray,
    XcamDetector, XcamSensitivity, XcamTelemetry, is_developer_mode,
};
pub use telemetry::{HeaterTemps, NozzleTemps, hex_bit, unpack_temperature};
pub use version::{VersionInfo, VersionModule};
