//! # X2 Series (X2D CoreXY) Quirks
//!
//! Handles parameters unique to the X2D dual-carriage auxiliary-cooling model.
//!
//! **Door Sensor & Dual Fan Key Routing [REF-NET-DOOR] [REF-CLIM-FANS]:**
//! Unlike the X1 series which maps the door switch to `home_flag`, the X2 series
//! maps the door to `stat` bit 23. Auxiliary fans are mapped inside the nested
//! `device.airduct.parts` telemetry array matching ID 160.

/// Part index inside the airduct array targeting the secondary right-hand auxiliary fan
pub const SECONDARY_AUX_FAN_PART_ID: u32 = 160;

/// Returns true if the secondary cooling fan speed is nested inside airduct structures
pub fn uses_nested_airduct_cooling_telemetry() -> bool {
    true
}

/// Returns true if this model series requires plaintext transmissions on the
/// FTPS passive data channel (PROT C) [REF-FTPS-CONN].
pub fn uses_plaintext_ftps_data_channel() -> bool {
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
pub fn has_active_chamber_heater() -> bool {
    false
}

/// Returns the number of physical extruder carriages present on the machine carriage bus.
pub fn physical_nozzle_count() -> u8 {
    2
}

/// Returns true if the build plate moves along the Z-axis (CoreXY bed-on-Z platforms) [REF-MOTO-GCODE].
pub fn is_bed_on_z() -> bool {
    true
}
