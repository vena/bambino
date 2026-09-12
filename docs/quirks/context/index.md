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
between `ModelQuirks::supports_ams_remote_drying` and
`PrinterClient::supports_ams_remote_drying` before #240. Requiring the context makes the
stale-answer call impossible to write rather than merely discouraged.

Every field is `Option` because every one of them can be genuinely absent, and absent is not
the same as `false`. The P1 and A1 families send no `fun`/`fun2` at all
(`reference/03_mqtt_telemetry.md`), and firmware version needs a `get_version` round trip
that a caller may never have made. A quirk reading `None` should fall back to its model
default, not treat it as a denial.

Build one from a client with [`PrinterClient::quirk_context`](../../client/index.md#printerclient),
or reach for [`PrinterClient::capabilities`](../../client/index.md#printerclient), which
supplies it for you.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`QuirkContext`](#quirkcontext) | struct | Wire-derived inputs a quirk may consult, all optional. |
| [`firmware_at_least`](#firmware-at-least) | fn | Compares two Bambu firmware version strings, returning true if `have` is at least `want`. |

## Types

### `QuirkContext<'a>`

```rust
struct QuirkContext<'a> {
    pub fun: Option<&'a str>,
    pub fun2: Option<&'a str>,
    pub firmware: Option<&'a str>,
    pub telemetry: Option<&'a crate::types::PrinterTelemetry>,
}
```

Wire-derived inputs a quirk may consult, all optional.

See the [module docs](self) for why these are passed in rather than composed at the call
site.

#### Fields

- **`fun`**: `Option<&'a str>`

  The `fun` capability bitfield, if the printer reported one.
  
  Absent on the P1 and A1 families entirely. Bit 29 is Developer LAN Mode; BambuStudio
  reads a dozen more (`DeviceManager.cpp:4433-4455`) that this crate does not yet.

- **`fun2`**: `Option<&'a str>`

  The `fun2` capability bitfield, if the printer reported one.
  
  Absent on the P1 and A1 families entirely, so a quirk that prefers a `fun2` bit is inert
  on those models and must still carry a sound model default. Single-source: only
  BambuStudio reads this field.

- **`firmware`**: `Option<&'a str>`

  The printer's OTA firmware version (`module[name="ota"].sw_ver`, e.g. `"01.09.00.00"`),
  if a `get_version` response has been seen.
  
  Several capabilities ship in a specific firmware release rather than being inherent to
  the model — remote AMS drying is version-gated on H2D, H2D Pro, H2S, H2C, P2S and X2D for
  exactly this reason.

- **`telemetry`**: `Option<&'a crate::types::PrinterTelemetry>`

  The most recent `print` telemetry object, for quirks that read a live state field.
  
  Distinct from the capability fields above: this is machine *state*
  ([`is_door_open`](../index.md#modelquirks) reads a door bit that flips as
  someone opens the door), not a capability claim.

#### Implementations

- <span id="quirkcontext-empty"></span>`fn empty() -> Self`

  An empty context — every input absent, so every quirk falls back to its model default.

  Use when no telemetry has been seen, or to ask what a model claims about itself before
  any report has arrived.

- <span id="quirkcontext-with-fun"></span>`fn with_fun(self, fun: Option<&'a str>) -> Self`

  Sets the `fun` capability bitfield.

- <span id="quirkcontext-with-fun2"></span>`fn with_fun2(self, fun2: Option<&'a str>) -> Self`

  Sets the `fun2` capability bitfield.

- <span id="quirkcontext-with-firmware"></span>`fn with_firmware(self, firmware: Option<&'a str>) -> Self`

  Sets the OTA firmware version.

- <span id="quirkcontext-with-telemetry"></span>`fn with_telemetry(self, telemetry: Option<&'a PrinterTelemetry>) -> Self` — [`PrinterTelemetry`](../../types/telemetry/report/index.md#printertelemetry)

  Sets the live `print` telemetry object.

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

