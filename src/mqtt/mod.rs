//! # MQTT Client & Command Serialization
//!
//! Low-level MQTT v3.1.1 implementation for talking to Bambu Lab printers.
//!
//! [`MqttClient`] handles the connection handshake, QoS 1 publish/subscribe,
//! keep-alive pings, and zombie detection. Keep-alive pings and read/write deadlines depend on
//! the [`TimerProvider`](crate::io::TimerProvider) passed to `poll_telemetry`,
//! `publish_command` and `send_ping`, so pass a real platform timer. Zombie detection still
//! needs the caller to call `tick_zombie_check` periodically. The [`commands`] submodule contains all
//! the serializable request structs (G-code dispatch, print control, AMS operations,
//! LED/fan/buzzer commands, etc.) that get published to the printer's command topic.
//!
//! Most users should use [`crate::client::PrinterClient`] instead of this module
//! directly — it wraps `MqttClient` with higher-level methods and safety checks.

pub mod client;
pub mod commands;

/// MQTT-over-TLS port every Bambu printer's local broker listens on.
pub const MQTTS_PORT: u16 = 8883;

pub use client::{
    EchoKey, Liveness, MQTT_ZOMBIE_TIMEOUT_SECS, MqttClient, MqttMessage, echo_key, report_topic,
    request_topic,
};
pub use commands::{
    AirductMode, AirductRequest, AmsChangeFilamentRequest, AmsControlRequest,
    AmsFilamentDryingRequest, AmsFilamentSettingRequest, AmsGetRfidRequest, AmsMappingTable,
    AmsSource, BuzzerRequest, CalibrationMode, CalibrationRequest, ClampedTaskId,
    CleanPrintErrorRequest, DryingParams, GCodeRequest, GetAccessCodeRequest, GetVersionRequest,
    HmsActionRequest, IdleIgnoreRequest, LedCtrlRequest, NozzleRack, PrintJobConfig,
    PrintSpeedRequest, ProjectFileRequest, PromptSoundRequest, PushAllRequest, SkipObjectsRequest,
    StandardControlRequest, UiopRequest, resolve_rack_nozzle_mapping,
};
