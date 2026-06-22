# Bambu Lab LAN Protocol Client Crate (`bambu-lan`)
## Multi-Platform Rust Crate Implementation Blueprint

This document tracks the architectural progress, finalized specifications, and the scheduled implementation phases for the `bambu-lan` crate across Host (`std`/`tokio`), ESP-IDF (`std`), and Bare-Metal (`no_std`/`embassy`) compilation targets.

---

## 1. Completed Architectures & Foundation Summary

### Abstract I/O, Platform Adaptations & Unified Error Systems (Phase 1)
*   **Abstract I/O Boundaries**: Developed asynchronous I/O trait bounds (`AsyncIo`, `AsyncUdpSocket`, `TlsConnector`, `TimerProvider`) complying with the `embedded-io-async` (v0.7.0) specification.
*   **Platform Adapters**:
    *   **Tokio (Host `std`)**: Implemented `tokio-rustls` wrapper utilizing a custom certificate verifier to accept self-signed printer certificates.
    *   **ESP-IDF (Embedded `std`)**: Integrated standard FreeRTOS BSD socket layers for Espressif environments.
    *   **Embassy (Bare-Metal `no_std`)**: Structured static, allocation-free adapters utilizing `embassy-net` and `embedded-tls` (v0.19.0).
*   **Unified Error Engine**: Consolidated error types into the `BambuError` enum to wrap socket, parsing, serialization, and disk-write failures.

### SSDP Network Discovery Engine (Phase 2)
*   **Zero-Copy Parse**: Integrated zero-allocation HTTP-style response and request parser utilizing `httparse` to extract printer IP, port, serial, and advertised model tags from UDP datagrams.
*   **Scanning Routines**: Structured the `DiscoveryEngine` and the standalone `discover_devices` helper to coordinate active multicast search queries (Port 2021) and passive NOTIFY monitoring.

### Model-Specific Telemetry Polymorphism & Bitmasks (Phase 3)
*   **Structured Telemetry Models**: Implemented Serde-deserializable structs representing printer status broadcasts (`TelemetryReport`, `PrintTelemetry`, `DeviceTelemetry`, and `AmsUnit`).
*   **Permissive Parsing**: Constructed custom deserializers to handle varying `sdcard` representation formats (booleans, integers, or status strings).
*   **Model Quirks Interface**: Defined the `ModelQuirks` trait directly on `BambuModel` to perform static-dispatch evaluations of physical door sensors, chamber temperature reporting exclusions, and TLS capability profiles.

### Async MQTT State Engine & Command Builders (Phase 4)
*   **Command Payload Builders**: Designed serialization structures (`PushAll`, `GetVersion`, `GCodeRequest`, `ProjectFileRequest`, `LedCtrlRequest` etc.) mapping trailing G-code newlines, local loopback `ftp://` paths, and rigid boolean variables.
*   **Task ID Clamping**: Integrated a `clamp_task_id` modulo controller (`epoch % 2147483647`) to constrain 64-bit timestamps within signed 32-bit integer boundaries.
*   **Lightweight Client (`BambuMqttClient`)**: Structured a custom transport-agnostic MQTT v3.1.1 client running on top of our `AsyncIo` trait bounds. Handles secure handshakes, QoS 1 in-flight packet bounds, keep-alive frames, and 10-second write-channel zombie detection.

### Custom Implicit FTPS Engine (Phase 5)
*   **Implicit Handshake Control**: Built `BambuFtpsClient` over abstract `AsyncIo` boundaries, immediately wrapping sessions in TLS on Port 990 to bypass explicit `AUTH TLS` triggers.
*   **Whitespace-Insensitive Directory Parsing**: Tokenizes variable whitespace gaps returned by UNIX `LIST` outputs to reconstruct file names, sizes, and times. Handles temporal rollover boundaries by comparing parsed datetimes to the system context.
*   **Model-Specific Protections**: Bypasses data channel TLS `PROT P` on A1 platforms and enforces TLS 1.2 on P2S/X2D platforms to avoid data channel session-close races.
*   **Flush Integrity Controls**: Automatically drops passive data sockets abruptly on completion to prevent vsFTPd session hangs, and monitors control channel responses to validate write buffer flushes.

### AMS Expansion Bus & Material Systems (Phase 6)
*   **Spool Presence**: Integrated bitwise evaluation logic to parse standard AMS slot presence from hex strings, including handling of AMS-HT (high-temperature dry chamber) units.
*   **State Sanitization**: Implemented slot cleansing routines that clear stale configuration parameters when a spool is extracted.
*   **Index Resolution**: Handled local-to-global indexing (`(ams_id * 4) + tray_id`), including multi-AMS and IDEX map translations (`ams_extruder_map` correlated with `active_extruder`).
*   **Slicer Mapping & Safe Overrides**: Implemented flat and structured mapping arrays (`ams_mapping` and `ams_mapping2`). Enforces `use_ams = false` on single-nozzle printers when printing exclusively from the external spool to prevent motion board exceptions.

### Video & Image Stream Capture (Phase 7)
*   **Chamber Image Protocol Parser (Port 6000)**: Implemented the 80-byte little-endian handshake packet builder and the 16-byte metadata stream parser. Performs safety checks on payload size boundaries (clamped to 5MB) and validates JPEG magic markers (`FF D8` and `FF D9`).
*   **RTSPS Integration (Port 322)**: Designed connection URL formatters and helper utilities to rewrite proxy request-lines from plain RTSP to secure RTSPS to preserve Digest Authentication hash calculations.
*   **RTP Timestamp Reconstruction**: Implemented `RtpTimestampCorrector` to resolve P2S static timestamp bugs by synthesizing monotonically advancing values mapped to the standard 90,000 Hz video stream clock.

### Diagnostic Unpacking (HMS) & K-Profile Database Calibration (Phase 8)
*   **Packed HMS Key Decoder**: Developed bitwise mathematical decoders to unpack 32-bit `attr` and `code` integers into support Wiki-slug keys (`MMMM_MMMM_CCCC_CCCC`) and local panel short-codes (`MMMM_CCCC`) [REF-DIAG-HMS].
*   **Module and Severity Identification**: Unpacked 4-character hardware module boundaries and severity ratings directly from the `attr` telemetry word.
*   **Fault Isolation Filters**: Structured safety rules that filter transient status progress updates (low-word indices < `0x4000`) and cancellation echoes (e.g., `0300_400C` and `0500_400E`) to isolate genuine hardware faults.
*   **K-Profile Calibration Payload Builders**: Designed payload serialization request wrappers for querying (`extrusion_cali_get`) and writing (`extrusion_cali_set`) Linear Advance calibration settings [REF-DIAG-KPROF].
*   **Validation & Deletion Schemas**: Enforced strict 19-character numeric `setting_id` bounds. Segmented database deletion tasks into standard single-nozzle deletions (Schema A, keyed on `setting_id`) and IDEX deletions (Schema B, keyed on coordinate parameters).

---

## 2. Future Development Phases

### Phase 9: Comprehensive Mock Integration & Protocol Validation
*   **Core Objective**: Expand our asynchronous mock test rig to cover MQTT command/report cycles, active AMS slot updates, and image frame extraction protocols.
*   **Files & Modules Layout**:
    *   `tests/mock_server.rs`
    *   `tests/integration_tests.rs`
*   **Execution Sequence**:
    1.  **Extend Mock Server Harness**: Implement background handlers inside the mock server to simulate local MQTT broker subscriptions and Port 6000 binary JPEG emissions.
    2.  **Concurrency Validation**: Confirm that multiple clients running concurrently on different thread contexts do not leak state or block asynchronous timers.
    3.  **Fuzzing & Boundary Recovery**: Submit randomized telemetry payloads to ensure the parsing models reject malformed structures gracefully without panic.
