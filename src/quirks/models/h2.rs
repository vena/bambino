//! # H2 Series (H2S, H2D, H2C IDEX and Tool Changer platforms) Quirks
//!
//! Manages the properties and kinematic characteristics of the IDEX and
//! tool-changer platforms [REF-MOTO-GCODE].
//!
//! **H2S/H2D Serial Prefix Collision [REF-NET-PORTS]:**
//! Single-nozzle H2S and dual-nozzle H2D share the same prefix ("094"). Secondary
//! evaluation is handled here to parse the `O1D` (H2D) or `O1S` (H2S) model identifiers
//! to route dual-carriage vs. single-carriage G-code sequences correctly.

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
/// Under the H2 family, only dual-extruder platforms (H2D, H2D Pro) are equipped with
/// active chamber heaters. Single-carriage (H2S) and tool changers (H2C) utilize passive monitoring.
pub fn has_active_chamber_heater(is_dual_nozzle: bool) -> bool {
    is_dual_nozzle
}

/// Returns the number of physical extruder carriages present on the machine carriage bus.
pub fn physical_nozzle_count(is_tool_changer: bool, is_dual_nozzle: bool) -> u8 {
    if is_tool_changer {
        6 // Tool Changer rack positions (H2C Vortek systems)
    } else if is_dual_nozzle {
        2 // Dual nozzle IDEX architecture (H2D / H2D Pro)
    } else {
        1 // Single nozzle architecture (H2S)
    }
}

/// Returns true if the build plate moves along the Z-axis (CoreXY bed-on-Z platforms) [REF-MOTO-GCODE].
pub fn is_bed_on_z() -> bool {
    true
}

/// Verification check for IDEX nozzle offset calibration capability.
pub fn supports_nozzle_offset_calibration(is_dual_nozzle: bool, is_tool_changer: bool) -> bool {
    // Only dual carriage and toolchanger systems require digital alignment scans
    physical_nozzle_count(is_tool_changer, is_dual_nozzle) > 1
}
