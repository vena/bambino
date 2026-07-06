**bambino > types**

# Module: types

## Contents

**Modules**

- [`telemetry`](#telemetry) - # State Telemetry Payload Schemas
- [`version`](#version) - Firmware version information returned by the `get_version` command.

---

## Module: telemetry

# State Telemetry Payload Schemas

Provides structured, allocation-friendly deserialization models for the
local MQTTS Port 8883 state telemetry streams [REF-MQTT-ENV].

Supports permissive parsing for platform discrepancies (such as the variable
types of `sdcard` presence markers) and implements binary unpacking helpers
for composite packed temperatures, home/status flags, and door sensors.

## Architectural Alignment
* **Quirks Integration:** Raw elements (e.g., `device.airduct.parts` or `ctc.info.temp`)
  are fully parsed into clean schemas to allow model-specific behaviors to be evaluated
  via the quirks engine.



## Module: version

Firmware version information returned by the `get_version` command.



