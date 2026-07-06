#[cfg(not(feature = "std"))]
use alloc::format;

use crate::error::BambuError;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};
use crate::mqtt::GCodeRequest;

use super::{POLL_UNTIL_MAX_MESSAGES, PrinterClient};

// home_flag bits 0-2 [REF-HOMEFLAG]
const HOME_FLAG_X_BIT: u32 = 0x01;
const HOME_FLAG_Y_BIT: u32 = 0x02;
const HOME_FLAG_Z_BIT: u32 = 0x04;
const HOME_FLAG_XYZ_BITS: u32 = HOME_FLAG_X_BIT | HOME_FLAG_Y_BIT | HOME_FLAG_Z_BIT;

// Homing took up to ~46s across wire-confirmed P1S runs [REF-HOMEFLAG]; 90s leaves margin.
const HOMING_WAIT_TIMEOUT_SECS: u64 = 90;

impl<
    MqttRawIO,
    MqttTls,
    MqttFactory,
    Timer,
    FtpsRawIO,
    FtpsTls,
    FtpsFactory,
    FtpsTimer,
    CameraRawIO,
    CameraTls,
    CameraFactory,
>
    PrinterClient<
        MqttRawIO,
        MqttTls,
        MqttFactory,
        Timer,
        FtpsRawIO,
        FtpsTls,
        FtpsFactory,
        FtpsTimer,
        CameraRawIO,
        CameraTls,
        CameraFactory,
    >
where
    MqttRawIO: AsyncIo,
    MqttTls: TlsConnector<MqttRawIO>,
    MqttFactory: RawStreamFactory<MqttRawIO>,
    Timer: TimerProvider,
    FtpsRawIO: AsyncIo,
    FtpsTls: TlsConnector<FtpsRawIO>,
    FtpsFactory: RawStreamFactory<FtpsRawIO>,
    FtpsTimer: TimerProvider,
    CameraRawIO: AsyncIo,
    CameraTls: TlsConnector<CameraRawIO>,
    CameraFactory: RawStreamFactory<CameraRawIO>,
{
    /// Returns whether `axis` (`'X'`/`'Y'`/`'Z'`, case-insensitive) was homed as of the
    /// last-observed `home_flag` telemetry. `None` means no telemetry carrying `home_flag`
    /// has been observed yet (via [`poll_telemetry()`](Self::poll_telemetry)) — not "unhomed".
    /// Advisory only: the firmware does not reject motion on unhomed axes [REF-MOTO-HOME].
    pub fn is_axis_homed(&self, axis: char) -> Option<bool> {
        let bit = match axis.to_ascii_uppercase() {
            'X' => HOME_FLAG_X_BIT,
            'Y' => HOME_FLAG_Y_BIT,
            'Z' => HOME_FLAG_Z_BIT,
            _ => return None,
        };
        self.cache.last_home_flag.map(|flag| flag & bit != 0)
    }

    /// Returns whether X, Y, and Z were all homed as of the last-observed `home_flag`
    /// telemetry. `None` means no telemetry carrying `home_flag` has been observed yet.
    pub fn is_all_axes_homed(&self) -> Option<bool> {
        self.cache
            .last_home_flag
            .map(|flag| flag & HOME_FLAG_XYZ_BITS == HOME_FLAG_XYZ_BITS)
    }

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
        self.dispatch(|seq| GCodeRequest::new(gcode_line, seq))
            .await
    }

    /// Dispatches safe homing operations to prevent hardware collisions.
    ///
    /// **Z-Axis Homing Crash Hazards [REF-MOTO-GCODE]:**
    /// * **Bed-on-Z models** (X1, X2D, P1, H2, P2S series) must strictly be homed using a bare `G28`
    ///   to execute the safe firmware-defined toolhead parking sequence. Specifying axis constraints
    ///   (such as `G28 Z`) bypasses this and risks driving the bed directly into a misplaced toolhead.
    /// * **Bed-Slingers** (A1, A1 Mini, A2L) can handle targeted homing macros safely, but a bare `G28` is
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
    ///
    /// A `distance` of exactly `0.0` is a no-op: no G-code is sent to the printer, and this
    /// returns `Ok(0)` (packet id `0` is reserved by the MQTT layer and never assigned to a
    /// real publish, so it unambiguously signals "nothing was sent"). This avoids surfacing
    /// the Z-axis travel-limit error for a request that isn't actually out of range.
    pub async fn move_relative(
        &mut self,
        axis: char,
        distance: f32,
        feedrate: u32,
    ) -> Result<u16, BambuError> {
        let axis_upper = axis.to_ascii_uppercase();
        if self.is_axis_homed(axis_upper) == Some(false) {
            log::warn!(
                "{} axis is not homed (last-known state) — move_relative proceeding anyway",
                axis_upper
            );
        }
        if distance == 0.0 {
            // Zero-distance move is a legitimate no-op (e.g. a UI slider at rest), not a
            // travel-limit violation — short-circuit before `relative_z_move_gcode` collapses
            // it to the same empty-string signal it uses for an out-of-range distance. No MQTT
            // packet is published, so there's no real packet identifier to return; `0` signals
            // "no-op, nothing sent" (`next_sequence_id()` never yields 0, so it can't collide
            // with a genuine in-flight packet id).
            return Ok(0);
        }
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
        if self.is_all_axes_homed() == Some(false) {
            log::warn!("not all axes are homed (last-known state) — extrude proceeding anyway");
        }
        let gcode = format!("M83\nG0 E{:.2} F{}", length, feedrate);
        self.send_gcode_raw(&gcode).await
    }

    /// Blocks until a `G28` homing cycle observed via telemetry has completed.
    ///
    /// Standalone — does not require this client to have issued [`home_axes()`](Self::home_axes).
    /// Resolves correctly whether homing was triggered by this client, the touchscreen, slicer
    /// software, or another `PrinterClient` instance, since it only relies on `home_flag`
    /// telemetry observed via [`poll_telemetry()`](Self::poll_telemetry).
    ///
    /// Only resolves successfully after observing a not-all-homed `home_flag` reading
    /// followed by an all-homed reading: an already-homed printer at call time does not
    /// resolve instantly, and a call where nothing ever homes times out rather than
    /// returning early.
    ///
    /// Like `poll_until` (`src/client/mod.rs`), `wait_for_homing_inner`'s own
    /// wall-clock timeout (`HOMING_WAIT_TIMEOUT_SECS`) and message-count valve
    /// (`POLL_UNTIL_MAX_MESSAGES`) only run *after* each `poll_telemetry().await` below
    /// has already returned — neither protects against that single call stalling
    /// forever on a connection that stops delivering bytes mid-homing (printer powered
    /// off, network drop). That protection is a distinct, lower layer: the underlying
    /// `BambuMqttClient::poll_wire()` (`src/mqtt/client/mod.rs`) races each low-level read
    /// step against `self.timer` internally, bounding a single call regardless of what
    /// this loop does above it.
    pub async fn wait_for_homing(&mut self) -> Result<(), BambuError> {
        let original_timeout = self.command_timeout_secs;
        self.command_timeout_secs = HOMING_WAIT_TIMEOUT_SECS;

        let result = self.wait_for_homing_inner().await;

        self.command_timeout_secs = original_timeout;
        result
    }

    async fn wait_for_homing_inner(&mut self) -> Result<(), BambuError> {
        let start = self.timer.now_millis();
        let timeout_ms = self.command_timeout_secs * 1000;
        let mut count: usize = 0;
        let mut saw_not_all_homed = false;

        loop {
            self.poll_telemetry().await?;

            if let Some(all_homed) = self.is_all_axes_homed() {
                if all_homed && saw_not_all_homed {
                    return Ok(());
                }
                if !all_homed {
                    saw_not_all_homed = true;
                }
            }

            count += 1;
            if count >= POLL_UNTIL_MAX_MESSAGES {
                return Err(BambuError::Timeout);
            }
            let elapsed = self.timer.now_millis().wrapping_sub(start);
            if timeout_ms > 0 && elapsed >= timeout_ms {
                return Err(BambuError::Timeout);
            }
        }
    }
}
