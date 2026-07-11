*[bambino](../../../index.md) / [mqtt](../../index.md) / [commands](../index.md) / [hardware](index.md)*

---

# Module `hardware`

Hardware control commands (LEDs, fans, airduct mode, buzzer, prompt sound).

## Contents

- [Types](#types)
  - [`AirductPayload`](#airductpayload)
  - [`AirductRequest`](#airductrequest)
  - [`BuzzerPayload`](#buzzerpayload)
  - [`BuzzerRequest`](#buzzerrequest)
  - [`LedCtrlPayload`](#ledctrlpayload)
  - [`LedCtrlRequest`](#ledctrlrequest)
  - [`PromptSoundPayload`](#promptsoundpayload)
  - [`PromptSoundRequest`](#promptsoundrequest)
  - [`AirductMode`](#airductmode)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`AirductPayload`](#airductpayload) | struct | Redirects internal climate airflows using active damper deflection plates. |
| [`AirductRequest`](#airductrequest) | struct | Switches the enclosure airduct damper between cooling, heating, and laser modes. |
| [`BuzzerPayload`](#buzzerpayload) | struct | Modifies active alarm or attention chime parameters on the printer cabinet buzzer module. |
| [`BuzzerRequest`](#buzzerrequest) | struct | Controls the printer's buzzer alarm mode (silent, alarm, or chirp). |
| [`LedCtrlPayload`](#ledctrlpayload) | struct | Chamber illumination and toolhead LED control configurations. |
| [`LedCtrlRequest`](#ledctrlrequest) | struct | Turns chamber or toolhead LEDs on or off. |
| [`PromptSoundPayload`](#promptsoundpayload) | struct | Controls structural notification sound output via speakers (Supported on A1, A1 Mini, and A2L only; H2-series buzzer alerts use the separate `buzzer_ctrl` command — see [`BuzzerPayload`]). |
| [`PromptSoundRequest`](#promptsoundrequest) | struct | Enables or disables the printer's notification sounds. |
| [`AirductMode`](#airductmode) | enum | Airduct damper operating mode [REF-MQTT-LIFECYCLE]. |

## Types

### `AirductPayload`

```rust
struct AirductPayload {
    pub command: &'static str,
    pub mode_id: i32,
    pub submode: i32,
    pub sequence_id: String,
}
```

Redirects internal climate airflows using active damper deflection plates.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"set_airduct"`.

- **`mode_id`**: `i32`

  Damper mode: 0=cooling (exhaust), 1=heating (sealed), 2=laser [REF-MQTT-LIFECYCLE].

- **`submode`**: `i32`

  Damper submode; always `-1` (unused) — [`AirductRequest::new`] never sets it otherwise.

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for AirductPayload`

- <span id="airductpayload-clone"></span>`fn clone(&self) -> AirductPayload` — [`AirductPayload`](#airductpayload)

##### `impl Debug for AirductPayload`

- <span id="airductpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AirductPayload`

- <span id="airductpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AirductRequest`

```rust
struct AirductRequest {
    pub print: AirductPayload,
}
```

Switches the enclosure airduct damper between cooling, heating, and laser modes.

#### Fields

- **`print`**: `AirductPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="airductrequest-new"></span>`fn new(mode: AirductMode, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`AirductMode`](#airductmode), [`ClampedTaskId`](../index.md#clampedtaskid)

  Builds a `set_airduct` request for the given damper mode.

#### Trait Implementations

##### `impl Clone for AirductRequest`

- <span id="airductrequest-clone"></span>`fn clone(&self) -> AirductRequest` — [`AirductRequest`](#airductrequest)

##### `impl Debug for AirductRequest`

- <span id="airductrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AirductRequest`

- <span id="airductrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `BuzzerPayload`

```rust
struct BuzzerPayload {
    pub command: &'static str,
    pub mode: i32,
    pub reason: &'static str,
    pub sequence_id: String,
}
```

Modifies active alarm or attention chime parameters on the printer cabinet buzzer module.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"buzzer_ctrl"`.

- **`mode`**: `i32`

  Alarm state representation: `0` (Silent), `1` (Alarm), `2` (Chirp/Beep) [REF-MQTT-LIFECYCLE].

- **`reason`**: `&'static str`

  Reason string shown alongside the alarm; always empty in practice, per [`BuzzerRequest::new`].

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for BuzzerPayload`

- <span id="buzzerpayload-clone"></span>`fn clone(&self) -> BuzzerPayload` — [`BuzzerPayload`](#buzzerpayload)

##### `impl Debug for BuzzerPayload`

- <span id="buzzerpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for BuzzerPayload`

- <span id="buzzerpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `BuzzerRequest`

```rust
struct BuzzerRequest {
    pub print: BuzzerPayload,
}
```

Controls the printer's buzzer alarm mode (silent, alarm, or chirp).

#### Fields

- **`print`**: `BuzzerPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="buzzerrequest-new"></span>`fn new(mode_code: i32, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../index.md#clampedtaskid)

  Builds a `buzzer_ctrl` request for the given alarm mode.

#### Trait Implementations

##### `impl Clone for BuzzerRequest`

- <span id="buzzerrequest-clone"></span>`fn clone(&self) -> BuzzerRequest` — [`BuzzerRequest`](#buzzerrequest)

##### `impl Debug for BuzzerRequest`

- <span id="buzzerrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for BuzzerRequest`

- <span id="buzzerrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `LedCtrlPayload`

```rust
struct LedCtrlPayload {
    pub command: &'static str,
    pub sequence_id: String,
    pub led_node: String,
    pub led_mode: String,
    pub led_on_time: u32,
    pub led_off_time: u32,
    pub loop_times: u32,
    pub interval_time: u32,
}
```

Chamber illumination and toolhead LED control configurations.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"ledctrl"`.

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

- **`led_node`**: `String`

  Targets specific physical fixtures (e.g. "chamber_light", "chamber_light2").

- **`led_mode`**: `String`

  Mode state transitions (e.g., "on", "off", "flashing").

- **`led_on_time`**: `u32`

  On-time per flash cycle (ms); only meaningful in flashing mode.

- **`led_off_time`**: `u32`

  Off-time per flash cycle (ms); only meaningful in flashing mode.

- **`loop_times`**: `u32`

  Number of flash loops; only meaningful in flashing mode.

- **`interval_time`**: `u32`

  Interval between flash cycles (ms); only meaningful in flashing mode.

#### Trait Implementations

##### `impl Clone for LedCtrlPayload`

- <span id="ledctrlpayload-clone"></span>`fn clone(&self) -> LedCtrlPayload` — [`LedCtrlPayload`](#ledctrlpayload)

##### `impl Debug for LedCtrlPayload`

- <span id="ledctrlpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for LedCtrlPayload`

- <span id="ledctrlpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `LedCtrlRequest`

```rust
struct LedCtrlRequest {
    pub system: LedCtrlPayload,
}
```

Turns chamber or toolhead LEDs on or off.

#### Fields

- **`system`**: `LedCtrlPayload`

  The `system` namespace envelope required by the wire protocol.

#### Implementations

- <span id="ledctrlrequest-new"></span>`fn new(led_node: &str, turn_on: bool, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../index.md#clampedtaskid)

  Builds a simple on/off `ledctrl` request for the given fixture.

- <span id="ledctrlrequest-new-flashing"></span>`fn new_flashing(led_node: &str, on_time: u32, off_time: u32, loop_times: u32, interval_time: u32, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../index.md#clampedtaskid)

  Builds a flashing-mode LED command with explicit on/off/loop/interval timing (`led_mode: "flashing"`), per [REF-MQTT-LIFECYCLE].

#### Trait Implementations

##### `impl Clone for LedCtrlRequest`

- <span id="ledctrlrequest-clone"></span>`fn clone(&self) -> LedCtrlRequest` — [`LedCtrlRequest`](#ledctrlrequest)

##### `impl Debug for LedCtrlRequest`

- <span id="ledctrlrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for LedCtrlRequest`

- <span id="ledctrlrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PromptSoundPayload`

```rust
struct PromptSoundPayload {
    pub command: &'static str,
    pub sound_enable: bool,
    pub sequence_id: String,
}
```

Controls structural notification sound output via speakers (Supported on A1, A1 Mini, and A2L only; H2-series buzzer alerts use the separate `buzzer_ctrl` command — see [`BuzzerPayload`](#buzzerpayload)).

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"print_option"`.

- **`sound_enable`**: `bool`

  Whether notification sounds are enabled.

- **`sequence_id`**: `String`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for PromptSoundPayload`

- <span id="promptsoundpayload-clone"></span>`fn clone(&self) -> PromptSoundPayload` — [`PromptSoundPayload`](#promptsoundpayload)

##### `impl Debug for PromptSoundPayload`

- <span id="promptsoundpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for PromptSoundPayload`

- <span id="promptsoundpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PromptSoundRequest`

```rust
struct PromptSoundRequest {
    pub print: PromptSoundPayload,
}
```

Enables or disables the printer's notification sounds.

#### Fields

- **`print`**: `PromptSoundPayload`

  The `print` namespace envelope required by the wire protocol.

#### Implementations

- <span id="promptsoundrequest-new"></span>`fn new(enable: bool, sequence_id: impl Into<ClampedTaskId>) -> Self` — [`ClampedTaskId`](../index.md#clampedtaskid)

  Builds a `print_option` request enabling or disabling notification sounds.

#### Trait Implementations

##### `impl Clone for PromptSoundRequest`

- <span id="promptsoundrequest-clone"></span>`fn clone(&self) -> PromptSoundRequest` — [`PromptSoundRequest`](#promptsoundrequest)

##### `impl Debug for PromptSoundRequest`

- <span id="promptsoundrequest-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for PromptSoundRequest`

- <span id="promptsoundrequest-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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

- <span id="airductmode-clone"></span>`fn clone(&self) -> AirductMode` — [`AirductMode`](#airductmode)

##### `impl Copy for AirductMode`

##### `impl Debug for AirductMode`

- <span id="airductmode-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AirductMode`

##### `impl PartialEq for AirductMode`

- <span id="airductmode-partialeq-eq"></span>`fn eq(&self, other: &AirductMode) -> bool` — [`AirductMode`](#airductmode)

