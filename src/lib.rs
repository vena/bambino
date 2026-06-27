#![cfg_attr(not(feature = "std"), no_std)]

//! # bambino
//!
//! Talk to Bambu Lab 3D printers over your local network.
//!
//! `bambino` is an async Rust library that speaks the Bambu Lab LAN protocol —
//! MQTT for commands and telemetry, implicit FTPS for file management, and SSDP
//! for printer discovery. It compiles to three targets from one codebase:
//!
//! | Target | Runtime | TLS | Feature flags |
//! |--------|---------|-----|---------------|
//! | Host (desktop/server) | tokio | rustls | `default` = `["std", "tokio"]` |
//! | ESP-IDF (ESP32, FreeRTOS) | std threads | ESP-TLS | `esp-idf` |
//! | Bare-metal (embassy) | embassy | embedded-tls | `embassy` (implies `no_std` + `alloc`) |
//!
//! All network I/O goes through abstract traits in the [`io`] module, so library
//! code never touches `tokio::` or `std::net::` directly.
//!
//! # Quick start
//!
//! ```ignore
//! use bambino::client::{PrinterClient, TelemetryEvent};
//! use bambino::mqtt::BambuMqttClient;
//! use bambino::models::BambuModel;
//! use bambino::io::tokio::{TokioTlsConnector, build_unsafe_client_config};
//! use bambino::io::TokioIo;
//!
//! async fn example() -> Result<(), bambino::BambuError> {
//!     // Set up TLS (printers use self-signed certs, so we skip verification)
//!     let tls_config = build_unsafe_client_config();
//!     let connector = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(tls_config));
//!
//!     // Connect to the printer's MQTT broker on port 8883
//!     let tcp = tokio::net::TcpStream::connect("192.168.1.100:8883").await.unwrap();
//!     let tls_stream = connector.connect("192.168.1.100", 8883, TokioIo(tcp)).await?;
//!
//!     // Authenticate with the printer's serial number and LAN access code
//!     let mqtt = BambuMqttClient::connect(tls_stream, "SERIAL123456", "12345678").await?;
//!
//!     // Wrap in a high-level client and request a full state dump
//!     let mut printer = PrinterClient::new(mqtt, "SERIAL123456", BambuModel::P1S);
//!     printer.request_pushall().await?;
//!
//!     // Poll for telemetry
//!     loop {
//!         match printer.poll_telemetry().await? {
//!             TelemetryEvent::Report(report, _raw) => {
//!                 let (bed_actual, bed_target) = report.bed_temperatures();
//!                 println!("Bed: {}°C / {}°C target", bed_actual, bed_target);
//!             }
//!             TelemetryEvent::Unknown(_) => {}
//!         }
//!     }
//! }
//! ```
//!
//! # Feature flags
//!
//! | Flag | What it enables |
//! |------|-----------------|
//! | `std` | Standard library, `thiserror`, `serde`/`serde_json` std features |
//! | `tokio` | Tokio runtime, rustls TLS, CLI binary (implies `std`) |
//! | `esp-idf` | ESP-IDF system services for embedded Linux-like targets (implies `std`) |
//! | `embassy` | Embassy async runtime, embedded-tls, embassy-net (implies `no_std` + `alloc`) |
//! | `alloc` | Heap allocation for `no_std` environments (String, Vec, format!) |
//!
//! # Module guide
//!
//! - [`client`] — The main entry point. [`client::PrinterClient`] wraps MQTT + FTPS into
//!   one coordinated interface with methods for thermal control, motion, print jobs, etc.
//! - [`mqtt`] — Low-level MQTT v3.1.1 client and command serialization.
//! - [`ftps`] — Implicit FTPS client for SD card file operations.
//! - [`discovery`] — SSDP-based printer discovery on the local network.
//! - [`types`] — Telemetry schemas, version info, and shared data types.
//! - [`models`] — Printer model identification from serial numbers.
//! - [`quirks`] — Per-model behavioral differences (fan mapping, door sensors, temp limits, etc.).
//! - [`io`] — Transport abstraction traits ([`io::AsyncIo`], [`io::TlsConnector`], etc.).
//! - [`ams`] — AMS filament system helpers (slot mapping, presence detection).
//! - [`camera`] — Camera streaming protocols (binary JPEG on port 6000, RTSPS on port 322).
//! - [`diagnostics`] — HMS alert decoding and K-profile (Linear Advance) management.
//! - [`error`] — The unified [`BambuError`] type.

#[cfg(all(not(feature = "std"), feature = "alloc"))]
extern crate alloc;

pub mod error;
pub mod io;
pub mod models;

pub mod ams;
pub mod camera;
pub mod client;
pub mod diagnostics;
pub mod discovery;
pub mod ftps;
pub mod mqtt;
pub mod quirks;
pub mod types;

#[doc(inline)]
pub use error::BambuError;
#[doc(inline)]
pub use models::BambuModel;
