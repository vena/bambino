**bambino > mqtt > commands > hardware**

# Module: mqtt::commands::hardware

## Contents

**Structs**

- [`AirductPayload`](#airductpayload) - Redirects internal climate airflows using active damper deflection plates.
- [`AirductRequest`](#airductrequest) - Switches the enclosure airduct damper between cooling, heating, and laser modes.
- [`BuzzerPayload`](#buzzerpayload) - Modifies active alarm or attention chime parameters on the printer cabinet buzzer module.
- [`BuzzerRequest`](#buzzerrequest) - Controls the printer's buzzer alarm mode (silent, alarm, or chirp).
- [`LedCtrlPayload`](#ledctrlpayload) - Chamber illumination and toolhead LED control configurations.
- [`LedCtrlRequest`](#ledctrlrequest) - Turns chamber or toolhead LEDs on or off.
- [`PromptSoundPayload`](#promptsoundpayload) - Controls structural notification sound output via speakers (Supported on A1 and H2D series only).
- [`PromptSoundRequest`](#promptsoundrequest) - Enables or disables the printer's notification sounds.

**Enums**

- [`AirductMode`](#airductmode) - Airduct damper operating mode [REF-MQTT-LIFECYCLE].

---

## bambino::mqtt::commands::hardware::AirductMode

*Enum*

Airduct damper operating mode [REF-MQTT-LIFECYCLE].

`Cooling` (0): closes internal recirculation dampers, routes hot air out through exhaust.
`Heating` (1): closes exhaust flaps, seals enclosure for heat retention.
`Laser` (2): configuration for laser engraving module operation.

**Variants:**
- `Cooling`
- `Heating`
- `Laser`

**Traits:** Eq, Copy

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> AirductMode`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **PartialEq**
  - `fn eq(self: &Self, other: &AirductMode) -> bool`



## bambino::mqtt::commands::hardware::AirductPayload

*Struct*

Redirects internal climate airflows using active damper deflection plates.

**Fields:**
- `command: &'static str`
- `mode_id: i32` - Damper mode: 0=cooling (exhaust), 1=heating (sealed), 2=laser [REF-MQTT-LIFECYCLE].
- `submode: i32`
- `sequence_id: String`

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> AirductPayload`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::mqtt::commands::hardware::AirductRequest

*Struct*

Switches the enclosure airduct damper between cooling, heating, and laser modes.

**Fields:**
- `print: AirductPayload`

**Methods:**

- `fn new(mode: AirductMode, sequence_id: u64) -> Self`

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> AirductRequest`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::mqtt::commands::hardware::BuzzerPayload

*Struct*

Modifies active alarm or attention chime parameters on the printer cabinet buzzer module.

**Fields:**
- `command: &'static str`
- `mode: i32` - Alarm state representation: `0` (Silent), `1` (Alarm), `2` (Chirp/Beep) [REF-MQTT-LIFECYCLE].
- `reason: &'static str`
- `sequence_id: String`

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> BuzzerPayload`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::mqtt::commands::hardware::BuzzerRequest

*Struct*

Controls the printer's buzzer alarm mode (silent, alarm, or chirp).

**Fields:**
- `print: BuzzerPayload`

**Methods:**

- `fn new(mode_code: i32, sequence_id: u64) -> Self`

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> BuzzerRequest`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::mqtt::commands::hardware::LedCtrlPayload

*Struct*

Chamber illumination and toolhead LED control configurations.

**Fields:**
- `command: &'static str`
- `sequence_id: String`
- `led_node: String` - Targets specific physical fixtures (e.g. "chamber_light", "chamber_light2").
- `led_mode: String` - Mode state transitions (e.g., "on", "off", "flashing").
- `led_on_time: u32`
- `led_off_time: u32`
- `loop_times: u32`
- `interval_time: u32`

**Trait Implementations:**

- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> LedCtrlPayload`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`



## bambino::mqtt::commands::hardware::LedCtrlRequest

*Struct*

Turns chamber or toolhead LEDs on or off.

**Fields:**
- `system: LedCtrlPayload`

**Methods:**

- `fn new(led_node: &str, turn_on: bool, sequence_id: u64) -> Self`
- `fn new_flashing(led_node: &str, on_time: u32, off_time: u32, loop_times: u32, interval_time: u32, sequence_id: u64) -> Self` - Builds a flashing-mode LED command with explicit on/off/loop/interval timing

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> LedCtrlRequest`



## bambino::mqtt::commands::hardware::PromptSoundPayload

*Struct*

Controls structural notification sound output via speakers (Supported on A1 and H2D series only).

**Fields:**
- `command: &'static str`
- `sound_enable: bool`
- `sequence_id: String`

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> PromptSoundPayload`
- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::mqtt::commands::hardware::PromptSoundRequest

*Struct*

Enables or disables the printer's notification sounds.

**Fields:**
- `print: PromptSoundPayload`

**Methods:**

- `fn new(enable: bool, sequence_id: u64) -> Self`

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Clone**
  - `fn clone(self: &Self) -> PromptSoundRequest`



