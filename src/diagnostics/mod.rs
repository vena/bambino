//! # Diagnostics & Calibration
//!
//! Tools for interpreting printer health alerts and managing calibration data.
//!
//! The [`hms`] submodule decodes HMS (Health Management System) fault codes and print
//! error registers into human-readable alerts with severity levels. The [`kprofile`]
//! submodule manages Linear Advance (K-factor) calibration profiles — querying the
//! printer's stored profiles, creating new ones, and deleting them (with separate
//! request types for standard and IDEX platforms).

pub mod hms;
pub mod kprofile;

pub use hms::{
    DecodedHmsAlert, DecodedPrintError, HmsSeverity, decode_hms_alert, decode_print_error,
};
pub use kprofile::{
    ExtrusionCaliGetRequest, ExtrusionCaliGetResponse, ExtrusionCaliSelRequest,
    ExtrusionCaliSetRequest, IdexCaliDelEntry, IdexCaliDelRequest, KProfileEntry,
    StandardCaliDelEntry, StandardCaliDelRequest, is_setting_id_valid,
};
