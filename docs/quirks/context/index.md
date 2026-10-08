*[bambino](../../index.md) / [quirks](../index.md) / [context](index.md)*

---

# Module `context`

# Quirk Context

The wire-derived inputs a [`ModelQuirks`](../index.md#modelquirks) method may consult.

Some capabilities are a property of the model alone — a printer either has a second
auxiliary fan or it does not, and nothing it reports changes that. Others the machine answers
for itself, either through a capability bitfield or through its firmware version, and for
those a per-model table is a claim about every unit of that model while the report is the
machine in front of you speaking.

Quirks in the second category take a `QuirkContext` rather than composing the report at the
call site. That is deliberate and load-bearing: when the composition lives outside the quirk,
two callers can reach different answers to the same question, which is exactly what happened
between `ModelQuirks::ams_remote_drying_support` and
`PrinterClient::supports_ams_remote_drying` before #240. Requiring the context makes the
stale-answer call impossible to write rather than merely discouraged.

Every field is `Option` because every one of them can be genuinely absent, and absent is not
the same as `false`. The P1 and A1 families send no `fun2` at all
(`reference/03_mqtt_telemetry.md`), and firmware version needs a `get_version` round trip
that a caller may never have made. A quirk reading `None` should fall back to its model
default, not treat it as a denial.

Build one from a client with [`PrinterClient::quirk_context`](../../client/index.md#printerclient),
or reach for [`PrinterClient::capabilities`](../../client/index.md#printerclient), which
supplies it for you.

A field is added when a quirk reads it, not ahead of need.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`QuirkContext`](#quirkcontext) | struct | Wire-derived inputs a quirk may consult, all optional. |
| [`firmware_at_least`](#firmware-at-least) | fn | Compares two Bambu firmware version strings, returning true if `have` is at least `want`. |

## Types

### `QuirkContext<'a>`

```rust
struct QuirkContext<'a> {
    pub fun2: Option<&'a str>,
    pub firmware: Option<&'a str>,
    pub fun: Option<&'a str>,
    pub home_flag: Option<u32>,
    pub xcam_cfg: Option<u32>,
}
```

Wire-derived inputs a quirk may consult, all optional.

See the [module docs](self) for why these are passed in rather than composed at the call
site.

#### Fields

- **`fun2`**: `Option<&'a str>`

  The `fun2` capability bitfield, if the printer reported one.
  
  Absent on the P1 and A1 families entirely, so a quirk that prefers a `fun2` bit is inert
  on those models and must still carry a sound model default. Single-source: only
  BambuStudio reads this field.

- **`firmware`**: `Option<&'a str>`

  The printer's OTA firmware version (`module[name="ota"].sw_ver`, see [`OTA_MODULE_NAME`](../../types/version/index.md#ota-module-name), e.g. `"01.09.00.00"`),
  if a `get_version` response has been seen.
  
  Several capabilities ship in a specific firmware release rather than being inherent to
  the model — remote AMS drying is version-gated on H2D, H2D Pro, H2S, H2C, P2S and X2D for
  exactly this reason.

- **`fun`**: `Option<&'a str>`

  The `fun` capability bitfield, if the printer reported one.
  
  Absent on the P1 and A1 families, like `fun2`. Where present it carries per-setting
  support bits for several `print_option` settings, and BambuStudio lets it override the
  matching `home_flag` bit.

- **`home_flag`**: `Option<u32>`

  The `home_flag` bitfield, if one trusted for capability bits was observed on the current connection.
  
  The capability field every family sends, so it is the only reported support signal on P1
  and A1. On a printer that sends `cfg` it comes from a full status report only, since
  those families' heartbeat frames carry a partial `home_flag`; on P1 and A1, from any
  status frame. From the current connection only, since what the printer supports can
  change across a reboot.

- **`xcam_cfg`**: `Option<u32>`

  The `print.xcam.cfg` detector bitmask, if one has been seen.
  
  Its presence is BambuStudio's AI-monitoring support signal (`ParseDetectionV1_0`). Absence
  means "not seen yet" as often as "unsupported", since `xcam` arrives only in full reports.

#### Implementations

- <span id="quirkcontext-empty"></span>`fn empty() -> Self`

  An empty context — every input absent, so every quirk falls back to its model default.

  Use when no telemetry has been seen, or to ask what a model claims about itself before
  any report has arrived.

- <span id="quirkcontext-with-fun2"></span>`fn with_fun2(self, fun2: Option<&'a str>) -> Self`

  Sets the `fun2` capability bitfield.

- <span id="quirkcontext-with-firmware"></span>`fn with_firmware(self, firmware: Option<&'a str>) -> Self`

  Sets the OTA firmware version.

- <span id="quirkcontext-with-fun"></span>`fn with_fun(self, fun: Option<&'a str>) -> Self`

  Sets the `fun` capability bitfield.

- <span id="quirkcontext-with-home-flag"></span>`fn with_home_flag(self, home_flag: Option<u32>) -> Self`

  Sets the `home_flag` bitfield.

- <span id="quirkcontext-with-xcam-cfg"></span>`fn with_xcam_cfg(self, xcam_cfg: Option<u32>) -> Self`

  Sets the `print.xcam.cfg` bitmask.

#### Trait Implementations

##### `impl Clone for QuirkContext<'a>`

- <span id="quirkcontext-clone"></span>`fn clone(&self) -> QuirkContext<'a>` — [`QuirkContext`](#quirkcontext)

##### `impl Copy for QuirkContext<'a>`

##### `impl Debug for QuirkContext<'a>`

- <span id="quirkcontext-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for QuirkContext<'a>`

- <span id="quirkcontext-default"></span>`fn default() -> QuirkContext<'a>` — [`QuirkContext`](#quirkcontext)


---

## Functions

### `firmware_at_least`

```rust
fn firmware_at_least(have: &str, want: &str) -> bool
```

Compares two Bambu firmware version strings, returning true if `have` is at least `want`.

Versions are dotted numeric quads (`"01.09.00.00"`). Compared component-wise as integers
rather than lexicographically: upstream's zero-padded strings happen to sort correctly as
text, but a single unpadded component (`"1.9.0.0"`) would silently compare wrong, and nothing
guarantees the padding.

Missing trailing components read as `0`, so `"01.09"` and `"01.09.00.00"` are equal. A
component that isn't a number makes the whole comparison `false` — an unparseable version
cannot be shown to meet a minimum, and claiming a capability on a string we failed to read is
the wrong direction to fail.

