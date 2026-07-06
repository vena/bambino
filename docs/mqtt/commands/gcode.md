**bambino > mqtt > commands > gcode**

# Module: mqtt::commands::gcode

## Contents

**Structs**

- [`GCodePayload`](#gcodepayload) - Queues raw G-code strings directly to the printer's motion execution controller.
- [`GCodeRequest`](#gcoderequest) - Sends a raw G-code line to the printer for immediate execution.

---

## bambino::mqtt::commands::gcode::GCodePayload

*Struct*

Queues raw G-code strings directly to the printer's motion execution controller.

Under the Bambu protocol specification, physical moves, manual extrusions, and
temperature targets are issued by packing standard G-code lines into this wrapper.

**Fields:**
- `command: &'static str` - Wire command name, always `"gcode_line"`.
- `param: String` - Raw G-code line, newline-terminated by [`GCodeRequest::new`].
- `sequence_id: String` - Request sequence ID, serialized as a string on the wire.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> GCodePayload`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::mqtt::commands::gcode::GCodeRequest

*Struct*

Sends a raw G-code line to the printer for immediate execution.

**Fields:**
- `print: GCodePayload` - The `print` namespace envelope required by the wire protocol.

**Methods:**

- `fn new(gcode_line: &str, sequence_id: u64) -> Self` - Creates a request envelope wrapping a raw G-code payload.

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> GCodeRequest`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



