#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use serde::Serialize;

/// Payload schema to trigger a complete state dump ("pushall") from the printer.
#[derive(Debug, Clone, Serialize)]
pub struct PushAllPayload {
    pub command: &'static str,
    pub sequence_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PushAllRequest {
    pub pushing: PushAllPayload,
}

impl PushAllRequest {
    pub fn new(sequence_id: u64) -> Self {
        Self {
            pushing: PushAllPayload {
                command: "pushall",
                sequence_id: sequence_id.to_string(),
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

#[derive(Debug, Clone, Serialize)]
pub struct GetVersionRequest {
    pub info: GetVersionPayload,
}

impl GetVersionRequest {
    pub fn new(sequence_id: u64) -> Self {
        Self {
            info: GetVersionPayload {
                command: "get_version",
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}
