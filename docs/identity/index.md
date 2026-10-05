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
| [`validate_access_code`](#validate-access-code) | fn | Checks that `access_code` is 1 to [`ACCESS_CODE_MAX_LEN`](#access-code-max-len) ASCII letters or digits. |
| [`validate_serial`](#validate-serial) | fn | Checks that `serial` is 1 to [`SERIAL_MAX_LEN`](#serial-max-len) ASCII letters or digits. |
| [`ACCESS_CODE_MAX_LEN`](#access-code-max-len) | const | Longest LAN access code any protocol accepts: the camera handshake's 32-byte password field. |
| [`SERIAL_MAX_LEN`](#serial-max-len) | const | Longest serial number accepted. |

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

[`Debug`](https://docs.rs/core/latest/core/fmt/trait.Debug.html) is implemented manually to redact `access_code`; see the impl below.

#### Fields

- **`ip`**: `String`

  LAN IP address or hostname of the printer.

- **`serial`**: `String`

  Printer's serial number, used for TLS SNI and MQTT topic scoping.

- **`access_code`**: `String`

  Printer's local network access code (found in its LAN-only settings screen).

- **`model`**: `crate::models::PrinterModel`

  Printer model, used for quirks dispatch. Derivable from `serial` via
  [`resolve_model`](../models/index.md#resolve-model); see [`PrinterIdentity::new`](#printeridentity) for the common case.

#### Implementations

- <span id="printeridentity-new"></span>`fn new(ip: impl Into<String>, serial: impl Into<String>, access_code: impl Into<String>) -> Self`

  Builds an identity, deriving `model` from `serial` via [`resolve_model`](../models/index.md#resolve-model).

  For callers who need a specific `model` regardless of what the serial
  prefix implies, construct the struct literal directly instead.

- <span id="printeridentity-try-new"></span>`fn try_new(ip: impl Into<String>, serial: impl Into<String>, access_code: impl Into<String>) -> Result<Self, Error>` — [`Error`](../error/index.md#error)

  Like [`PrinterIdentity::new`](#printeridentity), but rejects a malformed serial or access code up front.

- <span id="printeridentity-validate"></span>`fn validate(&self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Checks the serial and access code with [`validate_serial`](#validate-serial) and [`validate_access_code`](#validate-access-code).

  `ip` is not checked: it may be a hostname, which only the dial can resolve.

#### Trait Implementations

##### `impl Clone for PrinterIdentity`

- <span id="printeridentity-clone"></span>`fn clone(&self) -> PrinterIdentity` — [`PrinterIdentity`](#printeridentity)

##### `impl Debug for PrinterIdentity`

- <span id="printeridentity-debug-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

##### `impl Eq for PrinterIdentity`

##### `impl PartialEq for PrinterIdentity`

- <span id="printeridentity-partialeq-eq"></span>`fn eq(&self, other: &PrinterIdentity) -> bool` — [`PrinterIdentity`](#printeridentity)


---

## Functions

### `validate_access_code`

```rust
fn validate_access_code(access_code: &str) -> Result<(), crate::error::Error>
```

**Types:** [`Error`](../error/index.md#error)

Checks that `access_code` is 1 to [`ACCESS_CODE_MAX_LEN`](#access-code-max-len) ASCII letters or digits.

Printer-issued LAN access codes are 8 case-sensitive alphanumerics, so a rejection almost
always means a copy-paste mistake (whitespace, a trailing newline). The alphanumeric rule
is also what keeps the code safe to interpolate into an RTSPS URL's userinfo.

### `validate_serial`

```rust
fn validate_serial(serial: &str) -> Result<(), crate::error::Error>
```

**Types:** [`Error`](../error/index.md#error)

Checks that `serial` is 1 to [`SERIAL_MAX_LEN`](#serial-max-len) ASCII letters or digits.

The serial becomes an MQTT topic segment and the TLS SNI name, so anything else would
reach the wire malformed.


---

## Constants

### `ACCESS_CODE_MAX_LEN`
```rust
const ACCESS_CODE_MAX_LEN: usize = 32usize;
```

Longest LAN access code any protocol accepts: the camera handshake's 32-byte password field.

### `SERIAL_MAX_LEN`
```rust
const SERIAL_MAX_LEN: usize = 20usize;
```

Longest serial number accepted. Current Bambu serials are 15 characters.

