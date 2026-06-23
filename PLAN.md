# Bambu Lab LAN Protocol Client Crate (`bambu-lan`)
## Multi-Platform Rust Crate Implementation Blueprint

This document tracks the structural status and architectural design of the `bambu-lan` crate across Host (`std`/`tokio`), ESP-IDF (`std`), and Bare-Metal (`no_std`/`embassy`) compilation targets.

---

## 1. Completed Architectures & Foundation Summary

The `bambu-lan` library provides a platform-agnostic abstraction of local network (LAN mode) protocols. For the next session, understand the following completed architectural foundations:

* **Platform-Agnostic I/O Boundaries**: Core operations are decoupled from standard system dependencies. The transport layer relies on abstract traits (`AsyncIo`, `AsyncUdpSocket`, `TlsConnector`, and `TimerProvider`), enabling identical client code to compile across standard host operating systems (Tokio), RTOS microcontrollers (ESP-IDF), and bare-metal environments (Embassy).
* **Polymorphic Quirks Engine**: Printer-specific variations, mechanical safety interlocks (such as Z-axis homing crash protection on CoreXY machines), and unsupported commands are managed polymorphically. Model-specific constraints are encapsulated in decoupled strategy structs (e.g., `P1Quirks`, `X1Quirks`) implementing the `ModelQuirks` trait, resolved via the static `model.quirks()` strategy dispatcher.
* **Developer Verification CLI (`bambu-cli`)**: A lightweight, dependency-free binary module directory (`src/bin/bambu-cli/`) providing subcommands representing each library transport protocol:
  * `discover`: Broadcast/multicast dual SSDP network scanner.
  * `monitor`: Real-time telemetry, composite thermal unpacking, and live HMS decoder.
  * `control`: Safer coordinate movement, manual extrusion feed, fan speed rounding, and lighting.
  * `files`: Passive implicit FTPS file-system listing, space allocation check, chunked upload, and deletion.
