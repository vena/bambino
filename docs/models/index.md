*[bambino](../index.md) / [models](index.md)*

---

# Module `models`

# Printer Model Identification

Every Bambu Lab printer has a 3-character serial number prefix that identifies
its model. [`PrinterModel`](#printermodel) enumerates all known models, and [`resolve_model()`](#resolve-model)
maps serial prefixes (with an SSDP `DevModel` fallback) to the right variant.
The resolved model drives behavioral dispatch through the [`quirks`](../quirks/index.md) engine.

`MODELS` is the single source of truth: one row per supported model, carrying its
serial prefix, its wire-protocol tokens, and its human-readable name.
[`resolve_model()`](#resolve-model), [`supported_models()`](#supported-models), and [`PrinterModel::display_name()`](#printermodel) are
all views over that table, so adding a model means adding one enum variant and one row.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`PrinterModel`](#printermodel) | enum | Enumeration of physical Bambu Lab printer models supported on the local interface. |
| [`resolve_model`](#resolve-model) | fn | Resolves the specific printer model using physical serial number prefixes combined with target SSDP model advertisements as a secondary signal. |
| [`supported_models`](#supported-models) | fn | Returns every printer model this crate supports, in table order. |

## Types

### `PrinterModel`

```rust
enum PrinterModel {
    X1C,
    X1E,
    X2D,
    A1Mini,
    A1,
    A2L,
    P1P,
    P1S,
    P2S,
    H2D,
    H2DPro,
    H2C,
    H2S,
    Unknown,
}
```

Enumeration of physical Bambu Lab printer models supported on the local interface.

#### Variants

- **`X1C`**

  X1 and X1C Series (CoreXY architecture, RTSP-capable)

- **`X1E`**

  X1E (Enterprise CoreXY architecture, wired Ethernet)

- **`X2D`**

  X2D Series (CoreXY architecture, dual auxiliary cooling)

- **`A1Mini`**

  A1 Mini (Constrained bed-slinger, binary camera stream)

- **`A1`**

  A1 (Standard bed-slinger, binary camera stream)

- **`A2L`**

  A2L Series

- **`P1P`**

  P1P (Early CoreXY architecture, binary camera stream)

- **`P1S`**

  P1S (Enclosed CoreXY architecture, binary camera stream)

- **`P2S`**

  P2S Series (RTSP-capable)

- **`H2D`**

  H2D (Dual-nozzle IDEX platform)

- **`H2DPro`**

  H2D Pro (Premium IDEX platform)

- **`H2C`**

  H2C (Vortek tool-changer + fixed hotend, 7 nozzles total)

- **`H2S`**

  H2S (Single-nozzle platform sharing H2 mechanics)

- **`Unknown`**

  Fallback variant for newly released or unrecognized printer targets

#### Implementations

- <span id="printermodel-display-name"></span>`fn display_name(self) -> &'static str`

  Returns the human-readable model name, e.g. `"H2D Pro"` or `"A1 mini"`.

  Follows Bambu's own naming, so it is safe to show to a user directly.
  [`PrinterModel::Unknown`](#printermodel) renders as `"Unknown"`.

- <span id="printermodel-serial-prefix"></span>`fn serial_prefix(self) -> Option<&'static str>`

  Returns the 3-character serial number prefix identifying this model.

  `None` for [`PrinterModel::Unknown`](#printermodel). Useful for validating a serial before
  attempting a connection.

- <span id="cratemodelsprintermodel-quirks"></span>`fn quirks(&self) -> &'static dyn ModelQuirks` — [`ModelQuirks`](../quirks/index.md#modelquirks)

  Returns the [`ModelQuirks`](../quirks/index.md#modelquirks) strategy for this model variant.

  This is the single dispatch point — all model-specific behavior goes through
  the trait object returned here, rather than match-blocks scattered across the crate.

#### Trait Implementations

##### `impl Clone for PrinterModel`

- <span id="printermodel-clone"></span>`fn clone(&self) -> PrinterModel` — [`PrinterModel`](#printermodel)

##### `impl Copy for PrinterModel`

##### `impl Debug for PrinterModel`

- <span id="printermodel-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for PrinterModel`

- <span id="printermodel-display-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for PrinterModel`

##### `impl Hash for PrinterModel`

- <span id="printermodel-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for PrinterModel`

- <span id="printermodel-partialeq-eq"></span>`fn eq(&self, other: &PrinterModel) -> bool` — [`PrinterModel`](#printermodel)

##### `impl ToString for PrinterModel`

- <span id="printermodel-tostring-to-string"></span>`fn to_string(&self) -> String`


---

## Functions

### `resolve_model`

```rust
fn resolve_model(serial: &str, dev_model: Option<&str>) -> PrinterModel
```

**Types:** [`PrinterModel`](#printermodel)

Resolves the specific printer model using physical serial number prefixes combined with target SSDP model advertisements as a secondary signal.

Each H2-series model has a distinct serial prefix confirmed by the Bambu Lab wiki:
`094` = H2D, `093` = H2S, `239` = H2D Pro, `31B` = H2C. When the prefix is
unrecognized, the optional `DevModel` SSDP header provides a fallback path.

The two lookups are **separate full passes over the table**, and must stay that way:
a serial prefix on any row outranks a `dev_model` token on every row. Folding them
into a single pass would let an earlier row's token beat a later row's prefix,
silently changing which signal wins when the two disagree.

Both `serial` and `dev_model` are matched case-insensitively: SSDP USN serial casing
varies by firmware compile target (reference/01_network_discovery.md §1.6), and a
caller can also pass either value straight into [`PrinterIdentity::new`](../identity/index.md#printeridentity) with no
discovery-layer normalization.

### `supported_models`

```rust
fn supported_models() -> impl Iterator<Item = PrinterModel>
```

**Types:** [`PrinterModel`](#printermodel)

Returns every printer model this crate supports, in table order.

[`PrinterModel::Unknown`](#printermodel) is excluded: it is the fallback for targets this crate does
not recognize, not a supported model. Pair each item with [`quirks`](../quirks/index.md) to
build a capability matrix without matching on variants.

