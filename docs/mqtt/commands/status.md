**bambino > mqtt > commands > status**

# Module: mqtt::commands::status

## Contents

**Structs**

- [`GetVersionPayload`](#getversionpayload) - Payload schema to retrieve hardware/firmware version strings from the expansion bus.
- [`GetVersionRequest`](#getversionrequest) - Queries the printer for its hardware and firmware version info.
- [`PushAllPayload`](#pushallpayload) - Payload schema to trigger a complete state dump ("pushall") from the printer.
- [`PushAllRequest`](#pushallrequest) - Requests a full state dump from the printer (all telemetry fields at once).

---

## bambino::mqtt::commands::status::GetVersionPayload

*Struct*

Payload schema to retrieve hardware/firmware version strings from the expansion bus.

**Fields:**
- `command: &'static str`
- `sequence_id: String`

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> GetVersionPayload`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::mqtt::commands::status::GetVersionRequest

*Struct*

Queries the printer for its hardware and firmware version info.

**Fields:**
- `info: GetVersionPayload`

**Methods:**

- `fn new(sequence_id: u64) -> Self`

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> GetVersionRequest`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::mqtt::commands::status::PushAllPayload

*Struct*

Payload schema to trigger a complete state dump ("pushall") from the printer.

**Fields:**
- `command: &'static str`
- `sequence_id: String`

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> PushAllPayload`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::mqtt::commands::status::PushAllRequest

*Struct*

Requests a full state dump from the printer (all telemetry fields at once).

**Fields:**
- `pushing: PushAllPayload`

**Methods:**

- `fn new(sequence_id: u64) -> Self`

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> PushAllRequest`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



