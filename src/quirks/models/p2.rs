//! # P2 Series (P2S CoreXY) Quirks
//!
//! Configures transport parameters, thermal layouts, and camera corrections for the P2S platform.
//!
//! **FTPS TLS 1.3 Ticket Failure [REF-FTPS-CONN]:**
//! The embedded vsFTPd daemon fails to process asynchronous session-ticket
//! resumption on TLS 1.3 data channels, resulting in premature transfer truncation.
//! Security configurations must force TLS 1.2 on passive ports.

/// Forces TLS v1.2 restriction to avoid data channel session-close races
pub fn force_tls_v12_for_ftps() -> bool {
    true
}

/// Constant frame rate camera sync parameters to resolve RTP timestamp freezing bugs [REF-CAM-RTSPS]
pub fn requires_wallclock_rtsp_timestamps() -> bool {
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
///
/// Core P2S models support active heated chamber control loops.
pub fn has_active_chamber_heater() -> bool {
    true
}

/// Returns the number of physical extruder carriages present on the machine carriage bus.
pub fn physical_nozzle_count() -> u8 {
    1
}

/// Returns true if the build plate moves along the Z-axis (CoreXY bed-on-Z platforms) [REF-MOTO-GCODE].
pub fn is_bed_on_z() -> bool {
    true
}
