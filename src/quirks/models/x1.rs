//! # X1 Series (X1, X1C, X1E CoreXY) Quirks
//!
//! Implements hardware safety guidelines and thermal parameters for the premium CoreXY platforms.
//!
//! **Z-Axis Crash Hazard [REF-MOTO-GCODE]:**
//! Prematurely dispatching constrained axis homing commands (such as G28 Z) causes
//! immediate upward bed travel. If the toolhead is not parked, this leads to a
//! high-force collision. Standard homing must strictly use a bare G28 command.

/// Evaluates if a given homing command carries unsafe axis constraints on Bed-on-Z platforms.
pub fn is_unsafe_homing_command(gcode: &str) -> bool {
    let clean = gcode.to_uppercase();
    // Bare G28 is safe, but G28 with axis restraints on CoreXY risks carriage collisions
    clean.contains("G28") && (clean.contains('Z') || clean.contains('X') || clean.contains('Y'))
}

/// Returns true if this model series requires plaintext transmissions on the
/// FTPS passive data channel (PROT C) [REF-FTPS-CONN].
pub fn uses_plaintext_ftps_data_channel() -> bool {
    false
}

/// Returns true if this model series must restrict its TLS version strictly
/// to TLS 1.2 to prevent session resumption failure [REF-FTPS-CONN].
pub fn enforce_ftps_tls_1_2() -> bool {
    false
}

/// Returns true if the physical machine chassis is equipped with an electronic
/// front enclosure door open sensor switch [REF-NET-DOOR].
pub fn has_door_sensor() -> bool {
    true
}

/// Returns the physical local TCP port used by the model's camera interface [REF-NET-PORTS].
pub fn camera_stream_port() -> u16 {
    322
}

/// Returns true if the model lacks a physical chamber temperature sensor [REF-THER-DECODE].
pub fn ignores_chamber_temperature() -> bool {
    false
}

/// Returns true if the model series exhibits the idle state-machine bug where
/// `stg_cur = 0` (Printing) is reported in idle phases [REF-MQTT-IDLEBUG].
pub fn has_stg_cur_idle_bug() -> bool {
    false
}

/// Returns true if the model possesses an active heated chamber control loop [REF-MOTO-GCODE].
///
/// Within the X1 family, only the enterprise-grade X1E supports active chamber heaters.
pub fn has_active_chamber_heater(is_enterprise: bool) -> bool {
    is_enterprise
}

/// Returns the number of physical extruder carriages present on the machine carriage bus.
pub fn physical_nozzle_count() -> u8 {
    1
}

/// Returns true if the build plate moves along the Z-axis (CoreXY bed-on-Z platforms) [REF-MOTO-GCODE].
pub fn is_bed_on_z() -> bool {
    true
}
