//! # Model-Specific Kinematic and Operational Configuration Submodules
//!
//! Isolates physical constraints (such as safe homing rules, bed coordinate limits,
//! and relative axis orientation guidelines) into individual, model-specific modules,
//! one per `BambuModel` variant.

pub mod a1;
pub mod a2;
pub mod h2;
pub mod p1;
pub mod p2;
pub mod x1;
pub mod x2;
