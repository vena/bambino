//! G-code dispatch command payload.

#[cfg(not(feature = "std"))]
use alloc::string::String;

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
    pub sequence_id: ClampedTaskId,
}

/// Sends a raw G-code line to the printer for immediate execution.
pub type GCodeRequest = super::Print<GCodePayload>;

impl GCodeRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "gcode_line";

    /// Creates a request envelope wrapping a raw G-code payload.
    ///
    /// Ensures the line ends with a newline (`\n`), appending one only if missing, so the
    /// printer's stream parser sees the end-of-command boundary.
    pub fn new(gcode_line: &str, sequence_id: impl Into<ClampedTaskId>) -> Self {
        let mut param = String::from(gcode_line);
        if !param.ends_with('\n') {
            param.push('\n');
        }
        Self {
            print: GCodePayload {
                command: Self::COMMAND,
                param,
                sequence_id: sequence_id.into(),
            },
        }
    }
}
