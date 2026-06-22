#![cfg_attr(not(feature = "std"), no_std)]

//! # AMS Expansion Bus & Material Systems Module
//!
//! Exposes presence bitmask calculation helpers, slot-sanitization routines,
//! multi-AMS index resolution, and filament change/drying configuration builders.
//!
//! This module coordinates communication between sliced project materials and the
//! physical expansion slots of standard AMS units, AMS-HT dry-chambers, and
//! virtual external spools.

pub mod mapping;
pub mod parser;

pub use mapping::{build_ams_mapping, build_ams_mapping2, validate_external_spool_safety};
pub use parser::{clean_stale_tray_data, evaluate_spool_presence, resolve_global_tray_id};
