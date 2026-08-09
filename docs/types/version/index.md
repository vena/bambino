*[bambino](../../index.md) / [types](../index.md) / [version](index.md)*

---

# Module `version`

Firmware version information returned by the `get_version` command.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`VersionInfo`](#versioninfo) | struct | Typed response from a `get_version` command containing all expansion bus modules. |
| [`VersionModule`](#versionmodule) | struct | Hardware or firmware module entry from the printer's expansion bus version database. |

## Types

### `VersionInfo`

```rust
struct VersionInfo {
    pub command: String,
    pub sequence_id: String,
    pub module: Vec<VersionModule>,
}
```

Typed response from a `get_version` command containing all expansion bus modules.

#### Fields

- **`command`**: `String`

  Command name echoed back (always "get_version").

- **`sequence_id`**: `String`

  Sequence ID echoed back from the request.

- **`module`**: `Vec<VersionModule>`

  All hardware and firmware modules on the expansion bus.

#### Trait Implementations

##### `impl Clone for VersionInfo`

- <span id="versioninfo-clone"></span>`fn clone(&self) -> VersionInfo` — [`VersionInfo`](#versioninfo)

##### `impl Debug for VersionInfo`

- <span id="versioninfo-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for VersionInfo`

- <span id="versioninfo-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for VersionInfo`

### `VersionModule`

```rust
struct VersionModule {
    pub product_name: String,
    pub name: String,
    pub hw_ver: String,
    pub sw_ver: String,
    pub sn: String,
    pub visible: bool,
    pub project_name: Option<String>,
    pub loader_ver: Option<String>,
    pub ota_ver: Option<String>,
    pub flag: Option<i32>,
}
```

Hardware or firmware module entry from the printer's expansion bus version database.

#### Fields

- **`product_name`**: `String`

  Marketing product name (e.g. "Bambu Lab X1 Carbon"). Empty if not reported.

- **`name`**: `String`

  Internal module name (e.g. "ota", "esp32", "mc", "ams").

- **`hw_ver`**: `String`

  Hardware revision string.

- **`sw_ver`**: `String`

  Firmware version string (e.g. "01.09.00.00").

- **`sn`**: `String`

  Module serial number.

- **`visible`**: `bool`

  Whether this module shows in user-facing version lists. Defaults to true.

- **`project_name`**: `Option<String>`

  Used by older firmware (P1P/P1S/A1) for printer type identification via esp32 module.

- **`loader_ver`**: `Option<String>`

  Bootloader version.

- **`ota_ver`**: `Option<String>`

  OTA update version.

- **`flag`**: `Option<i32>`

  Module flags.

#### Trait Implementations

##### `impl Clone for VersionModule`

- <span id="versionmodule-clone"></span>`fn clone(&self) -> VersionModule` — [`VersionModule`](#versionmodule)

##### `impl Debug for VersionModule`

- <span id="versionmodule-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for VersionModule`

- <span id="versionmodule-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for VersionModule`

