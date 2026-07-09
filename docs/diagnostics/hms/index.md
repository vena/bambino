*[bambino](../../index.md) / [diagnostics](../index.md) / [hms](index.md)*

---

# Module `hms`

# HMS Diagnostic Telemetry Parsing & Unpacking Engine

Provides mathematical decoders to unpack physical printer hardware fault codes,
warning levels, and operational alerts from telemetry status streams [REF-DIAG-HMS].

This module parses:
1. The 32-bit `print_error` register into short-code formats.
2. The `hms` array containing active telemetry blocks (`attr` and `code`) into
   both 16-character Wiki slugs and 8-character local short-codes.

## Technical Specifications
* **Fault Isolation**: Filters out non-error statuses (low 16-bit word < `0x4000`)
  and user action confirmation echoes (such as user-initiated cancellation events)
  to isolate genuine hardware failures from routine system state updates.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`DecodedHmsAlert`](#decodedhmsalert) | struct | Fully decoded representation of an active diagnostic entry from the `hms` telemetry array. |
| [`DecodedPrintError`](#decodedprinterror) | struct | Fully decoded representation of the primary system `print_error` register. |
| [`HmsSeverity`](#hmsseverity) | enum | Numerical classification of the severity level of an HMS diagnostic alert. |
| [`decode_hms_alert`](#decode-hms-alert) | fn | Decodes an active entry from the `hms` telemetry array [REF-DIAG-HMS]. |
| [`decode_print_error`](#decode-print-error) | fn | Normalizes the 32-bit decimal `print_error` register into its active diagnostic short-code. |

## Types

### `DecodedHmsAlert`

```rust
struct DecodedHmsAlert {
    pub wiki_key: String,
    pub short_code: String,
    pub severity: HmsSeverity,
    pub module_id: u8,
    pub is_genuine_fault: bool,
}
```

Fully decoded representation of an active diagnostic entry from the `hms` telemetry array.

#### Fields

- **`wiki_key`**: `String`

  The standard 16-character wiki troubleshooting key (`MMMM_MMMM_CCCC_CCCC`).

- **`short_code`**: `String`

  The local 8-character short-code format displayed on the physical LCD panel (`MMMM_CCCC`).

- **`severity`**: `HmsSeverity`

  Decoded physical severity rating of the active system alert.

- **`module_id`**: `u8`

  Unique identifier of the source hardware module executing under failure.

- **`is_genuine_fault`**: `bool`

  Flags whether this alert represents a genuine hardware fault rather than a progress or state step.

#### Trait Implementations

##### `impl Clone for DecodedHmsAlert`

- <span id="decodedhmsalert-clone"></span>`fn clone(&self) -> DecodedHmsAlert` — [`DecodedHmsAlert`](#decodedhmsalert)

##### `impl Debug for DecodedHmsAlert`

- <span id="decodedhmsalert-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for DecodedHmsAlert`

##### `impl Hash for DecodedHmsAlert`

- <span id="decodedhmsalert-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for DecodedHmsAlert`

- <span id="decodedhmsalert-partialeq-eq"></span>`fn eq(&self, other: &DecodedHmsAlert) -> bool` — [`DecodedHmsAlert`](#decodedhmsalert)

### `DecodedPrintError`

```rust
struct DecodedPrintError {
    pub short_code: String,
    pub module_id: u8,
    pub is_genuine_fault: bool,
}
```

Fully decoded representation of the primary system `print_error` register.

#### Fields

- **`short_code`**: `String`

  The local 8-character short-code format displayed on the physical LCD panel (`MMMM_CCCC`).

- **`module_id`**: `u8`

  Unpacked system module code where the primary print execution halted.

- **`is_genuine_fault`**: `bool`

  Flags whether this error register holds a genuine hardware failure block.

#### Trait Implementations

##### `impl Clone for DecodedPrintError`

- <span id="decodedprinterror-clone"></span>`fn clone(&self) -> DecodedPrintError` — [`DecodedPrintError`](#decodedprinterror)

##### `impl Debug for DecodedPrintError`

- <span id="decodedprinterror-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for DecodedPrintError`

##### `impl Hash for DecodedPrintError`

- <span id="decodedprinterror-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for DecodedPrintError`

- <span id="decodedprinterror-partialeq-eq"></span>`fn eq(&self, other: &DecodedPrintError) -> bool` — [`DecodedPrintError`](#decodedprinterror)

### `HmsSeverity`

```rust
enum HmsSeverity {
    Fatal,
    Serious,
    Warning,
    Info,
    Unknown,
}
```

Numerical classification of the severity level of an HMS diagnostic alert.

#### Variants

- **`Fatal`**

  Severe operational failure requiring immediate print execution halt.

- **`Serious`**

  High-priority alert requiring user intervention before execution resumes.

- **`Warning`**

  Non-blocking warning indicating minor runtime or environment issues.

- **`Info`**

  Routine information prompt or system state confirmation event.

- **`Unknown`**

  Fallback classification for unrecognized alert bounds.

#### Implementations

- <span id="hmsseverity-from-attr"></span>`fn from_attr(attr: u32) -> Self`

  Extracts the severity level from the second byte of the 32-bit `attr` value.

#### Trait Implementations

##### `impl Clone for HmsSeverity`

- <span id="hmsseverity-clone"></span>`fn clone(&self) -> HmsSeverity` — [`HmsSeverity`](#hmsseverity)

##### `impl Copy for HmsSeverity`

##### `impl Debug for HmsSeverity`

- <span id="hmsseverity-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for HmsSeverity`

##### `impl Hash for HmsSeverity`

- <span id="hmsseverity-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for HmsSeverity`

- <span id="hmsseverity-partialeq-eq"></span>`fn eq(&self, other: &HmsSeverity) -> bool` — [`HmsSeverity`](#hmsseverity)


---

## Functions

### `decode_hms_alert`

```rust
fn decode_hms_alert(attr: u32, code: u32) -> DecodedHmsAlert
```

**Types:** [`DecodedHmsAlert`](#decodedhmsalert)

Decodes an active entry from the `hms` telemetry array [REF-DIAG-HMS].

Unpacks the 32-bit `attr` and `code` parameters to reconstruct standard Wiki-slug
tracking variables, extract severity ratings, isolate module indexes, and filter
transient state updates.

### `decode_print_error`

```rust
fn decode_print_error(print_error: u32) -> Option<DecodedPrintError>
```

**Types:** [`DecodedPrintError`](#decodedprinterror)

Normalizes the 32-bit decimal `print_error` register into its active diagnostic short-code.

Under the over-the-wire telemetry channel, the `print_error` status is passed as a packed
decimal integer. Reconstructing this to LCD standards requires hex-string conversion
and formatting with an underscore separator [REF-DIAG-HMS].

