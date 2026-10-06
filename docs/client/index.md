*[bambino](../index.md) / [client](index.md)*

---

# Module `client`

# Printer Client

This is the main entry point for most users. [`PrinterClient`](#printerclient) wraps an MQTT session
(and optionally an FTPS connection) into a single coordinated interface with methods
for thermal control, motion, print management, AMS operations, and hardware queries.

The client applies model-aware safety checks automatically:

- **Homing safety** — On CoreXY (bed-on-Z) printers, partial homing commands like
  `G28 Z` can crash the nozzle into the plate. The client enforces bare `G28` only.
- **Z-axis travel limits** — Relative Z moves are clamped to the model's mechanical
  bounds and wrapped in reference-mode push/pop (`M1002`) to prevent bed crashes.
- **Chamber heater guards** — `set_chamber_temperature()` rejects requests on models
  without an active PTC heater (open-frame machines like A1/P1).
- **Fan routing** — Fan commands are directed to the correct controller, including
  the second left-side auxiliary fan (port 10) on models that have one (P2S, X2D, etc.).

## Contents

- [Modules](#modules)
  - [`capabilities`](#capabilities)
  - [`command`](command/index.md)
  - [`drying`](drying/index.md)
  - [`dummy`](dummy/index.md)
  - [`types`](#types)
- [Types](#types)
  - [`PrinterClient`](#printerclient)
- [Constants](#constants)
  - [`DEFAULT_COMMAND_TIMEOUT`](#default-command-timeout)
  - [`DEFAULT_CONNECT_TIMEOUT`](#default-connect-timeout)
  - [`KEEPALIVE_TICK_SECS`](#keepalive-tick-secs)
  - [`SEQUENCE_ID_FLOOR`](#sequence-id-floor)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`capabilities`](#capabilities) | mod | # Client-Scoped Capabilities |
| [`command`](command/index.md) | mod | # Command Handles and Outcomes |
| [`drying`](drying/index.md) | mod | # Drying Cycle Builder |
| [`dummy`](dummy/index.md) | mod | Zero-cost dummy implementations for [`PrinterClient`](#printerclient)'s type parameters. |
| [`types`](#types) | mod | Client-facing helper types (telemetry events, print progress, calibration options). |
| [`PrinterClient`](#printerclient) | struct | High-level client for controlling a Bambu Lab printer. |
| [`DEFAULT_COMMAND_TIMEOUT`](#default-command-timeout) | const | Default command timeout; override with [`PrinterClient::with_command_timeout`](#printerclient). |
| [`DEFAULT_CONNECT_TIMEOUT`](#default-connect-timeout) | const | Default bound on each channel's dial+TLS+handshake; override with [`PrinterClient::with_connect_timeout`](#printerclient). |
| [`KEEPALIVE_TICK_SECS`](#keepalive-tick-secs) | const | How often to call [`PrinterClient::keepalive_tick`]: half the 30s keepalive this client advertises in CONNECT, so a missed tick still leaves margin before the broker's 45s cutoff. |
| [`SEQUENCE_ID_FLOOR`](#sequence-id-floor) | const | Lowest `sequence_id` this client mints, above every range another party on the shared report topic is known to use. |

## Modules

- [`capabilities`](capabilities/index.md#capabilities) — # Client-Scoped Capabilities
- [`command`](command/index.md) — # Command Handles and Outcomes
- [`drying`](drying/index.md) — # Drying Cycle Builder
- [`dummy`](dummy/index.md) — Zero-cost dummy implementations for [`PrinterClient`](#printerclient)'s type parameters.
- [`types`](types/index.md#types) — Client-facing helper types (telemetry events, print progress, calibration options).


---

## Types

### `CalibrationOption`

```rust
struct CalibrationOption();
```

Bitmask flags for selecting hardware calibration routines [REF-MQTT-LIFECYCLE].

Combine flags with `|` (or collect an iterator of them) to trigger several routines at once,
e.g. `CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION`. Only the
named constants can be built, so a value never carries bits no routine owns.

#### Implementations

- <span id="calibrationoption-const-bed-leveling"></span>`const BED_LEVELING: Self`

- <span id="calibrationoption-const-vibration-compensation"></span>`const VIBRATION_COMPENSATION: Self`

- <span id="calibrationoption-const-motor-noise-cancellation"></span>`const MOTOR_NOISE_CANCELLATION: Self`

- <span id="calibrationoption-const-nozzle-height"></span>`const NOZZLE_HEIGHT: Self`

- <span id="calibrationoption-const-heatbed-thermal"></span>`const HEATBED_THERMAL: Self`

- <span id="calibrationoption-empty"></span>`const fn empty() -> Self`

  No routines.

- <span id="calibrationoption-bits"></span>`const fn bits(self) -> u32`

  The wire `option` bitmask.

- <span id="calibrationoption-contains"></span>`const fn contains(self, other: Self) -> bool`

  Whether every routine in `other` is also in `self`.

- <span id="calibrationoption-is-empty"></span>`const fn is_empty(self) -> bool`

  Whether no routine is selected.

#### Trait Implementations

##### `impl BitOr for CalibrationOption`

- <span id="calibrationoption-bitor-type-output"></span>`type Output = CalibrationOption`

- <span id="calibrationoption-bitor"></span>`fn bitor(self, rhs: Self) -> Self`

##### `impl BitOrAssign for CalibrationOption`

- <span id="calibrationoption-bitorassign-bitor-assign"></span>`fn bitor_assign(&mut self, rhs: Self)`

##### `impl Clone for CalibrationOption`

- <span id="calibrationoption-clone"></span>`fn clone(&self) -> CalibrationOption` — [`CalibrationOption`](../types/control/index.md#calibrationoption)

##### `impl Copy for CalibrationOption`

##### `impl Debug for CalibrationOption`

- <span id="calibrationoption-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for CalibrationOption`

- <span id="calibrationoption-default"></span>`fn default() -> CalibrationOption` — [`CalibrationOption`](../types/control/index.md#calibrationoption)

##### `impl Eq for CalibrationOption`

##### `impl FromIterator<CalibrationOption> for CalibrationOption`

- <span id="calibrationoption-fromiterator-from-iter"></span>`fn from_iter<I: IntoIterator<Item = CalibrationOption>>(iter: I) -> Self`

##### `impl Hash for CalibrationOption`

- <span id="calibrationoption-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for CalibrationOption`

- <span id="calibrationoption-partialeq-eq"></span>`fn eq(&self, other: &CalibrationOption) -> bool` — [`CalibrationOption`](../types/control/index.md#calibrationoption)

### `HeaterTemps`

```rust
struct HeaterTemps {
    pub actual: u16,
    pub target: u16,
}
```

One heater's actual and target temperature, in °C.

#### Fields

- **`actual`**: `u16`

  Measured temperature.

- **`target`**: `u16`

  Target temperature; `0` when the heater is off or the wire carries no target.

#### Trait Implementations

##### `impl Clone for HeaterTemps`

- <span id="heatertemps-clone"></span>`fn clone(&self) -> HeaterTemps` — [`HeaterTemps`](#heatertemps)

##### `impl Copy for HeaterTemps`

##### `impl Debug for HeaterTemps`

- <span id="heatertemps-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for HeaterTemps`

- <span id="heatertemps-default"></span>`fn default() -> HeaterTemps` — [`HeaterTemps`](#heatertemps)

##### `impl Eq for HeaterTemps`

##### `impl Hash for HeaterTemps`

- <span id="heatertemps-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for HeaterTemps`

- <span id="heatertemps-partialeq-eq"></span>`fn eq(&self, other: &HeaterTemps) -> bool` — [`HeaterTemps`](#heatertemps)

### `NozzleTemps`

```rust
struct NozzleTemps {
    pub id: u8,
    pub actual: u16,
    pub target: u16,
}
```

One nozzle's temperatures, in °C.

#### Fields

- **`id`**: `u8`

  Nozzle id: `0` on single-nozzle models; `0` (right) and `1` (left) on IDEX.

- **`actual`**: `u16`

  Measured temperature.

- **`target`**: `u16`

  Target temperature.

#### Trait Implementations

##### `impl Clone for NozzleTemps`

- <span id="nozzletemps-clone"></span>`fn clone(&self) -> NozzleTemps` — [`NozzleTemps`](#nozzletemps)

##### `impl Copy for NozzleTemps`

##### `impl Debug for NozzleTemps`

- <span id="nozzletemps-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for NozzleTemps`

##### `impl Hash for NozzleTemps`

- <span id="nozzletemps-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for NozzleTemps`

- <span id="nozzletemps-partialeq-eq"></span>`fn eq(&self, other: &NozzleTemps) -> bool` — [`NozzleTemps`](#nozzletemps)

### `Capabilities<'a>`

```rust
struct Capabilities<'a> {
    // [REDACTED: Private Fields]
}
```

Capability answers for one printer, with its cached telemetry already supplied.

Created by [`PrinterClient::capabilities()`](#printerclient). See the
[module docs](self) for what is and isn't forwarded here.

#### Implementations

- <span id="capabilities-context"></span>`fn context(&self) -> &QuirkContext<'a>` — [`QuirkContext`](../quirks/context/index.md#quirkcontext)

  The context these answers are resolved against.

  Useful for asking the same question of a different model, or for seeing which inputs were
  actually available — an answer resolved with `firmware: None` rests on a model rule
  rather than on anything the printer said.

- <span id="capabilities-quirks"></span>`fn quirks(&self) -> &'static ModelQuirks` — [`ModelQuirks`](../quirks/index.md#modelquirks)

  The underlying model quirks, for the capabilities that take no context.

- <span id="capabilities-supports-ams-remote-drying"></span>`fn supports_ams_remote_drying(&self) -> bool`

  Whether this printer honors `ams_filament_drying` sent over MQTT.

  Resolves the printer's reported `fun2` bit 5 against the model's own rules — never
  supported on A1/A1 Mini, P1P/P1S and X1/X1C, firmware-gated on H2D/H2D Pro/H2S/H2C/P2S/X2D,
  always on A2L, assumed allowed elsewhere. See
  [`ModelQuirks::ams_remote_drying_support`](../quirks/index.md#modelquirks)
  for the sourcing.

  **Gate UI on this rather than on a model check.** It is the same value
  `DryingCycle::send` tests, so a control offered on the
  strength of it will not then be refused.

  On a firmware-gated model an unread version does **not** deny the capability — it falls
  back to the model's answer, and only a version actually read and found older refuses.
  [`connect_mqtt()`](#printerclient) and
  [`connect_all()`](#printerclient) fetch the version for you, so a
  normally-connected client has it; a caller relying on lazy connection gets the
  model-rule answer instead.

- <span id="capabilities-ams-remote-drying-support"></span>`fn ams_remote_drying_support(&self) -> Support` — [`Support`](../quirks/index.md#support)

  Remote-drying support with its provenance attached.

  The same answer as [`supports_ams_remote_drying`](capabilities/index.md#capabilities), plus
  whether it came from the printer ([`Support::Reported`](../quirks/index.md#support)), from its firmware version or a
  model rule ([`Support::Inferred`](../quirks/index.md#support)), or is the default because nothing was known yet
  ([`Support::Assumed`](../quirks/index.md#support)). Use it to tell "this printer can't" from "ask again once
  connected".

- <span id="capabilities-supports-ams-drying-while-printing"></span>`fn supports_ams_drying_while_printing(&self) -> bool`

  Whether an AMS drying cycle can run while a print is in progress.

  Strictly narrower than [`supports_ams_remote_drying`](capabilities/index.md#capabilities),
  and defaults to `false` when the firmware version is unknown — except on X2D and A2L,
  whose earliest firmware already has the feature, so they report `true` before
  `get_version()` completes. See
  [`ModelQuirks::ams_drying_while_printing_support`](../quirks/index.md#modelquirks) for the sourcing.

- <span id="capabilities-ams-drying-while-printing-support"></span>`fn ams_drying_while_printing_support(&self) -> Support` — [`Support`](../quirks/index.md#support)

  Drying-while-printing support with its provenance attached.

  The same answer as
  [`supports_ams_drying_while_printing`](capabilities/index.md#capabilities).

#### Trait Implementations

##### `impl Clone for Capabilities<'a>`

- <span id="capabilities-clone"></span>`fn clone(&self) -> Capabilities<'a>` — [`Capabilities`](capabilities/index.md#capabilities)

##### `impl Copy for Capabilities<'a>`

##### `impl Debug for Capabilities<'_>`

- <span id="capabilities-debug-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

### `CommandHandle`

```rust
struct CommandHandle {
    // [REDACTED: Private Fields]
}
```

Names a command this client published, for matching the printer's answer to it.

Only a [`PrinterClient`](#printerclient) mints one, so a handle always refers to a
`sequence_id` this client actually sent.

#### Implementations

- <span id="commandhandle-command"></span>`fn command(&self) -> &str`

  Returns the wire command name, e.g. `"gcode_line"` or `"ams_filament_drying"`.

- <span id="commandhandle-sequence-id"></span>`fn sequence_id(&self) -> u32`

  Returns the `sequence_id` the command was published under, which the printer echoes back.

- <span id="commandhandle-ack"></span>`fn ack(&self) -> AckExpectation` — [`AckExpectation`](command/index.md#ackexpectation)

  Returns whether an echo is coming for this command.

#### Trait Implementations

##### `impl Clone for CommandHandle`

- <span id="commandhandle-clone"></span>`fn clone(&self) -> CommandHandle` — [`CommandHandle`](command/index.md#commandhandle)

##### `impl Debug for CommandHandle`

- <span id="commandhandle-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for CommandHandle`

##### `impl Hash for CommandHandle`

- <span id="commandhandle-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for CommandHandle`

- <span id="commandhandle-partialeq-eq"></span>`fn eq(&self, other: &CommandHandle) -> bool` — [`CommandHandle`](command/index.md#commandhandle)

### `CommandRefusal`

```rust
struct CommandRefusal {
    pub result: Option<String>,
    pub reason: Option<String>,
    pub err_code: Option<u32>,
    pub errno: Option<i32>,
}
```

The printer's stated reasons for refusing a command.

Every field is as the printer sent it, and any of them may be absent: `result`/`reason` are
the generic pair, `err_code` a device error code, and `errno` a per-command code.

#### Fields

- **`result`**: `Option<String>`

  The echoed `result` string (`"fail"`, `"failed"`, …), when present.

- **`reason`**: `Option<String>`

  The echoed free-text `reason`, e.g. `"mqtt message verify failed"` when LAN developer
  mode is off. `None` when absent or empty.

- **`err_code`**: `Option<u32>`

  Non-zero device error code.
  
  BambuStudio shows it through the same dialog as the `print_error` register
  (`DeviceManager.cpp:3044`), so it decodes the same way — see
  [`decoded_error()`](command/index.md#commandrefusal).

- **`errno`**: `Option<i32>`

  Non-zero per-command code.
  
  For `ams_change_filament`, `-2` means the chamber and `-4` the AMS is too hot to load the
  filament without softening it; the echo's `soft_temp` field, when present, is the limit
  in °C (BambuStudio `DeviceManager.cpp:2993-3016`).

#### Implementations

- <span id="commandrefusal-decoded-error"></span>`fn decoded_error(&self) -> Option<DecodedPrintError>` — [`DecodedPrintError`](../diagnostics/hms/index.md#decodedprinterror)

  Decodes [`err_code`](command/index.md#commandrefusal) into its `MMMM_CCCC` short code.

#### Trait Implementations

##### `impl Clone for CommandRefusal`

- <span id="commandrefusal-clone"></span>`fn clone(&self) -> CommandRefusal` — [`CommandRefusal`](command/index.md#commandrefusal)

##### `impl Debug for CommandRefusal`

- <span id="commandrefusal-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for CommandRefusal`

##### `impl PartialEq for CommandRefusal`

- <span id="commandrefusal-partialeq-eq"></span>`fn eq(&self, other: &CommandRefusal) -> bool` — [`CommandRefusal`](command/index.md#commandrefusal)

### `CommandResolution`

```rust
struct CommandResolution {
    pub handle: CommandHandle,
    pub outcome: CommandOutcome,
}
```

A command paired with its terminal outcome.

#### Fields

- **`handle`**: `CommandHandle`

  The command, as returned when it was published.

- **`outcome`**: `CommandOutcome`

  What became of it.

#### Trait Implementations

##### `impl Clone for CommandResolution`

- <span id="commandresolution-clone"></span>`fn clone(&self) -> CommandResolution` — [`CommandResolution`](command/index.md#commandresolution)

##### `impl Debug for CommandResolution`

- <span id="commandresolution-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for CommandResolution`

##### `impl PartialEq for CommandResolution`

- <span id="commandresolution-partialeq-eq"></span>`fn eq(&self, other: &CommandResolution) -> bool` — [`CommandResolution`](command/index.md#commandresolution)

### `ConnectAllOutcome`

```rust
struct ConnectAllOutcome {
    pub mqtt: Option<Result<(), crate::error::Error>>,
    pub ftps: Option<Result<(), crate::error::Error>>,
    pub camera: Option<Result<(), crate::error::Error>>,
}
```

Per-channel outcome of [`PrinterClient::connect_all`](#printerclient), one field per connection channel.

Each field distinguishes three states, which is the whole reason this is a struct rather
than a plain `Result`:

- `None` — the channel was **not attempted**. Either it was already connected, it was
  never configured (no `.with_ftps()`/`.with_camera()`), or it cannot apply to this
  printer at all (the camera on an RTSPS model). Not an error, and not a failure to
  report to a user.
- `Some(Ok(()))` — connected, and the session is installed on the client.
- `Some(Err(e))` — that channel's own error, including its own
  [`SocketError::TimedOut`](../io/index.md#socketerror) if it alone exceeded the connect timeout.

Every channel is reported independently and none of them short-circuits the others, so
partial success is a normal result rather than an edge case: a client whose MQTT session
came up and whose camera refused the connection has a usable MQTT session, and the
camera error is still visible instead of being swallowed or masking the success.

#### Fields

- **`mqtt`**: `Option<Result<(), crate::error::Error>>`

  MQTT channel result — see the struct docs for what each state means.

- **`ftps`**: `Option<Result<(), crate::error::Error>>`

  FTPS channel result — see the struct docs for what each state means.

- **`camera`**: `Option<Result<(), crate::error::Error>>`

  Camera channel result — see the struct docs for what each state means.

#### Implementations

- <span id="connectalloutcome-errors"></span>`fn errors(&self) -> impl Iterator<Item = (Channel, &Error)>` — [`Error`](../error/index.md#error)

  Every channel that was attempted and failed, with its error.

  A view over the per-channel fields for the "did everything I configured connect?"
  question; channels not attempted (`None`) aren't failures and don't appear.

- <span id="connectalloutcome-into-result"></span>`fn into_result(self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  `Ok(())` if no attempted channel failed, else the first failure in MQTT, FTPS, camera order.

  For callers that treat a partial connect as a failed one. The per-channel fields stay
  available for those that don't.

#### Trait Implementations

##### `impl Clone for ConnectAllOutcome`

- <span id="connectalloutcome-clone"></span>`fn clone(&self) -> ConnectAllOutcome` — [`ConnectAllOutcome`](#connectalloutcome)

##### `impl Debug for ConnectAllOutcome`

- <span id="connectalloutcome-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

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

A drying cycle being configured, returned by [`PrinterClient::dry`](#printerclient).

Nothing is sent until [`send()`](drying/index.md#dryingcycle), which runs the same validation the command
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

- <span id="dryingcycle-material"></span>`fn material(self, material: DryingMaterial, unit: AmsUnitModel) -> Self` — [`DryingMaterial`](../types/drying/index.md#dryingmaterial), [`AmsUnitModel`](../types/telemetry/ams/index.md#amsunitmodel)

  Uses the vendor's published parameters for `material` on `unit` for temperature,
  duration, cooling temperature and the filament name.

  These are defaults, resolved in [`send()`](drying/index.md#dryingcycle): an explicit
  [`temp()`](drying/index.md#dryingcycle), [`duration_hours()`](drying/index.md#dryingcycle),
  [`cooling_temp()`](drying/index.md#dryingcycle) or [`filament()`](drying/index.md#dryingcycle) wins regardless
  of call order. Calling this again replaces the material.

  The cooling temperature sent is the material's
  [`softening_temp`](../types/drying/index.md#dryingmaterial), which is what the wire field carries
  (see its doc). A unit without a drying chamber has no published parameters, so temperature
  and duration stay unset and [`send()`](drying/index.md#dryingcycle) rejects rather than publishing a guess.

- <span id="dryingcycle-printing"></span>`fn printing(self) -> Self`

  Reads the material's defaults from the lower while-printing column, which exists because
  the AMS sits in the print's thermal envelope.

  Affects only defaults from [`material()`](drying/index.md#dryingcycle), in either call order; explicit
  values are sent as set.

- <span id="dryingcycle-temp"></span>`fn temp(self, temp: u32) -> Self`

  Sets the drying temperature in °C, overriding any material default.

- <span id="dryingcycle-duration-hours"></span>`fn duration_hours(self, hours: u32) -> Self`

  Sets the cycle duration in whole hours, overriding any material default.

- <span id="dryingcycle-filament"></span>`fn filament(self, filament: &str) -> Self`

  Sets the filament type string sent as the wire `dry_filament` field.

  Free-form by design — the wire field is arbitrary text and BambuStudio sends the tray's
  own `filament_type`. Use this for a material [`DryingMaterial`](../types/drying/index.md#dryingmaterial) does not name.

- <span id="dryingcycle-humidity"></span>`fn humidity(self, humidity: u32) -> Self`

  Sets the target humidity. `0`, the default, means "firmware default / no target".

- <span id="dryingcycle-rotate-tray"></span>`fn rotate_tray(self, rotate: bool) -> Self`

  Whether to rotate trays during the cycle. Defaults to `false`.

- <span id="dryingcycle-cooling-temp"></span>`fn cooling_temp(self, cooling_temp: u32) -> Self`

  Sets the cooling temperature sent with the command.

  Defaults to the [`material()`](drying/index.md#dryingcycle)'s softening temperature, else
  [`DEFAULT_COMMAND_COOLING_TEMP`](../types/drying/index.md#default-command-cooling-temp), BambuStudio's own fallback.

- <span id="dryingcycle-close-power-conflict"></span>`fn close_power_conflict(self, close: bool) -> Self`

  Whether to override the AMS unit's power-conflict interlock. Defaults to `false`.

  The interlock exists because several drying units on one supply can exceed it; overriding
  it is the caller asserting they know the power situation.

- <span id="dryingcycle-send"></span>`async fn send(self) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Validates and publishes the cycle, returning the published command's [`CommandHandle`](command/index.md#commandhandle) [REF-AMS-DRYER].

  Every gate lives here — this is the only path that publishes `ams_filament_drying`, so it
  is the only place a future check has to be added.

  # Errors

  [`Error::InvalidArgument`](../error/index.md#error) when no temperature or duration was set. These deliberately
  have no default: silently picking one would start a real heating cycle the caller never
  asked for. Set them with [`material()`](drying/index.md#dryingcycle) or explicitly.

  [`Error::ModelMismatch`](../error/index.md#error) on a host where
  [`supports_ams_remote_drying()`](#printerclient) is `false` —
  the printer's own `fun2` bit 5 where it reported one, else the model's rule: never on
  A1/A1 Mini, P1P/P1S or X1/X1C, and below the minimum firmware on
  H2D/H2D Pro/H2S/H2C/P2S/X2D. Such
  firmware acks this command `result: success` and silently discards it rather than driving
  the AMS heater.

  [`Error::ModelMismatch`](../error/index.md#error) also when the addressed unit has no drying chamber — an
  external-spool sentinel (`254`/`255`), an AMS Lite on an A2L (`6`/`16`, an id only that
  heaterless unit takes), or a cached [`AmsUnitModel`](../types/telemetry/ams/index.md#amsunitmodel) whose
  [`supports_drying`](../types/telemetry/ams/index.md#amsunitmodel) is `false`.
  These are two independent gates on purpose, matching the pair BambuStudio writes out
  longhand at `Widgets/AMSControl.cpp:348`: the printer must act on the command *and* the
  attached box must have a heater.

  [`Error::InvalidArgument`](../error/index.md#error) for an `ams_id` outside the documented address space.

  [`Error::InvalidArgument`](../error/index.md#error) when the temperature falls outside the unit's
  [`dry_temp_range`](../types/telemetry/ams/index.md#amsunitmodel). **Both bounds are rejected, not
  clamped**: BambuStudio refuses a temperature below the floor exactly as it refuses one
  above the ceiling (`AMSDryControl.cpp:1186-1199`), and silently rewriting a caller's value
  would start a heating cycle they did not ask for.

  The unit-model gate reads the **cached** AMS snapshot, so a unit this client has never
  observed passes through — the rule [`skip_objects`](#printerclient)
  established, and for the same reason: an idle printer's incremental pushes frequently
  carry no `ams` block at all, and refusing there would break a caller that connects and
  commands without polling. Call [`poll_telemetry()`](#printerclient) first
  to arm it. When the unit is unobserved the temperature range falls back to the
  `ams_id`-derived ceiling, the best guess the address alone supports.

  A temperature above the filament's heat-distortion temperature
  ([`DryingMaterial::heat_distortion_temp`](../types/drying/index.md#dryingmaterial), for a filament string
  [`DryingMaterial::from_filament_type`](../types/drying/index.md#dryingmaterial) recognizes) is sent as asked but logged with
  `log::warn!`. BambuStudio refuses such a cycle on a loaded tray; here an explicit
  [`temp()`](drying/index.md#dryingcycle) is the caller's call, and [`material()`](drying/index.md#dryingcycle) never
  picks one.

#### Trait Implementations

### `PreheatHandles`

```rust
struct PreheatHandles {
    pub airduct: Option<super::CommandHandle>,
    pub chamber: Option<super::CommandHandle>,
}
```

The commands [`preheat_chamber()`](#printerclient) sent; `None` for one it didn't send.

#### Fields

- **`airduct`**: `Option<super::CommandHandle>`

  The `set_airduct` command moving the flap.

- **`chamber`**: `Option<super::CommandHandle>`

  The `M141` setting the chamber target.

#### Trait Implementations

##### `impl Clone for PreheatHandles`

- <span id="preheathandles-clone"></span>`fn clone(&self) -> PreheatHandles` — [`PreheatHandles`](#preheathandles)

##### `impl Debug for PreheatHandles`

- <span id="preheathandles-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for PreheatHandles`

- <span id="preheathandles-default"></span>`fn default() -> PreheatHandles` — [`PreheatHandles`](#preheathandles)

##### `impl Eq for PreheatHandles`

##### `impl PartialEq for PreheatHandles`

- <span id="preheathandles-partialeq-eq"></span>`fn eq(&self, other: &PreheatHandles) -> bool` — [`PreheatHandles`](#preheathandles)

### `PrintProgress`

```rust
struct PrintProgress {
    pub percent: Option<i32>,
    pub remaining_secs: Option<i32>,
    pub layer_num: Option<i32>,
    pub total_layers: Option<i32>,
}
```

Cached print-progress snapshot as of the last-observed telemetry carrying any of these fields (via [`poll_telemetry()`](#printerclient)).

Bundled into one struct rather than four separate cached scalars (unlike `home_flag`/
`gcode_state`/`is_door_open`/`print_error`, which answer four independent questions) because
`mc_percent`, `mc_remaining_time`, `layer_num`, and `total_layers` are always consumed
together as one "how's the print going" question. Each field updates independently and
keeps its last-observed value across a telemetry message that omits it — a `None` field
means "never observed," not "printer reports zero/none."

#### Fields

- **`percent`**: `Option<i32>`

  Motion controller progress percentage (0-100).

- **`remaining_secs`**: `Option<i32>`

  Estimated remaining print duration, in seconds.
  
  Converted on ingest from `mc_remaining_time`, which the wire reports in **minutes**.

- **`layer_num`**: `Option<i32>`

  Active layer progress tracker.

- **`total_layers`**: `Option<i32>`

  Total layers within the sliced print pipeline.

#### Trait Implementations

##### `impl Clone for PrintProgress`

- <span id="printprogress-clone"></span>`fn clone(&self) -> PrintProgress` — [`PrintProgress`](types/index.md#printprogress)

##### `impl Copy for PrintProgress`

##### `impl Debug for PrintProgress`

- <span id="printprogress-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for PrintProgress`

- <span id="printprogress-default"></span>`fn default() -> PrintProgress` — [`PrintProgress`](types/index.md#printprogress)

##### `impl Eq for PrintProgress`

##### `impl PartialEq for PrintProgress`

- <span id="printprogress-partialeq-eq"></span>`fn eq(&self, other: &PrintProgress) -> bool` — [`PrintProgress`](types/index.md#printprogress)

### `PrinterClient<MqttRawIO, MqttTls, MqttFactory, Timer, FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer, CameraRawIO, CameraTls, CameraFactory>`

```rust
struct PrinterClient<MqttRawIO, MqttTls, MqttFactory, Timer, FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer, CameraRawIO, CameraTls, CameraFactory>
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

High-level client for controlling a Bambu Lab printer.

Wraps an MQTT session (connected or lazy) and optionally a [`FtpsClient`](../ftps/client/index.md#ftpsclient) for
SD card access. `MqttRawIO`/`MqttTls`/`MqttFactory` are MQTT's [`TlsConnector`](../io/index.md#tlsconnector)+
[`RawStreamFactory`](../io/index.md#rawstreamfactory) pair (mandatory — every `PrinterClient` needs MQTT);
`FtpsRawIO`/`FtpsTls`/`FtpsFactory` are FTPS's independent pair (defaulted, configured via
[`.with_ftps()`](#printerclient)). Use `PreConnected` for both MQTT slots when wrapping
an already-connected [`MqttClient`](../mqtt/client/index.md#mqttclient) (see [`from_mqtt()`](#printerclient)), or a
platform's `TlsConnector`+`RawStreamFactory` pair (e.g. `TokioTlsConnector`+
`TokioRawStreamFactory`) for lazy connection via [`new()`](#printerclient).

#### Implementations

- <span id="superprinterclient-change-filament"></span>`async fn change_filament(&mut self, ams_id: u8, slot_id: u8, curr_temp: i32, tar_temp: i32, extruder_id: Option<u8>) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Triggers a filament load or unload sequence on a physical AMS unit or external spool [REF-AMS-MAP].

  * `ams_id`: AMS unit index (`0..=3`), AMS-HT unit bus ID (`128..=135`), an A2L-attached
    AMS Lite (`6` as telemetry reports it, or its physical `16`), or `254`/`255` for
    external spool (IDEX Ext-L/Ext-R or single-nozzle, respectively).
  * `slot_id`: Slot within the AMS (`0..=3`), `254` for a single-nozzle external-spool
    load, or `255` to unload/retract (see `ams_change_filament` examples in
    `reference/05_materials_ams.md` §5.3 [REF-AMS-MAP]).
  * `curr_temp` / `tar_temp`: Nozzle temperatures (`-1` = let firmware decide).

  The wire's `target` field is derived, not caller-supplied — see
  [`AmsChangeFilamentRequest::load`](../mqtt/index.md).

  # Errors

  [`Error::InvalidArgument`](../error/index.md#error) for an address no unit answers to.

  `extruder_id` names the hotend to feed — `Some(0)` for right/main, `Some(1)` for
  left/deputy. Pass `None` on any printer without a Filament Track Switch, where the
  firmware derives the hotend from the AMS's own extruder binding and the payload is
  byte-identical to the pre-FTS form. On a machine *with* a switch (an H2C, for example)
  every AMS reports its extruder as "not fixed" (`0xE`) and can reach either hotend
  through the switch, so a `None` here means the firmware has nothing to derive from and
  **discards the command in silence** — load and unload simply do nothing.

- <span id="superprinterclient-supports-ams-remote-drying"></span>`fn supports_ams_remote_drying(&self) -> bool`

  Whether this printer supports remote AMS drying — the printer-side half of the gate.

  Supplies this client's [`quirk_context()`](#printerclient) to
  [`ModelQuirks::ams_remote_drying_support`](../quirks/index.md#modelquirks),
  which resolves the printer's own reported answer against the model's rules. This is the
  call to gate a UI on: it is the identical value a drying cycle's
  `send()` checks, so a control offered on the strength
  of it cannot then be refused.

  Shorthand for
  `capabilities().supports_ams_remote_drying()`;
  see there for how the answer is resolved and what has to be polled first.

- <span id="superprinterclient-ams-unit-model"></span>`fn ams_unit_model(&self, ams_id: u8) -> Option<AmsUnitModel>` — [`AmsUnitModel`](../types/telemetry/ams/index.md#amsunitmodel)

  Looks up the cached [`AmsUnitModel`](../types/telemetry/ams/index.md#amsunitmodel) for the unit at `ams_id`, if one has been observed.

  `None` covers three distinct cases that all mean the same thing to a caller — no AMS
  snapshot has arrived yet, no unit answers to this address, or the unit reports a type
  newer than this crate knows — and all three read as "don't assume a capability".

  Matches on the unit's own `id`, which is already normalized on deserialize (the A2L's AMS
  Lite reports physical `16` and is stored as `6`), so a caller-supplied physical `16` is
  normalized the same way before comparing.

- <span id="superprinterclient-scan-rfid"></span>`async fn scan_rfid(&mut self, ams_id: u8, slot_id: u8) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Scans proprietary RFID tag properties on a specific AMS tray [REF-AMS-MAP].

  * `ams_id`: AMS unit index (`0..=3`), AMS-HT unit bus ID (`128..=135`), or an A2L-attached
    AMS Lite (`6` or its physical `16`; sent as `16`). Only
    documented against a physical bus unit (`reference/03_mqtt_telemetry.md`
    `ams_get_rfid` example) — external spools have no RFID reader node, so no
    external-spool sentinel value applies here.
  * `slot_id`: Slot within the AMS (`0..=3`).

  **Two commands, chosen by firmware payload format** — BambuStudio's selector
  (`StatusPanel.cpp:5376-5399`). The MQTT transport is the same either way. A printer whose
  telemetry shows BambuStudio's "np" format
  (`PrinterTelemetry::reports_np_format`)
  gets `ams_get_rfid`; one whose `push_status` frames don't gets the G-code
  `M620 R<global tray>` (`command_ams_refresh_rfid`, `DeviceManager.cpp:1738-1743`), since
  older firmware acks `ams_get_rfid` and does nothing. Before any telemetry has arrived the
  format is unknown and `ams_get_rfid` is sent — call
  [`poll_telemetry()`](#printerclient) first on older firmware.

  **Refused while filament is loaded to the toolhead**, because the scan feeds filament to
  the reader: returns [`Error::InvalidState`](../error/index.md#error) when the cached `ams.tray_now` is anything but
  `255` (unloaded), matching bambuddy (`bambu_mqtt.py:7601-7615`). BambuStudio refuses the
  same case with a dialog (`StatusPanel.cpp:5386-5391`). An unobserved `tray_now` passes.

- <span id="superprinterclient-select-k-profile"></span>`async fn select_k_profile(&mut self, ams_id: u8, slot_id: u8, cali_idx: i32, filament_id: &str, nozzle_diameter: &str) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Binds a stored K-profile calibration entry to an AMS material slot [REF-AMS-MAP].

  **IDEX External-Spool Addressing Cheat-Sheet:** this command (`extrusion_cali_sel`)
  uses different `ams_id`/`tray_id` external-spool addressing than
  `ams_filament_setting` (filament configuration) — do not reuse one rule for both:
  * `extrusion_cali_sel` (this command) — Single-Nozzle Platforms: `ams_id: 254` /
    `tray_id: 254`. Dual-Nozzle IDEX: Ext-L requires `ams_id: 254` / `tray_id: 254`;
    Ext-R requires `ams_id: 255` / `tray_id: 255`. **Warning:** targeting the wrong
    address for Ext-R on IDEX machines mis-routes the pressure advance profile to
    the left carriage (Ext-L) EEPROM, leaving the primary right carriage completely
    uncalibrated.
  * `ams_filament_setting` — Single-Nozzle Platforms: `ams_id: 255` / `tray_id: 254`.
    Dual-Nozzle IDEX: both Ext-L (`ams_id: 254`) and Ext-R (`ams_id: 255`) require
    `tray_id: 254`.

  Takes the unit and its **local** slot; the global `tray_id` the wire carries is derived by
  [`CaliSelAddress`](../diagnostics/kprofile/index.md#caliseladdress). Taking the global id
  from the caller used to let `(2, 1)` bind unit 0's tray 1 while claiming unit 2 (#397).

- <span id="superprinterclient-get-version"></span>`async fn get_version(&mut self) -> Result<VersionInfo, Error>` — [`VersionInfo`](../types/version/index.md#versioninfo), [`Error`](../error/index.md#error)

  Queries the printer's expansion bus version database and returns typed module info.

  Sends a `get_version` command and waits for the response, buffering any
  telemetry messages that arrive in the interim. Wrap in a platform-specific
  timeout if you need a shorter deadline than the command timeout.

- <span id="superprinterclient-get-k-profiles"></span>`async fn get_k_profiles(&mut self, filament_id: Option<&str>, nozzle_diameter: Option<&str>) -> Result<ExtrusionCaliGetResponse, Error>` — [`ExtrusionCaliGetResponse`](../diagnostics/kprofile/index.md#extrusioncaligetresponse), [`Error`](../error/index.md#error)

  Requests a dump of the printer's stored K-profile calibration database [REF-DIAG-KPROF].

  Automatically sends a priming request on the first call after connection, because the
  firmware silently ignores the initial `extrusion_cali_get` command. Use
  `set_k_profile_primed(true)` to skip the automatic prime if you handle it yourself.

  `nozzle_diameter` scopes the query to one diameter; that reply is the complete table for
  the diameter *requested*, not a reflection of installed hardware. Passing `None` sends the
  bare request, which BambuStudio relies on to return the full table on multi-extruder and
  nozzle-rack machines. On other machines BambuStudio calls once per diameter the model
  supports and merges the results — see [REF-DIAG-KPROF].

  `filament_id` scopes the query the same way, to a single filament preset id. `None` omits
  the field entirely; `Some("")` is the "every filament" form `reference/07_diagnostics_hms.md`
  documents, and is distinct from omitting it. Exposed here because the alternative was to
  bypass this wrapper and hand-manage the priming quirk through
  [`set_k_profile_primed()`](#printerclient), forfeiting the automatic priming
  this method exists to guarantee.

- <span id="superprinterclient-set-k-profile-primed"></span>`fn set_k_profile_primed(&mut self, primed: bool)`

  Controls whether `get_k_profiles()` sends an automatic priming request.

  Set to `true` to skip the firmware priming quirk — useful if you handle priming
  yourself or target firmware that does not require it.

- <span id="superprinterclient-attach-camera"></span>`async fn attach_camera(&mut self, camera: BinaryCameraStream<<CameraTls as >::Stream>)` — [`BinaryCameraStream`](../camera/binary/index.md#binarycamerastream), [`TlsConnector`](../io/index.md#tlsconnector)

  Injects a pre-connected [`BinaryCameraStream`](../camera/binary/index.md#binarycamerastream) directly.

  Use this for test mocks or Embassy where the caller manages the camera
  connection. For lazy connection, use [`.with_camera()`](#printerclient). On a
  [`from_mqtt()`](#printerclient) client, whose camera type parameters are placeholders,
  use [`.with_attached_camera()`](#printerclient) instead.

  A session already in the slot is disconnected first, as
  [`disconnect_camera()`](#printerclient) does. The attached stream is closed on a
  later disconnect only if a connector is configured (`.with_camera()` or
  `.with_attached_camera()`); without one it is dropped without `close_notify`.

- <span id="superprinterclient-camera"></span>`async fn camera(&mut self) -> Result<&mut BinaryCameraStream<<CameraTls as >::Stream>, Error>` — [`BinaryCameraStream`](../camera/binary/index.md#binarycamerastream), [`TlsConnector`](../io/index.md#tlsconnector), [`Error`](../error/index.md#error)

  Returns direct access to the underlying [`BinaryCameraStream`](../camera/binary/index.md#binarycamerastream), auto-connecting if needed.

  Requires prior camera configuration via [`.with_camera()`](#printerclient),
  [`.attach_camera()`](#printerclient) or
  [`.with_attached_camera()`](#printerclient). Returns `Error::ModelMismatch`
  immediately for RTSPS models — see `ensure_camera()`'s doc
  comment.

- <span id="superprinterclient-read-camera-frame"></span>`async fn read_camera_frame(&mut self) -> Result<Vec<u8>, Error>` — [`Error`](../error/index.md#error)

  Reads and returns the next camera frame, auto-connecting (and authenticating) if needed.

  Bounds the read against `self.timer` (see
  `BinaryCameraStream::read_next_frame_with_timer`), mirroring
  [`poll_telemetry()`](#printerclient)'s relationship to
  [`.mqtt()`](#printerclient).

- <span id="superprinterclient-disconnect-camera"></span>`async fn disconnect_camera(&mut self)`

  Disconnects the camera session, if one exists, and clears it from the client.

  A dead stream (`ConnectionReset`, bad markers, etc.) would otherwise leave `self.camera`
  stuck `Some(...)` forever, since `ensure_camera()`'s `is_some()` short-circuit would
  keep handing back the same broken stream.

  There is no protocol-level teardown on `BinaryCameraStream` to call, but the TLS session
  underneath it is shut down properly before the slot is cleared —
  [`TlsConnector::close`](../io/index.md#tlsconnector) sends `close_notify` so the
  printer sees an orderly teardown rather than a truncated stream (GitHub issue #293).
  Failure there is logged and ignored: the connection is going away either way.

  Idempotent, and reconnectable like [`disconnect_ftps()`](#printerclient):
  `ensure_camera()` never consumes `camera_config` (nothing is moved out of it), so the
  next camera call redials.

- <span id="superprinterclient-connect-mqtt"></span>`async fn connect_mqtt(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Eagerly establishes the MQTT connection.

  Idempotent — returns `Ok(())` if already connected.

- <span id="superprinterclient-is-mqtt-connected"></span>`fn is_mqtt_connected(&self) -> bool`

  Returns whether the MQTT session is currently established.

- <span id="superprinterclient-attach-mqtt"></span>`async fn attach_mqtt(&mut self, mqtt: MqttClient<<MqttTls as >::Stream>)` — [`MqttClient`](../mqtt/client/index.md#mqttclient), [`TlsConnector`](../io/index.md#tlsconnector)

  Injects a pre-connected [`MqttClient`](../mqtt/client/index.md#mqttclient) directly.

  Use this for test mocks or Embassy where the caller manages the MQTT connection,
  mirroring [`attach_camera()`](#printerclient)/
  [`attach_ftps()`](#printerclient).

  A session already in the slot is closed first, as
  [`disconnect_mqtt()`](#printerclient) does. The new one then gets every step a
  session this client dials itself gets: the connection-scoped cache is invalidated, the
  sequence counter is reseeded (under a timer with a real clock), and a `pushall` refills
  the cache.

- <span id="superprinterclient-disconnect-mqtt"></span>`async fn disconnect_mqtt(&mut self)`

  Disconnects the MQTT session, if one exists, and clears it from the client.

  There is no protocol-level (MQTT DISCONNECT) teardown on `MqttClient` to call, but the
  TLS session underneath it is shut down properly before the slot is cleared —
  [`TlsConnector::close`](../io/index.md#tlsconnector) sends `close_notify` so the
  printer sees an orderly teardown rather than a truncated stream (GitHub issue #293).
  Failure there is logged and ignored: the connection is going away either way. Dropping
  the client is what releases the session's memory — on MbedTLS/embassy that is ~48 KB,
  freed in `Drop`, not in `close()`. Without this, a dead stream (a
  [`tick_zombie_check()`](../mqtt/index.md)-detected
  zombie, a transport error) left `self.mqtt` stuck `Some(...)` forever, since
  `ensure_mqtt()`'s `is_some()` short-circuit kept handing back the same broken
  connection with no supported redial path.

  Idempotent. Reconnecting requires [`.attach_mqtt()`](#printerclient) with a fresh
  `MqttClient` for a [`from_mqtt()`](#printerclient)-built client — its
  `PreConnected` factory's `dial()` always errors, so `ensure_mqtt()`'s lazy-dial fallback
  only recovers a `new()`-built client, never one built via `from_mqtt()`.

- <span id="superprinterclient-with-timer"></span>`fn with_timer<NewTimer: TimerProvider>(self, timer: NewTimer) -> PrinterClient<MqttRawIO, MqttTls, MqttFactory, NewTimer, FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer, CameraRawIO, CameraTls, CameraFactory>` — [`PrinterClient`](#printerclient)

  Sets a [`TimerProvider`](../io/index.md#timerprovider) for wall-clock command-response timeouts.

  Consuming builder — works on both [`new()`](#printerclient) and
  [`from_mqtt()`](#printerclient) construction paths. On a client already holding
  a session (`from_mqtt()`), the sequence counter is reseeded from the new timer's clock,
  which `from_mqtt()` itself cannot do under its `DummyTimer`.

- <span id="superprinterclient-with-mqtt-port"></span>`fn with_mqtt_port(self, port: u16) -> Self`

  Overrides the default MQTT port (8883).

- <span id="superprinterclient-with-connect-timeout"></span>`fn with_connect_timeout(self, timeout: Option<Duration>) -> Self`

  Sets the bound on each channel's dial+TLS+handshake; `None` disables it.

  The default is [`DEFAULT_CONNECT_TIMEOUT`](#default-connect-timeout) (10s). Needs a
  real clock ([`with_timer()`](#printerclient)) to fire. Keeps the type parameters;
  chain onto any construction path.

  This is the only connect budget on every backend. `EspIdfTlsConnector` has its own
  handshake deadline for direct use, but it is disabled unless set, so it doesn't cap this
  one; leave it unset under `PrinterClient`.

- <span id="superprinterclient-with-ftps"></span>`fn with_ftps<NewFtpsRawIO, NewFtpsTls, NewFtpsFactory, NewFtpsTimer>(self, tls: NewFtpsTls, factory: NewFtpsFactory, timer: NewFtpsTimer) -> PrinterClient<MqttRawIO, MqttTls, MqttFactory, Timer, NewFtpsRawIO, NewFtpsTls, NewFtpsFactory, NewFtpsTimer, CameraRawIO, CameraTls, CameraFactory>` — [`PrinterClient`](#printerclient)

  Configures FTPS for lazy connection on first storage method call.

  Consuming builder — changes the `FtpsRawIO`, `FtpsTls`, `FtpsFactory`, and `FtpsTimer`
  type parameters. The FTPS [`TlsConnector`](../io/index.md#tlsconnector) is independent from MQTT's (some models
  require different TLS settings for FTPS, e.g. `TlsVersions::Tls12Only`). `timer` is
  constructed fresh by the caller (e.g. `TokioTimer::new()`) — `FtpsClient` owns it
  independently of `PrinterClient`'s own `Timer`, since `PrinterClient::ftps()` hands
  out direct `&mut FtpsClient` access rather than mediating every FTPS call itself,
  so there's no call site to thread `self.timer` through the way MQTT/camera do.

  Call [`disconnect_ftps()`](#printerclient) first on a client with a
  connected FTPS session: this builder is synchronous and cannot close it, so the session is
  dropped without `close_notify` (see `.claude/rules/tls-session-teardown.md`).

  On a [`from_mqtt()`](#printerclient) client, which has no ip or access code to
  dial with, the first FTPS call returns [`Error::NotConfigured`](../error/index.md#error); use
  [`with_attached_ftps()`](#printerclient) there.

- <span id="superprinterclient-with-ftps-port"></span>`fn with_ftps_port(self, port: u16) -> Self`

  Overrides the default FTPS port (990).

- <span id="superprinterclient-with-ftps-allow-unverified-tls-1-2"></span>`fn with_ftps_allow_unverified_tls_1_2(self, allow: bool) -> Self`

  Overrides the default `false` for `FtpsClient`'s TLS-1.2-enforcement bypass.

  Rarely needed. Every backend can now *report* the negotiated version, so
  `require_tls_1_2_if_enforced` passes on its own whenever a P2S/X2D actually negotiates
  TLS 1.2 — see `src/ftps/CLAUDE.md` and `src/io/CLAUDE.md`. What differs between
  backends is the ability to *cap* the peer at 1.2: only `tokio` has that knob
  (`TlsVersions::Tls12Only` on `TokioTlsConnector::verified`/`unverified`). `esp-idf` and
  `embassy` set no maximum
  version — upstream exposes none on ESP-IDF, and this crate sets only `min_version` on
  embassy — so against a printer that insisted on TLS 1.3 they fail closed, and this
  bypass is the only way through. It skips the version check only; certificate
  verification is configured on the `TlsConnector` and is unaffected.
  Keeps the type parameters; chain onto any construction path.

- <span id="superprinterclient-connect-ftps"></span>`async fn connect_ftps(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Eagerly establishes the FTPS connection.

  Idempotent — returns `Ok(())` if already connected.

- <span id="superprinterclient-is-ftps-connected"></span>`fn is_ftps_connected(&self) -> bool`

  Returns whether a usable FTPS session is established (one that a transport failure poisoned is not).

- <span id="superprinterclient-connect-camera"></span>`async fn connect_camera(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Eagerly establishes the camera connection.

  Idempotent — returns `Ok(())` if already connected.

- <span id="superprinterclient-is-camera-connected"></span>`fn is_camera_connected(&self) -> bool`

  Returns whether the camera session is currently established.

- <span id="superprinterclient-connect-all"></span>`async fn connect_all(&mut self) -> ConnectAllOutcome` — [`ConnectAllOutcome`](#connectalloutcome)

  Connects every configured channel concurrently, overlapping their TLS handshakes.

  Same end state as calling [`connect_mqtt()`](#printerclient),
  [`connect_ftps()`](#printerclient) and [`connect_camera()`](#printerclient)
  in sequence, but the three dial+TLS sequences are interleaved on this task instead of
  running one after another, and the result is reported per channel via
  [`ConnectAllOutcome`](#connectalloutcome) rather than as a single `Result`.

  # Which channels are attempted

  Configuration *is* the selection — there is no channel argument. A channel is dialled
  only when it is configured and applicable, and is otherwise reported as `None`
  (not attempted) rather than as an error:

  - **MQTT** — attempted unless already connected.
  - **FTPS** — attempted only if `.with_ftps()` supplied a config and it is not already
    connected. A consumer that never configured FTPS simply gets `None`.
  - **Camera** — attempted only if `.with_camera()` supplied a config *and* the model's
    [`CameraProtocol`](../camera/index.md#cameraprotocol) is `BinaryJpeg`. Note the deliberate difference from
    [`connect_camera()`](#printerclient), which returns
    [`Error::ModelMismatch`](../error/index.md#error) on an RTSPS model: here an RTSPS camera is a channel
    that does not apply to this printer, not a failure, so reporting it as an error
    would hand every P2S/X2D consumer a guaranteed `Err` on an otherwise clean connect.
    Those models use `camera::rtsps::build_rtsps_url()` and have no client-managed
    connection to establish.

  # Timeouts

  The connect timeout is applied **per channel**, matching the individual
  `ensure_*` methods, so a slow or unreachable camera can never cause an otherwise
  healthy MQTT dial to be reported as timed out. Because the channels run concurrently
  the worst-case wall clock for the whole call is still one timeout, not three. A
  shared deadline around the joined future was rejected precisely because it cannot
  express partial success: it would discard an already-completed MQTT session when a
  hung camera pushed the *combined* future past the deadline.

  # Cost

  This future holds all three handshakes alive at once, so it costs roughly 4x the
  stack of connecting individually — measured on an ESP32-C6, 20296 bytes against a
  4808-byte peak for the largest single `connect_*`, in exchange for ~1.3s. Irrelevant
  on desktop; on Embassy, where task stacks are sized up front, connect one at a time
  if stack is tighter than time.

  # Failure isolation

  A channel that fails installs nothing and leaves its config intact, so a later
  `connect_*`/`ensure_*` call retries it — the same "a failed attempt must not
  permanently report 'not configured'" rule the sequential paths follow. One channel's
  failure never prevents another from being installed.

  # Why this exists

  A handshake against a Bambu printer is dominated by waiting on the peer (~800ms,
  measured on an ESP32-C6 against a P1S and reproduced from a laptop on the same LAN,
  so it is the printer being slow rather than the client). That wait overlaps freely;
  only the smaller per-handshake compute term still serialises on a single core.
  Connecting three channels therefore costs roughly one peer wait plus three compute
  terms instead of three of each. TLS session resumption would have attacked the peer
  term directly, but the printer declines to resume its own session IDs, so overlapping
  the waits is the available lever.

- <span id="superprinterclient-with-camera"></span>`fn with_camera<NewCameraRawIO, NewCameraTls, NewCameraFactory>(self, tls: NewCameraTls, factory: NewCameraFactory) -> PrinterClient<MqttRawIO, MqttTls, MqttFactory, Timer, FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer, NewCameraRawIO, NewCameraTls, NewCameraFactory>` — [`PrinterClient`](#printerclient)

  Configures the binary-JPEG camera for lazy connection on first camera method call.

  Consuming builder — changes the `CameraRawIO`, `CameraTls`, and `CameraFactory` type
  parameters. Independent of MQTT's and FTPS's connectors, mirroring `.with_ftps()`.

  Call [`disconnect_camera()`](#printerclient) first on a client with a connected
  camera session, for the same reason as `.with_ftps()`. On a
  [`from_mqtt()`](#printerclient) client the first camera call returns
  [`Error::NotConfigured`](../error/index.md#error); use [`with_attached_camera()`](#printerclient) there.

- <span id="superprinterclient-with-attached-camera"></span>`fn with_attached_camera<NewCameraRawIO, NewCameraTls>(self, tls: NewCameraTls, camera: BinaryCameraStream<<NewCameraTls as >::Stream>) -> PrinterClient<MqttRawIO, MqttTls, MqttFactory, Timer, FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer, NewCameraRawIO, NewCameraTls, super::PreConnected<NewCameraRawIO>>` — [`BinaryCameraStream`](../camera/binary/index.md#binarycamerastream), [`TlsConnector`](../io/index.md#tlsconnector), [`PrinterClient`](#printerclient)

  Installs a camera stream the caller connected, changing the camera type parameters to match it.

  The attach path for a [`from_mqtt()`](#printerclient) client: its camera slots
  are fixed to placeholder types, so [`attach_camera()`](#printerclient) cannot take a
  real stream there, and [`with_camera()`](#printerclient) needs the ip/access code such
  a client lacks. `tls` is the connector that produced the stream; it is kept so
  [`disconnect_camera()`](#printerclient) can send `close_notify`. There is no
  dialer, so after a disconnect the next camera call returns
  [`SocketError::NotConnected`](../io/index.md#socketerror) until a camera is
  attached again.

  Call [`disconnect_camera()`](#printerclient) first on a client with a connected
  camera session, for the same reason as `.with_ftps()`.

- <span id="superprinterclient-with-attached-ftps"></span>`fn with_attached_ftps<NewFtpsRawIO, NewFtpsTls, NewFtpsFactory, NewFtpsTimer>(self, ftps_client: FtpsClient<NewFtpsRawIO, NewFtpsTls, NewFtpsFactory, NewFtpsTimer>) -> PrinterClient<MqttRawIO, MqttTls, MqttFactory, Timer, NewFtpsRawIO, NewFtpsTls, NewFtpsFactory, NewFtpsTimer, CameraRawIO, CameraTls, CameraFactory>` — [`FtpsClient`](../ftps/client/index.md#ftpsclient), [`PrinterClient`](#printerclient)

  Installs an FTPS client the caller connected, changing the FTPS type parameters to match it.

  The FTPS counterpart of [`with_attached_camera()`](#printerclient), for a
  [`from_mqtt()`](#printerclient) client whose FTPS slots are placeholders.
  [`FtpsClient`](../ftps/client/index.md#ftpsclient) carries its own connector, so
  [`disconnect_ftps()`](#printerclient) closes it as usual. No FTPS
  configuration is kept, so after a disconnect [`ftps()`](#printerclient) reports FTPS
  as not configured until a client is attached again.

  Call [`disconnect_ftps()`](#printerclient) first on a client with a
  connected FTPS session, for the same reason as `.with_ftps()`.

- <span id="superprinterclient-with-camera-port"></span>`fn with_camera_port(self, port: u16) -> Self`

  Overrides the default camera port (6000, binary-JPEG only).

- <span id="superprinterclient-with-camera-max-frame-size"></span>`fn with_camera_max_frame_size(self, bytes: usize) -> Self`

  Overrides the default maximum accepted camera frame size (see `BinaryCameraStream::with_max_frame_size`).

- <span id="superprinterclient-dry"></span>`fn dry(&mut self, ams_id: u8) -> crate::client::DryingCycle<'_, MqttRawIO, MqttTls, MqttFactory, Timer, FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer, CameraRawIO, CameraTls, CameraFactory>` — [`DryingCycle`](drying/index.md#dryingcycle)

  Configures a drying cycle for the unit at `ams_id`, to be sent with
  `send()`.

  The way to start drying. Names each parameter at the call site instead of ordering nine
  of them, defaults the four most callers don't set, and lets
  `material()` fill temperature, duration and
  cooling temperature from one choice:

  ```rust,ignore
  client
      .dry(0)
      .material(DryingMaterial::Petg, AmsUnitModel::Ams2Pro)
      .rotate_tray(true)
      .send()
      .await?;
  ```

  Nothing is published until `send()`, which is where
  every gate runs — host capability, AMS addressing, the external-spool sentinels, the
  attached unit's model, and the temperature range [REF-AMS-DRYER].

- <span id="superprinterclient-stop-drying"></span>`async fn stop_drying(&mut self, ams_id: u8) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Terminates an active dry-chamber heating cycle on an AMS unit [REF-AMS-DRYER].

  Mirrors BambuStudio's `CtrlAmsStopDrying` (`DevFilaSystemCtrl.cpp:40-53`) exactly —
  every field zeroed/defaulted, only `mode: 0` (`Off`) is meaningful.

- <span id="superprinterclient-set-fan-speed"></span>`async fn set_fan_speed(&mut self, fan: FanTarget, speed_percent: u8) -> Result<CommandHandle, Error>` — [`FanTarget`](../types/control/index.md#fantarget), [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Sets the speed of a targeted onboard fan as a percentage (0 to 100) [REF-CLIM-FANS].

  Translates the percentage to the 0-255 PWM range of `M106`, on the fan's own port
  ([`FanTarget::write_port`](../types/control/index.md#fantarget)).

  # Errors

  [`Error::ModelMismatch`](../error/index.md#error) when this model doesn't have the fan.

- <span id="superprinterclient-set-led"></span>`async fn set_led(&mut self, node: LedNode, turn_on: bool) -> Result<CommandHandle, Error>` — [`LedNode`](../types/control/index.md#lednode), [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Turns an LED fixture on or off [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-set-airduct-mode"></span>`async fn set_airduct_mode(&mut self, mode: AirductMode) -> Result<CommandHandle, Error>` — [`AirductMode`](../mqtt/commands/hardware/index.md#airductmode), [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Configures the active climate airduct damper mode [REF-MQTT-LIFECYCLE].

  Supported on models with controllable airduct dampers (H2 series, P2S, X2D).

- <span id="superprinterclient-set-prompt-sound"></span>`async fn set_prompt_sound(&mut self, enable_sound: bool) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Configures whether the printer's speakers emit prompt notification sounds [REF-MQTT-LIFECYCLE].

  Supported on models with onboard speakers (A1, A1 Mini, A2L).

- <span id="superprinterclient-set-buzzer-mode"></span>`async fn set_buzzer_mode(&mut self, mode: BuzzerMode) -> Result<CommandHandle, Error>` — [`BuzzerMode`](../types/control/index.md#buzzermode), [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Modifies active alarm or attention chime parameters on the physical buzzer module [REF-MQTT-LIFECYCLE].

  Supported on models with a physical fire alarm buzzer (H2 series).

- <span id="superprinterclient-is-axis-homed"></span>`fn is_axis_homed(&self, axis: Axis) -> Option<bool>` — [`Axis`](../quirks/index.md#axis)

  Returns whether `axis` was homed as of the last-observed `home_flag` telemetry.

  `None` means no telemetry carrying `home_flag` has been observed **on the current MQTT
  connection** (via [`poll_telemetry()`](#printerclient)) — not "unhomed". A
  disconnect/reconnect resets this to `None` until the printer reports again; the two
  cases are deliberately not distinguished, since a caller must handle `None` either way.
  Advisory only: the firmware does not reject motion on unhomed axes [REF-MOTO-HOME].

- <span id="superprinterclient-is-all-axes-homed"></span>`fn is_all_axes_homed(&self) -> Option<bool>`

  Returns whether X, Y, and Z were all homed as of the last-observed `home_flag` telemetry.

  `None` means no telemetry carrying `home_flag` has been observed on the current MQTT
  connection — see [`is_axis_homed()`](#printerclient).

- <span id="superprinterclient-send-gcode"></span>`async fn send_gcode(&mut self, gcode_line: &str) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Sends a G-code command with model-aware safety validation.

  Rejects, without sending anything, G-code that is unsafe on the active model: partial-axis
  homing on bed-on-Z platforms, heater targets above the model's nozzle/bed/chamber ceilings,
  and any chamber-heater command on a model without one — see
  [`ModelQuirks::validate_gcode`](../quirks/index.md#modelquirks) for the exact
  rules. Nothing is clamped, and relative moves are not bounded. The bed ceiling uses the
  same mains region as [`set_bed_temperature()`](#printerclient). Use
  [`send_gcode_raw()`](#printerclient) to bypass validation when you need unchecked
  access.

  # Example

  ```rust,ignore
  // Turn on the part cooling fan at 100%
  printer.send_gcode("M106 P1 S255").await?;

  // This will be rejected on CoreXY printers (unsafe partial homing):
  // printer.send_gcode("G28 Z").await?;  // -> Err(ModelMismatch)
  // And on an A1 Mini (80°C bed ceiling):
  // printer.send_gcode("M140 S100").await?;  // -> Err(ModelMismatch)
  ```

- <span id="superprinterclient-send-gcode-raw"></span>`async fn send_gcode_raw(&mut self, gcode_line: &str) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Dispatches a raw G-code string without model safety checks [REF-MOTO-GCODE].

  Returns the [`CommandHandle`](command/index.md#commandhandle) of the published `gcode_line` command.

- <span id="superprinterclient-home-all"></span>`async fn home_all(&mut self) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Homes every axis with a bare `G28`, the firmware's own safe parking sequence [REF-MOTO-GCODE].

  The right call on every model. On bed-slingers (A1, A1 Mini, A2L) a targeted
  [`home_z_only()`](#printerclient) is also available.

- <span id="superprinterclient-home-z-only"></span>`async fn home_z_only(&mut self) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Homes only Z (`G28 Z`) — refused on bed-on-Z models [REF-MOTO-GCODE].

  **Bed-on-Z models** (X1, X2D, P1, H2, P2S series) must be homed with a bare `G28`
  ([`home_all()`](#printerclient)), which runs the firmware's toolhead parking sequence.
  `G28 Z` skips it and risks driving the bed into a misplaced toolhead, so those models get
  [`Error::ModelMismatch`](../error/index.md#error) and nothing is sent. Bed-slingers (A1, A1 Mini, A2L) accept it.

- <span id="superprinterclient-move-relative"></span>`async fn move_relative(&mut self, axis: Axis, distance: f32, feedrate: u32) -> Result<Option<CommandHandle>, Error>` — [`Axis`](../quirks/index.md#axis), [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Dispatches a manual relative axis movement block.

  **Relative Axis Movement Safety [REF-MOTO-GCODE]:**
  Each move is capped client-side at the axis's travel in the model's
  [`build_volume()`](../quirks/index.md#modelquirks) — a bound on one command's
  distance, not position-aware crash prevention, since the printer reports no absolute axis
  position over MQTT. A Z move is additionally wrapped in reference-mode push/pop
  (`M1002 push_ref_mode` / `M1002 pop_ref_mode`) to prevent frame shifting, inside
  BambuStudio's `M211 S` / `M211 X1 Y1 Z1` … `M211 R` save-enable-restore of the
  soft-endstop state. Per real H2D hardware testing (bambuddy #2579, confirmed 2026-07-16)
  firmware does not enforce software travel limits on G-code received over MQTT regardless
  of `M211` state — it is not a source of crash protection here.

  A `distance` of exactly `0.0` is a no-op: no G-code is sent to the printer, and this
  returns `Ok(None)`.

  # Errors

  [`Error::ModelMismatch`](../error/index.md#error) when `distance` is non-finite or exceeds the axis's travel.

- <span id="superprinterclient-extrude"></span>`async fn extrude(&mut self, length: f32, feedrate: u32) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Dispatches a manual relative extrusion command sequence [REF-GCODE-EXTRUDE].

  Configures the active extruder drive gear to relative mode (`M83`) and feeds
  the specified length of filament (in mm) at the designated feedrate (in mm/min).

- <span id="superprinterclient-wait-for-homing"></span>`async fn wait_for_homing(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Blocks until a `G28` homing cycle observed via telemetry has completed.

  Standalone — does not require this client to have issued [`home_all()`](#printerclient).
  Resolves correctly whether homing was triggered by this client, the touchscreen, slicer
  software, or another `PrinterClient` instance, since it only relies on `home_flag`
  telemetry observed via [`poll_telemetry()`](#printerclient).

  Only resolves successfully after observing a not-all-homed `home_flag` reading
  followed by an all-homed reading: an already-homed printer at call time does not
  resolve instantly, and a call where nothing ever homes times out rather than
  returning early.

  Times out after [`HOMING_WAIT_TIMEOUT`](#homing-wait-timeout), independent of the command timeout. Like
  `poll_until` (`src/client/mod.rs`), that deadline (or, without a real clock, the
  message-count valve) is only checked *after* each `poll_telemetry().await` below
  has already returned — neither protects against that single call stalling
  forever on a connection that stops delivering bytes mid-homing (printer powered
  off, network drop). That protection is a distinct, lower layer: the underlying
  `MqttClient::poll_wire()` (`src/mqtt/client/mod.rs`) races each low-level read
  step against `self.timer` internally, bounding a single call regardless of what
  this loop does above it.

- <span id="superprinterclient-pause-print"></span>`async fn pause_print(&mut self) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Pauses the currently active print job [REF-MQTT-LIFECYCLE].

  Deliberately **not** state-gated, unlike [`skip_objects`](#printerclient). Pausing an
  idle printer is a firmware no-op rather than a misdirected command, neither BambuStudio
  nor bambuddy gates this on `gcode_state`, and the CLI's `probe` sends it while idle on
  purpose to document what the firmware does. See `stop_print` for the staleness argument
  that applies to any cache-backed gate on this path.

- <span id="superprinterclient-resume-print"></span>`async fn resume_print(&mut self) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Resumes a paused print job [REF-MQTT-LIFECYCLE].

  Not state-gated, on the same terms as [`pause_print`](#printerclient).

- <span id="superprinterclient-stop-print"></span>`async fn stop_print(&mut self) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Aborts/cancels the currently running print job queue [REF-MQTT-LIFECYCLE].

  Deliberately **ungated**, unlike [`skip_objects`](#printerclient). A state gate reads
  the *cached* `gcode_state`, which is only as fresh as the last
  [`poll_telemetry()`](#printerclient); a caller that has not polled since before the
  job started holds a stale `IDLE`. Refusing an abort on a stale reading would leave the
  printer running while reporting the stop as rejected — the wrong direction to fail for
  the abort path. Stop is idempotent, so a no-op stop costs nothing on the other side.

- <span id="superprinterclient-clear-print-error"></span>`async fn clear_print_error(&mut self) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Clears active error codes from the printer's diagnostic fault register [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-ignore-error-and-resume"></span>`async fn ignore_error_and_resume(&mut self, error_code: impl Into<u32>) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Ignores `error_code` and resumes the paused print (the error dialog's "Ignore and resume").

  Sends `ignore`, which skips the firmware's next re-check of that one fault. A plain
  [`resume_print`](#printerclient) means "fixed it, re-check", so a fault such as a wrong
  build plate is re-detected and pauses the print again a second later (bambuddy #1869).
  `error_code` is the `print_error` register value: pass the
  `DecodedPrintError` from
  [`active_fault()`](#printerclient), or its raw `code`. The cached `job_id` is echoed
  back, or an empty string before any telemetry carried one. [REF-MQTT-LIFECYCLE]

- <span id="superprinterclient-resume-print-after-error"></span>`async fn resume_print_after_error(&mut self, error_code: impl Into<u32>) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Resumes naming the fault being answered — BambuStudio's error-dialog form of `resume`.

  An opt-in alternative to [`resume_print`](#printerclient), which stays the default:
  bambuddy sends the plain shape from its own error dialog and has it confirmed on H2D/H2S.
  Takes the same `error_code` and cached `job_id` as
  [`ignore_error_and_resume`](#printerclient). [REF-MQTT-LIFECYCLE]

- <span id="superprinterclient-stop-print-after-error"></span>`async fn stop_print_after_error(&mut self, error_code: impl Into<u32>) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Stops naming the fault being answered — BambuStudio's error-dialog form of `stop`.

  An opt-in alternative to [`stop_print`](#printerclient), on the same terms as
  [`resume_print_after_error`](#printerclient). [REF-MQTT-LIFECYCLE]

- <span id="superprinterclient-dismiss-error"></span>`async fn dismiss_error(&mut self, error_code: impl Into<u32>, scope: IdleIgnoreScope) -> Result<CommandHandle, Error>` — [`IdleIgnoreScope`](../mqtt/commands/control/index.md#idleignorescope), [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Dismisses a non-pausing warning without resuming anything (`idle_ignore`).

  `scope` dismisses this occurrence or suppresses the warning permanently.
  [REF-MQTT-LIFECYCLE]

- <span id="superprinterclient-close-error-dialog"></span>`async fn close_error_dialog(&mut self, error_code: impl Into<u32>) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Closes the `print_error` dialog on the printer's own screen (`system.uiop`).

  Separate from [`clear_print_error`](#printerclient), which clears the error
  latch but leaves the on-screen dialog; BambuStudio sends this once whenever its own copy
  of the dialog closes. [REF-MQTT-LIFECYCLE]

- <span id="superprinterclient-refresh-nozzle"></span>`async fn refresh_nozzle(&mut self) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Asks the printer to re-read its nozzle information (`refresh_nozzle`) [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-disable-air-purification"></span>`async fn disable_air_purification(&mut self) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Turns off air purification (`close_air_filt`), the error dialog's "disable purification" [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-auto-stop-ams-drying"></span>`async fn auto_stop_ams_drying(&mut self) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Sends `auto_stop_ams_dry`, the error dialog's "stop drying" [REF-MQTT-LIFECYCLE].

  A different command from [`stop_drying`](#printerclient), which sends
  `ams_filament_drying` for one unit. Whether the two are equivalent is not known, so this
  is offered alongside it rather than in place of it.

- <span id="superprinterclient-set-print-speed"></span>`async fn set_print_speed(&mut self, level: PrintSpeed) -> Result<CommandHandle, Error>` — [`PrintSpeed`](../types/control/index.md#printspeed), [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Dynamically scales maximum velocity and acceleration limits during an active print [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-skip-objects"></span>`async fn skip_objects(&mut self, object_ids: Vec<u32>) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Bypasses rendering of specific objects within an active multi-model print job [REF-MQTT-LIFECYCLE].

  `object_ids` are `identify_id` values from the `slice_info.config` inside the **currently
  loaded** job's 3MF. They reference nothing when no job is loaded, which is why this is
  gated more tightly than [`pause_print`](#printerclient).

  # Errors

  [`Error::InvalidArgument`](../error/index.md#error) when `object_ids` is empty — that would publish an empty
  `obj_list`, a command that cannot skip anything.

  [`Error::InvalidState`](../error/index.md#error) unless the cached print state is `Running` or `Paused` (or not yet
  observed). This follows bambuddy, which gates on exactly those two
  (`bambu_mqtt.py:7047`). Pausing to inspect a failed part, skipping it, then resuming is a
  legitimate workflow, so `Paused` belongs alongside `Running`.

  Deliberately **not** gated on `xcam.allow_skip_parts`: that field reads `false` in every
  capture, including hardware the vendor documents as supporting the feature, so gating on
  it would break skip-objects outright. bambuddy parses it and likewise does not gate on it.

- <span id="superprinterclient-start-calibration"></span>`async fn start_calibration(&mut self, options: CalibrationOption) -> Result<CommandHandle, Error>` — [`CalibrationOption`](../types/control/index.md#calibrationoption), [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Triggers automated physical calibration routines on the printer chassis [REF-MQTT-LIFECYCLE].

  Use `CalibrationOption` flags combined with `|` to select routines:
  ```rust,ignore
  client.start_calibration(
      CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION
  ).await?;
  ```

  The returned [`CommandHandle`](command/index.md#commandhandle) tracks the published command, not a completion signal, but
  the run is observable while it happens (verified on a P1S, firmware `01.10.00.00`):

  - [`print_progress()`](#printerclient) tracks it.
    `percent` ramps 0 to 100 and `remaining_secs` counts down. This is one aggregate bar
    across the whole sweep — a single-routine run spans the same full range as a
    multi-routine one, so per-routine progress cannot be derived from it.
  - Per-routine boundaries come from
    [`current_stage()`](../types/telemetry/index.md) and
    [`stage_queue()`](../types/telemetry/index.md), which decode
    `stg_cur`/`stg` into [`PrintStage`](../types/telemetry/index.md). Both wire
    fields arrive in incremental pushes, so this tracks in real time.
  - A [`PrintStage::Idle`](../types/telemetry/index.md) mid-run is normal:
    after the last queued stage finishes it reads idle for the rest of the run while
    `percent` keeps climbing. Completion is `gcode_state`/`percent`, never the stage.
  - A calibration run is distinguishable from a user print by `print_type == "system"`
    with `subtask_name == "auto_cali_for_user_param.gcode"`; `layer_num`/`total_layer_num`
    stay 0 and are meaningless here.

  # Unsupported routines

  The firmware accepts every option bit, acknowledges the command `"result": "success"`,
  and silently queues nothing for a routine the hardware doesn't run — so the wire never
  reports the skip. This method masks the request against
  [`supported_calibration()`](../quirks/index.md#modelquirks)
  instead of trusting that ack: unsupported routines are dropped with a `log::warn!` and the
  remaining routines still run.

  **Vibration compensation (bit 2) is kept on every model.** Both upstreams send the bit
  for any model: BambuStudio's calibration dialog offers Vibration Compensation with no
  model gate (`Calibration.cpp:57`, gates at `:225-260`), and bambuddy's
  `start_calibration` (`bambu_mqtt.py:6295-6345`) sets it unconditionally (#358). This is
  separate from the print job's `vibration_cali` field, which
  [`start_print`](#printerclient) sends as `false` by default on every model (#375).

  # Errors

  [`Error::ModelMismatch`](../error/index.md#error) when *none* of the requested routines are supported on this
  model, since that request would otherwise be a silent no-op reported as success. A
  partially-supported request is not an error — it proceeds with whatever the model runs.

  Wire observations are P1S firmware `01.10.00.00`. See `reference/03_mqtt_telemetry.md`
  for the wire detail and stage-ID mapping.

- <span id="superprinterclient-start-print"></span>`async fn start_print(&mut self, config: &PrintJobConfig) -> Result<CommandHandle, Error>` — [`PrintJobConfig`](../mqtt/commands/print_job/index.md#printjobconfig), [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Submits a `.3mf` print job from MicroSD storage for execution [REF-MQTT-LIFECYCLE].

  The model's quirks engine gates `nozzle_offset_cali`: it resolves the default when the
  config left it `None`, and forces it off on a single-nozzle model even if the caller set
  it explicitly.

- <span id="superprinterclient-attach-ftps"></span>`async fn attach_ftps(&mut self, ftps_client: FtpsClient<FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer>)` — [`FtpsClient`](../ftps/client/index.md#ftpsclient)

  Injects a pre-connected [`FtpsClient`](../ftps/client/index.md#ftpsclient) directly.

  Use this for test mocks or Embassy where the caller manages the FTPS
  connection. For lazy connection, use [`.with_ftps()`](#printerclient). On a
  [`from_mqtt()`](#printerclient) client, whose FTPS type parameters are placeholders,
  use [`.with_attached_ftps()`](#printerclient) instead.

  A session already in the slot is disconnected first, as
  [`disconnect_ftps()`](#printerclient) does, so its TLS session is closed
  rather than dropped mid-stream.

- <span id="superprinterclient-ftps"></span>`async fn ftps(&mut self) -> Result<&mut FtpsClient<FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer>, Error>` — [`FtpsClient`](../ftps/client/index.md#ftpsclient), [`Error`](../error/index.md#error)

  Returns direct access to the underlying [`FtpsClient`](../ftps/client/index.md#ftpsclient), auto-connecting if needed.

  Requires prior FTPS configuration via [`.with_ftps()`](#printerclient) or
  [`.attach_ftps()`](#printerclient). A session that a transport failure poisoned is
  disconnected and redialed here rather than handed back.

- <span id="superprinterclient-disconnect-ftps"></span>`async fn disconnect_ftps(&mut self)`

  Disconnects the FTPS session, if one exists, keeping its configuration for a reconnect.

  `FtpsClient::disconnect()` hands back the TLS connector, factory and timer; they go back
  into this client's FTPS configuration, so the next [`ftps()`](#printerclient) or
  [`connect_ftps()`](#printerclient) dials a fresh session, as the camera channel does.

  Idempotent — a no-op if no FTPS session is active. Infallible: a close failure on the way
  out is logged and swallowed, since the connection is going away either way.

- <span id="superprinterclient-poll-telemetry"></span>`async fn poll_telemetry(&mut self) -> Result<TelemetryEvent, Error>` — [`TelemetryEvent`](types/index.md#telemetryevent), [`Error`](../error/index.md#error)

  Pulls the next telemetry event from the MQTT channel.

  Returns, in order of precedence:

  - [`Command`](https://docs.rs/std/latest/std/process/struct.Command.html) for a command outcome that needs no message — a command
    past its deadline, or one lost to a disconnect — before touching the wire.
  - [`Command`](https://docs.rs/std/latest/std/process/struct.Command.html) for an echo answering a command this client published,
    with the printer's verdict decoded.
  - [`TelemetryEvent::Unknown`](types/index.md#telemetryevent) for any other command echo, under any wrapper (`print`,
    `system`, `info`): another client's command, or a response a request method such as
    [`get_version()`](#printerclient) did not claim. An echo shares envelopes and field
    names with telemetry (`extrusion_cali_get`'s reply carries `nozzle_diameter`), so it is
    never read as a report.
  - [`Report`](https://docs.rs/std/latest/std/error/struct.Report.html) if the payload deserializes as telemetry, else `Unknown`.

  Drains any internally buffered messages (from command-response round-trips) before
  reading from the wire. A timeout is noticed on the next call, so it is delivered late by
  however long this call blocks on the wire — at most `MQTT_READ_TIMEOUT_SECS` (30s) on a
  completely silent link, and in practice far sooner, since the printer pushes telemetry
  continuously and this call sends a keepalive every 20s.

  Cancellation-safe in a `select!`: outcome bookkeeping happens only after the wire read
  has returned, never across an await.

  # Example

  ```rust,ignore
  loop {
      match printer.poll_telemetry().await? {
          TelemetryEvent::Report(report, _raw) => {
              // `report.print` is an Option — absent on a report that carries only
              // top-level `device` data.
              if let Some(print) = &report.print {
                  println!("Printer state: {:?}", print.gcode_state);
              }
          }
          TelemetryEvent::Command(resolution, _raw) => {
              println!("{}: {:?}", resolution.handle.command(), resolution.outcome);
          }
          TelemetryEvent::Unknown(_) => {}
      }
  }
  ```

- <span id="superprinterclient-await-ack"></span>`async fn await_ack(&mut self, handle: &CommandHandle) -> Result<CommandOutcome, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`CommandOutcome`](command/index.md#commandoutcome), [`Error`](../error/index.md#error)

  Waits for the outcome of one command this client published, reading the wire until its echo arrives or its time runs out.

  The inline counterpart to receiving [`Command`](https://docs.rs/std/latest/std/process/struct.Command.html) from
  [`poll_telemetry()`](#printerclient), for scripts that send one command and act on
  the answer. Messages read while waiting are buffered and still delivered by later
  `poll_telemetry()` calls, and the outcome returned here is not delivered again as an
  event.

  - A command that never echoes returns [`CommandOutcome::SettledOnPublish`](command/index.md#commandoutcome) at once.
  - A command whose outcome is already known returns it at once — including one the
    caller's event loop has already received, for the most recent 32 outcomes.
  - Otherwise waits up to [`set_command_timeout()`](#printerclient) from this
    call and returns [`CommandOutcome::TimedOut`](command/index.md#commandoutcome) if no echo arrives. Without a real clock
    the wait is bounded only by the 200-message safety valve, which also ends a wait early
    on a busy link.

  **Blocks the caller's event loop for up to the timeout.** A UI should consume
  `TelemetryEvent::Command` from its existing `poll_telemetry()` loop instead.

  # Errors

  [`Error::InvalidArgument`](../error/index.md#error) for a handle whose outcome this client no longer holds — it
  was delivered more than 32 outcomes ago, or already returned by an earlier `await_ack`.
  Transport errors from reading the wire are returned as-is; the command then resolves as
  [`CommandOutcome::ConnectionLost`](command/index.md#commandoutcome) once the session is re-established.

- <span id="superprinterclient-print-status"></span>`fn print_status(&self) -> Option<PrintStatus>` — [`PrintStatus`](../types/control/index.md#printstatus)

  Returns the printer's high-level activity classification as of the last-observed `gcode_state` telemetry (via [`poll_telemetry()`](#printerclient)).
  `None` means no telemetry carrying `gcode_state` has been observed yet.

- <span id="superprinterclient-subtask-name"></span>`fn subtask_name(&self) -> Option<&str>`

  Returns the active job's name (`subtask_name`) as of the last-observed telemetry; `None` before any carried it.

- <span id="superprinterclient-legacy-nozzle"></span>`fn legacy_nozzle(&self) -> (Option<&str>, Option<&str>)`

  Returns the legacy single-nozzle `(nozzle_diameter, nozzle_type)` strings as of the last-observed telemetry.

  Pre-IDEX models report the fitted nozzle this way; newer ones report per-nozzle entries
  under [`device()`](#printerclient) instead.

- <span id="superprinterclient-sdcard-status"></span>`fn sdcard_status(&self) -> Option<SdcardState>` — [`SdcardState`](../types/telemetry/report/index.md#sdcardstate)

  Returns the SD-card state as of the last-observed telemetry that carried one — see [`PrinterTelemetry::sdcard_status`](../types/telemetry/report/index.md#printertelemetry).

- <span id="superprinterclient-device"></span>`fn device(&self) -> Option<&DeviceTelemetry>` — [`DeviceTelemetry`](../types/telemetry/device/index.md#devicetelemetry)

  Returns the merged `device` telemetry (nozzles, extruders, airduct, chamber controller) as of the last-observed telemetry.

  `None` before any telemetry carried a `device` object, from either wire location.

- <span id="superprinterclient-is-door-open"></span>`fn is_door_open(&self) -> Option<bool>`

  Returns whether the door was open as of the last-observed telemetry (via [`poll_telemetry()`](#printerclient)).

  Returns `None` on models without a door sensor (`ModelQuirks::door_sensor()` is
  `DoorSensor::None`, e.g. A1/A2) — distinct from `Some(false)`, which means a
  sensor-equipped model's telemetry confirms the door is closed. Also `None` before any
  telemetry carrying the model's door field has been observed.

- <span id="superprinterclient-is-220v-power"></span>`fn is_220v_power(&self) -> Option<bool>`

  Returns the printer's mains region as of the last-observed `home_flag` telemetry.
  `None` means no telemetry carrying `home_flag` has been observed yet — distinct from
  `Some(false)`. Feed this straight into
  [`ModelQuirks::bed_temp_max`](../quirks/index.md#modelquirks) (reachable via
  [`PrinterClient::quirks()`](#printerclient)) to read a printer's bed
  ceiling before issuing a command; `set_bed_temperature` uses this same accessor.

- <span id="superprinterclient-active-fault"></span>`fn active_fault(&self) -> Option<DecodedPrintError>` — [`DecodedPrintError`](../diagnostics/hms/index.md#decodedprinterror)

  Returns the decoded active print-error fault as of the last-observed `print_error` telemetry (via [`poll_telemetry()`](#printerclient)).

  `None` covers both "no telemetry carrying `print_error` observed yet" and "the
  register reads 0 (no fault)" — both warrant the same caller action, so they are not
  distinguished here.

- <span id="superprinterclient-print-progress"></span>`fn print_progress(&self) -> PrintProgress` — [`PrintProgress`](types/index.md#printprogress)

  Returns the print progress snapshot as of the last-observed telemetry (via [`poll_telemetry()`](#printerclient)).
  Each field independently tracks its own "last observed" value — see [`PrintProgress`](types/index.md#printprogress)'s doc
  comment.

- <span id="superprinterclient-bed-temperatures"></span>`fn bed_temperatures(&self) -> Option<HeaterTemps>` — [`HeaterTemps`](#heatertemps)

  Returns the bed's temperatures as of the last-observed telemetry (via [`poll_telemetry()`](#printerclient)); `None` before any telemetry carrying them.

  Shares its cross-model decode logic with
  `TelemetryReport::bed_temperatures()` —
  use that method instead if you already have a fresh `TelemetryReport` in hand.

- <span id="superprinterclient-ams"></span>`fn ams(&self) -> Option<&AmsStatusReport>` — [`AmsStatusReport`](../types/telemetry/ams/index.md#amsstatusreport)

  Returns the cached AMS/tray status report as of the last-observed telemetry (via [`poll_telemetry()`](#printerclient)).
  `None` means no telemetry carrying `print.ams` has been observed yet.

  This is the **raw** merged cache — every field independently keeps its most recently
  observed value ([`AmsStatusReport::merge_from`](../types/telemetry/ams/index.md#amsstatusreport)-level
  detail), but stale per-tray material fields (`tray_type`, `tray_color`, `remain`, etc.)
  are **not** proactively cleared when a slot empties — confirmed against BambuStudio's
  own `DevFilaSystem.cpp`, whose structural equivalent (`DevAmsTray::reset()`) is dead
  code with zero call sites in its own current codebase; the shipped BambuStudio/
  OrcaSlicer UI instead gates every read of a tray's material fields on
  `is_exists`/`is_tray_info_ready()`-equivalent checks (`AmsTray::is_loaded()` here) and
  never scrubs the raw cache. This crate mirrors that design rather than
  [`clean_stale_tray_data`](../ams/parser/index.md#clean-stale-tray-data)'s proactive-clearing
  approach: wiring proactive clearing into this cache would make it *less* faithful to
  on-wire state than BambuStudio's own model. Two opt-in ways to get sanitized output
  without losing that raw fidelity:
  - Check `AmsTray::is_loaded()` (or
    `evaluate_spool_presence`) before trusting a
    tray's material fields — the same check-before-trust contract BambuStudio itself
    relies on.
  - Call [`sanitized_ams()`](#printerclient) for a cloned, scrubbed copy — mirrors
    [`hms()`](#printerclient)/[`active_hms_alerts()`](#printerclient)'s raw-cache +
    opt-in-decoded accessor split.

- <span id="superprinterclient-printing-tray-global-id"></span>`fn printing_tray_global_id(&self) -> Option<u8>`

  Returns the global tray ID of the spool currently feeding the active extruder, as of
  the last-observed telemetry (via [`poll_telemetry()`](#printerclient)).

  Prefers `device.extruder.info[active].snow`, BambuStudio's own preferred resolution
  method (`DevExterSystem::ParseV2_0`, `DevExtderSystem.cpp:318-386`) — no
  `ams_extruder_map` inversion needed, since `snow` self-identifies both the AMS unit and
  slot directly. `None` when `device.extruder` telemetry hasn't been observed yet (common
  on single-nozzle models, which may not populate this sub-object at all) or the active
  extruder's `snow` is the unmapped sentinel.

- <span id="superprinterclient-sanitized-ams"></span>`fn sanitized_ams(&self) -> Option<AmsStatusReport>` — [`AmsStatusReport`](../types/telemetry/ams/index.md#amsstatusreport)

  Returns a cloned copy of the cached AMS status report with every tray's stale material
  fields cleared via [`clean_stale_tray_data`](../ams/parser/index.md#clean-stale-tray-data)
  (mirrors [`active_hms_alerts()`](#printerclient)'s raw-cache-decode-on-access
  shape). `None` under the same condition as [`ams()`](#printerclient) — no telemetry carrying
  `print.ams` observed yet. Does not mutate the underlying cache — [`ams()`](#printerclient)
  keeps returning the raw values; see its doc comment for why the raw cache is never
  proactively scrubbed.

- <span id="superprinterclient-vt-tray"></span>`fn vt_tray(&self) -> Option<&VirtualTray>` — [`VirtualTray`](../types/telemetry/ams/index.md#virtualtray)

  Returns the cached virtual/external spool holder state (single-nozzle models) as of the last-observed telemetry (via [`poll_telemetry()`](#printerclient)).
  `None` means no telemetry carrying `print.vt_tray` has been observed yet — including on IDEX
  models, which send [`vir_slot()`](#printerclient) instead.

- <span id="superprinterclient-vir-slot"></span>`fn vir_slot(&self) -> Option<&[VirtualTray]>` — [`VirtualTray`](../types/telemetry/ams/index.md#virtualtray)

  Returns the cached IDEX external spool holder array as of the last-observed telemetry (via [`poll_telemetry()`](#printerclient)).
  `None` means no telemetry carrying `print.vir_slot` has been observed yet — including on
  single-nozzle models, which send [`vt_tray()`](#printerclient) instead.

- <span id="superprinterclient-nozzle-temperatures"></span>`fn nozzle_temperatures(&self) -> Vec<NozzleTemps>` — [`NozzleTemps`](#nozzletemps)

  Returns the nozzle temperatures as of the last-observed telemetry (via [`poll_telemetry()`](#printerclient)), one entry per nozzle; empty before any telemetry carrying them.

  Single-nozzle models return one entry (`id` 0); IDEX models return one entry per physical
  nozzle. Same decode as
  `TelemetryReport::nozzle_temperatures()`,
  including the undocumented IDEX flat-field routing quirk.

- <span id="superprinterclient-chamber-temperature"></span>`fn chamber_temperature(&self) -> Option<HeaterTemps>` — [`HeaterTemps`](#heatertemps)

  Returns the chamber's temperatures as of the last-observed telemetry (via [`poll_telemetry()`](#printerclient)).

  `None` on models without a chamber temperature sensor
  (`ModelQuirks::has_chamber_temperature_sensor()` is `false`, e.g. A1/A1 Mini/A2L/P1P/
  P1S), and before any telemetry carrying `chamber_temper` has been observed.

- <span id="superprinterclient-hms"></span>`fn hms(&self) -> Option<&[HmsEntry]>` — [`HmsEntry`](../types/telemetry/diagnostics/index.md#hmsentry)

  Returns the cached active hardware-alert (HMS) entries as of the last-observed telemetry (via [`poll_telemetry()`](#printerclient)).
  `None` means no telemetry carrying `print.hms` has been observed yet.

- <span id="superprinterclient-ipcam"></span>`fn ipcam(&self) -> Option<&IpcamTelemetry>` — [`IpcamTelemetry`](../types/telemetry/diagnostics/index.md#ipcamtelemetry)

  Returns the cached camera/recording state as of the last-observed telemetry (via [`poll_telemetry()`](#printerclient)).
  `None` means no telemetry carrying `print.ipcam` has been observed yet.

- <span id="superprinterclient-xcam"></span>`fn xcam(&self) -> Option<&XcamTelemetry>` — [`XcamTelemetry`](../types/telemetry/xcam/index.md#xcamtelemetry)

  Returns the cached AI-detection and print-option settings as of the last-observed telemetry (via [`poll_telemetry()`](#printerclient)).

  `None` means no telemetry carrying `print.xcam` has been observed yet. Because `xcam`
  appears to be pushall-only, that can persist for a long stretch of incremental frames — it
  is not evidence the model lacks these settings. Use
  [`XcamTelemetry::supports_ai_monitoring`](../types/telemetry/xcam/index.md#xcamtelemetry) for that question instead.

- <span id="superprinterclient-active-hms-alerts"></span>`fn active_hms_alerts(&self) -> Vec<DecodedHmsAlert>` — [`DecodedHmsAlert`](../diagnostics/hms/index.md#decodedhmsalert)

  Returns every cached HMS entry decoded and filtered to genuine faults (mirrors `active_fault()`'s raw-cache-decode-on-access shape).
  Empty when nothing is cached or nothing currently decodes as a genuine fault — there's no caller
  action that would differ between those two cases.

- <span id="superprinterclient-fan-speed"></span>`fn fan_speed(&self, fan: FanTarget) -> Option<u8>` — [`FanTarget`](../types/control/index.md#fantarget)

  Returns `fan`'s speed as a percentage (0-100), decoded from the last-observed telemetry (via [`poll_telemetry()`](#printerclient)); `None` before any telemetry carrying it.

  [`FanTarget::AuxiliaryLeft2`](../types/control/index.md#fantarget) (X2D/P2S, port 10) reports at a different wire location
  than the other three — `device.airduct.parts[id=160].state`, already a percentage
  [REF-CLIM-FANS] — which this handles.

- <span id="superprinterclient-heatbreak-fan-speed"></span>`fn heatbreak_fan_speed(&self) -> Option<u8>`

  Returns the toolhead heatbreak fan speed as a percentage (0-100).

  Not independently controllable (no corresponding `FanTarget` variant/M106 port) — read-only
  telemetry.

- <span id="superprinterclient-print-speed"></span>`fn print_speed(&self) -> Option<PrintSpeed>` — [`PrintSpeed`](../types/control/index.md#printspeed)

  Returns the printer's current print-speed level as of the last-observed telemetry (via [`poll_telemetry()`](#printerclient)).
  `None` before any telemetry carrying `spd_lvl` has been observed, or if the observed value is
  out of the known 1-4 range.

- <span id="superprinterclient-print-speed-magnitude"></span>`fn print_speed_magnitude(&self) -> Option<u16>`

  Returns the printer's current print-speed magnitude (percentage of nominal feedrate) as of the last-observed telemetry (via [`poll_telemetry()`](#printerclient)).

- <span id="superprinterclient-wifi-signal"></span>`fn wifi_signal(&self) -> Option<&str>`

  Returns the raw wireless signal strength string (e.g. `"-52dBm"`) as of the last-observed telemetry (via [`poll_telemetry()`](#printerclient)).

- <span id="superprinterclient-is-ethernet-active-via-wifi-signal"></span>`fn is_ethernet_active_via_wifi_signal(&self) -> bool`

  Returns whether the printer is on wired Ethernet, per the cached `wifi_signal` sentinel (mirrors `PrinterTelemetry::is_ethernet_active_via_wifi_signal()` but works between polls off the cached value, the same way [`is_all_axes_homed()`](#printerclient) works off cached `home_flag`).

- <span id="superprinterclient-is-ethernet-active"></span>`fn is_ethernet_active(&self) -> bool`

  Returns whether the printer is on wired Ethernet, per the cached `print.net.conf` bit 0
  (mirrors `PrinterTelemetry::is_ethernet_active()`, the documented-preferred,
  confirmed-authoritative source) but works between polls off the cached
  value. `false` before any telemetry carrying `print.net.conf` has been observed; prefer
  `is_ethernet_active_via_wifi_signal()` as a fallback for firmware that doesn't send it.

- <span id="superprinterclient-poll-telemetry-until"></span>`async fn poll_telemetry_until(&mut self, timeout: core::time::Duration, done: impl FnMut(&Self) -> bool) -> Result<bool, Error>` — [`Error`](../error/index.md#error)

  Polls telemetry until `done` holds for this client's cache, or `timeout` passes; returns whether `done` was reached.

  `done` is checked before each poll, so an already-satisfied condition returns at once
  without touching the wire. Events read along the way update the cache as
  [`poll_telemetry()`](#printerclient) always does, and are otherwise dropped — use
  `poll_telemetry()` directly to see them.

  `timeout` is measured on this client's timer and checked between messages, so on a link
  that goes silent the wait can overrun it by up to one read deadline (30s). Without a real
  clock ([`with_timer()`](#printerclient)) the elapsed time can't be measured and the
  wait ends after the same 200-message backstop `get_version()` uses.

- <span id="superprinterclient-refresh-state"></span>`async fn refresh_state(&mut self, timeout: core::time::Duration) -> Result<bool, Error>` — [`Error`](../error/index.md#error)

  Requests a full state dump and waits until the cache holds a `gcode_state`, or `timeout` passes; returns whether it does.

  A `pushall` reply carries `gcode_state`, so this waits for the reply on a cold cache; on a
  cache that already holds one it returns at once without waiting for the new dump. For
  a field that only some frames carry (an AMS unit's type, say), follow this with
  [`poll_telemetry_until()`](#printerclient) on that field.

- <span id="superprinterclient-poll-raw"></span>`async fn poll_raw(&mut self) -> Result<MqttMessage, Error>` — [`MqttMessage`](../mqtt/client/index.md#mqttmessage), [`Error`](../error/index.md#error)

  Pulls the next raw MQTT message without deserialization.

  Bypasses command-outcome tracking: an echo read here is not matched to its command, so
  that command later resolves as [`CommandOutcome::TimedOut`](command/index.md#commandoutcome) instead. Don't mix this with
  [`poll_telemetry()`](#printerclient) while commands are outstanding.

- <span id="superprinterclient-set-bed-temperature"></span>`async fn set_bed_temperature(&mut self, target_temp: u16) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Sets the heated bed target temperature.

  Values exceeding the model's maximum are clamped automatically. Most models have a flat
  per-model ceiling (e.g. 80°C for A1 Mini), but the X1C's and X1's ceiling is
  voltage-dependent — 110°C on a 220V-region unit, 120°C on a 110V-region unit, per the
  official spec sheet. This is
  derived from the most recently observed `home_flag` telemetry
  (`self.core.cache.last_home_flag`, bit 3 — see `PrinterTelemetry::is_220v_power`);
  before any `home_flag` has been received (fresh connection, no `pushall` yet) the mains
  region is unknown and the X1C/X1 conservatively clamp to 110°C.

  # Example

  ```rust,ignore
  printer.set_bed_temperature(60).await?;
  ```

- <span id="superprinterclient-set-nozzle-temperature"></span>`async fn set_nozzle_temperature(&mut self, nozzle_id: u8, target_temp: u16) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Sets the target temperature of a specific hotend/nozzle [REF-MOTO-GCODE].

  * `nozzle_id`: The carriage ID (usually `0` for primary/single, or `1` for secondary on
    IDEX). On a tool changer (H2C) the fixed hotend is `0` and the rack slots are
    [`RACK_NOZZLE_IDS`](../quirks/index.md#rack-nozzle-ids) — see
    [`ModelQuirks::is_valid_nozzle_id`](../quirks/index.md#modelquirks).

  Values exceeding the model's maximum nozzle temperature are clamped automatically.

- <span id="superprinterclient-set-chamber-temperature"></span>`async fn set_chamber_temperature(&mut self, target_temp: u16) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Sets the target temperature of the active heated chamber loop [REF-MOTO-GCODE].

  **Chamber Temperature Safety Check [REF-THER-DECODE]:**
  Only supported on models with active PTC chamber heaters (X1E, X2D, H2 series).
  Models with passive chamber sensors but no heater (X1C, P2S) will return a capability
  mismatch error — their firmware silently ignores M141.

  **This does not manage the airduct flap, and on a model that has one the target will not
  be reached without it.** `M141` and the flap are independent: the flap stays wherever it
  was last left, and its default cooling position actively vents the chamber, so a
  chamber-heat request issued with the flap in cooling never converges — the heater is
  fighting an open exhaust, and this method still returns `Ok`. Use
  [`preheat_chamber()`](#printerclient) to drive both together, or call
  [`set_airduct_mode()`](#printerclient) yourself.

- <span id="superprinterclient-preheat-chamber"></span>`async fn preheat_chamber(&mut self, target_temp: u16) -> Result<PreheatHandles, Error>` — [`PreheatHandles`](#preheathandles), [`Error`](../error/index.md#error)

  Sets the chamber target *and* the airduct flap that has to agree with it.

  [`set_chamber_temperature()`](#printerclient) is the primitive: it emits
  `M141` and nothing else. That is not enough on any model fitted with the cooling/heating
  flap (H2C, H2D, H2D Pro, H2S, X2D, and P2S for the cooling direction only, having the
  flap but no active chamber heater). The flap is independent of `M141` and **persists**
  across jobs, and its default cooling position actively vents the chamber — so a heat
  request with the flap left in cooling never converges.

  This method sets the flap to [`AirductMode::Heating`](../mqtt/commands/hardware/index.md#airductmode) before raising the target, and back
  to [`AirductMode::Cooling`](../mqtt/commands/hardware/index.md#airductmode) when `target_temp` is `0`. The second half is not optional:
  a PLA job following an ABS job on the same machine would otherwise inherit the heating
  flap and overheat.

  On a model with no flap ([`ModelQuirks::supports_airduct_mode`](../quirks/index.md#modelquirks) false), this is exactly
  `set_chamber_temperature`. On a model with a flap but no heater (P2S), a non-zero
  `target_temp` still returns the same `ModelMismatch` the primitive would, and the flap is
  left alone — the caller wanted heat this model cannot make.

  Returns the handle of each command sent, so a caller can await the flap's outcome as well
  as the heater's — the flap is the command this method exists for.

- <span id="printerclient-new"></span>`fn new(tls: MqttTls, factory: MqttFactory, identity: PrinterIdentity) -> Self` — [`PrinterIdentity`](../identity/index.md#printeridentity)

  Creates a lazy client that defers MQTT connection until first use.

  The MQTT session is established automatically on the first method call that
  requires it (e.g. [`poll_telemetry()`](#printerclient),
  [`request_pushall()`](#printerclient)), or eagerly via
  [`connect_mqtt()`](#printerclient). `tls`/`factory` mirror
  [`.with_ftps(tls, factory, timer)`](#printerclient)'s call shape — `factory.dial()` opens the
  raw TCP socket, then `tls.connect()` wraps it in TLS.

  Without a [`TimerProvider`](../io/index.md#timerprovider), command-response methods like
  [`get_version()`](#printerclient) rely on a message-count safety valve
  instead of wall-clock timeouts. Chain [`.with_timer()`](#printerclient)
  for real timeouts.

- <span id="printerclient-from-mqtt"></span>`fn from_mqtt(mqtt_client: MqttClient<IO>, model: PrinterModel) -> Self` — [`MqttClient`](../mqtt/client/index.md#mqttclient), [`PrinterModel`](../models/index.md#printermodel)

  Wraps an already-connected [`MqttClient`](../mqtt/client/index.md#mqttclient) in a `PrinterClient`.

  Use this when you have a pre-established MQTT session (tests, Embassy,
  or any context where the caller manages the connection). The resulting client uses
  `PreConnected` for both the MQTT `Tls` and `Factory` slots. `ensure_mqtt()`
  short-circuits on `self.mqtt.is_some()`, so `PreConnected`'s `RawStreamFactory::dial` is
  reachable only after [`disconnect_mqtt()`](#printerclient): the next command then
  returns [`SocketError::NotConnected`](../io/index.md#socketerror) until
  [`attach_mqtt()`](#printerclient) supplies a new session.

  Being synchronous, this skips the connect-time `pushall` a dialled session gets, so
  connection-scoped telemetry stays `None` until the printer next reports it. Call
  [`request_pushall()`](#printerclient) once to refill it. The sequence counter is
  reseeded when [`with_timer()`](#printerclient) supplies a real clock.

- <span id="printerclient-next-sequence-id"></span>`fn next_sequence_id(&mut self) -> u64`

  Increments and returns the next transaction/sequence identifier tracking commands.

  Stays below the 32-bit signed integer limit firmware parses [REF-MQTT-ENV], and on
  reaching it wraps back to `SEQUENCE_ID_FLOOR` rather than to 0, so a long session never
  drifts into the low range the printer's own `push_status` counter and other clients use.

- <span id="printerclient-set-command-timeout"></span>`fn set_command_timeout(&mut self, timeout: Option<Duration>)`

  Sets the timeout used by command-response methods like [`get_version()`](#printerclient) and [`get_k_profiles()`](#printerclient); `None` disables it.

  The same value is the deadline after which a fire-and-forget command with no echo
  resolves as [`CommandOutcome::TimedOut`](command/index.md#commandoutcome), measured from its publish. A command keeps the
  deadline in force when it was sent; changing this later does not move it. The default is
  [`DEFAULT_COMMAND_TIMEOUT`](#default-command-timeout) (10 seconds), the same window write-zombie detection allows
  for an echo.

  The timeout needs a real clock ([`with_timer()`](#printerclient)). Without one, and with
  `None`, a wait is bounded only by the printer answering or the connection failing.

- <span id="printerclient-with-command-timeout"></span>`fn with_command_timeout(self, timeout: Option<Duration>) -> Self`

  Builder form of [`set_command_timeout()`](#printerclient).

- <span id="printerclient-request-pushall"></span>`async fn request_pushall(&mut self) -> Result<CommandHandle, Error>` — [`CommandHandle`](command/index.md#commandhandle), [`Error`](../error/index.md#error)

  Requests a full state dump from the printer [REF-MQTT-LIFECYCLE].

  Settles on publish: `pushall` has no echo, the state dump that follows is the answer.

- <span id="printerclient-send-ping"></span>`async fn send_ping(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Dispatches a PINGREQ keep-alive frame to maintain connection liveness.

- <span id="printerclient-keepalive-tick"></span>`async fn keepalive_tick(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Keeps the MQTT connection alive and checks its liveness; call every [`KEEPALIVE_TICK_SECS`](#keepalive-tick-secs).

  For a loop that races [`poll_telemetry()`](#printerclient) against other events
  in `select!`, where a cancelled poll can't be relied on to notice a dead link: sends a
  PINGREQ if one is due, then advances
  [`tick_zombie_check()`](../mqtt/client/index.md#mqttclient) by `KEEPALIVE_TICK_SECS`.

  An `Err` from either step means the connection is unusable — a failed write poisons
  it, and a zombie is dead by definition — so reconnect rather than retry.

- <span id="printerclient-serial"></span>`fn serial(&self) -> &str`

  Returns a reference to the printer's unique hardware serial number.

- <span id="printerclient-model"></span>`fn model(&self) -> PrinterModel` — [`PrinterModel`](../models/index.md#printermodel)

  Returns the resolved printer hardware model.

- <span id="printerclient-quirks"></span>`fn quirks(&self) -> &'static crate::quirks::ModelQuirks` — [`ModelQuirks`](../quirks/index.md#modelquirks)

  Returns the model quirks for this printer's resolved model.

  Equivalent to `client.model().quirks()` but skips the intermediate `model()` call —
  the single entry point for every model-level limit (`nozzle_temp_max()`, axis travel
  bounds, fan/AMS predicates, etc.). `bed_temp_max()` additionally needs the printer's
  mains region, which lives on the client, not the model — see
  [`is_220v_power()`](#printerclient).

  This is the model's static row: it knows the model and nothing about what this printer
  has reported. Quirks whose answer depends on the machine's own report take a
  [`QuirkContext`](../quirks/index.md) and cannot be called from here without one
  — use [`capabilities()`](#printerclient) for those, which supplies it from the cache.

- <span id="printerclient-quirk-context"></span>`fn quirk_context(&self) -> crate::quirks::QuirkContext<'_>` — [`QuirkContext`](../quirks/context/index.md#quirkcontext)

  Builds a [`QuirkContext`](../quirks/index.md) from this client's cached state.

  A snapshot of whatever has been observed so far: `fun2` from the last telemetry carrying
  it, and firmware from the last [`get_version()`](#printerclient). Fields never
  observed stay `None`, which quirks read as "the printer didn't say" rather than as a
  denial.

  Prefer [`capabilities()`](#printerclient) unless you need to hand the context to a
  quirk directly — for instance to ask what a *different* model would answer given this
  printer's report.

- <span id="printerclient-capabilities"></span>`fn capabilities(&self) -> Capabilities<'_>` — [`Capabilities`](capabilities/index.md#capabilities)

  Capability answers for this printer, with its cached telemetry already supplied.

  The entry point for "can this printer do X" — `client.capabilities().foo()` needs no
  arguments and resolves against what the machine has actually reported, where
  `client.quirks().foo(..)` would make you assemble the context yourself. See
  [`Capabilities`](capabilities/index.md#capabilities) for which questions are answered here and which stay on
  [`quirks()`](#printerclient).

  Cheap to build and a snapshot of the cache, so call it per question rather than holding
  one across a [`poll_telemetry()`](#printerclient).

- <span id="printerclient-mqtt"></span>`async fn mqtt(&mut self) -> Result<&mut MqttClient<<MqttTls as >::Stream>, Error>` — [`MqttClient`](../mqtt/client/index.md#mqttclient), [`TlsConnector`](../io/index.md#tlsconnector), [`Error`](../error/index.md#error)

  Returns direct access to the underlying [`MqttClient`](../mqtt/client/index.md#mqttclient), auto-connecting if needed.

  Use this for sending custom MQTT payloads, managing zombie detection via
  [`tick_zombie_check()`](../mqtt/client/index.md#mqttclient), or inspecting
  in-flight state — anything that [`PrinterClient`](#printerclient) doesn't expose directly.

  Pipelining multiple commands through this handle before awaiting a response forfeits
  write-zombie coverage beyond the first outstanding command: `tick_zombie_check()` tracks
  only one armed `(sequence_id, elapsed_secs)` pair at a time, so a second `publish_command`
  issued while the first is still unanswered gets no tracking of its own — if the broker
  acks the first but silently drops the second, the second can hang forever undetected.
  The default [`PrinterClient`](#printerclient) request flow awaits each command in turn and isn't affected.

#### Trait Implementations

### `AirductMode`

```rust
enum AirductMode {
    Cooling,
    Heating,
    Laser,
}
```

Airduct damper operating mode [REF-MQTT-LIFECYCLE].

`Cooling` (0): closes internal recirculation dampers, routes hot air out through exhaust.
`Heating` (1): closes exhaust flaps, seals enclosure for heat retention.
`Laser` (2): configuration for laser engraving module operation.

#### Variants

- **`Cooling`**

  Closes internal recirculation dampers, routes hot air out through exhaust.

- **`Heating`**

  Seals enclosure, closes exhaust flaps for heat retention.

- **`Laser`**

  Laser engraving module configuration.

#### Trait Implementations

##### `impl Clone for AirductMode`

- <span id="airductmode-clone"></span>`fn clone(&self) -> AirductMode` — [`AirductMode`](../mqtt/commands/hardware/index.md#airductmode)

##### `impl Copy for AirductMode`

##### `impl Debug for AirductMode`

- <span id="airductmode-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AirductMode`

##### `impl PartialEq for AirductMode`

- <span id="airductmode-partialeq-eq"></span>`fn eq(&self, other: &AirductMode) -> bool` — [`AirductMode`](../mqtt/commands/hardware/index.md#airductmode)

### `IdleIgnoreScope`

```rust
enum IdleIgnoreScope {
    Once,
    Permanent,
}
```

How long an `idle_ignore` dismissal lasts.

#### Variants

- **`Once`**

  Dismiss this occurrence only (`type: 0`).

- **`Permanent`**

  Never show this warning again (`type: 1`).

#### Trait Implementations

##### `impl Clone for IdleIgnoreScope`

- <span id="idleignorescope-clone"></span>`fn clone(&self) -> IdleIgnoreScope` — [`IdleIgnoreScope`](../mqtt/commands/control/index.md#idleignorescope)

##### `impl Copy for IdleIgnoreScope`

##### `impl Debug for IdleIgnoreScope`

- <span id="idleignorescope-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for IdleIgnoreScope`

##### `impl Hash for IdleIgnoreScope`

- <span id="idleignorescope-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for IdleIgnoreScope`

- <span id="idleignorescope-partialeq-eq"></span>`fn eq(&self, other: &IdleIgnoreScope) -> bool` — [`IdleIgnoreScope`](../mqtt/commands/control/index.md#idleignorescope)

### `Axis`

```rust
enum Axis {
    X,
    Y,
    Z,
}
```

A motion axis.

#### Variants

- **`X`**

  X.

- **`Y`**

  Y.

- **`Z`**

  Z.

#### Implementations

- <span id="axis-const-all"></span>`const ALL: [Axis; 3]`

- <span id="axis-letter"></span>`const fn letter(self) -> char`

  The G-code letter for this axis.

#### Trait Implementations

##### `impl Clone for Axis`

- <span id="axis-clone"></span>`fn clone(&self) -> Axis` — [`Axis`](../quirks/index.md#axis)

##### `impl Copy for Axis`

##### `impl Debug for Axis`

- <span id="axis-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for Axis`

- <span id="axis-display-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

##### `impl Eq for Axis`

##### `impl FromStr for Axis`

- <span id="axis-fromstr-type-err"></span>`type Err = Error`

- <span id="axis-fromstr-from-str"></span>`fn from_str(s: &str) -> Result<Self, Error>` — [`Error`](../error/index.md#error)

  Parses `x`/`y`/`z`, case-insensitively.

##### `impl Hash for Axis`

- <span id="axis-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for Axis`

- <span id="axis-partialeq-eq"></span>`fn eq(&self, other: &Axis) -> bool` — [`Axis`](../quirks/index.md#axis)

##### `impl ToString for Axis`

- <span id="axis-tostring-to-string"></span>`fn to_string(&self) -> String`

### `BuzzerMode`

```rust
enum BuzzerMode {
    Silent,
    Alarm,
    Chirp,
}
```

Buzzer alarm/attention chime mode [REF-MQTT-LIFECYCLE]; supported on models with a physical fire alarm buzzer (H2 series).

#### Variants

- **`Silent`**

  Silent/disarmed.

- **`Alarm`**

  Alarm triggered.

- **`Chirp`**

  Beeping attention chime.

#### Implementations

- <span id="buzzermode-code"></span>`const fn code(self) -> i32`

  The `buzzer_ctrl` `mode` code: `0` silent, `1` alarm, `2` chirp.

#### Trait Implementations

##### `impl Clone for BuzzerMode`

- <span id="buzzermode-clone"></span>`fn clone(&self) -> BuzzerMode` — [`BuzzerMode`](../types/control/index.md#buzzermode)

##### `impl Copy for BuzzerMode`

##### `impl Debug for BuzzerMode`

- <span id="buzzermode-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for BuzzerMode`

##### `impl Hash for BuzzerMode`

- <span id="buzzermode-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for BuzzerMode`

- <span id="buzzermode-partialeq-eq"></span>`fn eq(&self, other: &BuzzerMode) -> bool` — [`BuzzerMode`](../types/control/index.md#buzzermode)

### `FanTarget`

```rust
enum FanTarget {
    PartCooling,
    AuxiliaryLeft,
    ChamberExhaust,
    AuxiliaryLeft2,
}
```

Target onboard cooling fans [REF-CLIM-FANS].

#### Variants

- **`PartCooling`**

  Primary part cooling fan (Port 1).

- **`AuxiliaryLeft`**

  Primary left-side auxiliary fan (Port 2).

- **`ChamberExhaust`**

  Chamber exhaust/filtration fan (Port 3).

- **`AuxiliaryLeft2`**

  Secondary left-side auxiliary fan (Port 10, supported on X2D and P2S) [REF-CLIM-FANS].
  
  Despite the wire port number (M106 `P10`) and read-side airduct id (160) suggesting a
  "right" fan, BambuStudio's `DevFan.h` names decoded id 10 `FAN_REMOTE_COOLING_1_IDX` —
  a second left-side auxiliary fan, distinct from [`AuxiliaryLeft`](../types/control/index.md#fantarget)'s
  primary port-2 fan (`FAN_REMOTE_COOLING_0_IDX`, mirrored into `big_fan1_speed`).
  Confirmed against bambuddy's test suite, which titles this fan "P2S/X2D left auxiliary
  part cooling fan" throughout (issue #60).

#### Implementations

- <span id="fantarget-const-all"></span>`const ALL: &'static [FanTarget]`

- <span id="fantarget-write-port"></span>`const fn write_port(self) -> u16`

  The M106 `P` port that drives this fan.

- <span id="fantarget-airduct-part-id"></span>`const fn airduct_part_id(self) -> Option<u32>`

  The `device.airduct.parts[].id` this fan reports under, for the one fan read from there.

  A different address space from [`write_port`](../types/control/index.md#fantarget): the three other fans
  report through `print.*_fan_speed` strings instead and return `None`.

- <span id="fantarget-is-supported-by"></span>`fn is_supported_by(self, quirks: &crate::quirks::ModelQuirks) -> bool` — [`ModelQuirks`](../quirks/index.md#modelquirks)

  Whether `quirks` says this model has the fan.

#### Trait Implementations

##### `impl Clone for FanTarget`

- <span id="fantarget-clone"></span>`fn clone(&self) -> FanTarget` — [`FanTarget`](../types/control/index.md#fantarget)

##### `impl Copy for FanTarget`

##### `impl Debug for FanTarget`

- <span id="fantarget-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for FanTarget`

##### `impl Hash for FanTarget`

- <span id="fantarget-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for FanTarget`

- <span id="fantarget-partialeq-eq"></span>`fn eq(&self, other: &FanTarget) -> bool` — [`FanTarget`](../types/control/index.md#fantarget)

### `LedNode`

```rust
enum LedNode {
    Chamber,
    Chamber2,
    Work,
}
```

A printer LED fixture addressed by `ledctrl` and reported in `lights_report`.

#### Variants

- **`Chamber`**

  The chamber light (`chamber_light`).

- **`Chamber2`**

  The second chamber light on models with two (`chamber_light2`).

- **`Work`**

  The work light (`work_light`).

#### Implementations

- <span id="lednode-const-all"></span>`const ALL: &'static [LedNode]`

- <span id="lednode-as-wire"></span>`const fn as_wire(self) -> &'static str`

  The value's wire spelling.

#### Trait Implementations

##### `impl Clone for LedNode`

- <span id="lednode-clone"></span>`fn clone(&self) -> LedNode` — [`LedNode`](../types/control/index.md#lednode)

##### `impl Copy for LedNode`

##### `impl Debug for LedNode`

- <span id="lednode-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for LedNode`

- <span id="lednode-display-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

##### `impl Eq for LedNode`

##### `impl FromStr for LedNode`

- <span id="lednode-fromstr-type-err"></span>`type Err = Error`

- <span id="lednode-fromstr-from-str"></span>`fn from_str(s: &str) -> Result<Self, Error>` — [`Error`](../error/index.md#error)

##### `impl Hash for LedNode`

- <span id="lednode-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for LedNode`

- <span id="lednode-partialeq-eq"></span>`fn eq(&self, other: &LedNode) -> bool` — [`LedNode`](../types/control/index.md#lednode)

##### `impl ToString for LedNode`

- <span id="lednode-tostring-to-string"></span>`fn to_string(&self) -> String`

### `LightMode`

```rust
enum LightMode {
    On,
    Off,
    Flashing,
}
```

An LED fixture's mode, as sent in `ledctrl` and reported in `lights_report`.

#### Variants

- **`On`**

  Lit.

- **`Off`**

  Dark.

- **`Flashing`**

  Cycling on a flash timing.

#### Implementations

- <span id="lightmode-const-all"></span>`const ALL: &'static [LightMode]`

- <span id="lightmode-as-wire"></span>`const fn as_wire(self) -> &'static str`

  The value's wire spelling.

#### Trait Implementations

##### `impl Clone for LightMode`

- <span id="lightmode-clone"></span>`fn clone(&self) -> LightMode` — [`LightMode`](../types/control/index.md#lightmode)

##### `impl Copy for LightMode`

##### `impl Debug for LightMode`

- <span id="lightmode-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Display for LightMode`

- <span id="lightmode-display-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

##### `impl Eq for LightMode`

##### `impl FromStr for LightMode`

- <span id="lightmode-fromstr-type-err"></span>`type Err = Error`

- <span id="lightmode-fromstr-from-str"></span>`fn from_str(s: &str) -> Result<Self, Error>` — [`Error`](../error/index.md#error)

##### `impl Hash for LightMode`

- <span id="lightmode-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for LightMode`

- <span id="lightmode-partialeq-eq"></span>`fn eq(&self, other: &LightMode) -> bool` — [`LightMode`](../types/control/index.md#lightmode)

##### `impl ToString for LightMode`

- <span id="lightmode-tostring-to-string"></span>`fn to_string(&self) -> String`

### `PrintSpeed`

```rust
enum PrintSpeed {
    Silent,
    Standard,
    Sport,
    Ludicrous,
}
```

Velocity and acceleration scaling presets for active print jobs [REF-MQTT-LIFECYCLE].

#### Variants

- **`Silent`**

  50% max acceleration and feedrate limits.

- **`Standard`**

  100% nominal feedrate limit.

- **`Sport`**

  124% nominal feedrate limit.

- **`Ludicrous`**

  166% nominal feedrate limit.

#### Implementations

- <span id="printspeed-const-all"></span>`const ALL: &'static [PrintSpeed]`

- <span id="printspeed-from-level"></span>`fn from_level(level: u8) -> Option<Self>`

  Classifies a raw `spd_lvl` telemetry value (`1`-`4`, the same values `print_speed` sends); `None` for an out-of-range level.

- <span id="printspeed-level"></span>`const fn level(self) -> u8`

  The wire level, `1`-`4` — the inverse of [`from_level`](../types/control/index.md#printspeed).

#### Trait Implementations

##### `impl Clone for PrintSpeed`

- <span id="printspeed-clone"></span>`fn clone(&self) -> PrintSpeed` — [`PrintSpeed`](../types/control/index.md#printspeed)

##### `impl Copy for PrintSpeed`

##### `impl Debug for PrintSpeed`

- <span id="printspeed-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for PrintSpeed`

##### `impl Hash for PrintSpeed`

- <span id="printspeed-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for PrintSpeed`

- <span id="printspeed-partialeq-eq"></span>`fn eq(&self, other: &PrintSpeed) -> bool` — [`PrintSpeed`](../types/control/index.md#printspeed)

### `PrintStatus`

```rust
enum PrintStatus {
    Idle,
    Preparing,
    Slicing,
    Running,
    Paused,
    Finished,
    Failed,
    Unknown,
}
```

Decoded classification of the printer's high-level `gcode_state` telemetry field.

`Unknown` covers an unrecognized wire value; callers needing to tell that apart from a known
state should inspect the raw `gcode_state` string directly.

#### Variants

- **`Idle`**

  No print job active or loaded (wire: `"IDLE"`).

- **`Preparing`**

  Print preparing to start — homing, bed leveling, or priming, physical
  motion in progress (wire: `"PREPARE"`).

- **`Slicing`**

  Printer is slicing a job on-device, before any physical motion (wire: `"SLICING"`).
  
  Distinct from [`Preparing`](../types/control/index.md#printstatus): nothing is moving yet. It is still a
  busy state — a job is in flight — so treat it like the other active states when
  deciding whether the printer can accept new work.

- **`Running`**

  Print job actively executing (wire: `"RUNNING"`).

- **`Paused`**

  Print job paused, resumable (wire: `"PAUSE"`).

- **`Finished`**

  Print job completed successfully (wire: `"FINISH"`).

- **`Failed`**

  Print job aborted by an error condition (wire: `"FAILED"`).

- **`Unknown`**

  Unrecognized wire value — see the enum's doc comment.

#### Implementations

- <span id="printstatus-from-gcode-state"></span>`fn from_gcode_state(state: &str) -> Self`

  Classifies a raw `gcode_state` wire value (firmware casing: `"IDLE"`, `"PREPARE"`, `"SLICING"`, `"RUNNING"`, `"PAUSE"`, `"FINISH"`, `"FAILED"` [REF-MQTT-IDLEBUG]).

- <span id="printstatus-as-str"></span>`const fn as_str(self) -> Option<&'static str>`

  The `gcode_state` wire value for this status; `None` for [`Unknown`](../types/control/index.md#printstatus).

- <span id="printstatus-is-busy"></span>`fn is_busy(self) -> bool`

  True while a job is in flight — preparing, slicing, running or paused — so the printer
  shouldn't be given new work or motion that could collide with a part.

  `Unknown` is not busy, so a caller gating on safety must treat a missing status
  (`PrinterClient::print_status() == None`) or `Unknown` as "can't confirm idle" itself.

#### Trait Implementations

##### `impl Clone for PrintStatus`

- <span id="printstatus-clone"></span>`fn clone(&self) -> PrintStatus` — [`PrintStatus`](../types/control/index.md#printstatus)

##### `impl Copy for PrintStatus`

##### `impl Debug for PrintStatus`

- <span id="printstatus-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for PrintStatus`

##### `impl Hash for PrintStatus`

- <span id="printstatus-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for PrintStatus`

- <span id="printstatus-partialeq-eq"></span>`fn eq(&self, other: &PrintStatus) -> bool` — [`PrintStatus`](../types/control/index.md#printstatus)

### `AckExpectation`

```rust
enum AckExpectation {
    Echoes,
    SettlesOnPublish,
}
```

Whether the printer answers a command with an echo of its `sequence_id`.

#### Variants

- **`Echoes`**

  The printer echoes the command.
  
  Confirmed on a P1S for every command bambino sends except `pushall`
  (`reference/03_mqtt_telemetry.md` §REF-MQTT-ACK); other models are unmeasured.

- **`SettlesOnPublish`**

  The printer sends no echo, so publishing is the whole outcome.
  
  `pushall` is the one such command: it triggers a state dump instead
  [REF-MQTT-LIFECYCLE].

#### Trait Implementations

##### `impl Clone for AckExpectation`

- <span id="ackexpectation-clone"></span>`fn clone(&self) -> AckExpectation` — [`AckExpectation`](command/index.md#ackexpectation)

##### `impl Copy for AckExpectation`

##### `impl Debug for AckExpectation`

- <span id="ackexpectation-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AckExpectation`

##### `impl Hash for AckExpectation`

- <span id="ackexpectation-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for AckExpectation`

- <span id="ackexpectation-partialeq-eq"></span>`fn eq(&self, other: &AckExpectation) -> bool` — [`AckExpectation`](command/index.md#ackexpectation)

### `CommandOutcome`

```rust
enum CommandOutcome {
    Accepted,
    Refused(CommandRefusal),
    NoVerdict,
    TimedOut,
    ConnectionLost,
    SettledOnPublish,
}
```

The terminal outcome of one published command.

#### Variants

- **`Accepted`**

  The printer echoed the command with `result: "success"` and no error code.
  
  Confirms receipt only, not that the command had any effect — see the module docs.

- **`Refused`**

  The printer echoed the command with a failure verdict.

- **`NoVerdict`**

  The printer echoed the command without any verdict.
  
  P1S firmware 01.10.00.00 answers some commands with a bare `{command, sequence_id}` and
  no `result`, while refusing them through HMS instead (bambuddy #2732). Receipt is all
  this proves; it is not success.

- **`TimedOut`**

  No echo arrived before the command's deadline.
  
  Not evidence of rejection: the command may still have been executed. Deadlines are
  [`set_command_timeout()`](#printerclient) from publish, and are
  only measured with a real clock ([`with_timer()`](#printerclient)).

- **`ConnectionLost`**

  The MQTT session ended between publish and echo, so no echo can arrive.
  
  Sessions use Clean Session and subscribe afresh, and an echo arrives within milliseconds
  while a reconnect takes seconds, so an answer addressed to the old session is never
  delivered on the new one. The command may still have been executed.

- **`SettledOnPublish`**

  The command never echoes ([`AckExpectation::SettlesOnPublish`](command/index.md#ackexpectation)), so publishing was the
  whole outcome.

#### Trait Implementations

##### `impl Clone for CommandOutcome`

- <span id="commandoutcome-clone"></span>`fn clone(&self) -> CommandOutcome` — [`CommandOutcome`](command/index.md#commandoutcome)

##### `impl Debug for CommandOutcome`

- <span id="commandoutcome-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for CommandOutcome`

##### `impl PartialEq for CommandOutcome`

- <span id="commandoutcome-partialeq-eq"></span>`fn eq(&self, other: &CommandOutcome) -> bool` — [`CommandOutcome`](command/index.md#commandoutcome)

### `TelemetryEvent`

```rust
enum TelemetryEvent {
    Report(Box<crate::types::TelemetryReport>, crate::mqtt::MqttMessage),
    Command(super::command::CommandResolution, Option<crate::mqtt::MqttMessage>),
    Unknown(crate::mqtt::MqttMessage),
}
```

Typed telemetry event from the printer's MQTT channel.

The library deserializes wire payloads into structured types so consumers don't
have to reimplement JSON parsing and model-quirk handling. Raw access is always
available via [`into_raw`](types/index.md#telemetryevent).

#### Variants

- **`Report`**

  State telemetry update (print status, device hardware, or both).

- **`Command`**

  The terminal outcome of a command this client published.
  
  Carries the echo that decided it, or `None` for an outcome no message produced
  ([`CommandOutcome::TimedOut`](command/index.md#commandoutcome), [`CommandOutcome::ConnectionLost`](command/index.md#commandoutcome)).

- **`Unknown`**

  Payload that didn't match any known telemetry structure, including command echoes for
  `sequence_id`s this client did not send (other clients share the report topic).

#### Implementations

- <span id="telemetryevent-into-raw"></span>`fn into_raw(self) -> Option<MqttMessage>` — [`MqttMessage`](../mqtt/client/index.md#mqttmessage)

  Consumes the event and returns the underlying raw MQTT message, if one produced it.

  `None` only for a [`Command`](types/index.md#telemetryevent) outcome that no message produced.

- <span id="telemetryevent-raw"></span>`fn raw(&self) -> Option<&MqttMessage>` — [`MqttMessage`](../mqtt/client/index.md#mqttmessage)

  Returns a reference to the underlying raw MQTT message, if one produced it.

  `None` only for a [`Command`](types/index.md#telemetryevent) outcome that no message produced.

- <span id="telemetryevent-report"></span>`fn report(&self) -> Option<&TelemetryReport>` — [`TelemetryReport`](../types/telemetry/index.md#telemetryreport)

  Returns the typed report if this is a `Report` variant.

- <span id="telemetryevent-command"></span>`fn command(&self) -> Option<&CommandResolution>` — [`CommandResolution`](command/index.md#commandresolution)

  Returns the command resolution if this is a `Command` variant.

#### Trait Implementations

##### `impl Clone for TelemetryEvent`

- <span id="telemetryevent-clone"></span>`fn clone(&self) -> TelemetryEvent` — [`TelemetryEvent`](types/index.md#telemetryevent)

##### `impl Debug for TelemetryEvent`

- <span id="telemetryevent-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`


---

## Constants

### `HOMING_WAIT_TIMEOUT`
```rust
const HOMING_WAIT_TIMEOUT: core::time::Duration;
```

How long [`PrinterClient::wait_for_homing`](#printerclient) waits for a homing cycle to complete.

Homing took up to ~46s across wire-confirmed P1S runs [REF-HOMEFLAG]; 90s leaves margin.

### `DEFAULT_COMMAND_TIMEOUT`
```rust
const DEFAULT_COMMAND_TIMEOUT: core::time::Duration;
```

Default command timeout; override with [`PrinterClient::with_command_timeout`](#printerclient).

### `DEFAULT_CONNECT_TIMEOUT`
```rust
const DEFAULT_CONNECT_TIMEOUT: core::time::Duration;
```

Default bound on each channel's dial+TLS+handshake; override with [`PrinterClient::with_connect_timeout`](#printerclient).

### `KEEPALIVE_TICK_SECS`
```rust
const KEEPALIVE_TICK_SECS: u32 = 15u32;
```

How often to call [`PrinterClient::keepalive_tick`]: half the 30s keepalive this client
advertises in CONNECT, so a missed tick still leaves margin before the broker's 45s cutoff.

### `SEQUENCE_ID_FLOOR`
```rust
const SEQUENCE_ID_FLOOR: u64 = 30_000u64;
```

Lowest `sequence_id` this client mints, above every range another party on the shared report topic is known to use.

Every subscriber receives every client's command echoes on the one report topic, so an id
minted inside someone else's range can be mistaken for theirs, and theirs for ours. The
printer's `push_status` counter and bambuddy's counter both start near 0 (bambuddy also
hardcodes `"0"` for pause/resume/stop), and BambuStudio reserves `20000..30000`
(`DevUtil.h` `STUDIO_START_SEQ_ID`/`STUDIO_END_SEQ_ID`) — it raises an error dialog for any
echo in that range carrying an `err_code`, so an id of ours landing there would pop dialogs
in a user's open BambuStudio.

