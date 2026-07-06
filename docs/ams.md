**bambino > ams**

# Module: ams

## Contents

**Modules**

- [`mapping`](#mapping) - # AMS Slicer Mapping & Filament Change Builders
- [`parser`](#parser) - # AMS Telemetry & Bitmask Parser

---

## Module: mapping

# AMS Slicer Mapping & Filament Change Builders

Handles translation of slicer-allocated project materials into physical and
virtual printer hardware channels [REF-AMS-MAP]. Implements flat `ams_mapping` and
structured `ams_mapping2` payload arrays and enforces safety interlocks for single-nozzle
external spools [REF-AMS-USEAMS].



## Module: parser

# AMS Telemetry & Bitmask Parser

Implements low-level bitwise operations and sanitization logic for parsing
Bambu Lab AMS telemetry reports [REF-AMS-DECODE]. This includes checking spool
presence via hex bitmasks, managing power-down state anomalies, cleansing stale
tray data, and calculating global indexes.



