**bambino > diagnostics > hms**

# Module: diagnostics::hms

## Contents

**Structs**

- [`DecodedHmsAlert`](#decodedhmsalert) - Fully decoded representation of an active diagnostic entry from the `hms` telemetry array.
- [`DecodedPrintError`](#decodedprinterror) - Fully decoded representation of the primary system `print_error` register.

**Enums**

- [`HmsSeverity`](#hmsseverity) - Numerical classification of the severity level of an HMS diagnostic alert.

**Functions**

- [`decode_hms_alert`](#decode_hms_alert) - Decodes an active entry from the `hms` telemetry array [REF-DIAG-HMS].
- [`decode_print_error`](#decode_print_error) - Normalizes the 32-bit decimal `print_error` register into its active diagnostic short-code.

---

## bambino::diagnostics::hms::DecodedHmsAlert

*Struct*

Fully decoded representation of an active diagnostic entry from the `hms` telemetry array.

**Fields:**
- `wiki_key: String` - The standard 16-character wiki troubleshooting key (`MMMM_MMMM_CCCC_CCCC`).
- `short_code: String` - The local 8-character short-code format displayed on the physical LCD panel (`MMMM_CCCC`).
- `severity: HmsSeverity` - Decoded physical severity rating of the active system alert.
- `module_id: u8` - Unique identifier of the source hardware module executing under failure.
- `is_genuine_fault: bool` - Flags whether this alert represents a genuine hardware fault rather than a progress or state step.

**Traits:** Eq

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> DecodedHmsAlert`
- **PartialEq**
  - `fn eq(self: &Self, other: &DecodedHmsAlert) -> bool`
- **Hash**
  - `fn hash<__H>(self: &Self, state: & mut __H)`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::diagnostics::hms::DecodedPrintError

*Struct*

Fully decoded representation of the primary system `print_error` register.

**Fields:**
- `short_code: String` - The local 8-character short-code format displayed on the physical LCD panel (`MMMM_CCCC`).
- `module_id: u8` - Unpacked system module code where the primary print execution halted.
- `is_genuine_fault: bool` - Flags whether this error register holds a genuine hardware failure block.

**Traits:** Eq

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> DecodedPrintError`
- **PartialEq**
  - `fn eq(self: &Self, other: &DecodedPrintError) -> bool`
- **Hash**
  - `fn hash<__H>(self: &Self, state: & mut __H)`



## bambino::diagnostics::hms::HmsSeverity

*Enum*

Numerical classification of the severity level of an HMS diagnostic alert.

**Variants:**
- `Fatal` - Severe operational failure requiring immediate print execution halt.
- `Serious` - High-priority alert requiring user intervention before execution resumes.
- `Warning` - Non-blocking warning indicating minor runtime or environment issues.
- `Info` - Routine information prompt or system state confirmation event.
- `Unknown` - Fallback classification for unrecognized alert bounds.

**Methods:**

- `fn from_attr(attr: u32) -> Self` - Extracts the severity level from the second byte of the 32-bit `attr` value.

**Traits:** Copy, Eq

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **PartialEq**
  - `fn eq(self: &Self, other: &HmsSeverity) -> bool`
- **Hash**
  - `fn hash<__H>(self: &Self, state: & mut __H)`
- **Clone**
  - `fn clone(self: &Self) -> HmsSeverity`



## bambino::diagnostics::hms::decode_hms_alert

*Function*

Decodes an active entry from the `hms` telemetry array [REF-DIAG-HMS].

Unpacks the 32-bit `attr` and `code` parameters to reconstruct standard Wiki-slug
tracking variables, extract severity ratings, isolate module indexes, and filter
transient state updates.

```rust
fn decode_hms_alert(attr: u32, code: u32) -> DecodedHmsAlert
```



## bambino::diagnostics::hms::decode_print_error

*Function*

Normalizes the 32-bit decimal `print_error` register into its active diagnostic short-code.

Under the over-the-wire telemetry channel, the `print_error` status is passed as a packed
decimal integer. Reconstructing this to LCD standards requires hex-string conversion
and formatting with an underscore separator [REF-DIAG-HMS].

```rust
fn decode_print_error(print_error: u32) -> Option<DecodedPrintError>
```



