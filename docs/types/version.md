**bambino > types > version**

# Module: types::version

## Contents

**Structs**

- [`VersionInfo`](#versioninfo) - Typed response from a `get_version` command containing all expansion bus modules.
- [`VersionModule`](#versionmodule) - Hardware or firmware module entry from the printer's expansion bus version database.

---

## bambino::types::version::VersionInfo

*Struct*

Typed response from a `get_version` command containing all expansion bus modules.

**Fields:**
- `command: String` - Command name echoed back (always "get_version").
- `sequence_id: String` - Sequence ID echoed back from the request.
- `module: Vec<VersionModule>` - All hardware and firmware modules on the expansion bus.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> VersionInfo`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`



## bambino::types::version::VersionModule

*Struct*

Hardware or firmware module entry from the printer's expansion bus version database.

**Fields:**
- `product_name: String` - Marketing product name (e.g. "Bambu Lab X1 Carbon"). Empty if not reported.
- `name: String` - Internal module name (e.g. "ota", "esp32", "mc", "ams").
- `hw_ver: String` - Hardware revision string.
- `sw_ver: String` - Firmware version string (e.g. "01.09.00.00").
- `sn: String` - Module serial number.
- `visible: bool` - Whether this module shows in user-facing version lists. Defaults to true.
- `project_name: Option<String>` - Used by older firmware (P1P/P1S/A1) for printer type identification via esp32 module.
- `loader_ver: Option<String>` - Bootloader version.
- `ota_ver: Option<String>` - OTA update version.
- `flag: Option<i32>` - Module flags.

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> VersionModule`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`



