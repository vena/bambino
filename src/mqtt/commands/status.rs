//! Status query commands (pushall, get_version, get_access_code).

use serde::Serialize;

use super::ClampedTaskId;

/// Payload schema to trigger a complete state dump ("pushall") from the printer.
#[derive(Debug, Clone, Serialize)]
pub struct PushAllPayload {
    /// Wire command name, always `"pushall"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
}

/// Requests a full state dump from the printer (all telemetry fields at once).
pub type PushAllRequest = super::Pushing<PushAllPayload>;

impl PushAllRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "pushall";

    /// Builds a `pushall` request.
    pub fn new(sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            pushing: PushAllPayload {
                command: Self::COMMAND,
                sequence_id: sequence_id.into(),
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
    pub sequence_id: ClampedTaskId,
}

/// Queries the printer for its hardware and firmware version info.
pub type GetVersionRequest = super::Info<GetVersionPayload>;

impl GetVersionRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "get_version";

    /// Builds a `get_version` request.
    pub fn new(sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            info: GetVersionPayload {
                command: Self::COMMAND,
                sequence_id: sequence_id.into(),
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
    pub sequence_id: ClampedTaskId,
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
pub type GetAccessCodeRequest = super::System<GetAccessCodePayload>;

impl GetAccessCodeRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "get_access_code";

    /// Builds a `get_access_code` request.
    pub fn new(sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            system: GetAccessCodePayload {
                command: Self::COMMAND,
                sequence_id: sequence_id.into(),
            },
        }
    }
}
