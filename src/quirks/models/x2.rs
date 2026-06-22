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
