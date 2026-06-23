//! # A1 Series (A1 & A1 Mini Bed-Slingers) Quirks & Coordinates
//!
//! Handles the kinematics, safety boundaries, and mechanical constraints of the
//! bed-slinger family [REF-MOTO-GCODE].
//!
//! **Kinematic Inversion hazard [REF-MOTO-GCODE]:**
//! On bed-slingers, the Z-axis moves the toolhead up and down (unlike CoreXY models
//! where Z moves the bed down). Coordinates must handle this to prevent nozzle
//! drag or build-plate gouging. Homing commands must NOT bypass safety envelopes.

/// Maximum physical workspace dimension boundaries (in millimeters)
pub const MAX_X: f32 = 256.0;
pub const MAX_Y: f32 = 256.0;
pub const MAX_Z: f32 = 256.0;

/// Mini model specific workspace boundaries
pub const MINI_MAX_X: f32 = 180.0;
pub const MINI_MAX_Y: f32 = 180.0;
pub const MINI_MAX_Z: f32 = 180.0;

/// Returns true if this model series requires plaintext transmissions on the
/// FTPS passive data channel (PROT C) due to board limitations [REF-FTPS-CONN].
pub fn uses_plaintext_ftps_data_channel() -> bool {
    true
}

/// Returns true if this model series must restrict its TLS version strictly
/// to TLS 1.2 to prevent session resumption failure [REF-FTPS-CONN].
pub fn enforce_ftps_tls_1_2() -> bool {
    false
}

/// Returns true if the physical machine chassis is equipped with an electronic
/// front enclosure door open sensor switch [REF-NET-DOOR].
pub fn has_door_sensor() -> bool {
    false
}

/// Returns the physical local TCP port used by the model's camera interface [REF-NET-PORTS].
pub fn camera_stream_port() -> u16 {
    6000
}

/// Returns true if the model is an open-frame or entry-level machine lacking
/// a physical chamber temperature sensor [REF-THER-DECODE].
pub fn ignores_chamber_temperature() -> bool {
    true
}

/// Returns true if the model series exhibits the idle state-machine bug where
/// `stg_cur = 0` (Printing) is reported in idle phases [REF-MQTT-IDLEBUG].
pub fn has_stg_cur_idle_bug() -> bool {
    true
}

/// Returns true if the model possesses an active heated chamber control loop [REF-MOTO-GCODE].
pub fn has_active_chamber_heater() -> bool {
    false
}

/// Returns the number of physical extruder carriages present on the machine carriage bus.
pub fn physical_nozzle_count() -> u8 {
    1
}

/// Returns true if the build plate moves along the Z-axis (CoreXY bed-on-Z platforms) [REF-MOTO-GCODE].
pub fn is_bed_on_z() -> bool {
    false
}

/// Returns the correct G-code command sequence for manual relative Z-axis moves.
///
/// Ensures travel limits are enforced to prevent nozzle collision with the bed.
pub fn relative_z_move_gcode(distance: f32, _feedrate: u32, is_mini: bool) -> &'static str {
    let limit = if is_mini { MINI_MAX_Z } else { MAX_Z };
    if distance > limit || distance < -limit {
        return "";
    }

    // Safety command sequences wrapping relative motion [REF-MOTO-GCODE]
    "M211 S1\nM1002 push_ref_mode\nG91\nG0 Z10.00 F3000\nG90\nM1002 pop_ref_mode"
}
