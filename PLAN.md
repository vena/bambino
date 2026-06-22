# Bambu Lab LAN Protocol Client Crate (`bambu-lan`)
## Multi-Platform Rust Crate Implementation Blueprint

This document tracks the architectural progress, finalized specifications, and the scheduled implementation phases for the `bambu-lan` crate across Host (`std`/`tokio`), ESP-IDF (`std`), and Bare-Metal (`no_std`/`embassy`) compilation targets.

---

## 1. Completed Architectures & Foundation Summary

### Abstract I/O, Platform Adaptations & Unified Error Systems (Phase 1)
*   **Abstract Boundaries**: Implemented core asynchronous I/O traits (`AsyncIo`, `AsyncUdpSocket`, `TlsConnector`, `TimerProvider`) conforming strictly to the `embedded-io-async` (v0.7.0) specification.
*   **Platform Adapters**:
    *   **Tokio (Host `std`)**: Implemented safe, platform-native network wrappers using `tokio-rustls` and a non-checking trust verifier to process self-signed certificates.
    *   **ESP-IDF (Embedded `std`)**: Completed standard FreeRTOS BSD socket integration layer.
    *   **Embassy (Bare-Metal `no_std`)**: Developed static, allocation-free adapters utilizing `embassy-net` and `embedded-tls` (v0.19.0) using static unsafe-cell buffer pools.
*   **Unified Error Engine**: Standardized operational error boundaries via `BambuError` to map socket, parsing, and storage faults.

### SSDP Network Discovery Engine (Phase 2)
*   **Zero-Copy Parse**: Integrated a zero-allocation HTTP-style response and request parser using `httparse` to extract locations, user-defined names, and model properties from UDP datagrams.
*   **Active & Passive Scanning**: Structured `DiscoveryEngine` and the standalone `discover_devices` helper to coordinate active multicast sweeps and handle network timeout recovery loops.

### Model-Specific Telemetry Polymorphism & Bitmasks (Phase 3)
*   **Structured Telemetry Models**: Implemented complete Serde-deserializable structs representing printer status broadcasts (`TelemetryReport`, `PrintTelemetry`, `DeviceTelemetry`, and `AmsUnit`).
*   **Permissive Parsing**: Engineered custom deserializers to process varying data types for `sdcard` detection (`bool`, `integer`, or string constants).
*   **Model Quirks Interface**: Defined the `ModelQuirks` trait directly on `BambuModel` to perform static-dispatch evaluations for:
    *   Door-sensor routing variations (X1 `home_flag` bit 23 vs. other models' `stat` hex string bit 23).
    *   Chamber temperature and state-stage reporting exclusions for open-frame architectures.
    *   FTPS TLS 1.2 strict constraints and plaintext passive data-channel overrides.

---

## 2. Future Development Phases

### Phase 4: Async MQTT State Engine & Command Builders
*   **Core Objective**: Implement a thread-safe, non-singleton MQTT client layer supporting concurrent execution, automate status queries, enforce transaction bounds, and implement keep-alive zombie checks.
*   **Files & Modules Layout**:
    *   `src/mqtt/mod.rs`
    *   `src/mqtt/client.rs`
    *   `src/mqtt/commands.rs`
*   **Execution Sequence**:
    1.  **Configure Multi-Printer Session State**: Implement `PrinterSession` without using global static references, tracking custom atomic task ID counters.
    2.  **Gcode Line Wrap Builder**: In `src/mqtt/commands.rs`, construct serialization routines for `gcode_line` wrappers.
    3.  **Prevent 32-Bit Signed Integer Overflows**: Enforce safe value clamping (modulo 2,147,483,647) for generated task/subtask identifiers to prevent printer parsing lockups.
    4.  **Implement Keep-Alive Verification**: Monitor incoming telemetry timestamps. If no new messages are received on the `report` topic within 10 seconds of issuing a write command, mark the connection state as a zombie and initiate a reconnection sequence `[REF-MQTT-ZOMBIE]`.

### Phase 5: Custom Implicit FTPS Engine
*   **Core Objective**: Implement a custom asynchronous implicit FTPS client to handle file listings, directory traversals, and file uploads directly over the abstraction layer.
*   **Files & Modules Layout**:
    *   `src/ftps/mod.rs`
    *   `src/ftps/client.rs`
    *   `src/ftps/parser.rs`
*   **Execution Sequence**:
    1.  **Implicit TLS Initialization**: Create a command socket connection. Prior to executing any standard protocol handshakes, immediately wrap the raw stream in the `TlsConnector` to establish a secure channel on Port 990 `[REF-FTPS-CONN]`.
    2.  **Coordinate TLS Session Resumption**: Extract the TLS session ticket from the control socket and supply it during the passive data channel handshake.
    3.  **Whitespace-Insensitive UNIX Listing Parser**: In `src/ftps/parser.rs`, tokenize multi-spaced responses returned by `LIST` to extract size, name, and timestamp metadata.
    4.  **Implement Model-Specific Transport Rules**: Force TLS 1.2 on P2S/X2D models to prevent session truncation, and disable `PROT P` on A1 models to permit plaintext passive data channels.
    5.  **MicroSD Flush and Verification Blocks**: After executing a passive file transfer, immediately close the data connection and block-wait on the control socket for the `226 Transfer complete` response before dispatching downstream print commands `[REF-FTPS-FLUSH]`.

### Phase 6: AMS Expansion Bus & Material Systems
*   **Core Objective**: Implement presence bitmask calculation helpers, dynamic slot-cleansing routines on state transitions, multi-AMS index resolution, and filament change/drying configuration builders.
*   **Files & Modules Layout**:
    *   `src/ams/mod.rs`
    *   `src/ams/parser.rs`
    *   `src/ams/mapping.rs`
*   **Execution Sequence**:
    1.  **Presence Bitmask Parsing**: Implement bitwise evaluation formulas for standard and high-temperature material units.
    2.  **Printer Shutdown Detection Exception**: Ignore updates where the existence bitmask evaluates to zero strictly when `power_on_flag` is false.
    3.  **Active Slot Cleansing Logic**: When a slot transitions to state `9` (Empty) or the `tray_type` key is omitted during incremental status updates, explicitly clear or nullify all stale material telemetry parameters to prevent old configurations from persisting `[REF-AMS-DECODE]`.
    4.  **Slicer-to-Printer Material Array Builders**: In `src/ams/mapping.rs`, construct `ams_mapping` (flat array of integer channel indices, mapping unused or external slots to the `-1` marker) and `ams_mapping2` (structured unit-and-slot mapping objects).
    5.  **External Spool Safety Rules**: Implement a validator that enforces setting `use_ams = false` on single-nozzle printers if all mapped filaments reside on the virtual external spool `[REF-AMS-USEAMS]`.

### Phase 7: Video & Image Stream Capture
*   **Core Objective**: Implement inline processing of secure camera streams directly within the library, decoding both the Port 6000 binary JPEG frame headers and Port 322 RTSPS Digest authentication challenges without spinning up external proxy processes.
*   **Files & Modules Layout**:
    *   `src/camera/mod.rs`
    *   `src/camera/binary.rs`
    *   `src/camera/rtsps.rs`
*   **Execution Sequence**:
    1.  **Binary JPEG Handshake Encoder (Port 6000)**: In `src/camera/binary.rs`, implement the 80-byte authentication packet serialization using little-endian byte ordering `[REF-CAM-BINARY]`.
    2.  **16-Byte Header Stream Reader**: Parse incoming byte buffers from the Port 6000 socket, extracting the `uint32` frame size from the first 4 bytes and returning the parsed JPEG data after verifying the standard `\xff\xd8` start and `\xff\xd9` end markers.
    3.  **RTSPS Digest Authentication Engine (Port 322)**: Implement RTSP client handshakes, parsing incoming `WWW-Authenticate` headers and generating the appropriate Digest response hashes.
    4.  **Static Timestamp Recovery Parser**: If the active model capability flags indicate a P2S running target firmware versions, ignore stream-embedded RTP timestamps and calculate frame paces using relative host wall-clock arrival offsets.

### Phase 8: Diagnostic Unpacking (HMS) & K-Profile Database Calibration
*   **Core Objective**: Implement mathematical unpacking algorithms for physical HMS error codes and design database interfaces for pressure-advance (K-profile) calibrations.
*   **Files & Modules Layout**:
    *   `src/diagnostics/mod.rs`
    *   `src/diagnostics/hms.rs`
    *   `src/diagnostics/kprofile.rs`
*   **Execution Sequence**:
    1.  **Packed HMS Key Decoder**: Unpack the 32-bit `attr` and `code` integers into the standard 16-character wiki troubleshooting key (`MMMM_MMMM_CCCC_CCCC`) and the abbreviated local 8-character short-code (`MMMM_CCCC`) `[REF-DIAG-HMS]`.
    2.  **Severity and Module Identification Extraction**: Unpack severity metrics and source hardware module identifiers using bitwise operations.
    3.  **Real Fault Filters**: Flag alerts as genuine hardware faults only if the low 16-bit value of the code is equal to or greater than `0x4000`.
    4.  **K-Profile Calibration Payload Builders**: Implement serialization schemas for managing pressure-advance settings (`extrusion_cali_get`, `extrusion_cali_set`, `extrusion_cali_del`).
    5.  **Multi-Nozzle IDEX Deletions**: In `src/diagnostics/kprofile.rs`, implement deletion builders for both single-nozzle and dual-nozzle IDEX platforms `[REF-DIAG-KPROF]`.

### Phase 9: Comprehensive Mock Integration, Protocol Verification & Validation
*   **Core Objective**: Construct a complete, multi-protocol mock printer test rig to validate telemetry parsing, implicit FTPS transfers, secure camera streaming, and dynamic quirks behaviors locally.
*   **Files & Modules Layout**:
    *   `tests/mock_server.rs`
    *   `tests/integration_tests.rs`
*   **Execution Sequence**:
    1.  **Construct Multi-Protocol Mock Server**: Build a local test server running asynchronously inside a background process that binds to loopback ports (Port 8883 MQTTS, Port 990 FTPS, Port 6000 Binary Camera).
    2.  **Concurrency Isolation Verification**: Spin up multiple clients targeted at separate mock server instances, validating that there is no shared state or thread safety issues.
    3.  **Validate Telemetry Filtering**: Publish real telemetry frames containing the `stg_cur = 0` idle bug and verify the client correctly filters the state.
    4.  **Property-Based Telemetry Fuzzing**: Use random or boundary values to fuzz telemetry parsers, ensuring that they recover gracefully from unexpected or malformed inputs without panicking.
