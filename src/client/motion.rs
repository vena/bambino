#[cfg(not(feature = "std"))]
use alloc::format;

use crate::error::BambuError;
use crate::ftps::FtpDataStreamFactory;
use crate::io::{AsyncIo, SecureConnect, TimerProvider, TlsConnector};
use crate::mqtt::GCodeRequest;

use super::PrinterClient;

impl<Conn, Timer, RawIO, Tls, Factory> PrinterClient<Conn, Timer, RawIO, Tls, Factory>
where
    Conn: SecureConnect,
    Timer: TimerProvider,
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: FtpDataStreamFactory<RawIO>,
{
    /// Sends a G-code command with model-aware safety validation.
    ///
    /// Rejects commands that would be unsafe on the active model (e.g., partial-axis
    /// homing on bed-on-Z platforms). Use [`send_gcode_raw()`](Self::send_gcode_raw)
    /// to bypass validation when you need unchecked access.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Turn on the part cooling fan at 100%
    /// printer.send_gcode("M106 P1 S255").await?;
    ///
    /// // This will be rejected on CoreXY printers (unsafe partial homing):
    /// // printer.send_gcode("G28 Z").await?;  // -> Err(ModelMismatch)
    /// ```
    pub async fn send_gcode(&mut self, gcode_line: &str) -> Result<u16, BambuError> {
        if self.model.quirks().is_unsafe_homing_command(gcode_line) {
            return Err(BambuError::ModelMismatch(
                "partial-axis homing unsafe on bed-on-Z model".into(),
            ));
        }
        self.send_gcode_raw(gcode_line).await
    }

    /// Dispatches a raw G-code string without model safety checks [REF-MOTO-GCODE].
    ///
    /// Returns the MQTT packet identifier assigned to track publication delivery status.
    pub async fn send_gcode_raw(&mut self, gcode_line: &str) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = GCodeRequest::new(gcode_line, seq);
        self.publish_request(&req).await
    }

    /// Dispatches safe homing operations to prevent hardware collisions.
    ///
    /// **Z-Axis Homing Crash Hazards [REF-MOTO-GCODE]:**
    /// * **Bed-on-Z models** (X1, P1, H2, P2S series) must strictly be homed using a bare `G28`
    ///   to execute the safe firmware-defined toolhead parking sequence. Specifying axis constraints
    ///   (such as `G28 Z`) bypasses this and risks driving the bed directly into a misplaced toolhead.
    /// * **Bed-Slingers** (A1, A1 Mini) can handle targeted homing macros safely, but a bare `G28` is
    ///   highly recommended for standard configurations.
    pub async fn home_axes(&mut self, home_z_only_danger: bool) -> Result<u16, BambuError> {
        let is_bed_on_z = self.model.quirks().is_bed_on_z();

        let gcode = if is_bed_on_z {
            if home_z_only_danger {
                return Err(BambuError::ModelMismatch(
                    "Z-only homing unsafe on bed-on-Z model".into(),
                ));
            }
            "G28"
        } else if home_z_only_danger {
            "G28 Z"
        } else {
            "G28"
        };

        self.send_gcode_raw(gcode).await
    }

    /// Dispatches a manual relative axis movement block.
    ///
    /// **Relative Axis Movement Safety [REF-MOTO-GCODE]:**
    /// For relative movements on the Z-axis, this method automatically wraps the move
    /// in software travel limits (`M211 S1`) and safe reference-mode push/pop blocks
    /// (`M1002 push_ref_mode` / `M1002 pop_ref_mode`) to prevent frame shifting and endstop crashes.
    pub async fn move_relative(
        &mut self,
        axis: char,
        distance: f32,
        feedrate: u32,
    ) -> Result<u16, BambuError> {
        let axis_upper = axis.to_ascii_uppercase();
        if axis_upper == 'Z' {
            let gcode = self
                .model
                .quirks()
                .relative_z_move_gcode(distance, feedrate);
            if gcode.is_empty() {
                return Err(BambuError::ModelMismatch(
                    "Z-axis move exceeds model travel limits".into(),
                ));
            }
            self.send_gcode_raw(&gcode).await
        } else {
            let gcode = format!("G91\nG0 {}{:.2} F{}\nG90", axis_upper, distance, feedrate);
            self.send_gcode_raw(&gcode).await
        }
    }

    /// Dispatches a manual relative extrusion command sequence [REF-GCODE-EXTRUDE].
    ///
    /// Configures the active extruder drive gear to relative mode (`M83`) and feeds
    /// the specified length of filament (in mm) at the designated feedrate (in mm/min).
    pub async fn extrude(&mut self, length: f32, feedrate: u32) -> Result<u16, BambuError> {
        let gcode = format!("M83\nG0 E{:.2} F{}", length, feedrate);
        self.send_gcode_raw(&gcode).await
    }
}
