//! # Common Protocol Types and Schemas
//!
//! Exposes structural types and state schemas used across the printer control
//! interface and state telemetry report channels.

pub mod telemetry;

pub use telemetry::{
    AirductCollection, AirductModeListEntry, AirductPart, AmsDrySetting, AmsStatusReport, AmsTray,
    AmsUnit, CtcInfo, CtcTelemetry, DeviceTelemetry, HmsEntry, IpcamTelemetry, NozzleCollection,
    NozzleInfo, PrinterTelemetry, TelemetryReport, VirtualTray, is_developer_mode,
};
