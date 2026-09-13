//! # Command Handles
//!
//! Every fire-and-forget [`PrinterClient`](super::PrinterClient) command returns a
//! [`CommandHandle`] naming the `sequence_id` it was published under. The printer answers every
//! command except `pushall` with an echo carrying that id [REF-MQTT-ACK], so the handle is what
//! matches a response on the report topic to the command that caused it.

#[cfg(not(feature = "std"))]
use alloc::string::String;

/// Whether the printer answers a command with an echo of its `sequence_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AckExpectation {
    /// The printer echoes the command.
    ///
    /// Confirmed on a P1S for every command bambino sends except `pushall`
    /// (`reference/03_mqtt_telemetry.md` §REF-MQTT-ACK); other models are unmeasured.
    Echoes,
    /// The printer sends no echo, so publishing is the whole outcome.
    ///
    /// `pushall` is the one such command: it triggers a state dump instead
    /// [REF-MQTT-LIFECYCLE].
    SettlesOnPublish,
}

/// Names a command this client published, for matching the printer's answer to it.
///
/// Only a [`PrinterClient`](super::PrinterClient) mints one, so a handle always refers to a
/// `sequence_id` this client actually sent.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommandHandle {
    command: String,
    sequence_id: u32,
    ack: AckExpectation,
}

impl CommandHandle {
    pub(crate) fn new(command: String, sequence_id: u32, ack: AckExpectation) -> Self {
        Self {
            command,
            sequence_id,
            ack,
        }
    }

    /// Returns the wire command name, e.g. `"gcode_line"` or `"ams_filament_drying"`.
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Returns the `sequence_id` the command was published under, which the printer echoes back.
    pub fn sequence_id(&self) -> u32 {
        self.sequence_id
    }

    /// Returns whether an echo is coming for this command.
    pub fn ack(&self) -> AckExpectation {
        self.ack
    }
}
