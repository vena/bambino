**bambino > mqtt**

# Module: mqtt

## Contents

**Modules**

- [`client`](#client) - # Lightweight, Transport-Agnostic MQTT v3.1.1 Client Session
- [`commands`](#commands) - # MQTT Command Payloads & Serialization Builders

---

## Module: client

# Lightweight, Transport-Agnostic MQTT v3.1.1 Client Session

Implements a dedicated async MQTT client designed to execute over our abstract
`AsyncIo` trait bounds. This custom client facilitates secure MQTTS connection
negotiations, subscription registrations, QoS 1 publish queues, keep-alive frames,
and write-channel zombie detection [REF-MQTT-CONN] [REF-MQTT-ZOMBIE].

Designed for absolute execution safety across standard hosts, ESP-IDF microcontrollers,
and bare-metal Embassy targets.



## Module: commands

# MQTT Command Payloads & Serialization Builders

Provides the concrete data structures and serialization wrappers required to control
physical Bambu Lab printers over MQTTS Port 8883 [REF-MQTT-LIFECYCLE].

Handles complex polymorphic rules such as the string-vs-array mapping schemas for the
`ams_mapping` parameter, and enforces safety bounds on task identities.

## Architectural Alignment
* **Polymorphic Mapping Rules [REF-MQTT-LIFECYCLE]:** Handles conditional typing for
  material mappings, where inactive AMS sessions must present as empty strings while active
  sessions require integer arrays.
* **Task-ID Overflow Prevention [REF-MQTT-ENV]:** Clamps all generated sequence identifiers
  to 32-bit signed integer limits to prevent memory allocation overflows on hardware boards.



