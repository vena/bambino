//! # MQTT v3.1.1 State Engine & Printer Control API
//!
//! Exposes state telemetry handshakes, QoS 1 publish confirmation lists, keep-alive
//! PING checks, write-channel zombie detection trackers, and serializable G-code and
//! print dispatch payloads.
//!
//! Consolidates core structures under a unified module namespace, promoting zero-modification
//! usage across host systems and constraint-heavy bare-metal configurations.

pub mod client;
pub mod commands;

pub use client::{BambuMqttClient, MqttMessage};
pub use commands::{
    AirductRequest, AmsChangeFilamentRequest, AmsControlRequest, AmsFilamentDryingRequest,
    AmsFilamentSettingRequest, AmsGetRfidRequest, BuzzerRequest, CalibrationRequest,
    CleanPrintErrorRequest, GCodeRequest, GetVersionRequest, LedCtrlRequest, PrintJobConfig,
    ProjectAmsMapping2Entry, ProjectFileRequest, PromptSoundRequest, PushAllRequest,
    SkipObjectsRequest, StandardControlRequest, clamp_task_id,
};
