*[bambino](../../../index.md) / [mqtt](../../index.md) / [commands](../index.md) / [status](index.md)*

---

# Module `status`

Status query commands (pushall, get_version, clean_print_error).

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`GetVersionPayload`](#getversionpayload) | struct | Payload schema to retrieve hardware/firmware version strings from the expansion bus. |
| [`GetVersionRequest`](#getversionrequest) | struct | Queries the printer for its hardware and firmware version info. |
| [`PushAllPayload`](#pushallpayload) | struct | Payload schema to trigger a complete state dump ("pushall") from the printer. |
| [`PushAllRequest`](#pushallrequest) | struct | Requests a full state dump from the printer (all telemetry fields at once). |

## Types

### `GetVersionPayload`

```rust
struct GetVersionPayload {
    pub command: &'static str,
    pub sequence_id: String,
}
```

Payload schema to retrieve hardware/firmware version strings from the expansion bus.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"get_version"`.

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for GetVersionPayload`

- <span id="getversionpayload-clone"></span>`fn clone(&self) -> GetVersionPayload` — [`GetVersionPayload`](#getversionpayload)

##### `impl Debug for GetVersionPayload`

- <span id="getversionpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for GetVersionPayload`

- <span id="getversionpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `GetVersionRequest`

```rust
struct GetVersionRequest {
    pub info: GetVersionPayload,
}
```

Queries the printer for its hardware and firmware version info.

#### Fields

- **`info`**: `GetVersionPayload`

  The `info` namespace envelope required by the wire protocol.

#### Implementations

- <span id="getversionrequest-new"></span>`fn new(sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../index.md#clampedtaskid)

  Builds a `get_version` request.

#### Trait Implementations

##### `impl Clone for GetVersionRequest`

- <span id="getversionrequest-clone"></span>`fn clone(&self) -> GetVersionRequest` — [`GetVersionRequest`](#getversionrequest)

##### `impl Debug for GetVersionRequest`

- <span id="getversionrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for GetVersionRequest`

- <span id="getversionrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PushAllPayload`

```rust
struct PushAllPayload {
    pub command: &'static str,
    pub sequence_id: String,
}
```

Payload schema to trigger a complete state dump ("pushall") from the printer.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"pushall"`.

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for PushAllPayload`

- <span id="pushallpayload-clone"></span>`fn clone(&self) -> PushAllPayload` — [`PushAllPayload`](#pushallpayload)

##### `impl Debug for PushAllPayload`

- <span id="pushallpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for PushAllPayload`

- <span id="pushallpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PushAllRequest`

```rust
struct PushAllRequest {
    pub pushing: PushAllPayload,
}
```

Requests a full state dump from the printer (all telemetry fields at once).

#### Fields

- **`pushing`**: `PushAllPayload`

  The `pushing` namespace envelope required by the wire protocol.

#### Implementations

- <span id="pushallrequest-new"></span>`fn new(sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../index.md#clampedtaskid)

  Builds a `pushall` request.

#### Trait Implementations

##### `impl Clone for PushAllRequest`

- <span id="pushallrequest-clone"></span>`fn clone(&self) -> PushAllRequest` — [`PushAllRequest`](#pushallrequest)

##### `impl Debug for PushAllRequest`

- <span id="pushallrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for PushAllRequest`

- <span id="pushallrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

