//! # Diagnostics & Calibration Systems Module
//!
//! Consolidates hardware diagnostic utilities and linear advance (K-Profile) database
//! management APIs under a unified module namespace [REF-DIAG-HMS] [REF-DIAG-KPROF].
//!
//! Provides callers with direct access to:
//! 1. Telemetry parsing models resolving active `hms` fault matrices and `print_error` registers.
//! 2. Database command wrappers handling Linear Advance queries, creation commits, and
//!    polymorphic deletions.

pub mod hms;
pub mod kprofile;

pub use hms::{
    DecodedHmsAlert, DecodedPrintError, HmsSeverity, decode_hms_alert, decode_print_error,
};
pub use kprofile::{
    ExtrusionCaliGetRequest, ExtrusionCaliSelRequest, ExtrusionCaliSetRequest, IdexCaliDelEntry,
    IdexCaliDelRequest, KProfileEntry, StandardCaliDelEntry, StandardCaliDelRequest,
    validate_setting_id,
};
