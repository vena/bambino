#![cfg_attr(not(feature = "std"), no_std)]
#![allow(async_fn_in_trait)]

//! # Bambu Lab LAN Protocol Client Crate (`bambu-lan`)
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

// Exposes Phase 2 Simple Service Discovery Protocol (SSDP) engine to the crate's module tree.
pub mod discovery;

// Exposes Phase 3 State Telemetry structures and model-specific quirks.
pub mod quirks;
pub mod types;

// Exposes Phase 4 MQTT state client and control command structures.
pub mod mqtt;

// Exposes Phase 5 Custom Implicit FTPS storage manipulation client.
pub mod ftps;

// Exposes Phase 6 AMS expansion bus, material mapping and filament control structures.
pub mod ams;

pub use error::BambuError;
