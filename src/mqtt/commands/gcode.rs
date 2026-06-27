//! G-code dispatch command payload.

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use serde::Serialize;

/// Queues raw G-code strings directly to the printer's motion execution controller.
///
/// Under the Bambu protocol specification, physical moves, manual extrusions, and
/// temperature targets are issued by packing standard G-code lines into this wrapper.
#[derive(Debug, Clone, Serialize)]
pub struct GCodePayload {
    pub command: &'static str,
    pub param: String,
    pub sequence_id: String,
}

/// Sends a raw G-code line to the printer for immediate execution.
#[derive(Debug, Clone, Serialize)]
pub struct GCodeRequest {
    pub print: GCodePayload,
}

impl GCodeRequest {
    /// Creates a request envelope wrapping a raw G-code payload.
    ///
    /// **Execution Note:** The raw G-code string is strictly appended with a newline character (`\n`)
    /// to ensure the physical controller's stream parser identifies the end-of-command boundary.
    pub fn new(gcode_line: &str, sequence_id: u64) -> Self {
        let mut param = String::from(gcode_line);
        if !param.ends_with('\n') {
            param.push('\n');
        }
        Self {
            print: GCodePayload {
                command: "gcode_line",
                param,
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}
