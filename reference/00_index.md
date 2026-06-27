# Local Network (LAN Mode) Bambu Lab 3D Printer Protocols & Communication Specification
## Master Architectural Map

This document establishes the master architectural blueprint and domain directory structure for the reverse-engineered, language-agnostic, local network (LAN Mode) communication protocols of physical Bambu Lab 3D printers and native accessories. This specification is derived from first-principles analysis of direct over-the-wire communication libraries, socket streams, and raw telemetry data.

All subsequently generated chapters strictly adhere to the structural boundaries, dynamic reference tags, and terminology constraints defined in this map.

---

### Rationale & Boundary Specifications

To maintain spec-to-wire alignment across generations, any parsing library, integration wrapper, or automated controller must strictly implement the structural and logical properties detailed across the following chapters.

*   **Network Transport Layer & Device Discovery** (`01_network_discovery.md`): Defines the SSDP service structures, multicast socket configurations, direct TCP/UDP port mapping boundaries, and local TLS negotiation requirements.
*   **Secure FTP File & Storage System** (`02_ftps.md`): Establishes the implicit FTPS session handshake, command/data channel encryption management, local file/directory path boundaries, and transactional packet verification steps.
*   **Local Broker Control & State Telemetry** (`03_mqtt_telemetry.md`): Codifies the authenticated local broker topologies, direct command-to-report structures, and transactional print job lifecycle tracking.
*   **Toolhead, Thermal, Motion & Climate Systems** (`04_toolhead_thermal_motion.md`): Specifies the actual vs. target packed temperature formats, fan PWM scaling, and kinematic relative/absolute coordinates.
*   **Physical Material Expansion (AMS, AMS-HT & Spools)** (`05_materials_ams.md`): Details the expansion bus bitmask maps, moisture evaluation profiles, RFID serialization, and active dry-chamber configurations.
*   **Video Streaming & Chamber Image Handshakes** (`06_cameras.md`): Documents the dual video/imaging interfaces, direct RTSPS handshakes, and proprietary binary chamber image streams.
*   **Diagnostic Mapping & Calibration Profiles** (`07_diagnostics_hms.md`): Formulates the raw HMS code translation algorithms, error level parsing, and linear pressure-advance (K-profile) database storage.

---

## Technical Terminology Registry

*   **Local Broker**: The secure, self-signed TLS MQTT server hosted directly on the physical printer's network board (Port 8883).
*   **Physical AMS Unit**: Modular 4-slot filament expansion systems (Gen 1 / Gen 2 / AMS 2 Pro / N3F) connected to the printer's hardware expansion bus.
*   **AMS-HT**: The single-slot high-temperature dry-chamber unit (N3S) connected via the expansion bus.
*   **External Spool**: The physical spool holder located outside of the AMS environment, communicating via non-bus virtual channels (`vt_tray` / `vir_slot`).
*   **Active Toolhead**: The physical carriage (or carriages on IDEX architectures) carrying the active extruder, hotend, and primary sensor bus.
*   **Unified HMS Key**: The normalized, 8-character over-the-wire diagnostic code formatted as `MMMM_CCCC` (Module_Code) or the full 16-character Wiki format `MMMM_MMMM_CCCC_CCCC`.

---

## Chapters & Reference Index

### Chapter 1: Network Transport Layer & Device Discovery
*   **[REF-NET-DISC] Section 1.1: Simple Service Discovery Protocol (SSDP)**
    *   Multicast configuration (IP: `239.255.255.250`, Port: `2021`)
    *   M-SEARCH and NOTIFY packets, location ports, UPnP deviations, and virtual printer exclusion suffix
*   **[REF-NET-PORTS] Section 1.2: Direct Local Port Mapping Matrix**
    *   Port 8883 (MQTT/TLS), Port 990 (Implicit FTPS), Port 322 (RTSPS), Port 6000 (Chamber Image)
*   **[REF-NET-SECURE] Section 1.3: Cryptographic Context & Handshakes**
    *   TLS 1.2 / TLS 1.3 contexts, SNI hostname overrides, and X.509 constraints
*   **Section 1.4: SSDP Hardware Model Code Mapping**
    *   Hardware capabilities and camera interface allocations
*   **Section 1.5: Printer Serial Prefix Mapping Table**
    *   Model identification via serial number prefix (each H2-series model has a distinct prefix)
*   **Section 1.6: Mechanical & Firmware Discovery Quirks**
    *   SSDP Unicast Query Availability, Post-Boot Local Broker Handshake Delay, and Case-Sensitive Serial Routing

### Chapter 2: Secure FTP File & Storage System
*   **[REF-FTPS-CONN] Section 2.1: Network Boundary & Interface Parameters**
    *   Implicit FTPS connection initiation, passive port negotiation, and model-specific TLS & plaintext encryption constraints (P2S/X2D TLS 1.3 limits and A1 plaintext data-channel bypass)
*   **[REF-FTPS-OPS] Section 2.2: Over-the-Wire Telemetry Payload Schema (The Read Stream)**
    *   UNIX listing parser, spacing tokens, rollover logic, and AVBL/STAT space queries
*   **[REF-FTPS-XFER] Section 2.3: Over-the-Wire Control Command Schema (The Write Stream)**
    *   Handshake and session protection commands (including mandatory `TYPE I` binary mode), chunked binary uploads (STOR), and file downloads (RETR)
*   **Section 2.4: Mechanical & Firmware Quirks**
    *   P2S/X2D session close race, Command Channel Post-Transfer Response Synchronization, and MicroSD flush validation & 0500-C010 exceptions [REF-FTPS-FLUSH]

### Chapter 3: Local Broker Control & State Telemetry
*   **[REF-MQTT-CONN] Section 3.1: Network Boundary & Interface Parameters**
    *   Local broker authentication on Port 8883, topic topology, QoS/Clean Session rules, and broker in-flight limits
*   **[REF-MQTT-ENV] Section 3.2: Over-the-Wire Telemetry Payload Schema (The Read Stream)**
    *   Status telemetry structures, dual-location device telemetry, speed profiles, light state arrays, string emission anomalies, task-ID overflow limits, and A1/P1 series "stg_cur = 0" idle bug state gating [REF-MQTT-IDLEBUG]
*   **Section 3.2.1: Model-Specific Telemetry Polymorphism & Bitmasks**
    *   Wired Ethernet Wi-Fi Signal Sentinel & home_flag Bit 18 [REF-NET-PORTS]
    *   Enclosure Door Open Sensor Routing [REF-NET-DOOR]
    *   Divergent Nozzle Info Telemetry Keys [REF-NOZZLE-KEYS]
    *   Fan Speed Telemetry Key Mapping [REF-CLIM-FANS]
    *   Developer LAN Mode bitmask evaluation
    *   A1 and P1 Series Hardware Probing Protocol
*   **[REF-MQTT-LIFECYCLE] Section 3.3: Over-the-Wire Control Command Schema (The Write Stream)**
    *   Command wrappers (project_file, ams_filament_setting, and mandatory error clearing coupling)
    *   Polymorphic Path (url), Schema (ams_mapping), and Typing (use_ams boolean constraints) rules
    *   G-Code Command Queue Wrapper (gcode_line [REF-MOTO-GCODE])
    *   Enclosure LED Lighting Control (ledctrl)
    *   Airduct AC Mode Selection (set_airduct)
    *   Prompt Sound & Buzzer Commands (print_option, buzzer_ctrl)
    *   Physical Calibration Controls (calibration option bitmask calculation)
    *   AMS Controls (ams_control and ams_get_rfid commands)
    *   Feed Speed Level Configurations (print_speed command parameters)
*   **Section 3.4: Mechanical & Firmware Quirks**
    *   Keep-alive socket zombie detection [REF-MQTT-ZOMBIE]
    *   Local QoS 1 Queue Replay Errors [REF-MQTT-REPLAY]

### Chapter 4: Toolhead, Thermal, Motion & Climate Systems
*   **[REF-MOTO-GCODE] Section 4.1: Network Boundary & Interface Parameters**
    *   Synchronous G-code command line queue routing over MQTTS Port 8883 using the gcode_line control wrapper
*   **[REF-THER-DECODE] Section 4.2: State Telemetry Decoding**
    *   High-word/low-word temperature decoding, Right (T0)/Left (T1) IDEX thermal mappings, Hotend Configuration Query Stale Data Quirk, and Fan Speed Telemetry Quantization with oscillation artifacts [REF-CLIM-FANS]
    *   Bed temperature wire paths (`device.bed` vs `bed_temper`/`bed_target_temper`) and extruder state bitmask
    *   Divergent nozzle properties physical mappings [REF-NOZZLE-KEYS]
    *   Active Chamber Thermal Targeting State Encoding
    *   Chamber temperature sensor availability constraints
*   **Section 4.3: Over-the-Wire Control G-Code Streams**
    *   Raw G-code command strings (M140, M104, manual active chamber M141, manual extrusion [REF-GCODE-EXTRUDE], active RFID tag scan M620 [REF-GCODE-RFID])
    *   Manual relative axis movement and travel limit wrap controls
    *   Raw chamber and auxiliary fan PWM speed control channels [REF-CLIM-FANS]
    *   Physical control envelopes for lights, climate dampers, buzzers, and external tool mount telemetry
*   **Section 4.4: Mechanical & Firmware Quirks**
    *   Z-axis homing crash hazards (bare G28 constraints vs bed-slinger coordinates) and hotend fan safety control overrides

### Chapter 5: Physical Material Expansion (AMS, AMS-HT & Spools)
*   **[REF-AMS-DECODE] Section 5.1: Bus Telemetry & Bitmask Parsing**
    *   Tray presence masks, printer-shutdown telemetry exception, and ams_status combined state bitmask
    *   Incremental telemetry update slot cleansing rules
    *   Symmetrical absent-key empty slot signalling (P1S & A1 Mini)
    *   Multi-AMS local index resolution (`tray_now`)
    *   AMS unit info hex bitmask (type, dry status, extruder assignment)
    *   Virtual/external spool telemetry paths (`vt_tray` and `vir_slot`)
    *   Bus module firmware and serial number query (`get_version` response)
*   **[REF-AMS-SP_CFG] Section 5.2: Spool Presets, Colors & RFID Serialization**
    *   RFID tag UIDs, preset short index lookup tables, and RRGGBBAA hex colors
*   **[REF-AMS-MAP] Section 5.3: AMS Slicer Mappings & Filament Changes**
    *   Flat ams_mapping array with unmapped external spool flat-mapping restrictions
    *   Structured ams_mapping2 objects and mandatory use_ams override on single-nozzle configurations [REF-AMS-USEAMS]
    *   Extrusion_cali_sel calibration profile mapping, single vs IDEX carriage rules, and single-nozzle virtual slot remappings
    *   Filament load & unload command structures and target remappings
*   **[REF-AMS-DRYER] Section 5.4: Dry-Chamber Operations**
    *   Drying command structures, thermal scales, and error feedback properties
    *   Telemetry edge-triggering and omitted drying time parameters

### Chapter 6: Video Streaming & Chamber Image Handshakes
*   **[REF-CAM-RTSPS] Section 6.1: Network Boundary & Interface Parameters**
    *   RTSP streaming over Port 322, proprietary binary TCP stream over Port 6000 [REF-CAM-BINARY]
*   **Section 6.2: Over-the-Wire Telemetry Payload Schema (The Read Stream)**
    *   Interactive camera and recording indicators, RTSP URL availability field
*   **Section 6.3: Over-the-Wire Control Command Schema (The Write Stream)**
    *   RTSPS connection headers, digest auth challenges, and 80-byte binary handshake payloads [REF-CAM-BINARY]
*   **Section 6.4: Mechanical & Firmware Quirks**
    *   RTSPS handshake negotiations, static RTP timestamps, P2S keyframe delay, and discrete frame pacing

### Chapter 7: Diagnostic Mapping & Calibration Profiles
*   **[REF-DIAG-HMS] Section 7.1: HMS Telemetry Decoding**
    *   Converting 32-bit packed integers to Unified HMS Keys, severity module IDs, and local 8-character short-code format
    *   Optional timestamp fields (`ts_boot`, `ts_unix`) on X2/H2/P2 models
    *   Real hardware faults vs. non-error status codes and user-action cancellation echoes
*   **[REF-DIAG-KPROF] Section 7.2: Pressure Advance (K-Profile) Calibration**
    *   Onboard EEPROM querying, creation/edition/deletion transactions, and single vs IDEX command parameters
    *   K-Profile telemetry database schemas (read stream)
*   **Section 7.3: Mechanical & Firmware Quirks**
    *   K-profile request priming
