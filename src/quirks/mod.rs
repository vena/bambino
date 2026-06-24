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

    /// Evaluates if the specified command string is unsupported or ignored on the target model.
    ///
    /// Enables core command routers to filter out invalid command payloads before transmitting
    /// them across socket networks.
    fn is_unsupported_command(&self, command: &str) -> bool;
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
            BambuModel::X1C | BambuModel::X1E => &models::x1::X1Quirks,
            BambuModel::X2D => &models::x2::X2Quirks,
            BambuModel::H2D | BambuModel::H2DPro | BambuModel::H2C | BambuModel::H2S => {
                &models::h2::H2Quirks
            }
            _ => &models::x1::X1Quirks, // Safe default fallback
        }
    }
}

// ============================================================================
// Specialized Telemetry Signal Processing Helpers
// ============================================================================

pub(crate) const DEFAULT_Z_MOVE_GCODE: &str =
    "M211 S1\nM1002 push_ref_mode\nG91\nG0 Z10.00 F3000\nG90\nM1002 pop_ref_mode";

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
}
