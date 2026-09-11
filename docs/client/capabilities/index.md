*[bambino](../../index.md) / [client](../index.md) / [capabilities](index.md)*

---

# Module `capabilities`

# Client-Scoped Capabilities

[`Capabilities`](#capabilities) answers "can this printer do X" with the client's own cached telemetry
already supplied.

A [`ModelQuirks`](../../quirks/index.md#modelquirks) method that depends on what the machine
reported takes a [`QuirkContext`](../../quirks/context/index.md#quirkcontext). That is what keeps a single answer to each question — a
caller cannot get a stale reading by forgetting to compose the report, because the context is
required. The cost is that every such call needs a context built and threaded in, and the
obvious call (`client.quirks().foo(..)`) makes the caller assemble it.

This view removes that cost: it holds the model's quirks alongside a context built from the
client's cache, so `client.capabilities().supports_ams_remote_drying()` takes no arguments
and cannot be given the wrong ones.

**Only context-taking quirks are forwarded here.** Everything a model answers on its own —
build volume, fan layout, camera protocol — stays on
[`PrinterClient::quirks()`](../index.md#printerclient), where no telemetry could change
the answer and a bare `&'static dyn ModelQuirks` is the honest shape.

The context is a snapshot taken when the view is created. It reflects what the client had
cached at that moment, so a `Capabilities` held across a
[`poll_telemetry()`](../index.md#printerclient) goes stale — build a fresh one per
question rather than storing it.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`Capabilities`](#capabilities) | struct | Capability answers for one printer, with its cached telemetry already supplied. |

## Types

### `Capabilities<'a>`

```rust
struct Capabilities<'a> {
    // [REDACTED: Private Fields]
}
```

Capability answers for one printer, with its cached telemetry already supplied.

Created by [`PrinterClient::capabilities()`](../index.md#printerclient). See the
[module docs](self) for what is and isn't forwarded here.

#### Implementations

- <span id="capabilities-context"></span>`fn context(&self) -> &QuirkContext<'a>` — [`QuirkContext`](../../quirks/context/index.md#quirkcontext)

  The context these answers are resolved against.

  Useful for asking the same question of a different model, or for seeing which inputs were
  actually available — an answer resolved with `firmware: None` rests on a model rule
  rather than on anything the printer said.

- <span id="capabilities-quirks"></span>`fn quirks(&self) -> &'static dyn ModelQuirks` — [`ModelQuirks`](../../quirks/index.md#modelquirks)

  The underlying model quirks, for the capabilities that take no context.

- <span id="capabilities-supports-ams-remote-drying"></span>`fn supports_ams_remote_drying(&self) -> bool`

  Whether this printer honors `ams_filament_drying` sent over MQTT.

  Resolves the printer's reported `fun2` bit 5 against the model's own rules — never
  supported on A1/A1 Mini and P1P/P1S, firmware-gated on X1C/P2S/H2D/H2S/H2C, allowed
  elsewhere. See
  [`ModelQuirks::supports_ams_remote_drying`](../../quirks/index.md#modelquirks)
  for the sourcing.

  **Gate UI on this rather than on a model check.** It is the same value
  [`start_drying`](../index.md#printerclient) tests, so a control offered on the
  strength of it will not then be refused.

  For a firmware-gated model this reads `false` until
  [`get_version()`](../index.md#printerclient) has been called — an unread version
  cannot be shown to meet a minimum. Call it once after connecting if you intend to ask.

#### Trait Implementations

##### `impl Clone for Capabilities<'a>`

- <span id="capabilities-clone"></span>`fn clone(&self) -> Capabilities<'a>` — [`Capabilities`](#capabilities)

##### `impl Copy for Capabilities<'a>`

##### `impl Debug for Capabilities<'_>`

- <span id="capabilities-debug-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

