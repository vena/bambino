//! # Types & Telemetry Schemas
//!
//! Shared data types used across the crate — most importantly [`PrinterTelemetry`],
//! the deserialized form of the JSON state reports the printer pushes over MQTT.
//! Also includes [`VersionInfo`] for firmware version queries and AMS/device
//! sub-structures like [`AmsTray`], [`DeviceTelemetry`], and [`ExtruderInfo`].

pub mod telemetry;
pub mod version;

pub use telemetry::{
    AirductCollection, AirductModeListEntry, AirductPart, AmsDrySetting, AmsFilamentStep,
    AmsStatusReport, AmsTray, AmsUnit, BedInfo, BedTelemetry, CtcInfo, CtcTelemetry,
    DeviceTelemetry, ExtToolTelemetry, ExtruderCollection, ExtruderInfo, HmsEntry, IpcamTelemetry,
    LightReport, NetInfo, NozzleCollection, NozzleInfo, PrinterTelemetry, SdcardState,
    TelemetryReport, VirtualTray, decode_nozzle_temperatures, is_developer_mode,
};
pub use version::{VersionInfo, VersionModule};
