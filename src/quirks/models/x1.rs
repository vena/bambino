//! # X1 Series (X1, X1C, X1E CoreXY) Quirks
//!
//! Implements hardware safety guidelines for the premium CoreXY platforms.
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
