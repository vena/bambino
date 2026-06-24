//! # Physical Printer Quirks & Polymorphic Model Behaviors
//!
//! Defines the core `ModelQuirks` trait and peripheral filtering helpers to isolate
//! model-specific network configurations, thermal architectures, kinematics, and telemetry
//! quirks [REF-NET-DOOR] [REF-THER-DECODE] [REF-CLIM-FANS].
//!
//! Behavioral variations are isolated polymorphically using the Strategy Pattern to avoid
//! match-statement cluttering and duplicate dispatching branches inside primary commands.

pub mod models;

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use crate::models::BambuModel;
use crate::types::PrintTelemetry;

/// Polymorphic interface tracking model-specific hardware variations and transport exceptions.
pub trait ModelQuirks {
    /// Returns true if this model series requires plaintext transmissions on the
    /// FTPS passive data channel (PROT C) due to board limitations [REF-FTPS-CONN].
    fn uses_plaintext_ftps_data_channel(&self) -> bool;

    /// Returns true if this model series must restrict its TLS version strictly
    /// to TLS 1.2 to prevent session resumption failure [REF-FTPS-CONN].
    fn enforce_ftps_tls_1_2(&self) -> bool;

    /// Evaluates whether the physical front enclosure door is open based on
    /// model-specific sensor routing [REF-NET-DOOR].
    ///
    /// If the target model lacks an electronic door sensor switch, returns `false`.
    fn is_door_open(&self, telemetry: &PrintTelemetry) -> bool;

    /// Returns true if the physical machine chassis is equipped with an electronic
    /// front enclosure door open sensor switch.
    fn has_door_sensor(&self) -> bool;

    /// Returns the physical local TCP port used by the model's camera interface [REF-NET-PORTS].
    ///
    /// * Port `322`: High-capability RTSPS stream.
    /// * Port `6000`: Binary JPEG frame-buffer socket.
    fn camera_stream_port(&self) -> u16;

    /// Returns true if the model is an open-frame or entry-level machine lacking
    /// a physical chamber temperature sensor [REF-THER-DECODE].
    fn ignores_chamber_temperature(&self) -> bool;

    /// Returns true if the model series exhibits the idle state-machine bug where
    /// `stg_cur = 0` (Printing) is reported in idle phases [REF-MQTT-IDLEBUG].
    fn has_stg_cur_idle_bug(&self) -> bool;

    /// Returns true if the model possesses an active heated chamber control loop [REF-MOTO-GCODE].
    ///
    /// Active chamber heating is restricted to specific enclosed models (e.g., X1E, P2S).
    fn has_active_chamber_heater(&self) -> bool;

    /// Returns the number of physical extruder carriages present on the machine carriage bus.
    ///
    /// * `1` for standard single-nozzle configurations.
    /// * `2` for independent dual-extruder (IDEX) platforms.
    /// * `7` for automatic tool changer storage racks (1 dedicated + 6 interchangeable).
    fn physical_nozzle_count(&self) -> u8;

    /// Returns true if the model supports electronic alignment and nozzle offset calibration sweeps.
    fn supports_nozzle_offset_calibration(&self) -> bool;

    /// Returns true if the build plate moves along the Z-axis (CoreXY bed-on-Z platforms) [REF-MOTO-GCODE].
    fn is_bed_on_z(&self) -> bool;

    /// Evaluates if a given G-code command carries unsafe axis-constrained homing directions [REF-MOTO-GCODE].
    ///
    /// Default: bed-on-Z models reject G28 with axis constraints (Z, X, or Y) to prevent
    /// nozzle-to-plate collisions. Bed-slingers allow all homing variants.
    fn is_unsafe_homing_command(&self, gcode: &str) -> bool {
        if !self.is_bed_on_z() {
            return false;
        }
        let clean = gcode.to_uppercase();
        clean.contains("G28") && (clean.contains('Z') || clean.contains('X') || clean.contains('Y'))
    }

    /// Returns the maximum safe Z-axis travel distance in millimeters for this model.
    fn z_max(&self) -> f32 {
        256.0
    }

    /// Generates a model-compliant safe relative Z-axis movement G-code command [REF-MOTO-GCODE].
    ///
    /// Evaluates travel limits specific to Bed-Slinger or CoreXY build envelopes. Returns an empty
    /// string if commanded relative distances exceed mechanical bounds.
    fn relative_z_move_gcode(&self, distance: f32, feedrate: u32) -> String {
        format_z_move_gcode(distance, feedrate, self.z_max())
    }

    /// Evaluates if the specified command string is unsupported or ignored on the target model.
    fn is_unsupported_command(&self, _command: &str) -> bool {
        false
    }

    /// Returns true if the model's RTSP camera stream requires wallclock timestamps
    /// instead of embedded RTP clock ticks to avoid frame freezing [REF-CAM-RTSPS].
    fn requires_wallclock_rtsp_timestamps(&self) -> bool {
        false
    }

    /// Returns true if the model has a secondary right-side auxiliary fan (port 10) [REF-CLIM-FANS].
    fn supports_auxiliary_right_fan(&self) -> bool {
        false
    }

    /// Returns true if the model's auxiliary fan telemetry reports speed as a direct
    /// percentage (0-100) instead of discrete PWM steps (0-15) [REF-CLIM-FANS].
    fn auxiliary_fan_uses_percentage(&self) -> bool {
        false
    }
}

impl BambuModel {
    /// Resolves the static `ModelQuirks` strategy matching this model variant.
    ///
    /// **Why this is used:**
    /// Consolidates polymorphic dispatching into exactly one place, eliminating duplicated
    /// match-blocks across every single trait function in the quirks library.
    pub fn quirks(&self) -> &'static dyn ModelQuirks {
        match self {
            BambuModel::A1 | BambuModel::A1Mini | BambuModel::A2L => &models::a1::A1Quirks,
            BambuModel::P1P | BambuModel::P1S => &models::p1::P1Quirks,
            BambuModel::P2S => &models::p2::P2Quirks,
            BambuModel::X1C => &models::x1::X1CQuirks,
            BambuModel::X1E => &models::x1::X1EQuirks,
            BambuModel::X2D => &models::x2::X2Quirks,
            BambuModel::H2S => &models::h2::H2SQuirks,
            BambuModel::H2D => &models::h2::H2DQuirks,
            BambuModel::H2DPro => &models::h2::H2DProQuirks,
            BambuModel::H2C => &models::h2::H2CQuirks,
            _ => &models::x1::X1CQuirks,
        }
    }
}

// ============================================================================
// Specialized Telemetry Signal Processing Helpers
// ============================================================================

/// Generates a safe relative Z-axis movement G-code block with travel limit guards.
///
/// Returns an empty string if `distance` is zero or exceeds the model's Z bounds.
pub(crate) fn format_z_move_gcode(distance: f32, feedrate: u32, z_max: f32) -> String {
    if distance == 0.0 || distance.abs() > z_max {
        return String::new();
    }
    format!(
        "M211 S1\nM1002 push_ref_mode\nG91\nG0 Z{:.2} F{}\nG90\nM1002 pop_ref_mode",
        distance, feedrate
    )
}

pub(crate) const FAN_STEP_MAX: u8 = 15;
pub(crate) const FAN_ROUNDING_OFFSET: u32 = 7;

/// Converts a discrete fan speed step (0 to 15) to an integer percentage (0 to 100) [REF-CLIM-FANS].
///
/// Implements standard mathematical rounding logic: `Round(Step * 100 / 15)`.
pub fn fan_step_to_percentage(step: u8) -> u8 {
    if step >= FAN_STEP_MAX {
        100
    } else {
        ((step as u32 * 100 + FAN_ROUNDING_OFFSET) / FAN_STEP_MAX as u32) as u8
    }
}

/// Filters out transient quantization oscillation artifacts emitted by physical fan controllers.
///
/// **Why this is required [REF-CLIM-FANS]:**
/// Due to the low-resolution 0–15 PWM mapping on physical boards, minor fan throttle drift
/// can cause telemetry reports to bounce rapidly between adjacent steps (e.g. step 7 and step 8),
/// triggering interface flickering. This state tracker dampens steps by requiring persistent,
/// consecutive readings before committing a one-step change.
#[derive(Debug, Clone)]
pub struct FanSpeedDebouncer {
    last_stable_percentage: u8,
    consecutive_counts: u8,
    target_value: u8,
}

impl Default for FanSpeedDebouncer {
    fn default() -> Self {
        Self::new()
    }
}

impl FanSpeedDebouncer {
    /// Instantiates a new debouncer initialized to 0% speed.
    pub fn new() -> Self {
        Self {
            last_stable_percentage: 0,
            consecutive_counts: 0,
            target_value: 0,
        }
    }

    /// Processes an raw incoming fan speed percentage, filtering minor step oscillations.
    ///
    /// Allows large transitions (greater than 1 step or ~7% diff) to commit immediately
    /// to maintain user responsiveness, while locking single-step toggles until they persist
    /// for at least 3 consecutive frames.
    pub fn debounce(&mut self, incoming_percentage: u8) -> u8 {
        let diff = (incoming_percentage as i16 - self.last_stable_percentage as i16).abs();

        if diff <= 7 {
            // Evaluates whether the change is a transient step bounce or a permanent shift.
            if incoming_percentage == self.last_stable_percentage {
                self.consecutive_counts = 0;
                self.target_value = incoming_percentage;
            } else if incoming_percentage == self.target_value {
                self.consecutive_counts += 1;
                if self.consecutive_counts >= 3 {
                    self.last_stable_percentage = incoming_percentage;
                    self.consecutive_counts = 0;
                }
            } else {
                self.target_value = incoming_percentage;
                self.consecutive_counts = 1;
            }
        } else {
            // Significant control shift (e.g., fan turning off/on completely). Bypass filter.
            self.last_stable_percentage = incoming_percentage;
            self.target_value = incoming_percentage;
            self.consecutive_counts = 0;
        }

        self.last_stable_percentage
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BambuModel;

    #[test]
    fn test_fan_step_rounding() {
        assert_eq!(fan_step_to_percentage(0), 0);
        assert_eq!(fan_step_to_percentage(15), 100);
        assert_eq!(fan_step_to_percentage(8), 53);
        assert_eq!(fan_step_to_percentage(4), 27);
    }

    #[test]
    fn test_fan_debounce_filter() {
        let mut debouncer = FanSpeedDebouncer::new();

        assert_eq!(debouncer.debounce(53), 53);

        assert_eq!(debouncer.debounce(47), 53);
        assert_eq!(debouncer.debounce(47), 53);

        assert_eq!(debouncer.debounce(47), 47);

        assert_eq!(debouncer.debounce(53), 47);
        assert_eq!(debouncer.debounce(47), 47);
    }

    // Per-model quirks assertion tests

    #[test]
    fn test_a1_quirks() {
        for model in [BambuModel::A1, BambuModel::A1Mini, BambuModel::A2L] {
            let q = model.quirks();
            assert!(q.uses_plaintext_ftps_data_channel());
            assert!(!q.enforce_ftps_tls_1_2());
            assert!(!q.has_door_sensor());
            assert_eq!(q.camera_stream_port(), 6000);
            assert!(q.ignores_chamber_temperature());
            assert!(q.has_stg_cur_idle_bug());
            assert!(!q.has_active_chamber_heater());
            assert_eq!(q.physical_nozzle_count(), 1);
            assert!(!q.supports_nozzle_offset_calibration());
            assert!(!q.is_bed_on_z());
            assert!(!q.requires_wallclock_rtsp_timestamps());
            assert!(!q.supports_auxiliary_right_fan());
            assert!(!q.auxiliary_fan_uses_percentage());
        }
    }

    #[test]
    fn test_p1_quirks() {
        for model in [BambuModel::P1P, BambuModel::P1S] {
            let q = model.quirks();
            assert!(!q.uses_plaintext_ftps_data_channel());
            assert!(!q.enforce_ftps_tls_1_2());
            assert!(!q.has_door_sensor());
            assert_eq!(q.camera_stream_port(), 6000);
            assert!(q.ignores_chamber_temperature());
            assert!(q.has_stg_cur_idle_bug());
            assert!(!q.has_active_chamber_heater());
            assert_eq!(q.physical_nozzle_count(), 1);
            assert!(!q.supports_nozzle_offset_calibration());
            assert!(q.is_bed_on_z());
            assert!(!q.requires_wallclock_rtsp_timestamps());
            assert!(!q.supports_auxiliary_right_fan());
        }
    }

    #[test]
    fn test_p2s_quirks() {
        let q = BambuModel::P2S.quirks();
        assert!(!q.uses_plaintext_ftps_data_channel());
        assert!(q.enforce_ftps_tls_1_2());
        assert!(q.has_door_sensor());
        assert_eq!(q.camera_stream_port(), 322);
        assert!(!q.ignores_chamber_temperature());
        assert!(!q.has_stg_cur_idle_bug());
        assert!(q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 1);
        assert!(!q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
        assert!(q.requires_wallclock_rtsp_timestamps());
        assert!(!q.supports_auxiliary_right_fan());
    }

    #[test]
    fn test_x1c_quirks() {
        let q = BambuModel::X1C.quirks();
        assert!(!q.uses_plaintext_ftps_data_channel());
        assert!(!q.enforce_ftps_tls_1_2());
        assert!(q.has_door_sensor());
        assert_eq!(q.camera_stream_port(), 322);
        assert!(!q.ignores_chamber_temperature());
        assert!(!q.has_stg_cur_idle_bug());
        assert!(!q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 1);
        assert!(!q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
        assert!(!q.requires_wallclock_rtsp_timestamps());
        assert!(!q.supports_auxiliary_right_fan());
    }

    #[test]
    fn test_x1e_quirks() {
        let q = BambuModel::X1E.quirks();
        assert!(!q.uses_plaintext_ftps_data_channel());
        assert!(!q.enforce_ftps_tls_1_2());
        assert!(q.has_door_sensor());
        assert_eq!(q.camera_stream_port(), 322);
        assert!(!q.ignores_chamber_temperature());
        assert!(!q.has_stg_cur_idle_bug());
        assert!(q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 1);
        assert!(!q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
    }

    #[test]
    fn test_x2d_quirks() {
        let q = BambuModel::X2D.quirks();
        assert!(!q.uses_plaintext_ftps_data_channel());
        assert!(q.enforce_ftps_tls_1_2());
        assert!(q.has_door_sensor());
        assert_eq!(q.camera_stream_port(), 322);
        assert!(!q.ignores_chamber_temperature());
        assert!(!q.has_stg_cur_idle_bug());
        assert!(!q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 2);
        assert!(q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
        assert!(q.supports_auxiliary_right_fan());
        assert!(q.auxiliary_fan_uses_percentage());
    }

    #[test]
    fn test_h2s_quirks() {
        let q = BambuModel::H2S.quirks();
        assert!(!q.uses_plaintext_ftps_data_channel());
        assert!(!q.enforce_ftps_tls_1_2());
        assert!(q.has_door_sensor());
        assert_eq!(q.camera_stream_port(), 322);
        assert!(!q.ignores_chamber_temperature());
        assert!(!q.has_stg_cur_idle_bug());
        assert!(q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 1);
        assert!(!q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
    }

    #[test]
    fn test_h2d_quirks() {
        let q = BambuModel::H2D.quirks();
        assert!(q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 2);
        assert!(q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
    }

    #[test]
    fn test_h2d_pro_quirks() {
        let q = BambuModel::H2DPro.quirks();
        assert!(q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 2);
        assert!(q.supports_nozzle_offset_calibration());
    }

    #[test]
    fn test_h2c_quirks() {
        let q = BambuModel::H2C.quirks();
        assert!(q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 7);
        assert!(q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
    }

    #[test]
    fn test_unknown_fallback_quirks() {
        let q = BambuModel::Unknown.quirks();
        assert!(!q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 1);
    }

    // Z-move gcode parameterization tests

    #[test]
    fn test_z_move_gcode_parameterized() {
        let gcode = format_z_move_gcode(10.0, 3000, 256.0);
        assert!(gcode.contains("Z10.00"));
        assert!(gcode.contains("F3000"));
        assert!(gcode.contains("M211 S1"));
        assert!(gcode.contains("push_ref_mode"));
    }

    #[test]
    fn test_z_move_gcode_negative_distance() {
        let gcode = format_z_move_gcode(-5.5, 1500, 256.0);
        assert!(gcode.contains("Z-5.50"));
        assert!(gcode.contains("F1500"));
    }

    #[test]
    fn test_z_move_gcode_exceeds_bounds() {
        assert!(format_z_move_gcode(300.0, 3000, 256.0).is_empty());
        assert!(format_z_move_gcode(-300.0, 3000, 256.0).is_empty());
    }

    #[test]
    fn test_z_move_gcode_zero_distance() {
        assert!(format_z_move_gcode(0.0, 3000, 256.0).is_empty());
    }

    #[test]
    fn test_z_move_gcode_at_boundary() {
        let gcode = format_z_move_gcode(256.0, 3000, 256.0);
        assert!(gcode.contains("Z256.00"));
    }

    #[test]
    fn test_z_move_via_trait() {
        let q = BambuModel::P1P.quirks();
        let gcode = q.relative_z_move_gcode(15.0, 2000);
        assert!(gcode.contains("Z15.00"));
        assert!(gcode.contains("F2000"));
    }

    // Homing safety tests

    #[test]
    fn test_unsafe_homing_bed_on_z() {
        let q = BambuModel::P1P.quirks();
        assert!(q.is_unsafe_homing_command("G28 Z"));
        assert!(q.is_unsafe_homing_command("g28 x"));
        assert!(q.is_unsafe_homing_command("G28 X Y Z"));
        assert!(!q.is_unsafe_homing_command("G28"));
        assert!(!q.is_unsafe_homing_command("G1 Z10"));
    }

    #[test]
    fn test_a1_homing_always_safe() {
        let q = BambuModel::A1.quirks();
        assert!(!q.is_unsafe_homing_command("G28 Z"));
        assert!(!q.is_unsafe_homing_command("G28"));
    }
}
