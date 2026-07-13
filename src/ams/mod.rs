//! # AMS Filament System
//!
//! Helpers for working with Bambu Lab's Automatic Material System.
//!
//! Handles the mapping between slicer material slots and physical AMS tray positions,
//! including multi-AMS index resolution, spool presence detection, and stale tray data
//! cleanup. Supports standard AMS units, AMS-HT dry chambers, and virtual external spools.

pub mod mapping;
pub mod parser;

pub use mapping::{
    AmsMapping2Entry, AmsPoolComposition, MaterialSource, build_ams_mapping, build_ams_mapping2,
    validate_ams_pool_composition, validate_external_spool_safety,
    validate_external_spool_safety_flat,
};
pub use parser::{
    clean_stale_tray_data, evaluate_spool_presence, resolve_global_tray_id,
    resolve_printing_global_id,
};
