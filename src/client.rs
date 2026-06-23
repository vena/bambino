//! # Unified Printer Client Coordinator & Developer API
//!
//! Provides a high-level, platform-agnostic client interface designed to aggregate
//! MQTTS telemetry channels, implicit FTPS storage nodes, and video feeds under a
//! safe, unified controller boundary.
//!
//! ## Architectural Safety Interlocks
//! 1. **Bed-on-Z vs Bed-Slinger Homing [REF-MOTO-GCODE]:** Prevents structural nozzle
//!    collisions by enforcing bare `G28` homing commands on Bed-on-Z platforms (CoreXY),
//!    blocking dangerous partial homing parameters (such as `G28 Z`) that bypass safe parking.
//! 2. **Reference Mode Position Isolation:** Wraps relative movements on the Z-axis in
//!    travel-limit clamps (`M211 S1`) and coordinate push/pop boundaries (`M1002`) to insulate
//!    against mechanical bed crashes.
//! 3. **Chamber Thermal Guards [REF-THER-DECODE]:** Enforces capability checks prior to
//!    dispatching active heated chamber operations (`M141`), rejecting requests on open-frame models.
//! 4. **Auxiliary Fan Safety Routing [REF-CLIM-FANS]:** Directs fan cooling commands dynamically,
//!    handling secondary right-hand auxiliary fan controllers on specialized platforms.

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::discovery::BambuModel;
use crate::error::BambuError;
use crate::ftps::{BambuFtpsClient, FtpDataStreamFactory};
use crate::io::{AsyncIo, TlsConnector};
use crate::mqtt::{BambuMqttClient, GCodeRequest, MqttMessage, StandardControlRequest};
use crate::quirks::ModelQuirks;

// ============================================================================
// Internal Default Dummy Types (Satisfies Recursive Inner Bounds)
// ============================================================================

/// Private internal helper dummy type satisfying `AsyncIo` to enable trait default parameterization.
#[derive(Debug, Clone, Copy)]
pub struct DummyRawIo;

impl embedded_io_async::ErrorType for DummyRawIo {
    type Error = embedded_io_async::ErrorKind;
}

impl embedded_io_async::Read for DummyRawIo {
    async fn read(&mut self, _buf: &mut [u8]) -> Result<usize, Self::Error> {
        Ok(0)
    }
}

impl embedded_io_async::Write for DummyRawIo {
    async fn write(&mut self, _buf: &[u8]) -> Result<usize, Self::Error> {
        Ok(0)
    }
    async fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Private internal helper dummy type satisfying `TlsConnector` to enable trait default parameterization.
pub struct DummyTls;

impl TlsConnector<DummyRawIo> for DummyTls {
    type Stream = DummyRawIo;
    async fn connect(
        &self,
        _host: &str,
        _port: u16,
        _raw_stream: DummyRawIo,
    ) -> Result<Self::Stream, crate::io::SocketError> {
        Ok(DummyRawIo)
    }
}

/// Private internal helper dummy type satisfying `FtpDataStreamFactory` to enable trait default parameterization.
pub struct DummyFactory;

impl FtpDataStreamFactory<DummyRawIo> for DummyFactory {
    async fn create_data_stream(
        &self,
        _host: &str,
        _port: u16,
    ) -> Result<DummyRawIo, crate::io::SocketError> {
        Ok(DummyRawIo)
    }
}

// ============================================================================
// Core Struct Definition
// ============================================================================

/// Enumeration representing target onboard cooling fans [REF-CLIM-FANS].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FanTarget {
    /// Primary part cooling fan (Port 1).
    PartCooling,
    /// Primary left-side auxiliary fan (Port 2).
    AuxiliaryLeft,
    /// Chamber exhaust/filtration fan (Port 3).
    ChamberExhaust,
    /// Secondary right-side auxiliary fan (Port 10, specifically supported on X2D).
    AuxiliaryRight,
}

/// A high-level, multi-platform coordinator client for Bambu Lab printers.
///
/// This struct wraps an active MQTT session and an optional FTPS file-system client.
/// Type parameters default to dummy implementations to allow lightweight MQTT-only deployment on
/// memory-constrained microcontrollers without violating recursive trait boundaries.
pub struct PrinterClient<IO, RawIO = DummyRawIo, Tls = DummyTls, Factory = DummyFactory>
where
    IO: AsyncIo,
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: FtpDataStreamFactory<RawIO>,
{
    mqtt: BambuMqttClient<IO>,
    ftps: Option<BambuFtpsClient<RawIO, Tls, Factory>>,
    serial: String,
    model: BambuModel,
    sequence_counter: u64,
}

// ============================================================================
// MQTT-Only & Shared Connection Block
// ============================================================================

impl<IO> PrinterClient<IO, DummyRawIo, DummyTls, DummyFactory>
where
    IO: AsyncIo,
{
    /// Instantiates a standard, lightweight coordinate client wrapping an active MQTT session.
    pub fn new(mqtt_client: BambuMqttClient<IO>, serial: &str, model: BambuModel) -> Self {
        Self {
            mqtt: mqtt_client,
            ftps: None,
            serial: String::from(serial),
            model,
            sequence_counter: 10000,
        }
    }
}

// ============================================================================
// Core Functional API Blocks (Thermals, Motion, Queue Lifecycle)
// ============================================================================

impl<IO, RawIO, Tls, Factory> PrinterClient<IO, RawIO, Tls, Factory>
where
    IO: AsyncIo,
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: FtpDataStreamFactory<RawIO>,
{
    /// Increments and returns the next transaction/sequence identifier tracking commands.
    pub fn next_sequence_id(&mut self) -> u64 {
        self.sequence_counter = self.sequence_counter.wrapping_add(1);
        self.sequence_counter
    }

    /// Pulls the next available telemetry update or response payload from the MQTTS channel.
    pub async fn poll_telemetry(&mut self) -> Result<MqttMessage, BambuError> {
        self.mqtt.poll_telemetry().await
    }

    /// Dispatches a manual G-code string encapsulated in a `gcode_line` JSON request [REF-MOTO-GCODE].
    ///
    /// Returns the MQTT packet identifier assigned to track publication delivery status.
    pub async fn send_gcode(&mut self, gcode_line: &str) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = GCodeRequest::new(gcode_line, seq);
        let payload = serde_json::to_vec(&req).map_err(|_| BambuError::SerializationError)?;
        self.mqtt.publish_command(&payload).await
    }

    /// Returns a reference to the printer's unique hardware serial number.
    pub fn serial(&self) -> &str {
        &self.serial
    }

    /// Returns the resolved printer hardware model.
    pub fn model(&self) -> BambuModel {
        self.model
    }

    // ------------------------------------------------------------------------
    // Homing and Motion Control Helpers
    // ------------------------------------------------------------------------

    /// Dispatches safe homing operations to prevent hardware collisions.
    ///
    /// **Z-Axis Homing Crash Hazards [REF-MOTO-GCODE]:**
    /// * **Bed-on-Z models** (X1, P1, H2, P2S series) must strictly be homed using a bare `G28`
    ///   to execute the safe firmware-defined toolhead parking sequence. Specifying axis constraints
    ///   (such as `G28 Z`) bypasses this and risks driving the bed directly into a misplaced toolhead.
    /// * **Bed-Slingers** (A1, A1 Mini) can handle targeted homing macros safely, but a bare `G28` is
    ///   highly recommended for standard configurations.
    pub async fn home_axes(&mut self, home_z_only_danger: bool) -> Result<u16, BambuError> {
        let is_bed_on_z = self.model.is_bed_on_z();

        let gcode = if is_bed_on_z {
            if home_z_only_danger {
                return Err(BambuError::ModelMismatch);
            }
            "G28"
        } else if home_z_only_danger {
            "G28 Z"
        } else {
            "G28"
        };

        self.send_gcode(gcode).await
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
            let gcode = self.model.relative_z_move_gcode(distance, feedrate);
            if gcode.is_empty() {
                return Err(BambuError::ModelMismatch);
            }
            self.send_gcode(&gcode).await
        } else {
            let gcode = format!("G91\nG0 {}{:.2} F{}\nG90", axis_upper, distance, feedrate);
            self.send_gcode(&gcode).await
        }
    }

    /// Dispatches a manual relative extrusion command sequence [REF-GCODE-EXTRUDE].
    ///
    /// Configures the active extruder drive gear to relative mode (`M83`) and feeds
    /// the specified length of filament (in mm) at the designated feedrate (in mm/min).
    pub async fn extrude(&mut self, length: f32, feedrate: u32) -> Result<u16, BambuError> {
        let gcode = format!("M83\nG0 E{:.2} F{}", length, feedrate);
        self.send_gcode(&gcode).await
    }

    // ------------------------------------------------------------------------
    // Thermal Control Helpers
    // ------------------------------------------------------------------------

    /// Sets the target temperature of the build plate (bed) [REF-MOTO-GCODE].
    pub async fn set_bed_temperature(&mut self, target_temp: u16) -> Result<u16, BambuError> {
        let gcode = format!("M140 S{}", target_temp);
        self.send_gcode(&gcode).await
    }

    /// Sets the target temperature of a specific hotend/nozzle [REF-MOTO-GCODE].
    ///
    /// * `nozzle_id`: The carriage ID (usually `0` for primary/single, or `1` for secondary on IDEX).
    pub async fn set_nozzle_temperature(
        &mut self,
        nozzle_id: u8,
        target_temp: u16,
    ) -> Result<u16, BambuError> {
        let gcode = format!("M104 T{} S{}", nozzle_id, target_temp);
        self.send_gcode(&gcode).await
    }

    /// Sets the target temperature of the active heated chamber loop [REF-MOTO-GCODE].
    ///
    /// **Chamber Temperature Safety Check [REF-THER-DECODE]:**
    /// This is only supported on enclosed models equipped with active chamber heaters (such as X1E or P2S).
    /// If issued to models without active chamber thermal capabilities, returns a capability mismatch error.
    pub async fn set_chamber_temperature(&mut self, target_temp: u16) -> Result<u16, BambuError> {
        if self.model.ignores_chamber_temperature() {
            return Err(BambuError::ModelMismatch);
        }
        let gcode = format!("M141 S{}", target_temp);
        self.send_gcode(&gcode).await
    }

    // ------------------------------------------------------------------------
    // Climate & Peripheral Control Helpers
    // ------------------------------------------------------------------------

    /// Sets the speed of a targeted onboard fan as a percentage (0 to 100) [REF-CLIM-FANS].
    ///
    /// Translates percentage input to standard PWM ranges (0 to 255) in the G-code envelope.
    /// For models with unique secondary cooling configurations (like the X2D), directs commands
    /// to the correct target port ID.
    pub async fn set_fan_speed(
        &mut self,
        fan_type: FanTarget,
        speed_percent: u8,
    ) -> Result<u16, BambuError> {
        let speed_clamped = core::cmp::min(speed_percent, 100);
        let pwm = ((speed_clamped as u32 * 255) / 100) as u16;

        let port_id = match fan_type {
            FanTarget::PartCooling => 1,
            FanTarget::AuxiliaryLeft => 2,
            FanTarget::ChamberExhaust => 3,
            FanTarget::AuxiliaryRight => {
                if self.model != BambuModel::X2D {
                    return Err(BambuError::ModelMismatch);
                }
                10
            }
        };

        let gcode = format!("M106 P{} S{}", port_id, pwm);
        self.send_gcode(&gcode).await
    }

    /// Configures the active state of a targeted enclosure LED lighting node [REF-MQTT-LIFECYCLE].
    pub async fn toggle_led(&mut self, node: &str, turn_on: bool) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::commands::LedCtrlRequest::new(node, turn_on, seq);
        let payload = serde_json::to_vec(&req).map_err(|_| BambuError::SerializationError)?;
        self.mqtt.publish_command(&payload).await
    }

    /// Configures the active climate airduct damper mode (cooling vs heating recirculation) [REF-MQTT-LIFECYCLE].
    pub async fn set_airduct_mode(&mut self, recirculate_air: bool) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::commands::AirductRequest::new(recirculate_air, seq);
        let payload = serde_json::to_vec(&req).map_err(|_| BambuError::SerializationError)?;
        self.mqtt.publish_command(&payload).await
    }

    /// Configures whether the printer's speakers emit prompt notification sounds [REF-MQTT-LIFECYCLE].
    ///
    /// Available on supported hardware architectures only (such as the A1 and H2D series).
    pub async fn set_prompt_sound(&mut self, enable_sound: bool) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::commands::PromptSoundRequest::new(enable_sound, seq);
        let payload = serde_json::to_vec(&req).map_err(|_| BambuError::SerializationError)?;
        self.mqtt.publish_command(&payload).await
    }

    /// Modifies active alarm or attention chime parameters on the physical buzzer module [REF-MQTT-LIFECYCLE].
    ///
    /// Buzzer mode codes map to: `0` (Silent/disarmed), `1` (Alarm triggered), `2` (Beeping attention).
    pub async fn set_buzzer_mode(&mut self, mode_code: i32) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::commands::BuzzerRequest::new(mode_code, seq);
        let payload = serde_json::to_vec(&req).map_err(|_| BambuError::SerializationError)?;
        self.mqtt.publish_command(&payload).await
    }

    // ------------------------------------------------------------------------
    // Print Job Lifecycle Management
    // ------------------------------------------------------------------------

    /// Pauses the currently active print job [REF-MQTT-LIFECYCLE].
    pub async fn pause_print(&mut self) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = StandardControlRequest::new("pause", seq);
        let payload = serde_json::to_vec(&req).map_err(|_| BambuError::SerializationError)?;
        self.mqtt.publish_command(&payload).await
    }

    /// Resumes a paused print job [REF-MQTT-LIFECYCLE].
    pub async fn resume_print(&mut self) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = StandardControlRequest::new("resume", seq);
        let payload = serde_json::to_vec(&req).map_err(|_| BambuError::SerializationError)?;
        self.mqtt.publish_command(&payload).await
    }

    /// Aborts/cancels the currently running print job queue [REF-MQTT-LIFECYCLE].
    pub async fn stop_print(&mut self) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = StandardControlRequest::new("stop", seq);
        let payload = serde_json::to_vec(&req).map_err(|_| BambuError::SerializationError)?;
        self.mqtt.publish_command(&payload).await
    }
}

// ============================================================================
// Full Storage / FTPS Dual Client Block
// ============================================================================

impl<IO, RawIO, Tls, Factory> PrinterClient<IO, RawIO, Tls, Factory>
where
    IO: AsyncIo,
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: FtpDataStreamFactory<RawIO>,
{
    /// Instantiates a coordinator client holding both active MQTTS and implicit FTPS sessions.
    pub fn new_with_storage(
        mqtt_client: BambuMqttClient<IO>,
        ftps_client: BambuFtpsClient<RawIO, Tls, Factory>,
        serial: &str,
        model: BambuModel,
    ) -> Self {
        Self {
            mqtt: mqtt_client,
            ftps: Some(ftps_client),
            serial: String::from(serial),
            model,
            sequence_counter: 10000,
        }
    }

    /// Connects and registers an FTPS client on demand if not set during initialization.
    pub fn attach_storage(&mut self, ftps_client: BambuFtpsClient<RawIO, Tls, Factory>) {
        self.ftps = Some(ftps_client);
    }

    /// Exposes a reference to the active FTPS client.
    ///
    /// Returns `None` if storage capabilities have not been attached.
    pub fn storage(&mut self) -> Option<&mut BambuFtpsClient<RawIO, Tls, Factory>> {
        self.ftps.as_mut()
    }
}
