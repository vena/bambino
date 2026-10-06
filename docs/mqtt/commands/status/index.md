*[bambino](../../../index.md) / [mqtt](../../index.md) / [commands](../index.md) / [status](index.md)*

---

# Module `status`

Status query commands (pushall, get_version, get_access_code).

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`GetAccessCodePayload`](#getaccesscodepayload) | struct | Payload schema to ask the printer to report its own current LAN access code. |
| [`GetVersionPayload`](#getversionpayload) | struct | Payload schema to retrieve hardware/firmware version strings from the expansion bus. |
| [`PushAllPayload`](#pushallpayload) | struct | Payload schema to trigger a complete state dump ("pushall") from the printer. |
| [`GetAccessCodeRequest`](#getaccesscoderequest) | type | Queries the printer for its own current LAN access code. |
| [`GetVersionRequest`](#getversionrequest) | type | Queries the printer for its hardware and firmware version info. |
| [`PushAllRequest`](#pushallrequest) | type | Requests a full state dump from the printer (all telemetry fields at once). |

## Types

### `GetAccessCodePayload`

```rust
struct GetAccessCodePayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
}
```

Payload schema to ask the printer to report its own current LAN access code.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"get_access_code"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for GetAccessCodePayload`

- <span id="getaccesscodepayload-clone"></span>`fn clone(&self) -> GetAccessCodePayload` — [`GetAccessCodePayload`](#getaccesscodepayload)

##### `impl Debug for GetAccessCodePayload`

- <span id="getaccesscodepayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for GetAccessCodePayload`

- <span id="getaccesscodepayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `GetVersionPayload`

```rust
struct GetVersionPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
}
```

Payload schema to retrieve hardware/firmware version strings from the expansion bus.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"get_version"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for GetVersionPayload`

- <span id="getversionpayload-clone"></span>`fn clone(&self) -> GetVersionPayload` — [`GetVersionPayload`](#getversionpayload)

##### `impl Debug for GetVersionPayload`

- <span id="getversionpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for GetVersionPayload`

- <span id="getversionpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PushAllPayload`

```rust
struct PushAllPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
}
```

Payload schema to trigger a complete state dump ("pushall") from the printer.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"pushall"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for PushAllPayload`

- <span id="pushallpayload-clone"></span>`fn clone(&self) -> PushAllPayload` — [`PushAllPayload`](#pushallpayload)

##### `impl Debug for PushAllPayload`

- <span id="pushallpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for PushAllPayload`

- <span id="pushallpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `GetAccessCodeRequest`

```rust
type GetAccessCodeRequest = super::System<GetAccessCodePayload>;
```

Queries the printer for its own current LAN access code.

Distinct from the access code the caller supplies to authenticate: this re-reads the value
from the printer over an already-authenticated session, which is how a client notices that a
rotated code has invalidated its cached credential.

The reply is `system`-wrapped and echoes the request's `sequence_id`, alongside
`access_code`, `result`, and `reason` — confirmed on a P1S via `bambino-cli ack-probe`
(issue #140); see `reference/03_mqtt_telemetry.md` for the observed shape.

Treat the returned code as a credential: it must never be logged or written to disk.

### `GetVersionRequest`

```rust
type GetVersionRequest = super::Info<GetVersionPayload>;
```

Queries the printer for its hardware and firmware version info.

### `PushAllRequest`

```rust
type PushAllRequest = super::Pushing<PushAllPayload>;
```

Requests a full state dump from the printer (all telemetry fields at once).

