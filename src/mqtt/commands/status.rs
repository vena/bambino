//! Status query commands (pushall, get_version, clean_print_error).

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use serde::Serialize;

use super::clamp_task_id;

/// Payload schema to trigger a complete state dump ("pushall") from the printer.
#[derive(Debug, Clone, Serialize)]
pub struct PushAllPayload {
    pub command: &'static str,
    pub sequence_id: String,
}

/// Requests a full state dump from the printer (all telemetry fields at once).
#[derive(Debug, Clone, Serialize)]
pub struct PushAllRequest {
    pub pushing: PushAllPayload,
}

impl PushAllRequest {
    pub fn new(sequence_id: u64) -> Self {
        Self {
            pushing: PushAllPayload {
                command: "pushall",
                sequence_id: clamp_task_id(sequence_id).to_string(),
            },
        }
    }
}

/// Payload schema to retrieve hardware/firmware version strings from the expansion bus.
#[derive(Debug, Clone, Serialize)]
pub struct GetVersionPayload {
    pub command: &'static str,
    pub sequence_id: String,
}

/// Queries the printer for its hardware and firmware version info.
#[derive(Debug, Clone, Serialize)]
pub struct GetVersionRequest {
    pub info: GetVersionPayload,
}

impl GetVersionRequest {
    pub fn new(sequence_id: u64) -> Self {
        Self {
            info: GetVersionPayload {
                command: "get_version",
                sequence_id: clamp_task_id(sequence_id).to_string(),
            },
        }
    }
}
