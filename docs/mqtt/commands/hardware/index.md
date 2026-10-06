*[bambino](../../../index.md) / [mqtt](../../index.md) / [commands](../index.md) / [hardware](index.md)*

---

# Module `hardware`

Hardware control commands (LEDs, fans, airduct mode, buzzer, prompt sound).

## Contents

- [Types](#types)
  - [`AirductPayload`](#airductpayload)
  - [`BuzzerPayload`](#buzzerpayload)
  - [`FlashTiming`](#flashtiming)
  - [`LedCtrlPayload`](#ledctrlpayload)
  - [`PromptSoundPayload`](#promptsoundpayload)
  - [`AirductMode`](#airductmode)
  - [`AirductRequest`](#airductrequest)
  - [`BuzzerRequest`](#buzzerrequest)
  - [`LedCtrlRequest`](#ledctrlrequest)
  - [`PromptSoundRequest`](#promptsoundrequest)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`AirductPayload`](#airductpayload) | struct | Redirects internal climate airflows using active damper deflection plates. |
| [`BuzzerPayload`](#buzzerpayload) | struct | Modifies active alarm or attention chime parameters on the printer cabinet buzzer module. |
| [`FlashTiming`](#flashtiming) | struct | Flash cycle timing for [`LedCtrlRequest::new_flashing`](#ledctrlrequest); every field is in milliseconds except `loops`. |
| [`LedCtrlPayload`](#ledctrlpayload) | struct | Chamber illumination and toolhead LED control configurations. |
| [`PromptSoundPayload`](#promptsoundpayload) | struct | Controls structural notification sound output via speakers (Supported on A1, A1 Mini, and A2L only; H2-series buzzer alerts use the separate `buzzer_ctrl` command — see [`BuzzerPayload`](#buzzerpayload)). |
| [`AirductMode`](#airductmode) | enum | Airduct damper operating mode [REF-MQTT-LIFECYCLE]. |
| [`AirductRequest`](#airductrequest) | type | Switches the enclosure airduct damper between cooling, heating, and laser modes. |
| [`BuzzerRequest`](#buzzerrequest) | type | Controls the printer's buzzer alarm mode (silent, alarm, or chirp). |
| [`LedCtrlRequest`](#ledctrlrequest) | type | Turns chamber or toolhead LEDs on or off. |
| [`PromptSoundRequest`](#promptsoundrequest) | type | Enables or disables the printer's notification sounds. |

## Types

### `AirductPayload`

```rust
struct AirductPayload {
    pub command: &'static str,
    pub mode_id: i32,
    pub submode: i32,
    pub sequence_id: super::ClampedTaskId,
}
```

Redirects internal climate airflows using active damper deflection plates.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"set_airduct"`.

- **`mode_id`**: `i32`

  Damper mode: 0=cooling (exhaust), 1=heating (sealed), 2=laser [REF-MQTT-LIFECYCLE].

- **`submode`**: `i32`

  Damper submode; always `-1` (unused) — [`AirductRequest::new`](#airductrequest) never sets it otherwise.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for AirductPayload`

- <span id="airductpayload-clone"></span>`fn clone(&self) -> AirductPayload` — [`AirductPayload`](#airductpayload)

##### `impl Debug for AirductPayload`

- <span id="airductpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AirductPayload`

- <span id="airductpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `BuzzerPayload`

```rust
struct BuzzerPayload {
    pub command: &'static str,
    pub mode: i32,
    pub reason: &'static str,
    pub sequence_id: super::ClampedTaskId,
}
```

Modifies active alarm or attention chime parameters on the printer cabinet buzzer module.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"buzzer_ctrl"`.

- **`mode`**: `i32`

  Alarm state representation: `0` (Silent), `1` (Alarm), `2` (Chirp/Beep) [REF-MQTT-LIFECYCLE].

- **`reason`**: `&'static str`

  Reason string shown alongside the alarm; always empty in practice, per [`BuzzerRequest::new`](#buzzerrequest).

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for BuzzerPayload`

- <span id="buzzerpayload-clone"></span>`fn clone(&self) -> BuzzerPayload` — [`BuzzerPayload`](#buzzerpayload)

##### `impl Debug for BuzzerPayload`

- <span id="buzzerpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for BuzzerPayload`

- <span id="buzzerpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `FlashTiming`

```rust
struct FlashTiming {
    pub on_ms: u32,
    pub off_ms: u32,
    pub loops: u32,
    pub interval_ms: u32,
}
```

Flash cycle timing for [`LedCtrlRequest::new_flashing`](#ledctrlrequest); every field is in milliseconds except `loops`.

#### Fields

- **`on_ms`**: `u32`

  Time lit per cycle, in ms.

- **`off_ms`**: `u32`

  Time dark per cycle, in ms.

- **`loops`**: `u32`

  Number of cycles.

- **`interval_ms`**: `u32`

  Pause between cycles, in ms.

#### Trait Implementations

##### `impl Clone for FlashTiming`

- <span id="flashtiming-clone"></span>`fn clone(&self) -> FlashTiming` — [`FlashTiming`](#flashtiming)

##### `impl Copy for FlashTiming`

##### `impl Debug for FlashTiming`

- <span id="flashtiming-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Default for FlashTiming`

- <span id="flashtiming-default"></span>`fn default() -> FlashTiming` — [`FlashTiming`](#flashtiming)

##### `impl Eq for FlashTiming`

##### `impl Hash for FlashTiming`

- <span id="flashtiming-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for FlashTiming`

- <span id="flashtiming-partialeq-eq"></span>`fn eq(&self, other: &FlashTiming) -> bool` — [`FlashTiming`](#flashtiming)

### `LedCtrlPayload`

```rust
struct LedCtrlPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
    pub led_node: &'static str,
    pub led_mode: &'static str,
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

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

- **`led_node`**: `&'static str`

  The fixture addressed — see [`LedNode`](../../../types/control/index.md#lednode).

- **`led_mode`**: `&'static str`

  The mode set — see [`LightMode`](../../../types/control/index.md#lightmode).

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

### `PromptSoundPayload`

```rust
struct PromptSoundPayload {
    pub command: &'static str,
    pub sound_enable: bool,
    pub sequence_id: super::ClampedTaskId,
}
```

Controls structural notification sound output via speakers (Supported on A1, A1 Mini, and A2L only; H2-series buzzer alerts use the separate `buzzer_ctrl` command — see [`BuzzerPayload`](#buzzerpayload)).

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"print_option"`.

- **`sound_enable`**: `bool`

  Whether notification sounds are enabled.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

#### Trait Implementations

##### `impl Clone for PromptSoundPayload`

- <span id="promptsoundpayload-clone"></span>`fn clone(&self) -> PromptSoundPayload` — [`PromptSoundPayload`](#promptsoundpayload)

##### `impl Debug for PromptSoundPayload`

- <span id="promptsoundpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for PromptSoundPayload`

- <span id="promptsoundpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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

### `AirductRequest`

```rust
type AirductRequest = super::Print<AirductPayload>;
```

Switches the enclosure airduct damper between cooling, heating, and laser modes.

### `BuzzerRequest`

```rust
type BuzzerRequest = super::Print<BuzzerPayload>;
```

Controls the printer's buzzer alarm mode (silent, alarm, or chirp).

### `LedCtrlRequest`

```rust
type LedCtrlRequest = super::System<LedCtrlPayload>;
```

Turns chamber or toolhead LEDs on or off.

### `PromptSoundRequest`

```rust
type PromptSoundRequest = super::Print<PromptSoundPayload>;
```

Enables or disables the printer's notification sounds.

