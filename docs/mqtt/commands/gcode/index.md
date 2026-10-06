*[bambino](../../../index.md) / [mqtt](../../index.md) / [commands](../index.md) / [gcode](index.md)*

---

# Module `gcode`

G-code dispatch command payload.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`GCodePayload`](#gcodepayload) | struct | Queues raw G-code strings directly to the printer's motion execution controller. |
| [`GCodeRequest`](#gcoderequest) | type | Sends a raw G-code line to the printer for immediate execution. |

## Types

### `GCodePayload`

```rust
struct GCodePayload {
    pub command: &'static str,
    pub param: String,
    pub sequence_id: super::ClampedTaskId,
}
```

Queues raw G-code strings directly to the printer's motion execution controller.

Under the Bambu protocol specification, physical moves, manual extrusions, and
temperature targets are issued by packing standard G-code lines into this wrapper.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"gcode_line"`.

- **`param`**: `String`

  Raw G-code line, newline-terminated by [`GCodeRequest::new`](#gcoderequest).

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for GCodePayload`

- <span id="gcodepayload-clone"></span>`fn clone(&self) -> GCodePayload` — [`GCodePayload`](#gcodepayload)

##### `impl Debug for GCodePayload`

- <span id="gcodepayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for GCodePayload`

- <span id="gcodepayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `GCodeRequest`

```rust
type GCodeRequest = super::Print<GCodePayload>;
```

Sends a raw G-code line to the printer for immediate execution.

