//! # Physical Printer Quirks & Polymorphic Model Behaviors
//!
//! Defines the core `ModelQuirks` trait and peripheral filtering helpers to isolate
//! model-specific network configurations, thermal architectures, kinematics, and telemetry
//! quirks [REF-NET-DOOR] [REF-THER-DECODE] [REF-CLIM-FANS].
//!
//! Behavioral variations are isolated polymorphically to avoid cluttering primary commands
//! with conditional checks. `ModelQuirks` delegates to family-specific submodules inside
//! `models/` to ensure that `quirks/mod.rs` remains strictly a dispatcher and entirely
//! unaware of specific semantic quirks of any individual printer model.

pub mod models;

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use crate::discovery::BambuModel;
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
    /// * `6` or more for automatic tool changer storage racks.
    fn physical_nozzle_count(&self) -> u8;

    /// Returns true if the model supports electronic alignment and nozzle offset calibration sweeps.
    fn supports_nozzle_offset_calibration(&self) -> bool;

    /// Returns true if the build plate moves along the Z-axis (CoreXY bed-on-Z platforms) [REF-MOTO-GCODE].
    fn is_bed_on_z(&self) -> bool;

    /// Evaluates if a given G-code command carries unsafe axis-constrained homing directions [REF-MOTO-GCODE].
    fn is_unsafe_homing_command(&self, gcode: &str) -> bool;

    /// Generates a model-compliant safe relative Z-axis movement G-code command [REF-MOTO-GCODE].
    ///
    /// Evaluates travel limits specific to Bed-Slinger or CoreXY build envelopes. Returns an empty
    /// string if commanded relative distances exceed mechanical bounds.
    fn relative_z_move_gcode(&self, distance: f32, feedrate: u32) -> String;
}

impl ModelQuirks for BambuModel {
    fn uses_plaintext_ftps_data_channel(&self) -> bool {
        match self {
            BambuModel::A1 | BambuModel::A1Mini | BambuModel::A2L => {
                models::a1::uses_plaintext_ftps_data_channel()
            }
            BambuModel::P1P | BambuModel::P1S => models::p1::uses_plaintext_ftps_data_channel(),
            BambuModel::P2S => models::p2::uses_plaintext_ftps_data_channel(),
            BambuModel::X1C | BambuModel::X1E => models::x1::uses_plaintext_ftps_data_channel(),
            BambuModel::X2D => models::x2::uses_plaintext_ftps_data_channel(),
            BambuModel::H2D | BambuModel::H2DPro | BambuModel::H2C | BambuModel::H2S => {
                models::h2::uses_plaintext_ftps_data_channel()
            }
            _ => false,
        }
    }

    fn enforce_ftps_tls_1_2(&self) -> bool {
        match self {
            BambuModel::P2S => models::p2::force_tls_v12_for_ftps(),
            BambuModel::X2D => true, // Enforce TLS 1.3 PASSIVE socket restrictions on X2D
            _ => false,
        }
    }

    fn is_door_open(&self, telemetry: &PrintTelemetry) -> bool {
        if !self.has_door_sensor() {
            return false;
        }
        match self {
            BambuModel::X1C | BambuModel::X1E => telemetry.is_door_open(true),
            BambuModel::X2D
            | BambuModel::P2S
            | BambuModel::H2D
            | BambuModel::H2DPro
            | BambuModel::H2C
            | BambuModel::H2S => telemetry.is_door_open(false),
            _ => false,
        }
    }

    fn has_door_sensor(&self) -> bool {
        match self {
            BambuModel::A1 | BambuModel::A1Mini | BambuModel::A2L => models::a1::has_door_sensor(),
            BambuModel::P1P | BambuModel::P1S => models::p1::has_door_sensor(),
            BambuModel::P2S => models::p2::has_door_sensor(),
            BambuModel::X1C | BambuModel::X1E => models::x1::has_door_sensor(),
            BambuModel::X2D => models::x2::has_door_sensor(),
            BambuModel::H2D | BambuModel::H2DPro | BambuModel::H2C | BambuModel::H2S => {
                models::h2::has_door_sensor()
            }
            _ => false,
        }
    }

    fn camera_stream_port(&self) -> u16 {
        match self {
            BambuModel::A1 | BambuModel::A1Mini | BambuModel::A2L => {
                models::a1::camera_stream_port()
            }
            BambuModel::P1P | BambuModel::P1S => models::p1::camera_stream_port(),
            BambuModel::P2S => models::p2::camera_stream_port(),
            BambuModel::X1C | BambuModel::X1E => models::x1::camera_stream_port(),
            BambuModel::X2D => models::x2::camera_stream_port(),
            BambuModel::H2D | BambuModel::H2DPro | BambuModel::H2C | BambuModel::H2S => {
                models::h2::camera_stream_port()
            }
            _ => 322,
        }
    }

    fn ignores_chamber_temperature(&self) -> bool {
        match self {
            BambuModel::A1 | BambuModel::A1Mini | BambuModel::A2L => {
                models::a1::ignores_chamber_temperature()
            }
            BambuModel::P1P | BambuModel::P1S => models::p1::ignores_chamber_temperature(),
            BambuModel::P2S => models::p2::ignores_chamber_temperature(),
            BambuModel::X1C | BambuModel::X1E => models::x1::ignores_chamber_temperature(),
            BambuModel::X2D => models::x2::ignores_chamber_temperature(),
            BambuModel::H2D | BambuModel::H2DPro | BambuModel::H2C | BambuModel::H2S => {
                models::h2::ignores_chamber_temperature()
            }
            _ => false,
        }
    }

    fn has_stg_cur_idle_bug(&self) -> bool {
        match self {
            BambuModel::A1 | BambuModel::A1Mini | BambuModel::A2L => {
                models::a1::has_stg_cur_idle_bug()
            }
            BambuModel::P1P | BambuModel::P1S => models::p1::has_stg_cur_idle_bug(),
            BambuModel::P2S => models::p2::has_stg_cur_idle_bug(),
            BambuModel::X1C | BambuModel::X1E => models::x1::has_stg_cur_idle_bug(),
            BambuModel::X2D => models::x2::has_stg_cur_idle_bug(),
            BambuModel::H2D | BambuModel::H2DPro | BambuModel::H2C | BambuModel::H2S => {
                models::h2::has_stg_cur_idle_bug()
            }
            _ => false,
        }
    }

    fn has_active_chamber_heater(&self) -> bool {
        match self {
            BambuModel::A1 | BambuModel::A1Mini | BambuModel::A2L => {
                models::a1::has_active_chamber_heater()
            }
            BambuModel::P1P | BambuModel::P1S => models::p1::has_active_chamber_heater(),
            BambuModel::P2S => models::p2::has_active_chamber_heater(),
            BambuModel::X1E => models::x1::has_active_chamber_heater(true),
            BambuModel::X1C => models::x1::has_active_chamber_heater(false),
            BambuModel::X2D => models::x2::has_active_chamber_heater(),
            BambuModel::H2D | BambuModel::H2DPro => models::h2::has_active_chamber_heater(true),
            BambuModel::H2C | BambuModel::H2S => models::h2::has_active_chamber_heater(false),
            _ => false,
        }
    }

    fn physical_nozzle_count(&self) -> u8 {
        match self {
            BambuModel::A1 | BambuModel::A1Mini | BambuModel::A2L => {
                models::a1::physical_nozzle_count()
            }
            BambuModel::P1P | BambuModel::P1S => models::p1::physical_nozzle_count(),
            BambuModel::P2S => models::p2::physical_nozzle_count(),
            BambuModel::X1C | BambuModel::X1E => models::x1::physical_nozzle_count(),
            BambuModel::X2D => models::x2::physical_nozzle_count(),
            BambuModel::H2C => models::h2::physical_nozzle_count(true, false),
            BambuModel::H2D | BambuModel::H2DPro => models::h2::physical_nozzle_count(false, true),
            BambuModel::H2S => models::h2::physical_nozzle_count(false, false),
            _ => 1,
        }
    }

    fn supports_nozzle_offset_calibration(&self) -> bool {
        match self {
            BambuModel::H2C => models::h2::supports_nozzle_offset_calibration(false, true),
            BambuModel::H2D | BambuModel::H2DPro => {
                models::h2::supports_nozzle_offset_calibration(true, false)
            }
            BambuModel::H2S => models::h2::supports_nozzle_offset_calibration(false, false),
            _ => self.physical_nozzle_count() > 1,
        }
    }

    fn is_bed_on_z(&self) -> bool {
        match self {
            BambuModel::A1 | BambuModel::A1Mini | BambuModel::A2L => models::a1::is_bed_on_z(),
            BambuModel::P1P | BambuModel::P1S => models::p1::is_bed_on_z(),
            BambuModel::P2S => models::p2::is_bed_on_z(),
            BambuModel::X1C | BambuModel::X1E => models::x1::is_bed_on_z(),
            BambuModel::X2D => models::x2::is_bed_on_z(),
            BambuModel::H2D | BambuModel::H2DPro | BambuModel::H2C | BambuModel::H2S => {
                models::h2::is_bed_on_z()
            }
            _ => false,
        }
    }

    fn is_unsafe_homing_command(&self, gcode: &str) -> bool {
        if self.is_bed_on_z() {
            models::x1::is_unsafe_homing_command(gcode)
        } else {
            false
        }
    }

    fn relative_z_move_gcode(&self, distance: f32, feedrate: u32) -> String {
        match self {
            BambuModel::A1Mini => {
                let code = models::a1::relative_z_move_gcode(distance, feedrate, true);
                if code.is_empty() {
                    String::new()
                } else {
                    format!(
                        "M211 S1\nM1002 push_ref_mode\nG91\nG0 Z{:.2} F{}\nG90\nM1002 pop_ref_mode",
                        distance, feedrate
                    )
                }
            }
            BambuModel::A1 | BambuModel::A2L => {
                let code = models::a1::relative_z_move_gcode(distance, feedrate, false);
                if code.is_empty() {
                    String::new()
                } else {
                    format!(
                        "M211 S1\nM1002 push_ref_mode\nG91\nG0 Z{:.2} F{}\nG90\nM1002 pop_ref_mode",
                        distance, feedrate
                    )
                }
            }
            _ => {
                // CoreXY or default kinematic models
                format!(
                    "M211 S1\nM1002 push_ref_mode\nG91\nG0 Z{:.2} F{}\nG90\nM1002 pop_ref_mode",
                    distance, feedrate
                )
            }
        }
    }
}

// ============================================================================
// Specialized Telemetry Signal Processing Helpers
// ============================================================================

/// Converts a discrete fan speed step (0 to 15) to an integer percentage (0 to 100) [REF-CLIM-FANS].
///
/// Implements standard mathematical rounding logic: `Round(Step * 100 / 15)`.
pub fn fan_step_to_percentage(step: u8) -> u8 {
    if step >= 15 {
        100
    } else {
        // Safe integer-based rounding equivalent to: round(step * 6.67)
        ((step as u32 * 100 + 7) / 15) as u8
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

    #[test]
    fn test_homing_unsafe_check() {
        let model = BambuModel::X1C;
        assert!(model.is_unsafe_homing_command("G28 Z"));
        assert!(!model.is_unsafe_homing_command("G28"));

        let a1 = BambuModel::A1;
        assert!(!a1.is_unsafe_homing_command("G28 Z")); // Safe for bed-slingers
    }
}
