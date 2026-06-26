//! # Common Protocol Types and Schemas
//!
//! Exposes structural types and state schemas used across the printer control
//! interface and state telemetry report channels.

pub mod telemetry;
pub mod version;

pub use telemetry::{
    AirductCollection, AirductModeListEntry, AirductPart, AmsDrySetting, AmsStatusReport, AmsTray,
    AmsUnit, BedInfo, BedTelemetry, CtcInfo, CtcTelemetry, DeviceTelemetry, ExtToolTelemetry,
    ExtruderCollection, ExtruderInfo, HmsEntry, IpcamTelemetry, LightReport, NozzleCollection,
    NozzleInfo, PrinterTelemetry, TelemetryReport, VirtualTray, is_developer_mode,
};
pub use version::{VersionInfo, VersionModule};
