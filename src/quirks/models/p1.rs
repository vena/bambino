//! # P1 Series (P1P & P1S CoreXY) Quirks
//!
//! Tracks constraints and kinematic properties of early and enclosed low-power RTOS machines.
//!
//! **Post-Boot Delay Quirk [REF-NET-SECURE]:**
//! ESP32-based RTOS boards exhibit high cryptographic latency, requiring up to
//! 30 seconds after hardware boot to load the MQTTS broker certificates.
//! Handshake timeout budgets must be scaled dynamically to prevent connection drops.

/// Standard post-boot socket preparation delay, in seconds
pub const POST_BOOT_CONNECT_DELAY: u64 = 25;

/// Connection handshake timeout limits specifically configured for low-resource ESP32 platforms
pub const CRYPTO_HANDSHAKE_TIMEOUT_MS: u64 = 5000;

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
    false
}

/// Returns the physical local TCP port used by the model's camera interface [REF-NET-PORTS].
pub fn camera_stream_port() -> u16 {
    6000
}

/// Returns true if the model lacks a physical chamber temperature sensor [REF-THER-DECODE].
///
/// Core P1 models are unequipped with physical chamber sensors on the bus.
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
    true
}
