**bambino > models**

# Module: models

## Contents

**Enums**

- [`BambuModel`](#bambumodel) - Enumeration of physical Bambu Lab printer models supported on the local interface.

**Functions**

- [`resolve_model`](#resolve_model) - Resolves the specific printer model using physical serial number prefixes combined with target SSDP model advertisements as a secondary signal.

---

## bambino::models::BambuModel

*Enum*

Enumeration of physical Bambu Lab printer models supported on the local interface.

**Variants:**
- `X1C` - X1 and X1C Series (CoreXY architecture, RTSP-capable)
- `X1E` - X1E (Enterprise CoreXY architecture, wired Ethernet)
- `X2D` - X2D Series (CoreXY architecture, dual auxiliary cooling)
- `A1Mini` - A1 Mini (Constrained bed-slinger, binary camera stream)
- `A1` - A1 (Standard bed-slinger, binary camera stream)
- `A2L` - A2L Series
- `P1P` - P1P (Early CoreXY architecture, binary camera stream)
- `P1S` - P1S (Enclosed CoreXY architecture, binary camera stream)
- `P2S` - P2S Series (RTSP-capable)
- `H2D` - H2D (Dual-nozzle IDEX platform)
- `H2DPro` - H2D Pro (Premium IDEX platform)
- `H2C` - H2C (Vortek tool-changer + fixed hotend, 7 nozzles total)
- `H2S` - H2S (Single-nozzle platform sharing H2 mechanics)
- `Unknown` - Fallback variant for newly released or unrecognized printer targets

**Methods:**

- `fn quirks(self: &Self) -> &'static dyn ModelQuirks` - Returns the [`ModelQuirks`] strategy for this model variant.

**Traits:** Copy, Eq

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> BambuModel`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Hash**
  - `fn hash<__H>(self: &Self, state: & mut __H)`
- **PartialEq**
  - `fn eq(self: &Self, other: &BambuModel) -> bool`



## bambino::models::resolve_model

*Function*

Resolves the specific printer model using physical serial number prefixes combined with target SSDP model advertisements as a secondary signal.

Each H2-series model has a distinct serial prefix confirmed by the Bambu Lab wiki:
`094` = H2D, `093` = H2S, `239` = H2D Pro, `31B` = H2C. When the prefix is
unrecognized, the optional `DevModel` SSDP header provides a fallback path.

```rust
fn resolve_model(serial: &str, dev_model: Option<&str>) -> BambuModel
```



