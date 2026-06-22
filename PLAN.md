# Bambu Lab LAN Protocol Client Crate (`bambu-lan`)
## Multi-Platform Rust Crate Implementation Blueprint

This document tracks the architectural progress, finalized specifications, and the scheduled implementation phases for the `bambu-lan` crate across Host (`std`/`tokio`), ESP-IDF (`std`), and Bare-Metal (`no_std`/`embassy`) compilation targets.

---

## 1. Completed Architectures & Foundation Summary

### Abstract I/O, Platform Adaptations & Unified Error Systems (Phase 1)
*   **Abstract I/O Boundaries**: Implemented asynchronous I/O trait bounds (`AsyncIo`, `AsyncUdpSocket`, `TlsConnector`, `TimerProvider`) conforming to the `embedded-io-async` (v0.7.0) specification.
*   **Platform Adapters**:
    *   **Tokio (Host `std`)**: Leveraged `tokio-rustls` and a non-validating verifier to accept self-signed printer certificates.
    *   **ESP-IDF (Embedded `std`)**: Completed standard FreeRTOS BSD socket integration layer.
    *   **Embassy (Bare-Metal `no_std`)**: Developed static, allocation-free adapters using `embassy-net` and `embedded-tls` (v0.19.0).
*   **Unified Error Engine**: Standardized operational error boundaries via the `BambuError` enum to wrap socket, parsing, and non-volatile storage write exceptions.

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

### Async MQTT State Engine & Command Builders (Phase 4)
*   **Command Payload Builders**: Implemented exact serialization wrappers (`PushAll`, `GetVersion`, `GCodeRequest`, `ProjectFileRequest`, `LedCtrlRequest`, `AmsFilamentSettingRequest` etc.) mapping trailing G-code newlines, local loopback `ftp://` paths, and rigid boolean parameters for IDEX compliance.
*   **Task ID Clamping**: Integrated a `clamp_task_id` modulo controller (`epoch % 2147483647`) to constrain 64-bit timestamps within 32-bit signed limits, preventing printer buffer lockups.
*   **Lightweight Client (`BambuMqttClient`)**: Constructed a custom transport-agnostic MQTT v3.1.1 client running on top of our `AsyncIo` trait bounds. Handles secure handshakes (`CONNECT` with clean session, credential rejection routing to `BambuError::AccessDenied`), QoS 1 subscriptions (`SUBSCRIBE` and `SUBACK` handling), and telemetry routing.
*   **QoS 1 Queue Tracking**: Enforces strict in-flight buffer limits. Refuses message publication if unacknowledged outbox frames meet or exceed the printer's 200-packet capacity limit.
*   **Keep-Alive & Zombie Detection**: Decoupled scheduling ticks via `send_ping` and a platform-independent `tick_zombie_check` routine. Detects silent write-channel failures (receiving reports but dropping commands) and throws error recovery timeouts after 10 seconds of unanswered write frames.

### Custom Implicit FTPS Engine (Phase 5)
*   **Implicit Handshake Control**: Implemented `BambuFtpsClient` over abstract `AsyncIo` boundaries. Socket connections are immediately wrapped in TLS prior to receiving or sending greetings on Port 990, avoiding explicit `AUTH TLS` triggers.
*   **Whitespace-Insensitive Directory Parsing**: In `src/ftps/parser.rs`, tokenizes variable whitespace gaps returned by UNIX `LIST` outputs to reconstruct names, directory nodes, sizes, and times.
*   **Temporal Rollover Mitigation**: Implemented strict tuple comparison (`parsed_datetime > current_datetime`) to decrement the year component by 1 during boundary rollovers (e.g., parsing a December modification date while the host clock is in January).
*   **Model-Specific Passive Protections**: Leverages `ModelQuirks` to bypass `PROT P` on A1 models (leaving data channels in plaintext `PROT C`) and enforce TLS 1.2 on P2S/X2D models to avoid data channel session-close races.
*   **Flush Integrity Controls**: Automatically closes the passive data channel abruptly upon completion and waits up to 300 seconds on the control channel for the `226` response to prevent microSD write latency exceptions. Validates partial uploads by querying file sizes via `get_file_size` on close-race errors.
*   **Integration Verified**: Validated all transactional states (greetings, login, list directory parsing, AVBL capacity fallback parsing, file size query, passive uploads, and deletion commands) using an in-memory loopback duplex-pipe testing harness (`tests/ftps_test.rs`).

### AMS Expansion Bus & Material Systems (Phase 6)
*   **Presence Evaluation**: Implemented bitwise evaluation logic to parse standard AMS slot presence from hex strings, including handling of AMS-HT (high-temperature dry chamber) units.
*   **State Sanitization**: Integrated active slot cleansing routines that clear stale configuration parameters (like `tray_type`, `tray_color`, `tag_uid`, etc.) when a physical spool is removed or transitions to an empty/absent state (state `9`/`0` or empty string).
*   **Shutdown Exception**: Added a safety boundary to ignore updates where `tray_exist_bits` evaluates to zero strictly when `power_on_flag` is false to prevent false "spool removed" alerts.
*   **Index Resolution**: Handled local-to-global indexing (`(ams_id * 4) + tray_id`), including multi-AMS and IDEX map translations (`ams_extruder_map` correlated with `active_extruder`).
*   **Slicer Mapping & Safe Overrides**: Implemented `build_ams_mapping` (flat mapping array) and `build_ams_mapping2` (structured objects). Integrated `validate_external_spool_safety` to enforce setting `use_ams = false` on single-nozzle printers when printing exclusively from the external spool, avoiding motion board exceptions.

---

## 2. Future Development Phases

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

### Phase 9: Comprehensive Mock Integration & Protocol Validation
*   **Core Objective**: Expand our asynchronous mock test rig to cover MQTT command/report cycles, active AMS slot updates, and image frame extraction protocols.
*   **Files & Modules Layout**:
    *   `tests/mock_server.rs` (To be expanded or structured)
    *   `tests/integration_tests.rs`
*   **Execution Sequence**:
    1.  **Extend Mock Server Harness**: Implement background handlers inside the mock server to simulate local MQTT broker subscriptions and Port 6000 binary JPEG emissions.
    2.  **Concurrency Validation**: Confirm that multiple clients running concurrently on different thread contexts do not leak state or block asynchronous timers.
    3.  **Fuzzing & Boundary Recovery**: Submit randomized telemetry payloads to ensure the parsing models reject malformed structures gracefully without panic.
