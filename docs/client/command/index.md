*[bambino](../../index.md) / [client](../index.md) / [command](index.md)*

---

# Module `command`

# Command Handles and Outcomes

Every fire-and-forget [`PrinterClient`](../index.md#printerclient) command returns a
[`CommandHandle`](#commandhandle) naming the `sequence_id` it was published under. The printer answers every
command except `pushall` with an echo carrying that id [REF-MQTT-ACK].
[`poll_telemetry()`](../index.md#printerclient) decodes that echo into a
[`CommandOutcome`](#commandoutcome) and delivers it as
[`TelemetryEvent::Command`](../types/index.md#telemetryevent); when no echo arrives it
reports the command as timed out or lost to a disconnect instead, so every echoing command
ends in exactly one outcome.
[`await_ack()`](../index.md#printerclient) waits for one command's outcome inline.

**An accepted command is a received command, not an executed one.** On a P1S,
`set_airduct` and `buzzer_ctrl` ack `result: "success"` on hardware the printer does not
have, and `project_file` acks success for a file that does not exist
(`reference/03_mqtt_telemetry.md` §REF-MQTT-ACK). Whether a command took effect shows up in
later telemetry, and which field depends on the command — see that section's effect-signal
table.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`CommandHandle`](#commandhandle) | struct | Names a command this client published, for matching the printer's answer to it. |
| [`CommandRefusal`](#commandrefusal) | struct | The printer's stated reasons for refusing a command. |
| [`CommandResolution`](#commandresolution) | struct | A command paired with its terminal outcome. |
| [`AckExpectation`](#ackexpectation) | enum | Whether the printer answers a command with an echo of its `sequence_id`. |
| [`CommandOutcome`](#commandoutcome) | enum | The terminal outcome of one published command. |

## Types

### `CommandHandle`

```rust
struct CommandHandle {
    // [REDACTED: Private Fields]
}
```

Names a command this client published, for matching the printer's answer to it.

Only a [`PrinterClient`](../index.md#printerclient) mints one, so a handle always refers to a
`sequence_id` this client actually sent.

#### Implementations

- <span id="commandhandle-command"></span>`fn command(&self) -> &str`

  Returns the wire command name, e.g. `"gcode_line"` or `"ams_filament_drying"`.

- <span id="commandhandle-sequence-id"></span>`fn sequence_id(&self) -> u32`

  Returns the `sequence_id` the command was published under, which the printer echoes back.

- <span id="commandhandle-ack"></span>`fn ack(&self) -> AckExpectation` — [`AckExpectation`](#ackexpectation)

  Returns whether an echo is coming for this command.

#### Trait Implementations

##### `impl Clone for CommandHandle`

- <span id="commandhandle-clone"></span>`fn clone(&self) -> CommandHandle` — [`CommandHandle`](#commandhandle)

##### `impl Debug for CommandHandle`

- <span id="commandhandle-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for CommandHandle`

##### `impl Hash for CommandHandle`

- <span id="commandhandle-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for CommandHandle`

- <span id="commandhandle-partialeq-eq"></span>`fn eq(&self, other: &CommandHandle) -> bool` — [`CommandHandle`](#commandhandle)

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
  [`decoded_error()`](#commandrefusal).

- **`errno`**: `Option<i32>`

  Non-zero per-command code.
  
  For `ams_change_filament`, `-2` means the chamber and `-4` the AMS is too hot to load the
  filament without softening it; the echo's `soft_temp` field, when present, is the limit
  in °C (BambuStudio `DeviceManager.cpp:2993-3016`).

#### Implementations

- <span id="commandrefusal-decoded-error"></span>`fn decoded_error(&self) -> Option<DecodedPrintError>` — [`DecodedPrintError`](../../diagnostics/hms/index.md#decodedprinterror)

  Decodes [`err_code`](#commandrefusal) into its `MMMM_CCCC` short code.

#### Trait Implementations

##### `impl Clone for CommandRefusal`

- <span id="commandrefusal-clone"></span>`fn clone(&self) -> CommandRefusal` — [`CommandRefusal`](#commandrefusal)

##### `impl Debug for CommandRefusal`

- <span id="commandrefusal-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for CommandRefusal`

##### `impl PartialEq for CommandRefusal`

- <span id="commandrefusal-partialeq-eq"></span>`fn eq(&self, other: &CommandRefusal) -> bool` — [`CommandRefusal`](#commandrefusal)

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

- <span id="commandresolution-clone"></span>`fn clone(&self) -> CommandResolution` — [`CommandResolution`](#commandresolution)

##### `impl Debug for CommandResolution`

- <span id="commandresolution-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for CommandResolution`

##### `impl PartialEq for CommandResolution`

- <span id="commandresolution-partialeq-eq"></span>`fn eq(&self, other: &CommandResolution) -> bool` — [`CommandResolution`](#commandresolution)

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

- <span id="ackexpectation-clone"></span>`fn clone(&self) -> AckExpectation` — [`AckExpectation`](#ackexpectation)

##### `impl Copy for AckExpectation`

##### `impl Debug for AckExpectation`

- <span id="ackexpectation-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AckExpectation`

##### `impl Hash for AckExpectation`

- <span id="ackexpectation-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for AckExpectation`

- <span id="ackexpectation-partialeq-eq"></span>`fn eq(&self, other: &AckExpectation) -> bool` — [`AckExpectation`](#ackexpectation)

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
  [`set_command_timeout()`](../index.md#printerclient) from publish, and are
  only measured with a real clock ([`with_timer()`](../index.md#printerclient)).

- **`ConnectionLost`**

  The MQTT session ended between publish and echo, so no echo can arrive.
  
  Sessions use Clean Session and subscribe afresh, and an echo arrives within milliseconds
  while a reconnect takes seconds, so an answer addressed to the old session is never
  delivered on the new one. The command may still have been executed.

- **`SettledOnPublish`**

  The command never echoes ([`AckExpectation::SettlesOnPublish`](#ackexpectation)), so publishing was the
  whole outcome.

#### Trait Implementations

##### `impl Clone for CommandOutcome`

- <span id="commandoutcome-clone"></span>`fn clone(&self) -> CommandOutcome` — [`CommandOutcome`](#commandoutcome)

##### `impl Debug for CommandOutcome`

- <span id="commandoutcome-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for CommandOutcome`

##### `impl PartialEq for CommandOutcome`

- <span id="commandoutcome-partialeq-eq"></span>`fn eq(&self, other: &CommandOutcome) -> bool` — [`CommandOutcome`](#commandoutcome)

