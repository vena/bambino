//! Status query commands (pushall, get_version, get_access_code, clean_print_error).

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use serde::Serialize;

use super::ClampedTaskId;

/// Payload schema to trigger a complete state dump ("pushall") from the printer.
#[derive(Debug, Clone, Serialize)]
pub struct PushAllPayload {
    /// Wire command name, always `"pushall"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
}

/// Requests a full state dump from the printer (all telemetry fields at once).
#[derive(Debug, Clone, Serialize)]
pub struct PushAllRequest {
    /// The `pushing` namespace envelope required by the wire protocol.
    pub pushing: PushAllPayload,
}

impl PushAllRequest {
    /// Builds a `pushall` request.
    pub fn new(sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            pushing: PushAllPayload {
                command: "pushall",
                sequence_id: sequence_id.into().to_string(),
            },
        }
    }
}

/// Payload schema to retrieve hardware/firmware version strings from the expansion bus.
#[derive(Debug, Clone, Serialize)]
pub struct GetVersionPayload {
    /// Wire command name, always `"get_version"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
}

/// Queries the printer for its hardware and firmware version info.
#[derive(Debug, Clone, Serialize)]
pub struct GetVersionRequest {
    /// The `info` namespace envelope required by the wire protocol.
    pub info: GetVersionPayload,
}

impl GetVersionRequest {
    /// Builds a `get_version` request.
    pub fn new(sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            info: GetVersionPayload {
                command: "get_version",
                sequence_id: sequence_id.into().to_string(),
            },
        }
    }
}

/// Payload schema to ask the printer to report its own current LAN access code.
#[derive(Debug, Clone, Serialize)]
pub struct GetAccessCodePayload {
    /// Wire command name, always `"get_access_code"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
}

/// Queries the printer for its own current LAN access code.
///
/// Distinct from the access code the caller supplies to authenticate: this re-reads the value
/// from the printer over an already-authenticated session, which is how a client notices that a
/// rotated code has invalidated its cached credential.
///
/// The reply is `system`-wrapped and echoes the request's `sequence_id`, alongside
/// `access_code`, `result`, and `reason` — confirmed on a P1S via `bambino-cli ack-probe`
/// (issue #140); see `reference/03_mqtt_telemetry.md` for the observed shape.
///
/// Treat the returned code as a credential: it must never be logged or written to disk.
#[derive(Debug, Clone, Serialize)]
pub struct GetAccessCodeRequest {
    /// The `system` namespace envelope required by the wire protocol.
    pub system: GetAccessCodePayload,
}

impl GetAccessCodeRequest {
    /// Builds a `get_access_code` request.
    pub fn new(sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            system: GetAccessCodePayload {
                command: "get_access_code",
                sequence_id: sequence_id.into().to_string(),
            },
        }
    }
}
