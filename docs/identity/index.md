*[bambino](../index.md) / [identity](index.md)*

---

# Module `identity`

# Printer Identity

[`PrinterIdentity`](#printeridentity) bundles the LAN address, serial number, and access code every
"connect to protocol X" entry point in this crate needs to dial and authenticate
against a specific printer, instead of passing them as three adjacent same-typed
`&str` parameters a caller could transpose without a compile error.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`PrinterIdentity`](#printeridentity) | struct | Address, serial number, and access code identifying one printer on the LAN. |

## Types

### `PrinterIdentity`

```rust
struct PrinterIdentity {
    pub ip: String,
    pub serial: String,
    pub access_code: String,
    pub model: crate::models::PrinterModel,
}
```

Address, serial number, and access code identifying one printer on the LAN.

#### Fields

- **`ip`**: `String`

  LAN IP address or hostname of the printer.

- **`serial`**: `String`

  Printer's serial number, used for TLS SNI and MQTT topic scoping.

- **`access_code`**: `String`

  Printer's local network access code (found in its LAN-only settings screen).

- **`model`**: `crate::models::PrinterModel`

  Printer model, used for quirks dispatch. Derivable from `serial` via
  [`resolve_model`](../models/index.md#resolve-model); see [`PrinterIdentity::new`] for the common case.

#### Implementations

- <span id="printeridentity-new"></span>`fn new(ip: impl Into<String>, serial: impl Into<String>, access_code: impl Into<String>) -> Self`

  Builds an identity, deriving `model` from `serial` via [`resolve_model`](../models/index.md#resolve-model).

#### Trait Implementations

##### `impl<E> AsTaggedExplicit<'a, E> for PrinterIdentity`

##### `impl<E> AsTaggedImplicit<'a, E> for PrinterIdentity`

##### `impl Clone for PrinterIdentity`

- <span id="printeridentity-clone"></span>`fn clone(&self) -> PrinterIdentity` — [`PrinterIdentity`](#printeridentity)

##### `impl Debug for PrinterIdentity`

- <span id="printeridentity-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for PrinterIdentity`

##### `impl PartialEq for PrinterIdentity`

- <span id="printeridentity-partialeq-eq"></span>`fn eq(&self, other: &PrinterIdentity) -> bool` — [`PrinterIdentity`](#printeridentity)

##### `impl StructuralPartialEq for PrinterIdentity`

