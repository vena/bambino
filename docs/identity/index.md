*[bambino](../index.md) / [identity](index.md)*

---

# Module `identity`

# Printer Identity

[`PrinterIdentity`](#printeridentity) bundles the three pieces of data every "connect to protocol X"
entry point in this crate needs to dial and authenticate against a specific
printer: its LAN address, serial number, and access code.

Bundling these into one struct (instead of three adjacent same-typed `&str`
parameters) removes a transposition risk that isn't compiler-catchable
otherwise — nothing stops `fn connect(ip: &str, serial: &str, access_code: &str)`
from being called with two of those arguments swapped, since all three are the
same type.

`ip`/`serial`/`access_code` are never `Option` — an omitted field would compile
away the caller's obligation to supply it, but a caller could then just as
easily supply a fabricated placeholder that type-checks fine and is silently
wrong. A missing constructor argument is a compile error; a wrong `Some(value)`
is not. Trading the former for the latter is a regression, not a fix.

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

#### Trait Implementations

##### `impl Clone for PrinterIdentity`

- <span id="printeridentity-clone"></span>`fn clone(&self) -> PrinterIdentity` — [`PrinterIdentity`](#printeridentity)

##### `impl Debug for PrinterIdentity`

- <span id="printeridentity-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for PrinterIdentity`

##### `impl PartialEq for PrinterIdentity`

- <span id="printeridentity-partialeq-eq"></span>`fn eq(&self, other: &PrinterIdentity) -> bool` — [`PrinterIdentity`](#printeridentity)

