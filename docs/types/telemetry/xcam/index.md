*[bambino](../../../index.md) / [types](../../index.md) / [telemetry](../index.md) / [xcam](index.md)*

---

# Module `xcam`

AI failure-detection and print-option settings (`print.xcam`).

Two protocol generations share this one wire object, and BambuStudio branches between them
(`DeviceCore/DevPrintOptions.cpp:39-85`). New-gen firmware sends `xcam.cfg`, a packed integer
bitmask carrying every detector's enable bit and sensitivity level. Old-gen firmware sends
discrete boolean keys instead. Both are modeled here; the accessors prefer `cfg` when present,
mirroring BambuStudio's own precedence.

`xcam.cfg` is **not** the top-level `print.cfg` hex string ([`PrinterTelemetry`](../report/index.md#printertelemetry)) —
same key name, different container, different type. Do not conflate them.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`XcamDetector`](#xcamdetector) | struct | One AI failure detector's decoded state. |
| [`XcamTelemetry`](#xcamtelemetry) | struct | AI detection and print-option settings, nested as `print.xcam` on the wire. |
| [`XcamSensitivity`](#xcamsensitivity) | enum | Sensitivity level attached to an AI failure detector. |

## Types

### `XcamDetector`

```rust
struct XcamDetector {
    pub enabled: bool,
    pub sensitivity: Option<XcamSensitivity>,
}
```

One AI failure detector's decoded state.

#### Fields

- **`enabled`**: `bool`

  Whether the firmware is running this detector.

- **`sensitivity`**: `Option<XcamSensitivity>`

  How eagerly it halts the print, or `None` if the field holds the unassigned value `3`.

#### Trait Implementations

##### `impl Clone for XcamDetector`

- <span id="xcamdetector-clone"></span>`fn clone(&self) -> XcamDetector` — [`XcamDetector`](#xcamdetector)

##### `impl Copy for XcamDetector`

##### `impl Debug for XcamDetector`

- <span id="xcamdetector-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for XcamDetector`

##### `impl PartialEq for XcamDetector`

- <span id="xcamdetector-partialeq-eq"></span>`fn eq(&self, other: &XcamDetector) -> bool` — [`XcamDetector`](#xcamdetector)

### `XcamTelemetry`

```rust
struct XcamTelemetry {
    pub cfg: Option<u32>,
    pub printing_monitor: Option<bool>,
    pub spaghetti_detector: Option<bool>,
    pub print_halt: Option<bool>,
    pub halt_print_sensitivity: Option<String>,
    pub first_layer_inspector: Option<bool>,
    pub buildplate_marker_detector: Option<bool>,
    pub allow_skip_parts: Option<bool>,
    pub extra: std::collections::BTreeMap<String, serde_json::Value>,
}
```

AI detection and print-option settings, nested as `print.xcam` on the wire.

Every field is `Option` because which keys arrive depends on both firmware generation and
model, and because `xcam` appears to be pushall-only — an incremental `msg: 1` frame carries
none of it. Use [`merge_from`](#xcamtelemetry) rather than replacing a cached copy wholesale.

Unmodeled keys survive in [`extra`](#xcamtelemetry): this wire object is still largely uncharted
and model-dependent, so round-tripping a report must not silently drop what it carries.

#### Fields

- **`cfg`**: `Option<u32>`

  New-gen packed detector bitmask. Decode via the detector accessors, not by hand.
  
  Its presence is also what BambuStudio uses to decide the printer supports AI monitoring
  at all — see [`supports_ai_monitoring`](#xcamtelemetry).

- **`printing_monitor`**: `Option<bool>`

  Mid-generation AI-monitoring master switch, superseded by `cfg`.

- **`spaghetti_detector`**: `Option<bool>`

  Oldest-generation AI-monitoring master switch, superseded by `printing_monitor`.

- **`print_halt`**: `Option<bool>`

  Whether a detected failure halts the print rather than only warning.

- **`halt_print_sensitivity`**: `Option<String>`

  AI-monitoring sensitivity as a bare string (`"low"`/`"medium"`/`"high"`).
  
  Old-gen only, and bambuddy reports it as reliably stale (`bambu_mqtt.py:2723`, "it's always
  stale"). Prefer the per-detector sensitivity off `cfg` whenever `cfg` is present.

- **`first_layer_inspector`**: `Option<bool>`

  Whether first-layer inspection is enabled.

- **`buildplate_marker_detector`**: `Option<bool>`

  Whether buildplate-marker detection is enabled.

- **`allow_skip_parts`**: `Option<bool>`

  Per-job or runtime flag whose exact semantics are **not settled** — do not gate anything
  on it.
  
  It is not a model-capability flag (Bambu documents every current model as supporting
  skip-objects, yet this reads `false` in all twelve upstream fixtures carrying it) and not a
  user setting (no such setting exists in Bambu Studio or on the printer). Gating
  [`crate::client::PrinterClient::skip_objects`](../../../client/index.md#printerclient) on it would break skip-objects outright on
  hardware that supports the feature.

- **`extra`**: `std::collections::BTreeMap<String, serde_json::Value>`

  Every `xcam` key this struct does not model, preserved verbatim.
  
  `auto_recovery_step_loss` and `filament_tangle_detect` land here deliberately: bambuddy
  reads them out of `xcam` (`bambu_mqtt.py:2792-2795`, itself commented "tracked locally
  only"), but BambuStudio sources both from `home_flag` instead (bits 4 and 20), and no
  capture shows either inside `xcam`. Same for `ipcam_record`/`timelapse`, which bambino
  models under [`IpcamTelemetry`](../diagnostics/index.md#ipcamtelemetry) from `print.ipcam`.

#### Implementations

- <span id="xcamtelemetry-supports-ai-monitoring"></span>`fn supports_ai_monitoring(&self) -> bool`

  Returns whether this printer supports on-device AI failure monitoring.

  Presence of `cfg` is the signal, matching BambuStudio's `is_support_detect` assignment.

- <span id="xcamtelemetry-ai-monitoring-enabled"></span>`fn ai_monitoring_enabled(&self) -> Option<bool>`

  Returns whether AI print monitoring is enabled, across all three protocol generations.

  Checks `cfg`'s spaghetti-detection bit first, then `printing_monitor`, then
  `spaghetti_detector` — the same precedence BambuStudio applies. `None` means no
  generation's key was present.

- <span id="xcamtelemetry-spaghetti-detection"></span>`fn spaghetti_detection(&self) -> Option<XcamDetector>` — [`XcamDetector`](#xcamdetector)

  Returns spaghetti-detection state decoded from `cfg`, or `None` when `cfg` is absent.

- <span id="xcamtelemetry-purge-chute-pileup-detection"></span>`fn purge_chute_pileup_detection(&self) -> Option<XcamDetector>` — [`XcamDetector`](#xcamdetector)

  Returns purge-chute-pileup detection state decoded from `cfg`, or `None` when `cfg` is absent.

- <span id="xcamtelemetry-nozzle-clumping-detection"></span>`fn nozzle_clumping_detection(&self) -> Option<XcamDetector>` — [`XcamDetector`](#xcamdetector)

  Returns nozzle-clumping detection state decoded from `cfg`, or `None` when `cfg` is absent.

- <span id="xcamtelemetry-air-printing-detection"></span>`fn air_printing_detection(&self) -> Option<XcamDetector>` — [`XcamDetector`](#xcamdetector)

  Returns air-printing detection state decoded from `cfg`, or `None` when `cfg` is absent.

- <span id="xcamtelemetry-buildplate-align-detection"></span>`fn buildplate_align_detection(&self) -> Option<bool>`

  Returns whether buildplate-alignment detection is enabled, or `None` when `cfg` is absent.

- <span id="xcamtelemetry-fod-check"></span>`fn fod_check(&self) -> Option<bool>`

  Returns whether the foreign-object-detection check is enabled, or `None` when `cfg` is absent.

- <span id="xcamtelemetry-displacement-detection"></span>`fn displacement_detection(&self) -> Option<bool>`

  Returns whether displacement detection is enabled, or `None` when `cfg` is absent.

- <span id="xcamtelemetry-merge-from"></span>`fn merge_from(&mut self, incoming: &XcamTelemetry)` — [`XcamTelemetry`](#xcamtelemetry)

  Merges a freshly-parsed `XcamTelemetry` into `self` field-by-field.

  Mirrors `super::diagnostics::IpcamTelemetry::merge_from` and exists for the same reason:
  a frame that carries only part of the object must not blank the rest of a cached copy.
  Present fields overwrite; absent ones leave the cached value alone. `extra` merges per key
  rather than being replaced, so an unmodeled key seen once survives later partial frames.

#### Trait Implementations

##### `impl Clone for XcamTelemetry`

- <span id="xcamtelemetry-clone"></span>`fn clone(&self) -> XcamTelemetry` — [`XcamTelemetry`](#xcamtelemetry)

##### `impl Debug for XcamTelemetry`

- <span id="xcamtelemetry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for XcamTelemetry`

- <span id="xcamtelemetry-default"></span>`fn default() -> XcamTelemetry` — [`XcamTelemetry`](#xcamtelemetry)

##### `impl Deserialize<'de> for XcamTelemetry`

- <span id="xcamtelemetry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for XcamTelemetry`

##### `impl Serialize for XcamTelemetry`

- <span id="xcamtelemetry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `XcamSensitivity`

```rust
enum XcamSensitivity {
    Low,
    Medium,
    High,
}
```

Sensitivity level attached to an AI failure detector.

Applies only to the four camera-based failure detectors (spaghetti, purge-chute pileup,
nozzle clumping, air printing). Unrelated to skip-objects or to `allow_skip_parts`.

#### Variants

- **`Low`**

  Least eager to halt the print.

- **`Medium`**

  Firmware default on every capture observed so far.

- **`High`**

  Most eager to halt the print.

#### Implementations

- <span id="xcamsensitivity-as-str"></span>`fn as_str(&self) -> &'static str`

  Returns the wire spelling BambuStudio uses for this level (`"low"`/`"medium"`/`"high"`).

#### Trait Implementations

##### `impl Clone for XcamSensitivity`

- <span id="xcamsensitivity-clone"></span>`fn clone(&self) -> XcamSensitivity` — [`XcamSensitivity`](#xcamsensitivity)

##### `impl Copy for XcamSensitivity`

##### `impl Debug for XcamSensitivity`

- <span id="xcamsensitivity-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for XcamSensitivity`

- <span id="xcamsensitivity-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for XcamSensitivity`

##### `impl Eq for XcamSensitivity`

##### `impl PartialEq for XcamSensitivity`

- <span id="xcamsensitivity-partialeq-eq"></span>`fn eq(&self, other: &XcamSensitivity) -> bool` — [`XcamSensitivity`](#xcamsensitivity)

##### `impl Serialize for XcamSensitivity`

- <span id="xcamsensitivity-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

