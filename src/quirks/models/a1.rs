//! # A1 Series (A1 & A1 Mini Bed-Slingers) Quirks & Coordinates
//!
//! Handles the kinematics and mechanical constraints of the bed-slinger family.
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

/// Returns the correct G-code command sequence for manual relative Z axis moves.
///
/// Ensures travel limits are enforced to prevent nozzle collision with the bed.
///
/// **Why `_feedrate` is prefixed with an underscore:** The parameter is retained
/// to maintain interface alignment across other models, but is prefixed here
/// to suppress unused-variable warnings since this model uses static G-code templates.
pub fn relative_z_move_gcode(distance: f32, _feedrate: u32, is_mini: bool) -> &'static str {
    // Bed-slingers invert Z movements compared to CoreXY models.
    // Moving positive distance raises the toolhead, which is physically safe.
    // However, negative moves drive the nozzle directly into the plate if not bound.
    let limit = if is_mini { MINI_MAX_Z } else { MAX_Z };
    if distance > limit || distance < -limit {
        return "";
    }

    // Safety command sequences wrapping relative motion [REF-MOTO-GCODE]
    "M211 S1\nM1002 push_ref_mode\nG91\nG0 Z10.00 F3000\nG90\nM1002 pop_ref_mode"
}
