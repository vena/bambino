#[cfg(not(feature = "std"))]
use alloc::format;

use crate::error::Error;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};
use crate::mqtt::GCodeRequest;

use crate::quirks::Axis;
use crate::types::telemetry::bits;

use super::{CommandHandle, PrinterClient, WaitBudget};

/// How long [`PrinterClient::wait_for_homing`] waits for a homing cycle to complete.
///
/// Homing took up to ~46s across wire-confirmed P1S runs [REF-HOMEFLAG]; 90s leaves margin.
pub const HOMING_WAIT_TIMEOUT: core::time::Duration = core::time::Duration::from_secs(90);

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
    /// Returns the cached `home_flag` only if it was observed on the current MQTT connection.
    ///
    /// A disconnect may itself be caused by the same event that lost homing — a power cut, a
    /// physical intervention — so a flag observed before the boundary says nothing about the
    /// machine on the other side of it. Firmware broadcasts carry only *changed* fields
    /// [REF-MQTT-TELEMETRY], so an unchanged `home_flag` may never be re-sent after a
    /// reconnect and the stale value would otherwise persist indefinitely rather than
    /// self-correcting on the next report.
    fn home_flag_this_connection(&self) -> Option<u32> {
        if self.core.cache.last_home_flag_generation? != self.core.connection_generation {
            return None;
        }
        self.core.cache.last_home_flag
    }

    /// Returns whether `axis` was homed as of the last-observed `home_flag` telemetry.
    ///
    /// `None` means no telemetry carrying `home_flag` has been observed **on the current MQTT
    /// connection** (via [`poll_telemetry()`](Self::poll_telemetry)) — not "unhomed". A
    /// disconnect/reconnect resets this to `None` until the printer reports again; the two
    /// cases are deliberately not distinguished, since a caller must handle `None` either way.
    /// Advisory only: the firmware does not reject motion on unhomed axes [REF-MOTO-HOME].
    pub fn is_axis_homed(&self, axis: Axis) -> Option<bool> {
        self.home_flag_this_connection()
            .map(|flag| bits::is_axis_homed(flag, axis))
    }

    /// Returns whether X, Y, and Z were all homed as of the last-observed `home_flag` telemetry.
    ///
    /// `None` means no telemetry carrying `home_flag` has been observed on the current MQTT
    /// connection — see [`is_axis_homed()`](Self::is_axis_homed).
    pub fn is_all_axes_homed(&self) -> Option<bool> {
        self.home_flag_this_connection()
            .map(bits::is_all_axes_homed)
    }

    /// Sends a G-code command with model-aware safety validation.
    ///
    /// Rejects, without sending anything, G-code that is unsafe on the active model: partial-axis
    /// homing on bed-on-Z platforms, heater targets above the model's nozzle/bed/chamber ceilings,
    /// and any chamber-heater command on a model without one — see
    /// [`ModelQuirks::validate_gcode`](crate::quirks::ModelQuirks::validate_gcode) for the exact
    /// rules. Nothing is clamped, and relative moves are not bounded. The bed ceiling uses the
    /// same mains region as [`set_bed_temperature()`](Self::set_bed_temperature). Use
    /// [`send_gcode_raw()`](Self::send_gcode_raw) to bypass validation when you need unchecked
    /// access.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Turn on the part cooling fan at 100%
    /// printer.send_gcode("M106 P1 S255").await?;
    ///
    /// // This will be rejected on CoreXY printers (unsafe partial homing):
    /// // printer.send_gcode("G28 Z").await?;  // -> Err(ModelMismatch)
    /// // And on an A1 Mini (80°C bed ceiling):
    /// // printer.send_gcode("M140 S100").await?;  // -> Err(ModelMismatch)
    /// ```
    pub async fn send_gcode(&mut self, gcode_line: &str) -> Result<CommandHandle, Error> {
        let mains_220v = self.is_220v_power();
        self.quirks().validate_gcode(gcode_line, mains_220v)?;
        self.send_gcode_raw(gcode_line).await
    }

    /// Dispatches a raw G-code string without model safety checks [REF-MOTO-GCODE].
    ///
    /// Returns the [`CommandHandle`] of the published `gcode_line` command.
    pub async fn send_gcode_raw(&mut self, gcode_line: &str) -> Result<CommandHandle, Error> {
        self.dispatch(|seq| GCodeRequest::new(gcode_line, seq))
            .await
    }

    /// Homes every axis with a bare `G28`, the firmware's own safe parking sequence [REF-MOTO-GCODE].
    ///
    /// The right call on every model. On bed-slingers (A1, A1 Mini, A2L) a targeted
    /// [`home_z_only()`](Self::home_z_only) is also available.
    pub async fn home_all(&mut self) -> Result<CommandHandle, Error> {
        self.send_gcode_raw("G28").await
    }

    /// Homes only Z (`G28 Z`) — refused on bed-on-Z models [REF-MOTO-GCODE].
    ///
    /// **Bed-on-Z models** (X1, X2D, P1, H2, P2S series) must be homed with a bare `G28`
    /// ([`home_all()`](Self::home_all)), which runs the firmware's toolhead parking sequence.
    /// `G28 Z` skips it and risks driving the bed into a misplaced toolhead, so those models get
    /// [`Error::ModelMismatch`] and nothing is sent. Bed-slingers (A1, A1 Mini, A2L) accept it.
    pub async fn home_z_only(&mut self) -> Result<CommandHandle, Error> {
        if self.quirks().is_bed_on_z() {
            return Err(Error::ModelMismatch(
                "Z-only homing unsafe on bed-on-Z model".into(),
            ));
        }
        self.send_gcode_raw("G28 Z").await
    }

    /// Dispatches a manual relative axis movement block.
    ///
    /// **Relative Axis Movement Safety [REF-MOTO-GCODE]:**
    /// Each move is capped client-side at the axis's travel in the model's
    /// [`build_volume()`](crate::quirks::ModelQuirks::build_volume) — a bound on one command's
    /// distance, not position-aware crash prevention, since the printer reports no absolute axis
    /// position over MQTT. A Z move is additionally wrapped in reference-mode push/pop
    /// (`M1002 push_ref_mode` / `M1002 pop_ref_mode`) to prevent frame shifting, inside
    /// BambuStudio's `M211 S` / `M211 X1 Y1 Z1` … `M211 R` save-enable-restore of the
    /// soft-endstop state. Per real H2D hardware testing (bambuddy #2579, confirmed 2026-07-16)
    /// firmware does not enforce software travel limits on G-code received over MQTT regardless
    /// of `M211` state — it is not a source of crash protection here.
    ///
    /// A `distance` of exactly `0.0` is a no-op: no G-code is sent to the printer, and this
    /// returns `Ok(None)`.
    ///
    /// # Errors
    ///
    /// [`Error::ModelMismatch`] when `distance` is non-finite or exceeds the axis's travel.
    pub async fn move_relative(
        &mut self,
        axis: Axis,
        distance: f32,
        feedrate: u32,
    ) -> Result<Option<CommandHandle>, Error> {
        if self.is_axis_homed(axis) == Some(false) {
            log::warn!(
                "{} axis is not homed (last-known state) — move_relative proceeding anyway",
                axis
            );
        }
        if distance == 0.0 {
            // A zero-distance move is a legitimate no-op (e.g. a UI slider at rest), not a
            // travel-limit violation. Nothing is published, so there is no command to hand back.
            return Ok(None);
        }
        let gcode = self
            .quirks()
            .relative_move_gcode(axis, distance, feedrate)
            .ok_or_else(|| {
                Error::ModelMismatch(
                    format!("{axis}-axis move of {distance} exceeds model travel limits").into(),
                )
            })?;
        self.send_gcode_raw(&gcode).await.map(Some)
    }

    /// Dispatches a manual relative extrusion command sequence [REF-GCODE-EXTRUDE].
    ///
    /// Configures the active extruder drive gear to relative mode (`M83`) and feeds
    /// the specified length of filament (in mm) at the designated feedrate (in mm/min).
    pub async fn extrude(&mut self, length: f32, feedrate: u32) -> Result<CommandHandle, Error> {
        if self.is_all_axes_homed() == Some(false) {
            log::warn!("not all axes are homed (last-known state) — extrude proceeding anyway");
        }
        let gcode = format!("M83\nG0 E{:.2} F{}", length, feedrate);
        self.send_gcode_raw(&gcode).await
    }

    /// Blocks until a `G28` homing cycle observed via telemetry has completed.
    ///
    /// Standalone — does not require this client to have issued [`home_all()`](Self::home_all).
    /// Resolves correctly whether homing was triggered by this client, the touchscreen, slicer
    /// software, or another `PrinterClient` instance, since it only relies on `home_flag`
    /// telemetry observed via [`poll_telemetry()`](Self::poll_telemetry).
    ///
    /// Only resolves successfully after observing a not-all-homed `home_flag` reading
    /// followed by an all-homed reading: an already-homed printer at call time does not
    /// resolve instantly, and a call where nothing ever homes times out rather than
    /// returning early.
    ///
    /// Times out after [`HOMING_WAIT_TIMEOUT`], independent of the command timeout. Like
    /// `poll_until` (`src/client/mod.rs`), that deadline (or, without a real clock, the
    /// message-count valve) is only checked *after* each `poll_telemetry().await` below
    /// has already returned — neither protects against that single call stalling
    /// forever on a connection that stops delivering bytes mid-homing (printer powered
    /// off, network drop). That protection is a distinct, lower layer: the underlying
    /// `MqttClient::poll_wire()` (`src/mqtt/client/mod.rs`) races each low-level read
    /// step against `self.timer` internally, bounding a single call regardless of what
    /// this loop does above it.
    pub async fn wait_for_homing(&mut self) -> Result<(), Error> {
        // The timeout is a local, not a temporary write to the command timeout: a caller that
        // drops this future mid-wait (`select!`, `tokio::time::timeout`) would otherwise leave
        // every later command running against the homing deadline.
        let mut wait = WaitBudget::start(&self.timer, Some(HOMING_WAIT_TIMEOUT));
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

            wait.after_message(&self.timer)?;
        }
    }
}
