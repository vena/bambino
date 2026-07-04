//! # Model-Specific Quirks
//!
//! Bambu Lab printers vary in hardware capabilities — door sensors, chamber heaters,
//! fan step resolution, FTPS TLS requirements, camera protocols, and more. Rather than
//! scattering `match model { ... }` blocks everywhere, the [`ModelQuirks`] trait captures
//! all model-specific behavior in one place. Call [`BambuModel::quirks()`] to get the
//! strategy implementation for any model.
//!
//! Per-model strategy structs live in the [`models`] submodule. This module also provides
//! shared helpers like [`fan_step_to_percentage()`] and [`FanSpeedDebouncer`] for dealing
//! with the low-resolution PWM fan telemetry common across most models.

pub mod models;

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use crate::camera::CameraProtocol;
use crate::models::BambuModel;
use crate::types::PrinterTelemetry;

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
    fn is_door_open(&self, telemetry: &PrinterTelemetry) -> bool;

    /// Returns true if the physical machine chassis is equipped with an electronic
    /// front enclosure door open sensor switch.
    fn has_door_sensor(&self) -> bool;

    /// Returns the camera streaming protocol used by this model's hardware [REF-NET-PORTS].
    fn camera_protocol(&self) -> CameraProtocol;

    /// Returns true if the model is an open-frame or entry-level machine lacking
    /// a physical chamber temperature sensor [REF-THER-DECODE].
    fn ignores_chamber_temperature(&self) -> bool;

    /// Returns true if the model series exhibits the idle state-machine bug where
    /// `stg_cur = 0` (Printing) is reported in idle phases [REF-MQTT-IDLEBUG].
    fn has_stg_cur_idle_bug(&self) -> bool;

    /// Returns true if the model possesses an active PTC chamber heater (M141) [REF-MOTO-GCODE].
    ///
    /// Supported on: X1E, X2D, H2S, H2D, H2D Pro, H2C.
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
    ///
    /// Scans every line of `gcode` independently — multi-statement `\n`-joined payloads are a
    /// documented, supported wire shape (see `GCodeRequest`) — and recognizes `G28` as a
    /// case-insensitive prefix match on a line rather than requiring it to be the entire leading
    /// whitespace-split token, so glued forms like `G28X` (no space before the axis letter) are
    /// caught too, alongside the already-handled space-separated form (`G28 X`).
    fn is_unsafe_homing_command(&self, gcode: &str) -> bool {
        if !self.is_bed_on_z() {
            return false;
        }
        gcode.lines().any(line_has_unsafe_homing)
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

    /// Returns true if the model's RTSP camera stream requires wallclock timestamps
    /// instead of embedded RTP clock ticks to avoid frame freezing [REF-CAM-RTSPS].
    fn requires_wallclock_rtsp_timestamps(&self) -> bool {
        false
    }

    /// Returns true if the model has a secondary right-side auxiliary fan (port 10) [REF-CLIM-FANS].
    fn supports_auxiliary_right_fan(&self) -> bool {
        false
    }

    /// Returns true if the model has a primary left-side auxiliary fan (port 2) [REF-CLIM-FANS].
    ///
    /// Universal default: only A1, A1 Mini, A2L (open-frame bed-slingers lacking this fan)
    /// override this to `false`.
    fn supports_auxiliary_left_fan(&self) -> bool {
        true
    }

    /// Returns true if the model has a chamber exhaust/filtration fan (port 3) [REF-CLIM-FANS].
    ///
    /// Supported on: H2S, H2D, H2D Pro, H2C, X2D.
    fn has_chamber_exhaust_fan(&self) -> bool {
        false
    }

    /// Returns true if the model's auxiliary fan telemetry reports speed as a direct
    /// percentage (0-100) instead of discrete PWM steps (0-15) [REF-CLIM-FANS].
    fn auxiliary_fan_uses_percentage(&self) -> bool {
        false
    }

    /// Returns true if the model has controllable airduct dampers for climate
    /// mode switching (cooling vs heating recirculation) [REF-CLIM-FANS].
    ///
    /// Supported on: H2S, H2D, H2D Pro, H2C, P2S, X2D.
    fn supports_airduct_mode(&self) -> bool {
        false
    }

    /// Returns true if the model has onboard speakers for prompt sound notifications.
    ///
    /// Supported on: A1, A1 Mini, A2L (confirmed by Bambu Studio profiles).
    fn supports_prompt_sound(&self) -> bool {
        false
    }

    /// Returns true if the model has a physical fire alarm buzzer module.
    ///
    /// Supported on: H2S, H2D, H2D Pro, H2C (confirmed by pybambu).
    fn supports_buzzer(&self) -> bool {
        false
    }

    /// Returns the maximum safe nozzle/hotend temperature in °C for this model.
    fn nozzle_temp_max(&self) -> u16;

    /// Returns the maximum safe heated bed temperature in °C for this model.
    ///
    /// `mains_220v` is `Some(true)`/`Some(false)` when the printer's mains voltage region is
    /// known (from `PrinterTelemetry::is_220v_power()`, derived from `home_flag` bit 3), or
    /// `None` before any `home_flag` telemetry has been received. Every model except X1C ignores
    /// this parameter and returns a flat constant — see `X1CQuirks::bed_temp_max` for the one
    /// model where the ceiling is genuinely voltage-dependent per the official spec sheet
    /// ("Max Build Plate Temperature: 110°C @220V, 120°C @110V").
    fn bed_temp_max(&self, mains_220v: Option<bool>) -> u16;

    /// Returns the maximum active chamber heater temperature in °C for this model.
    ///
    /// Returns 0 for models without an active PTC chamber heater.
    fn chamber_temp_max(&self) -> u16 {
        0
    }
}

impl BambuModel {
    /// Returns the [`ModelQuirks`] strategy for this model variant.
    ///
    /// This is the single dispatch point — all model-specific behavior goes through
    /// the trait object returned here, rather than match-blocks scattered across the crate.
    pub fn quirks(&self) -> &'static dyn ModelQuirks {
        match self {
            BambuModel::A1 => &models::a1::A1Quirks,
            BambuModel::A2L => &models::a2::A2LQuirks,
            BambuModel::A1Mini => &models::a1::A1MiniQuirks,
            BambuModel::P1P | BambuModel::P1S => &models::p1::P1Quirks,
            BambuModel::P2S => &models::p2::P2Quirks,
            BambuModel::X1C => &models::x1::X1CQuirks,
            BambuModel::X1E => &models::x1::X1EQuirks,
            BambuModel::X2D => &models::x2::X2Quirks,
            BambuModel::H2S => &models::h2::H2SQuirks,
            BambuModel::H2D => &models::h2::H2DQuirks,
            BambuModel::H2DPro => &models::h2::H2DProQuirks,
            BambuModel::H2C => &models::h2::H2CQuirks,
            BambuModel::Unknown => {
                log::warn!(
                    "Unrecognized printer model — applying X1C quirks as a conservative default"
                );
                &models::x1::X1CQuirks
            }
        }
    }
}

// ============================================================================
// Specialized Telemetry Signal Processing Helpers
// ============================================================================

/// Returns true if `line` contains an axis-constrained `G28` homing command.
///
/// Recognizes `G28` as a case-insensitive prefix match rather than requiring the whole token to
/// equal `G28` — this catches axis letters glued directly to the command (`G28X`) in addition to
/// the space-separated form (`G28 X`). Rejects numeric extensions of the command number (e.g.
/// `G280`, `G281`), which are distinct G-codes, not `G28` with a trailing digit. `no_std` rules
/// out `regex`, hence the manual byte/char scan.
fn line_has_unsafe_homing(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if is_g28_prefix(bytes, i) {
            let rest = &line[i + 3..];
            let next_is_digit = rest
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false);
            if !next_is_digit
                && rest
                    .chars()
                    .any(|c| matches!(c.to_ascii_uppercase(), 'X' | 'Y' | 'Z'))
            {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Returns true if `bytes[i..]` starts with `G28` (case-insensitive on the `G`; `2`/`8` are digits
/// with no case to normalize).
fn is_g28_prefix(bytes: &[u8], i: usize) -> bool {
    bytes.len() >= i + 3
        && bytes[i].eq_ignore_ascii_case(&b'G')
        && bytes[i + 1] == b'2'
        && bytes[i + 2] == b'8'
}

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

/// Decodes a raw fan-speed telemetry string (`cooling_fan_speed`/`big_fan1_speed`/
/// `big_fan2_speed`/`heatbreak_fan_speed`) into a 0-100 percentage.
///
/// `uses_percentage` should come from [`ModelQuirks::auxiliary_fan_uses_percentage()`] — most
/// models report a 0-15 step value needing [`fan_step_to_percentage()`], but some report an
/// already-clamped percentage directly. Returns `None` if `raw` is absent or not a valid `u8`.
pub fn decode_fan_percentage(raw: Option<&str>, uses_percentage: bool) -> Option<u8> {
    let step: u8 = raw?.parse().ok()?;
    Some(if uses_percentage {
        step.min(100)
    } else {
        fan_step_to_percentage(step)
    })
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
    use crate::camera::CameraProtocol;
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
        let q = BambuModel::A1.quirks();
        assert!(q.uses_plaintext_ftps_data_channel());
        assert!(!q.enforce_ftps_tls_1_2());
        assert!(!q.has_door_sensor());
        assert_eq!(q.camera_protocol(), CameraProtocol::BinaryJpeg);
        assert!(q.ignores_chamber_temperature());
        assert!(q.has_stg_cur_idle_bug());
        assert!(!q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 1);
        assert!(!q.supports_nozzle_offset_calibration());
        assert!(!q.is_bed_on_z());
        assert!(!q.requires_wallclock_rtsp_timestamps());
        assert!(!q.supports_auxiliary_right_fan());
        assert!(!q.supports_auxiliary_left_fan());
        assert!(!q.has_chamber_exhaust_fan());
        assert!(!q.auxiliary_fan_uses_percentage());
        assert_eq!(q.z_max(), 256.0);
        assert_eq!(q.nozzle_temp_max(), 300);
        assert_eq!(q.bed_temp_max(None), 100);
        assert_eq!(q.chamber_temp_max(), 0);
        assert!(!q.supports_airduct_mode());
        assert!(q.supports_prompt_sound());
        assert!(!q.supports_buzzer());
    }

    #[test]
    fn test_a2l_quirks() {
        let q = BambuModel::A2L.quirks();
        assert!(!q.uses_plaintext_ftps_data_channel());
        assert!(!q.enforce_ftps_tls_1_2());
        assert!(!q.has_door_sensor());
        assert_eq!(q.camera_protocol(), CameraProtocol::BinaryJpeg);
        assert!(q.ignores_chamber_temperature());
        assert!(!q.has_stg_cur_idle_bug());
        assert!(!q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 1);
        assert!(!q.supports_nozzle_offset_calibration());
        assert!(!q.is_bed_on_z());
        assert_eq!(q.z_max(), 325.0);
        assert!(q.relative_z_move_gcode(330.0, 3000).is_empty());
        assert!(!q.relative_z_move_gcode(300.0, 3000).is_empty());
        assert_eq!(q.nozzle_temp_max(), 300);
        assert_eq!(q.bed_temp_max(None), 80);
        assert!(!q.supports_auxiliary_left_fan());
        assert!(!q.has_chamber_exhaust_fan());
        assert!(!q.supports_airduct_mode());
        assert!(q.supports_prompt_sound());
        assert!(!q.supports_buzzer());
    }

    #[test]
    fn test_a1_mini_quirks() {
        let q = BambuModel::A1Mini.quirks();
        assert!(q.uses_plaintext_ftps_data_channel());
        assert!(!q.enforce_ftps_tls_1_2());
        assert!(!q.has_door_sensor());
        assert_eq!(q.camera_protocol(), CameraProtocol::BinaryJpeg);
        assert!(q.ignores_chamber_temperature());
        assert!(q.has_stg_cur_idle_bug());
        assert!(!q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 1);
        assert!(!q.supports_nozzle_offset_calibration());
        assert!(!q.is_bed_on_z());
        assert_eq!(q.z_max(), 180.0);
        assert!(q.relative_z_move_gcode(200.0, 3000).is_empty());
        assert!(!q.relative_z_move_gcode(150.0, 3000).is_empty());
        assert_eq!(q.nozzle_temp_max(), 300);
        assert_eq!(q.bed_temp_max(None), 80);
        assert!(!q.supports_auxiliary_left_fan());
        assert!(!q.has_chamber_exhaust_fan());
        assert!(!q.supports_airduct_mode());
        assert!(q.supports_prompt_sound());
        assert!(!q.supports_buzzer());
    }

    #[test]
    fn test_p1_quirks() {
        for model in [BambuModel::P1P, BambuModel::P1S] {
            let q = model.quirks();
            assert!(!q.uses_plaintext_ftps_data_channel());
            assert!(!q.enforce_ftps_tls_1_2());
            assert!(!q.has_door_sensor());
            assert_eq!(q.camera_protocol(), CameraProtocol::BinaryJpeg);
            assert!(q.ignores_chamber_temperature());
            assert!(q.has_stg_cur_idle_bug());
            assert!(!q.has_active_chamber_heater());
            assert_eq!(q.physical_nozzle_count(), 1);
            assert!(!q.supports_nozzle_offset_calibration());
            assert!(q.is_bed_on_z());
            assert!(!q.requires_wallclock_rtsp_timestamps());
            assert!(!q.supports_auxiliary_right_fan());
            assert!(q.supports_auxiliary_left_fan());
            assert!(!q.has_chamber_exhaust_fan());
            assert_eq!(q.z_max(), 256.0);
            assert_eq!(q.nozzle_temp_max(), 300);
            assert_eq!(q.bed_temp_max(None), 100);
            assert!(!q.supports_airduct_mode());
            assert!(!q.supports_prompt_sound());
            assert!(!q.supports_buzzer());
        }
    }

    #[test]
    fn test_p2s_quirks() {
        let q = BambuModel::P2S.quirks();
        assert!(!q.uses_plaintext_ftps_data_channel());
        assert!(q.enforce_ftps_tls_1_2());
        assert!(q.has_door_sensor());
        assert_eq!(q.camera_protocol(), CameraProtocol::Rtsps);
        assert!(!q.ignores_chamber_temperature());
        assert!(!q.has_stg_cur_idle_bug());
        assert!(!q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 1);
        assert!(!q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
        assert!(q.requires_wallclock_rtsp_timestamps());
        assert!(q.supports_auxiliary_right_fan());
        assert!(q.supports_auxiliary_left_fan());
        assert!(!q.has_chamber_exhaust_fan());
        assert!(q.auxiliary_fan_uses_percentage());
        assert_eq!(q.z_max(), 256.0);
        assert_eq!(q.nozzle_temp_max(), 300);
        assert_eq!(q.bed_temp_max(None), 110);
        assert_eq!(q.chamber_temp_max(), 0);
        assert!(q.supports_airduct_mode());
        assert!(!q.supports_prompt_sound());
        assert!(!q.supports_buzzer());
    }

    #[test]
    fn test_x1c_quirks() {
        let q = BambuModel::X1C.quirks();
        assert!(!q.uses_plaintext_ftps_data_channel());
        assert!(!q.enforce_ftps_tls_1_2());
        assert!(q.has_door_sensor());
        assert_eq!(q.camera_protocol(), CameraProtocol::Rtsps);
        assert!(!q.ignores_chamber_temperature());
        assert!(!q.has_stg_cur_idle_bug());
        assert!(!q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 1);
        assert!(!q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
        assert!(!q.requires_wallclock_rtsp_timestamps());
        assert!(!q.supports_auxiliary_right_fan());
        assert!(q.supports_auxiliary_left_fan());
        assert!(!q.has_chamber_exhaust_fan());
        assert_eq!(q.z_max(), 256.0);
        assert_eq!(q.nozzle_temp_max(), 300);
        assert_eq!(q.bed_temp_max(Some(true)), 110);
        assert_eq!(q.bed_temp_max(Some(false)), 120);
        assert_eq!(q.bed_temp_max(None), 110);
        assert_eq!(q.chamber_temp_max(), 0);
        assert!(!q.supports_airduct_mode());
        assert!(!q.supports_prompt_sound());
        assert!(!q.supports_buzzer());
    }

    #[test]
    fn test_x1e_quirks() {
        let q = BambuModel::X1E.quirks();
        assert!(!q.uses_plaintext_ftps_data_channel());
        assert!(!q.enforce_ftps_tls_1_2());
        assert!(q.has_door_sensor());
        assert_eq!(q.camera_protocol(), CameraProtocol::Rtsps);
        assert!(!q.ignores_chamber_temperature());
        assert!(!q.has_stg_cur_idle_bug());
        assert!(q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 1);
        assert!(!q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
        assert_eq!(q.z_max(), 256.0);
        assert_eq!(q.nozzle_temp_max(), 320);
        assert_eq!(q.bed_temp_max(None), 110);
        assert_eq!(q.chamber_temp_max(), 60);
        assert!(q.supports_auxiliary_left_fan());
        assert!(!q.has_chamber_exhaust_fan());
        assert!(!q.supports_airduct_mode());
        assert!(!q.supports_prompt_sound());
        assert!(!q.supports_buzzer());
    }

    #[test]
    fn test_x2d_quirks() {
        let q = BambuModel::X2D.quirks();
        assert!(!q.uses_plaintext_ftps_data_channel());
        assert!(q.enforce_ftps_tls_1_2());
        assert!(q.has_door_sensor());
        assert_eq!(q.camera_protocol(), CameraProtocol::Rtsps);
        assert!(!q.ignores_chamber_temperature());
        assert!(!q.has_stg_cur_idle_bug());
        assert!(q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 2);
        assert!(q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
        assert!(q.supports_auxiliary_right_fan());
        assert!(q.supports_auxiliary_left_fan());
        assert!(q.has_chamber_exhaust_fan());
        assert!(q.auxiliary_fan_uses_percentage());
        assert_eq!(q.z_max(), 256.0);
        assert_eq!(q.nozzle_temp_max(), 300);
        assert_eq!(q.bed_temp_max(None), 120);
        assert_eq!(q.chamber_temp_max(), 65);
        assert!(q.supports_airduct_mode());
        assert!(!q.supports_prompt_sound());
        assert!(!q.supports_buzzer());
    }

    #[test]
    fn test_h2s_quirks() {
        let q = BambuModel::H2S.quirks();
        assert!(!q.uses_plaintext_ftps_data_channel());
        assert!(!q.enforce_ftps_tls_1_2());
        assert!(q.has_door_sensor());
        assert_eq!(q.camera_protocol(), CameraProtocol::Rtsps);
        assert!(!q.ignores_chamber_temperature());
        assert!(!q.has_stg_cur_idle_bug());
        assert!(q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 1);
        assert!(!q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
        assert_eq!(q.z_max(), 340.0);
        assert_eq!(q.nozzle_temp_max(), 350);
        assert_eq!(q.bed_temp_max(None), 120);
        assert_eq!(q.chamber_temp_max(), 65);
        assert!(q.supports_auxiliary_left_fan());
        assert!(q.has_chamber_exhaust_fan());
        assert!(q.supports_airduct_mode());
        assert!(!q.supports_prompt_sound());
        assert!(q.supports_buzzer());
    }

    #[test]
    fn test_h2d_quirks() {
        let q = BambuModel::H2D.quirks();
        assert!(q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 2);
        assert!(q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
        assert_eq!(q.camera_protocol(), CameraProtocol::Rtsps);
        assert_eq!(q.z_max(), 325.0);
        assert_eq!(q.nozzle_temp_max(), 350);
        assert_eq!(q.bed_temp_max(None), 120);
        assert_eq!(q.chamber_temp_max(), 65);
        assert!(q.supports_auxiliary_left_fan());
        assert!(q.has_chamber_exhaust_fan());
        assert!(q.supports_airduct_mode());
        assert!(!q.supports_prompt_sound());
        assert!(q.supports_buzzer());
    }

    #[test]
    fn test_h2d_pro_quirks() {
        let q = BambuModel::H2DPro.quirks();
        assert!(q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 2);
        assert!(q.supports_nozzle_offset_calibration());
        assert_eq!(q.camera_protocol(), CameraProtocol::Rtsps);
        assert_eq!(q.z_max(), 325.0);
        assert_eq!(q.nozzle_temp_max(), 350);
        assert_eq!(q.bed_temp_max(None), 120);
        assert_eq!(q.chamber_temp_max(), 65);
        assert!(q.supports_auxiliary_left_fan());
        assert!(q.has_chamber_exhaust_fan());
        assert!(q.supports_airduct_mode());
        assert!(!q.supports_prompt_sound());
        assert!(q.supports_buzzer());
    }

    #[test]
    fn test_h2c_quirks() {
        let q = BambuModel::H2C.quirks();
        assert!(q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 7);
        assert!(q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
        assert_eq!(q.camera_protocol(), CameraProtocol::Rtsps);
        assert_eq!(q.z_max(), 325.0);
        assert_eq!(q.nozzle_temp_max(), 350);
        assert_eq!(q.bed_temp_max(None), 120);
        assert_eq!(q.chamber_temp_max(), 65);
        assert!(q.supports_auxiliary_left_fan());
        assert!(q.has_chamber_exhaust_fan());
        assert!(q.supports_airduct_mode());
        assert!(!q.supports_prompt_sound());
        assert!(q.supports_buzzer());
    }

    #[test]
    fn test_unknown_fallback_quirks() {
        let q = BambuModel::Unknown.quirks();
        assert!(!q.has_active_chamber_heater());
        assert_eq!(q.physical_nozzle_count(), 1);
        assert_eq!(q.camera_protocol(), CameraProtocol::Rtsps);
        assert!(q.supports_auxiliary_left_fan());
        assert!(!q.has_chamber_exhaust_fan());
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
        assert!(!q.is_unsafe_homing_command("G280 Z"));
        assert!(!q.is_unsafe_homing_command(""));
        assert!(!q.is_unsafe_homing_command("G28"));
        assert!(q.is_unsafe_homing_command("G28 z"));
    }

    #[test]
    fn test_unsafe_homing_hidden_on_later_line() {
        // Regression: is_unsafe_homing_command used to inspect only the first
        // whitespace-split token of the whole string, so an unsafe G28 buried on a
        // later line of a multi-statement payload passed through unchecked.
        let q = BambuModel::P1P.quirks();
        assert!(q.is_unsafe_homing_command("M104 S200\nG28 Z"));
    }

    #[test]
    fn test_unsafe_homing_glued_axis() {
        // Regression: "G28X" (no whitespace between the command and the axis
        // letter) used to fail the exact "G28" token match and pass through unchecked.
        let q = BambuModel::P1P.quirks();
        assert!(q.is_unsafe_homing_command("G28X"));
    }

    #[test]
    fn test_a1_homing_always_safe() {
        let q = BambuModel::A1.quirks();
        assert!(!q.is_unsafe_homing_command("G28 Z"));
        assert!(!q.is_unsafe_homing_command("G28"));
    }
}
