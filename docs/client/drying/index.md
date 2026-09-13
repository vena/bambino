*[bambino](../../index.md) / [client](../index.md) / [drying](index.md)*

---

# Module `drying`

# Drying Cycle Builder

[`DryingCycle`](#dryingcycle) starts an AMS drying cycle without a nine-argument positional call.

The wire command carries eight parameters, four of which most callers want defaulted
(`humidity`, `rotate_tray`, `cooling_temp`, `close_power_conflict`), and two of which are
adjacent `bool`s that nothing in the type system keeps in order. The builder names each one
at the call site and defaults the rest to what BambuStudio itself sends.

It also gives [`DryingMaterial`](../../types/drying/index.md#dryingmaterial) a seam. The vendor publishes a temperature, a duration and a
cooling temperature per material, and on a positional call a caller has to thread those into
three of nine slots by hand; [`material`](#dryingcycle) sets all three from one
choice.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`DryingCycle`](#dryingcycle) | struct | A drying cycle being configured, returned by [`PrinterClient::dry`](../index.md#printerclient). |

## Types

### `DryingCycle<'a, MqttRawIO, MqttTls, MqttFactory, Timer, FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer, CameraRawIO, CameraTls, CameraFactory>`

```rust
struct DryingCycle<'a, MqttRawIO, MqttTls, MqttFactory, Timer, FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer, CameraRawIO, CameraTls, CameraFactory>
where
    MqttRawIO: AsyncIo,
    MqttTls: TlsConnector<MqttRawIO>,
    MqttFactory: RawStreamFactory<MqttRawIO>,
    Timer: TimerProvider,
    FtpsRawIO: AsyncIo,
    FtpsTls: TlsConnector<FtpsRawIO>,
    FtpsFactory: RawStreamFactory<FtpsRawIO>,
    FtpsTimer: TimerProvider,
    CameraRawIO: AsyncIo,
    CameraTls: TlsConnector<CameraRawIO>,
    CameraFactory: RawStreamFactory<CameraRawIO> {
    // [REDACTED: Private Fields]
}
```

A drying cycle being configured, returned by [`PrinterClient::dry`](../index.md#printerclient).

Nothing is sent until [`send()`](#dryingcycle), which runs the same validation the command
always did — host capability, unit model, temperature range — and publishes.

```rust,ignore
client
    .dry(0)
    .material(DryingMaterial::Petg, AmsUnitModel::Ams2Pro)
    .rotate_tray(true)
    .send()
    .await?;
```

#### Implementations

- <span id="dryingcycle-material"></span>`fn material(self, material: DryingMaterial, unit: AmsUnitModel) -> Self` — [`DryingMaterial`](../../types/drying/index.md#dryingmaterial), [`AmsUnitModel`](../../types/telemetry/ams/index.md#amsunitmodel)

  Fills temperature, duration, cooling temperature and the filament name from the vendor's
  published parameters for `material` on `unit`.

  Sets four fields at once, which is the whole reason this builder exists — the same choice
  on a positional call means threading three numbers and a string into four of nine slots.

  Assumes an idle printer. For a cycle that runs alongside a print, follow with
  [`printing()`](#dryingcycle), which re-reads the lower while-printing column.

  A material with no published parameters for this unit (any unit without a drying chamber)
  leaves the values untouched, so [`send()`](#dryingcycle) still rejects rather than
  publishing a guess.

- <span id="dryingcycle-printing"></span>`fn printing(self, material: DryingMaterial, unit: AmsUnitModel) -> Self` — [`DryingMaterial`](../../types/drying/index.md#dryingmaterial), [`AmsUnitModel`](../../types/telemetry/ams/index.md#amsunitmodel)

  Re-reads the material's parameters from the while-printing column.

  Only meaningful after [`material()`](#dryingcycle); on its own it does nothing, since
  there is no material to re-read. The printing column is lower because the AMS sits in the
  print's thermal envelope.

- <span id="dryingcycle-temp"></span>`fn temp(self, temp: u32) -> Self`

  Sets the drying temperature in °C, overriding any material default.

- <span id="dryingcycle-duration-hours"></span>`fn duration_hours(self, hours: u32) -> Self`

  Sets the cycle duration in whole hours, overriding any material default.

- <span id="dryingcycle-filament"></span>`fn filament(self, filament: &str) -> Self`

  Sets the filament type string sent as the wire `dry_filament` field.

  Free-form by design — the wire field is arbitrary text and BambuStudio sends the tray's
  own `filament_type`. Use this for a material [`DryingMaterial`](../../types/drying/index.md#dryingmaterial) does not name.

- <span id="dryingcycle-humidity"></span>`fn humidity(self, humidity: u32) -> Self`

  Sets the target humidity. `0`, the default, means "firmware default / no target".

- <span id="dryingcycle-rotate-tray"></span>`fn rotate_tray(self, rotate: bool) -> Self`

  Whether to rotate trays during the cycle. Defaults to `false`.

- <span id="dryingcycle-cooling-temp"></span>`fn cooling_temp(self, cooling_temp: i32) -> Self`

  Sets the cooling temperature sent with the command.

  Defaults to [`DEFAULT_COMMAND_COOLING_TEMP`](../../types/drying/index.md#default-command-cooling-temp), and [`material()`](#dryingcycle) sets it
  to that material's *softening* temperature — which is what the wire field actually
  carries, despite the profiles also having a similarly-named
  `filament_dev_drying_cooling_temperature` that BambuStudio never sends.

- <span id="dryingcycle-close-power-conflict"></span>`fn close_power_conflict(self, close: bool) -> Self`

  Whether to override the AMS unit's power-conflict interlock. Defaults to `false`.

  The interlock exists because several drying units on one supply can exceed it; overriding
  it is the caller asserting they know the power situation.

- <span id="dryingcycle-send"></span>`async fn send(self) -> Result<CommandHandle, Error>` — [`CommandHandle`](../command/index.md#commandhandle), [`Error`](../../error/index.md#error)

  Validates and publishes the cycle, returning the command's sequence ID [REF-AMS-DRYER].

  Every gate lives here — this is the only path that publishes `ams_filament_drying`, so it
  is the only place a future check has to be added.

  # Errors

  [`Error::InvalidArgument`](../../error/index.md#error) when no temperature or duration was set. These deliberately
  have no default: silently picking one would start a real heating cycle the caller never
  asked for. Set them with [`material()`](#dryingcycle) or explicitly.

  [`Error::ModelMismatch`](../../error/index.md#error) on a host where
  [`supports_ams_remote_drying()`](../index.md#printerclient) is `false` —
  the printer's own `fun2` bit 5 where it reported one, else the model's rule: never on
  A1/A1 Mini or P1P/P1S, and below the minimum firmware on X1C/P2S/H2D/H2S/H2C. Such
  firmware acks this command `result: success` and silently discards it rather than driving
  the AMS heater.

  [`Error::ModelMismatch`](../../error/index.md#error) also when the addressed unit has no drying chamber — an
  external-spool sentinel (`254`/`255`), or a cached
  [`AmsUnitModel`](../../types/telemetry/ams/index.md#amsunitmodel) whose [`supports_drying`](../../types/telemetry/ams/index.md#amsunitmodel) is `false`.
  These are two independent gates on purpose, matching the pair BambuStudio writes out
  longhand at `Widgets/AMSControl.cpp:348`: the printer must act on the command *and* the
  attached box must have a heater.

  [`Error::ProtocolViolation`](../../error/index.md#error) for an `ams_id` outside the documented address space.

  [`Error::InvalidArgument`](../../error/index.md#error) when the temperature falls outside the unit's
  [`dry_temp_range`](../../types/telemetry/ams/index.md#amsunitmodel). **Both bounds are rejected, not
  clamped**: BambuStudio refuses a temperature below the floor exactly as it refuses one
  above the ceiling (`AMSDryControl.cpp:1186-1199`), and silently rewriting a caller's value
  would start a heating cycle they did not ask for.

  The unit-model gate reads the **cached** AMS snapshot, so a unit this client has never
  observed passes through — the rule [`skip_objects`](../index.md#printerclient)
  established, and for the same reason: an idle printer's incremental pushes frequently
  carry no `ams` block at all, and refusing there would break a caller that connects and
  commands without polling. Call [`poll_telemetry()`](../index.md#printerclient) first
  to arm it. When the unit is unobserved the temperature range falls back to the
  `ams_id`-derived ceiling, the best guess the address alone supports.

#### Trait Implementations

