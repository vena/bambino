//! G-code dispatch command payload.

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use serde::Serialize;

use super::ClampedTaskId;

/// Queues raw G-code strings directly to the printer's motion execution controller.
///
/// Under the Bambu protocol specification, physical moves, manual extrusions, and
/// temperature targets are issued by packing standard G-code lines into this wrapper.
#[derive(Debug, Clone, Serialize)]
pub struct GCodePayload {
    /// Wire command name, always `"gcode_line"`.
    pub command: &'static str,
    /// Raw G-code line, newline-terminated by [`GCodeRequest::new`].
    pub param: String,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
}

/// Sends a raw G-code line to the printer for immediate execution.
#[derive(Debug, Clone, Serialize)]
pub struct GCodeRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: GCodePayload,
}

impl GCodeRequest {
    /// Creates a request envelope wrapping a raw G-code payload.
    ///
    /// **Execution Note:** The raw G-code string is strictly appended with a newline character (`\n`)
    /// to ensure the physical controller's stream parser identifies the end-of-command boundary.
    pub fn new(gcode_line: &str, sequence_id: impl Into<ClampedTaskId>) -> Self {
        let mut param = String::from(gcode_line);
        if !param.ends_with('\n') {
            param.push('\n');
        }
        Self {
            print: GCodePayload {
                command: "gcode_line",
                param,
                sequence_id: sequence_id.into().to_string(),
            },
        }
    }
}
