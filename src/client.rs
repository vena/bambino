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
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use serde::Serialize;

use crate::error::BambuError;
use crate::ftps::{BambuFtpsClient, FtpDataStreamFactory};
use crate::io::{AsyncIo, TlsConnector};
use crate::models::BambuModel;
use crate::mqtt::commands::TASK_ID_MAX;
use crate::mqtt::{
    BambuMqttClient, GCodeRequest, MqttMessage, PrintJobConfig, StandardControlRequest,
};

pub(crate) const INITIAL_SEQUENCE_ID: u64 = 10000;

// ============================================================================
// Internal Default Dummy Types (Satisfies Recursive Inner Bounds)
// ============================================================================

#[doc(hidden)]
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

#[doc(hidden)]
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

#[doc(hidden)]
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

/// Velocity and acceleration scaling presets for active print jobs [REF-MQTT-LIFECYCLE].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrintSpeed {
    /// 50% max acceleration and feedrate limits.
    Silent = 1,
    /// 100% nominal feedrate limit.
    Standard = 2,
    /// 124% nominal feedrate limit.
    Sport = 3,
    /// 166% nominal feedrate limit.
    Ludicrous = 4,
}

/// Bitmask flags for selecting hardware calibration routines [REF-MQTT-LIFECYCLE].
///
/// Combine flags with bitwise OR to trigger multiple calibration routines simultaneously
/// (e.g., `CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CalibrationOption(pub u32);

impl CalibrationOption {
    pub const BED_LEVELING: Self = Self(2);
    pub const VIBRATION_COMPENSATION: Self = Self(4);
    pub const MOTOR_NOISE_CANCELLATION: Self = Self(8);
    pub const NOZZLE_HEIGHT: Self = Self(16);
    pub const HEATBED_THERMAL: Self = Self(32);
}

impl core::ops::BitOr for CalibrationOption {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
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
    k_profile_primed: bool,
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
            sequence_counter: INITIAL_SEQUENCE_ID,
            k_profile_primed: false,
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
    ///
    /// Wraps at `TASK_ID_MAX` (32-bit signed integer limit) to stay within firmware
    /// parsing constraints [REF-MQTT-ENV].
    pub fn next_sequence_id(&mut self) -> u64 {
        self.sequence_counter = self.sequence_counter.wrapping_add(1);
        if self.sequence_counter > TASK_ID_MAX {
            self.sequence_counter = INITIAL_SEQUENCE_ID;
        }
        self.sequence_counter
    }

    /// Pulls the next available telemetry update or response payload from the MQTTS channel.
    pub async fn poll_telemetry(&mut self) -> Result<MqttMessage, BambuError> {
        self.mqtt.poll_telemetry().await
    }

    /// Serializes a request struct and publishes it to the printer's MQTT command channel.
    async fn publish_request<T: Serialize>(&mut self, request: &T) -> Result<u16, BambuError> {
        let payload = serde_json::to_vec(request).map_err(|_| BambuError::SerializationError)?;
        self.mqtt.publish_command(&payload).await
    }

    /// Dispatches a G-code string with model-aware safety validation [REF-MOTO-GCODE].
    ///
    /// Rejects commands that would be unsafe on the active model (e.g., partial-axis
    /// homing on bed-on-Z platforms). Use `send_gcode_raw()` to bypass validation when
    /// you need unchecked access.
    pub async fn send_gcode(&mut self, gcode_line: &str) -> Result<u16, BambuError> {
        if self.model.quirks().is_unsafe_homing_command(gcode_line) {
            return Err(BambuError::ModelMismatch);
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
        let is_bed_on_z = self.model.quirks().is_bed_on_z();

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
                return Err(BambuError::ModelMismatch);
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

    // ------------------------------------------------------------------------
    // Thermal Control Helpers
    // ------------------------------------------------------------------------

    /// Sets the target temperature of the build plate (bed) [REF-MOTO-GCODE].
    ///
    /// Values exceeding the model's maximum bed temperature are clamped automatically.
    pub async fn set_bed_temperature(&mut self, target_temp: u16) -> Result<u16, BambuError> {
        let max = self.model.quirks().bed_temp_max();
        let target_temp = if target_temp > max {
            log::warn!(
                "Bed temperature {}°C exceeds model max {}°C, clamping",
                target_temp,
                max
            );
            max
        } else {
            target_temp
        };
        let gcode = format!("M140 S{}", target_temp);
        self.send_gcode_raw(&gcode).await
    }

    /// Sets the target temperature of a specific hotend/nozzle [REF-MOTO-GCODE].
    ///
    /// * `nozzle_id`: The carriage ID (usually `0` for primary/single, or `1` for secondary on IDEX).
    ///
    /// Values exceeding the model's maximum nozzle temperature are clamped automatically.
    pub async fn set_nozzle_temperature(
        &mut self,
        nozzle_id: u8,
        target_temp: u16,
    ) -> Result<u16, BambuError> {
        let max = self.model.quirks().nozzle_temp_max();
        let target_temp = if target_temp > max {
            log::warn!(
                "Nozzle temperature {}°C exceeds model max {}°C, clamping",
                target_temp,
                max
            );
            max
        } else {
            target_temp
        };
        let gcode = format!("M104 T{} S{}", nozzle_id, target_temp);
        self.send_gcode_raw(&gcode).await
    }

    /// Sets the target temperature of the active heated chamber loop [REF-MOTO-GCODE].
    ///
    /// **Chamber Temperature Safety Check [REF-THER-DECODE]:**
    /// Only supported on models with active PTC chamber heaters (X1E, X2D, H2 series).
    /// Models with passive chamber sensors but no heater (X1C, P2S) will return a capability
    /// mismatch error — their firmware silently ignores M141.
    pub async fn set_chamber_temperature(&mut self, target_temp: u16) -> Result<u16, BambuError> {
        if !self.model.quirks().has_active_chamber_heater() {
            return Err(BambuError::ModelMismatch);
        }
        let max = self.model.quirks().chamber_temp_max();
        let target_temp = if target_temp > max {
            log::warn!(
                "Chamber temperature {}°C exceeds model max {}°C, clamping",
                target_temp,
                max
            );
            max
        } else {
            target_temp
        };
        let gcode = format!("M141 S{}", target_temp);
        self.send_gcode_raw(&gcode).await
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
                if !self.model.quirks().supports_auxiliary_right_fan() {
                    return Err(BambuError::ModelMismatch);
                }
                10
            }
        };

        let gcode = format!("M106 P{} S{}", port_id, pwm);
        self.send_gcode_raw(&gcode).await
    }

    /// Configures the active state of a targeted enclosure LED lighting node [REF-MQTT-LIFECYCLE].
    pub async fn toggle_led(&mut self, node: &str, turn_on: bool) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::commands::LedCtrlRequest::new(node, turn_on, seq);
        self.publish_request(&req).await
    }

    /// Configures the active climate airduct damper mode (cooling vs heating recirculation) [REF-MQTT-LIFECYCLE].
    pub async fn set_airduct_mode(&mut self, recirculate_air: bool) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::commands::AirductRequest::new(recirculate_air, seq);
        self.publish_request(&req).await
    }

    /// Configures whether the printer's speakers emit prompt notification sounds [REF-MQTT-LIFECYCLE].
    ///
    /// Available on supported hardware architectures only (such as the A1 and H2D series).
    pub async fn set_prompt_sound(&mut self, enable_sound: bool) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::commands::PromptSoundRequest::new(enable_sound, seq);
        self.publish_request(&req).await
    }

    /// Modifies active alarm or attention chime parameters on the physical buzzer module [REF-MQTT-LIFECYCLE].
    ///
    /// Buzzer mode codes map to: `0` (Silent/disarmed), `1` (Alarm triggered), `2` (Beeping attention).
    pub async fn set_buzzer_mode(&mut self, mode_code: i32) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::commands::BuzzerRequest::new(mode_code, seq);
        self.publish_request(&req).await
    }

    // ------------------------------------------------------------------------
    // Print Job Lifecycle Management
    // ------------------------------------------------------------------------

    /// Pauses the currently active print job [REF-MQTT-LIFECYCLE].
    pub async fn pause_print(&mut self) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = StandardControlRequest::new("pause", seq);
        self.publish_request(&req).await
    }

    /// Resumes a paused print job [REF-MQTT-LIFECYCLE].
    pub async fn resume_print(&mut self) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = StandardControlRequest::new("resume", seq);
        self.publish_request(&req).await
    }

    /// Aborts/cancels the currently running print job queue [REF-MQTT-LIFECYCLE].
    pub async fn stop_print(&mut self) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = StandardControlRequest::new("stop", seq);
        self.publish_request(&req).await
    }

    // ------------------------------------------------------------------------
    // AMS Filament Control Helpers
    // ------------------------------------------------------------------------

    /// Triggers a filament load or unload sequence on a physical AMS unit or external spool [REF-AMS-MAP].
    ///
    /// * `ams_id`: AMS unit index (0-3), or `255` for external spool.
    /// * `slot_id`: Slot within the AMS (0-3), or `254` for external spool.
    /// * `target`: Load destination (`1` = toolhead load, `255` = unload/retract).
    /// * `curr_temp` / `tar_temp`: Nozzle temperatures (`-1` = let firmware decide).
    pub async fn change_filament(
        &mut self,
        ams_id: i32,
        slot_id: i32,
        target: i32,
        curr_temp: i32,
        tar_temp: i32,
    ) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::AmsChangeFilamentRequest::new(
            ams_id, slot_id, target, curr_temp, tar_temp, seq,
        );
        self.publish_request(&req).await
    }

    /// Initiates a dry-chamber heating cycle on an AMS-HT or AMS 2 Pro unit [REF-AMS-DRYER].
    ///
    /// * `ams_id`: Target AMS unit index.
    /// * `dry_temp`: Drying temperature in degrees Celsius.
    /// * `dry_time`: Duration in minutes (e.g., 480 for an 8-hour cycle).
    /// * `rotate_tray`: Whether to rotate trays during the cycle.
    /// * `filament`: Filament type string (e.g., "PA-CF").
    pub async fn start_drying(
        &mut self,
        ams_id: i32,
        dry_temp: u32,
        dry_time: u32,
        rotate_tray: bool,
        filament: &str,
    ) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::AmsFilamentDryingRequest::new(
            ams_id,
            1,
            dry_temp,
            dry_time,
            rotate_tray,
            filament,
            seq,
        );
        self.publish_request(&req).await
    }

    /// Terminates an active dry-chamber heating cycle on an AMS unit [REF-AMS-DRYER].
    pub async fn stop_drying(&mut self, ams_id: i32) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::AmsFilamentDryingRequest::new(ams_id, 0, 0, 0, false, "", seq);
        self.publish_request(&req).await
    }

    /// Scans proprietary RFID tag properties on a specific AMS tray [REF-AMS-MAP].
    pub async fn scan_rfid(&mut self, ams_id: i32, slot_id: i32) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::AmsGetRfidRequest::new(ams_id, slot_id, seq);
        self.publish_request(&req).await
    }

    /// Binds a stored K-profile calibration entry to an AMS material slot [REF-AMS-MAP].
    pub async fn select_k_profile(
        &mut self,
        ams_id: i32,
        tray_id: i32,
        cali_idx: i32,
        filament_id: &str,
        nozzle_diameter: &str,
    ) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::diagnostics::ExtrusionCaliSelRequest::new(
            ams_id,
            tray_id,
            cali_idx,
            filament_id,
            nozzle_diameter,
            seq,
        );
        self.publish_request(&req).await
    }

    /// Requests a dump of the printer's stored K-profile calibration database [REF-DIAG-KPROF].
    ///
    /// Automatically sends a priming request on the first call after connection, because the
    /// firmware silently ignores the initial `extrusion_cali_get` command. Use
    /// `set_k_profile_primed(true)` to skip the automatic prime if you handle it yourself.
    ///
    /// The response arrives asynchronously via `poll_telemetry()` — deserialize it with
    /// `ExtrusionCaliGetResponse`.
    pub async fn get_k_profiles(&mut self) -> Result<u16, BambuError> {
        if !self.k_profile_primed {
            let prime_seq = self.next_sequence_id();
            let prime_req = crate::diagnostics::ExtrusionCaliGetRequest::new(prime_seq);
            self.publish_request(&prime_req).await?;
            self.k_profile_primed = true;
        }

        let seq = self.next_sequence_id();
        let req = crate::diagnostics::ExtrusionCaliGetRequest::new(seq);
        self.publish_request(&req).await
    }

    /// Controls whether `get_k_profiles()` sends an automatic priming request.
    ///
    /// Set to `true` to skip the firmware priming quirk — useful if you handle priming
    /// yourself or target firmware that does not require it.
    pub fn set_k_profile_primed(&mut self, primed: bool) {
        self.k_profile_primed = primed;
    }

    // ------------------------------------------------------------------------
    // Error, Speed & Calibration Helpers
    // ------------------------------------------------------------------------

    /// Clears active error codes from the printer's diagnostic fault register [REF-MQTT-LIFECYCLE].
    pub async fn clear_print_error(&mut self) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::CleanPrintErrorRequest::new(seq);
        self.publish_request(&req).await
    }

    /// Dynamically scales maximum velocity and acceleration limits during an active print [REF-MQTT-LIFECYCLE].
    pub async fn set_print_speed(&mut self, level: PrintSpeed) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let speed_str = match level {
            PrintSpeed::Silent => "1",
            PrintSpeed::Standard => "2",
            PrintSpeed::Sport => "3",
            PrintSpeed::Ludicrous => "4",
        };
        let req = crate::mqtt::commands::PrintSpeedRequest::new(speed_str, seq);
        self.publish_request(&req).await
    }

    /// Bypasses rendering of specific objects within an active multi-model print job [REF-MQTT-LIFECYCLE].
    pub async fn skip_objects(&mut self, object_ids: Vec<u32>) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::SkipObjectsRequest::new(object_ids, seq);
        self.publish_request(&req).await
    }

    /// Triggers automated physical calibration routines on the printer chassis [REF-MQTT-LIFECYCLE].
    ///
    /// Use `CalibrationOption` flags combined with `|` to select routines:
    /// ```ignore
    /// client.start_calibration(
    ///     CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION
    /// ).await?;
    /// ```
    pub async fn start_calibration(
        &mut self,
        options: CalibrationOption,
    ) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::CalibrationRequest::new(options.0, seq);
        self.publish_request(&req).await
    }

    /// Submits a `.3mf` print job from MicroSD storage for execution [REF-MQTT-LIFECYCLE].
    ///
    /// When `nozzle_offset_cali` is `None`, the model's quirks engine resolves the default.
    pub async fn start_print(&mut self, config: &PrintJobConfig) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::ProjectFileRequest::from_config(&config, seq, self.model);
        self.publish_request(&req).await
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
            sequence_counter: INITIAL_SEQUENCE_ID,
            k_profile_primed: false,
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
