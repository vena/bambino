//! # MQTT Client & Command Serialization
//!
//! Low-level MQTT v3.1.1 implementation for talking to Bambu Lab printers.
//!
//! [`MqttClient`] handles the connection handshake, QoS 1 publish/subscribe,
//! keep-alive pings, and zombie detection. The [`commands`] submodule contains all
//! the serializable request structs (G-code dispatch, print control, AMS operations,
//! LED/fan/buzzer commands, etc.) that get published to the printer's command topic.
//!
//! Most users should use [`crate::client::PrinterClient`] instead of this module
//! directly — it wraps `MqttClient` with higher-level methods and safety checks.

pub mod client;
pub mod commands;

pub(crate) const MQTTS_PORT: u16 = 8883;

pub use client::{MqttClient, MqttMessage};
pub use commands::{
    AirductMode, AirductRequest, AmsChangeFilamentRequest, AmsControlRequest,
    AmsFilamentDryingRequest, AmsFilamentSettingRequest, AmsGetRfidRequest, AmsMappingTable,
    BuzzerRequest, CalibrationRequest, CleanPrintErrorRequest, GCodeRequest, GetVersionRequest,
    LedCtrlRequest, PrintJobConfig, PrintSpeedRequest, ProjectFileRequest, PromptSoundRequest,
    PushAllRequest, SkipObjectsRequest, StandardControlRequest, clamp_task_id,
};
