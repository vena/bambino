#![cfg_attr(not(feature = "std"), no_std)]

//! # Bambu Lab LAN Protocol Client Crate (`bambino`)
//!
//! A multi-platform, asynchronous Rust library designed to interface directly
//! with physical Bambu Lab 3D printers via local network (LAN mode) protocols.
//!
//! This library abstracts the network transport, cryptographic contexts, and
//! OS-level timer mechanisms behind a clean, asynchronous trait boundary,
//! permitting zero-modification compilation across three distinct target profiles:
//! 1. Standard Host Systems (`std` with `tokio` and `rustls`)
//! 2. Standard Embedded Controllers (e.g., ESP-IDF on ESP32 running FreeRTOS with `std`)
//! 3. Bare-Metal Controllers (`no_std` with `embassy` and `embedded-tls`)

#[cfg(all(not(feature = "std"), feature = "alloc"))]
extern crate alloc;

pub mod error;
pub mod io;
pub mod models;

pub mod discovery;
pub mod quirks;
pub mod types;
pub mod mqtt;
pub mod ftps;
pub mod ams;
pub mod camera;
pub mod diagnostics;
pub mod client;

pub use error::BambuError;
pub use models::BambuModel;
