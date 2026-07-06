**bambino**

# Module: bambino

## Contents

**Modules**

- [`ams`](#ams) - # AMS Filament System
- [`camera`](#camera) - # Camera & Video Streaming
- [`client`](#client) - # Printer Client
- [`diagnostics`](#diagnostics) - # Diagnostics & Calibration
- [`discovery`](#discovery) - # Printer Discovery (SSDP)
- [`error`](#error) - # Error Types
- [`ftps`](#ftps) - # FTPS File Transfer Client
- [`io`](#io) - # Transport Abstraction Layer
- [`models`](#models) - # Printer Model Identification
- [`mqtt`](#mqtt) - # MQTT Client & Command Serialization
- [`quirks`](#quirks) - # Model-Specific Quirks
- [`types`](#types) - # Types & Telemetry Schemas

---

## Module: ams

# AMS Filament System

Helpers for working with Bambu Lab's Automatic Material System.

Handles the mapping between slicer material slots and physical AMS tray positions,
including multi-AMS index resolution, spool presence detection, and stale tray data
cleanup. Supports standard AMS units, AMS-HT dry chambers, and virtual external spools.



## Module: camera

# Camera & Video Streaming

Bambu Lab printers expose camera feeds through two protocols:

1. **Binary JPEG (Port 6000)** — A1, A1 Mini, A2L, and P1 series. A lightweight binary protocol that
   streams discrete JPEG frames over TLS. This module provides a complete client
   ([`binary::BambuBinaryCameraStream`]) that handles the handshake and frame extraction.

2. **RTSPS (Port 322)** — X1, X2, H2, and P2S series. An RTSP server behind implicit TLS
   with Digest authentication. This module provides helper utilities ([`rtsps`]) for
   integrating with external media frameworks (FFmpeg, GStreamer, VLC), including URL
   generation, proxy URI rewriting, and P2S timestamp correction. It does **not** include
   an RTSP client or TLS proxy — see the [`rtsps`] module docs for the proxy architecture.



## Module: client

# Printer Client

This is the main entry point for most users. [`PrinterClient`] wraps an MQTT session
(and optionally an FTPS connection) into a single coordinated interface with methods
for thermal control, motion, print management, AMS operations, and hardware queries.

The client applies model-aware safety checks automatically:

- **Homing safety** — On CoreXY (bed-on-Z) printers, partial homing commands like
  `G28 Z` can crash the nozzle into the plate. The client enforces bare `G28` only.
- **Z-axis travel limits** — Relative Z moves are clamped to the model's mechanical
  bounds and wrapped in reference-mode push/pop (`M1002`) to prevent bed crashes.
- **Chamber heater guards** — `set_chamber_temperature()` rejects requests on models
  without an active PTC heater (open-frame machines like A1/P1).
- **Fan routing** — Fan commands are directed to the correct controller, including
  the secondary right-side auxiliary fan on models that have one (P2S, X2D, etc.).



## Module: diagnostics

# Diagnostics & Calibration

Tools for interpreting printer health alerts and managing calibration data.

The [`hms`] submodule decodes HMS (Health Management System) fault codes and print
error registers into human-readable alerts with severity levels. The [`kprofile`]
submodule manages Linear Advance (K-factor) calibration profiles — querying the
printer's stored profiles, creating new ones, and deleting them (with separate
request types for standard and IDEX platforms).



## Module: discovery

# Printer Discovery (SSDP)

Find Bambu Lab printers on the local network using SSDP (Simple Service Discovery Protocol).

[`DiscoveryEngine`] sends M-SEARCH queries on UDP port 2021 (and the alternate port 1990)
and parses incoming NOTIFY/response packets into [`SsdpDevice`] records.
The [`discover_devices()`] convenience function runs a timed broadcast-and-listen sweep
and returns all unique printers found. Works across std, ESP-IDF, and Embassy via the
[`AsyncUdpSocket`] trait.



## Module: error

# Error Types

[`BambuError`] is the single error type returned by all fallible operations in the
crate. It covers network failures, TLS handshake issues, protocol violations,
authentication rejections, timeouts, and model capability mismatches.

Under `std`, variants get `Display`/`Error` impls via `thiserror`. Under `no_std`,
a manual `Display` impl is kept in sync (verified by `test_display_consistency`).



## Module: ftps

# FTPS File Transfer Client

Implicit FTPS client for reading and writing files on the printer's SD card.

[`BambuFtpsClient`] handles the TLS control channel, passive-mode data connections,
and FTP command sequencing. It supports listing directories, uploading/downloading
files, checking free space, and basic file management (rename, delete, mkdir).
The [`parser`] submodule handles UNIX-style directory listing output.



## Module: io

# Transport Abstraction Layer

Defines the async I/O traits that let the rest of the crate work without knowing
which runtime it's running on. The key traits:

- [`AsyncIo`] — Read + Write (blanket-implemented for anything satisfying `embedded-io-async`).
- [`TlsConnector`] — Wraps a raw stream in TLS (used by tokio/rustls and embassy/embedded-tls).
- [`RawStreamFactory`] — Dials a fresh raw (pre-TLS) stream to a host:port. Used for MQTT's
  lazy connect and FTPS's per-transfer data channel.
- [`AsyncUdpSocket`] — UDP send/recv for SSDP discovery.
- [`BindableUdpSocket`] — construct-and-bind a new UDP socket by address (std/tokio, ESP-IDF only).
- [`TimerProvider`] — Async sleep and monotonic clock for platform-agnostic timeouts.

Platform implementations live in the `tokio`, `esp_idf`, and `embassy` submodules
(each gated behind its respective feature flag).
The [`TokioIo`] adapter bridges Tokio's `AsyncRead`/`AsyncWrite` to `embedded-io-async`.



## Module: models

# Printer Model Identification

Every Bambu Lab printer has a 3-character serial number prefix that identifies
its model. [`BambuModel`] enumerates all known models, and [`resolve_model()`]
maps serial prefixes (with an SSDP `DevModel` fallback) to the right variant.
The resolved model drives behavioral dispatch through the [`crate::quirks`] engine.



## Module: mqtt

# MQTT Client & Command Serialization

Low-level MQTT v3.1.1 implementation for talking to Bambu Lab printers.

[`BambuMqttClient`] handles the connection handshake, QoS 1 publish/subscribe,
keep-alive pings, and zombie detection. The [`commands`] submodule contains all
the serializable request structs (G-code dispatch, print control, AMS operations,
LED/fan/buzzer commands, etc.) that get published to the printer's command topic.

Most users should use [`crate::client::PrinterClient`] instead of this module
directly — it wraps `BambuMqttClient` with higher-level methods and safety checks.



## Module: quirks

# Model-Specific Quirks

Bambu Lab printers vary in hardware capabilities — door sensors, chamber heaters,
fan step resolution, FTPS TLS requirements, camera protocols, and more. Rather than
scattering `match model { ... }` blocks everywhere, the [`ModelQuirks`] trait captures
all model-specific behavior in one place. Call [`BambuModel::quirks()`] to get the
strategy implementation for any model.

Per-model strategy structs live in the [`models`] submodule. This module also provides
shared helpers like [`fan_step_to_percentage()`] and [`FanSpeedDebouncer`] for dealing
with the low-resolution PWM fan telemetry common across most models.



## Module: types

# Types & Telemetry Schemas

Shared data types used across the crate — most importantly [`PrinterTelemetry`],
the deserialized form of the JSON state reports the printer pushes over MQTT.
Also includes [`VersionInfo`] for firmware version queries and AMS/device
sub-structures like [`AmsTray`], [`DeviceTelemetry`], and [`ExtruderInfo`].



