//! # H2 Series (H2S, H2D, H2C IDEX and Tool Changer platforms) Quirks
//!
//! Manages the properties of the IDEX and tool-changer platforms.
//!
//! **H2S/H2D Serial Prefix Collision [REF-NET-PORTS]:**
//! Single-nozzle H2S and dual-nozzle H2D share the same prefix ("094"). Secondary
//! evaluation is handled here to parse the `O1D` (H2D) or `O1S` (H2S) model identifiers
//! to route dual-carriage vs. single-carriage G-code sequences correctly.

/// Returns the number of physical extruder carriages present on the machine.
pub fn physical_nozzle_count(dev_model: &str) -> u8 {
    if dev_model.contains("O1D") || dev_model.contains("O1E") || dev_model.contains("O2D") {
        2 // Dual nozzle IDEX architecture (H2D / H2D Pro)
    } else if dev_model.contains("O1C") {
        6 // Tool Changer rack positions (H2C Vortek systems)
    } else {
        1 // Single nozzle architecture (H2S)
    }
}

/// Verification check for IDEX nozzle offset calibration capability.
pub fn supports_nozzle_offset_calibration(dev_model: &str) -> bool {
    // Only dual carriage systems require digital alignment scans
    physical_nozzle_count(dev_model) > 1
}
