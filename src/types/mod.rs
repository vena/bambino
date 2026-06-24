//! # Common Protocol Types and Schemas
//!
//! Exposes structural types and state schemas used across the printer control
//! interface and state telemetry report channels.

pub mod telemetry;

pub use telemetry::{
    AmsStatusReport, AmsTray, AmsUnit, DeviceTelemetry, HmsEntry, NozzleCollection, NozzleInfo,
    PrintTelemetry, TelemetryReport,
};
