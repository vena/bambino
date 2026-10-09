*[bambino](../../../index.md) / [mqtt](../../index.md) / [commands](../index.md) / [hardware](index.md)*

---

# Module `hardware`

Hardware control commands (LEDs, fans, airduct mode, buzzer, `print_option` settings).

## Contents

- [Types](#types)
  - [`AirPrintDetectPayload`](#airprintdetectpayload)
  - [`AirPurificationPayload`](#airpurificationpayload)
  - [`AirductPayload`](#airductpayload)
  - [`AutoRecoveryPayload`](#autorecoverypayload)
  - [`BuzzerPayload`](#buzzerpayload)
  - [`DoorOpenCheckPayload`](#dooropencheckpayload)
  - [`FilamentBackupPayload`](#filamentbackuppayload)
  - [`FilamentTangleDetectPayload`](#filamenttangledetectpayload)
  - [`FlashTiming`](#flashtiming)
  - [`IdleHeatingProtectionPayload`](#idleheatingprotectionpayload)
  - [`LedCtrlPayload`](#ledctrlpayload)
  - [`NozzleBlobDetectPayload`](#nozzleblobdetectpayload)
  - [`PromptSoundPayload`](#promptsoundpayload)
  - [`SmartNozzleBlobDetectPayload`](#smartnozzleblobdetectpayload)
  - [`StoreSentFilesPayload`](#storesentfilespayload)
  - [`XcamControlPayload`](#xcamcontrolpayload)
  - [`AirductMode`](#airductmode)
  - [`AirPrintDetectRequest`](#airprintdetectrequest)
  - [`AirPurificationRequest`](#airpurificationrequest)
  - [`AirductRequest`](#airductrequest)
  - [`AutoRecoveryRequest`](#autorecoveryrequest)
  - [`BuzzerRequest`](#buzzerrequest)
  - [`DoorOpenCheckRequest`](#dooropencheckrequest)
  - [`FilamentBackupRequest`](#filamentbackuprequest)
  - [`FilamentTangleDetectRequest`](#filamenttangledetectrequest)
  - [`IdleHeatingProtectionRequest`](#idleheatingprotectionrequest)
  - [`LedCtrlRequest`](#ledctrlrequest)
  - [`NozzleBlobDetectRequest`](#nozzleblobdetectrequest)
  - [`PromptSoundRequest`](#promptsoundrequest)
  - [`SmartNozzleBlobDetectRequest`](#smartnozzleblobdetectrequest)
  - [`StoreSentFilesRequest`](#storesentfilesrequest)
  - [`XcamControlRequest`](#xcamcontrolrequest)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`AirPrintDetectPayload`](#airprintdetectpayload) | struct | Turns non-visual air-printing detection on or off. |
| [`AirPurificationPayload`](#airpurificationpayload) | struct | Sets where chamber air is purified at the end of a print. |
| [`AirductPayload`](#airductpayload) | struct | Redirects internal climate airflows using active damper deflection plates. |
| [`AutoRecoveryPayload`](#autorecoverypayload) | struct | Turns step-loss auto-recovery on or off. |
| [`BuzzerPayload`](#buzzerpayload) | struct | Modifies active alarm or attention chime parameters on the printer cabinet buzzer module. |
| [`DoorOpenCheckPayload`](#dooropencheckpayload) | struct | Sets what the printer does when its door opens mid-print. |
| [`FilamentBackupPayload`](#filamentbackuppayload) | struct | Turns AMS Filament Backup (auto-refill from a matching spool) on or off. |
| [`FilamentTangleDetectPayload`](#filamenttangledetectpayload) | struct | Turns filament tangle detection on or off. |
| [`FlashTiming`](#flashtiming) | struct | Flash cycle timing for [`LedCtrlRequest::new_flashing`](#ledctrlrequest); every field is in milliseconds except `loops`. |
| [`IdleHeatingProtectionPayload`](#idleheatingprotectionpayload) | struct | Turns idle heating protection on or off. |
| [`LedCtrlPayload`](#ledctrlpayload) | struct | Chamber illumination and toolhead LED control configurations. |
| [`NozzleBlobDetectPayload`](#nozzleblobdetectpayload) | struct | Turns nozzle blob detection (the original, on/off form) on or off. |
| [`PromptSoundPayload`](#promptsoundpayload) | struct | Turns prompt notification sounds on or off; BambuStudio's model profiles enable it on A1, A1 Mini and A2L, and a printer can report support itself. |
| [`SmartNozzleBlobDetectPayload`](#smartnozzleblobdetectpayload) | struct | Sets the smart nozzle blob detection mode (off, on, or auto). |
| [`StoreSentFilesPayload`](#storesentfilespayload) | struct | Sets whether files sent from Bambu Studio, Bambu Handy and MakerWorld are kept on external storage. |
| [`XcamControlPayload`](#xcamcontrolpayload) | struct | Turns one camera detector on or off, optionally with its halt sensitivity. |
| [`AirductMode`](#airductmode) | enum | Airduct damper operating mode [REF-MQTT-LIFECYCLE]. |
| [`AirPrintDetectRequest`](#airprintdetectrequest) | type | Enables or disables non-visual air-printing detection. |
| [`AirPurificationRequest`](#airpurificationrequest) | type | Sets the end-of-print air purification mode. |
| [`AirductRequest`](#airductrequest) | type | Switches the enclosure airduct damper between cooling, heating, and laser modes. |
| [`AutoRecoveryRequest`](#autorecoveryrequest) | type | Enables or disables step-loss auto-recovery. |
| [`BuzzerRequest`](#buzzerrequest) | type | Controls the printer's buzzer alarm mode (silent, alarm, or chirp). |
| [`DoorOpenCheckRequest`](#dooropencheckrequest) | type | Sets the door-open check mode (BambuStudio `MachineObject::command_set_door_open_check`). |
| [`FilamentBackupRequest`](#filamentbackuprequest) | type | Enables or disables AMS Filament Backup. |
| [`FilamentTangleDetectRequest`](#filamenttangledetectrequest) | type | Enables or disables filament tangle detection. |
| [`IdleHeatingProtectionRequest`](#idleheatingprotectionrequest) | type | Turns idle heating protection on or off (BambuStudio `DevPrintOptions::command_set_against_continued_heating_mode`). |
| [`LedCtrlRequest`](#ledctrlrequest) | type | Turns chamber or toolhead LEDs on or off. |
| [`NozzleBlobDetectRequest`](#nozzleblobdetectrequest) | type | Enables or disables nozzle blob detection. |
| [`PromptSoundRequest`](#promptsoundrequest) | type | Enables or disables the printer's notification sounds. |
| [`SmartNozzleBlobDetectRequest`](#smartnozzleblobdetectrequest) | type | Sets the smart nozzle blob detection mode. |
| [`StoreSentFilesRequest`](#storesentfilesrequest) | type | Sets whether sent files are kept on external storage (BambuStudio `MachineObject::command_set_save_remote_print_file_to_storage`). |
| [`XcamControlRequest`](#xcamcontrolrequest) | type | Turns a camera detector on or off (BambuStudio `DevPrintOptions::command_xcam_control`). |

## Types

### `AirPrintDetectPayload`

```rust
struct AirPrintDetectPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
    pub air_print_detect: bool,
}
```

Turns non-visual air-printing detection on or off.

Not the camera's AI air-printing detector, which `xcam_control_set` drives.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"print_option"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

- **`air_print_detect`**: `bool`

  Whether air-printing detection is enabled.

#### Trait Implementations

##### `impl Clone for AirPrintDetectPayload`

- <span id="airprintdetectpayload-clone"></span>`fn clone(&self) -> AirPrintDetectPayload` — [`AirPrintDetectPayload`](#airprintdetectpayload)

##### `impl Debug for AirPrintDetectPayload`

- <span id="airprintdetectpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AirPrintDetectPayload`

- <span id="airprintdetectpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `AirPurificationPayload`

```rust
struct AirPurificationPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
    pub air_purification: u8,
}
```

Sets where chamber air is purified at the end of a print.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"print_option"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

- **`air_purification`**: `u8`

  The mode's code — see [`AirPurificationMode::code`](../../../types/control/index.md#airpurificationmode).

#### Trait Implementations

##### `impl Clone for AirPurificationPayload`

- <span id="airpurificationpayload-clone"></span>`fn clone(&self) -> AirPurificationPayload` — [`AirPurificationPayload`](#airpurificationpayload)

##### `impl Debug for AirPurificationPayload`

- <span id="airpurificationpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AirPurificationPayload`

- <span id="airpurificationpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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

### `AutoRecoveryPayload`

```rust
struct AutoRecoveryPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
    pub option: u32,
    pub auto_recovery: bool,
}
```

Turns step-loss auto-recovery on or off.

Carries the setting twice, as BambuStudio's `command_set_printing_option` does
(`DeviceManager.cpp`): as bit `PRINT_OP_AUTO_RECOVERY` (0) of `option` and as
`auto_recovery`. bambuddy sends only `auto_recovery`.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"print_option"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

- **`option`**: `u32`

  The setting as a bit of the legacy option bitmask.

- **`auto_recovery`**: `bool`

  Whether auto-recovery is enabled.

#### Trait Implementations

##### `impl Clone for AutoRecoveryPayload`

- <span id="autorecoverypayload-clone"></span>`fn clone(&self) -> AutoRecoveryPayload` — [`AutoRecoveryPayload`](#autorecoverypayload)

##### `impl Debug for AutoRecoveryPayload`

- <span id="autorecoverypayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for AutoRecoveryPayload`

- <span id="autorecoverypayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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

### `DoorOpenCheckPayload`

```rust
struct DoorOpenCheckPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
    pub config: u8,
}
```

Sets what the printer does when its door opens mid-print.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"set_door_stat"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

- **`config`**: `u8`

  The mode's code — see [`DoorOpenCheck::code`](../../../types/control/index.md#dooropencheck).

#### Trait Implementations

##### `impl Clone for DoorOpenCheckPayload`

- <span id="dooropencheckpayload-clone"></span>`fn clone(&self) -> DoorOpenCheckPayload` — [`DoorOpenCheckPayload`](#dooropencheckpayload)

##### `impl Debug for DoorOpenCheckPayload`

- <span id="dooropencheckpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for DoorOpenCheckPayload`

- <span id="dooropencheckpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `FilamentBackupPayload`

```rust
struct FilamentBackupPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
    pub auto_switch_filament: bool,
}
```

Turns AMS Filament Backup (auto-refill from a matching spool) on or off.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"print_option"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

- **`auto_switch_filament`**: `bool`

  Whether Filament Backup is enabled.

#### Trait Implementations

##### `impl Clone for FilamentBackupPayload`

- <span id="filamentbackuppayload-clone"></span>`fn clone(&self) -> FilamentBackupPayload` — [`FilamentBackupPayload`](#filamentbackuppayload)

##### `impl Debug for FilamentBackupPayload`

- <span id="filamentbackuppayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for FilamentBackupPayload`

- <span id="filamentbackuppayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `FilamentTangleDetectPayload`

```rust
struct FilamentTangleDetectPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
    pub filament_tangle_detect: bool,
}
```

Turns filament tangle detection on or off.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"print_option"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

- **`filament_tangle_detect`**: `bool`

  Whether tangle detection is enabled.

#### Trait Implementations

##### `impl Clone for FilamentTangleDetectPayload`

- <span id="filamenttangledetectpayload-clone"></span>`fn clone(&self) -> FilamentTangleDetectPayload` — [`FilamentTangleDetectPayload`](#filamenttangledetectpayload)

##### `impl Debug for FilamentTangleDetectPayload`

- <span id="filamenttangledetectpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for FilamentTangleDetectPayload`

- <span id="filamenttangledetectpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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

### `IdleHeatingProtectionPayload`

```rust
struct IdleHeatingProtectionPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
    pub enable: bool,
}
```

Turns idle heating protection on or off.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"set_against_continued_heating_mode"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

- **`enable`**: `bool`

  Whether idle heating protection is enabled.

#### Trait Implementations

##### `impl Clone for IdleHeatingProtectionPayload`

- <span id="idleheatingprotectionpayload-clone"></span>`fn clone(&self) -> IdleHeatingProtectionPayload` — [`IdleHeatingProtectionPayload`](#idleheatingprotectionpayload)

##### `impl Debug for IdleHeatingProtectionPayload`

- <span id="idleheatingprotectionpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for IdleHeatingProtectionPayload`

- <span id="idleheatingprotectionpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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

### `NozzleBlobDetectPayload`

```rust
struct NozzleBlobDetectPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
    pub nozzle_blob_detect: bool,
}
```

Turns nozzle blob detection (the original, on/off form) on or off.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"print_option"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

- **`nozzle_blob_detect`**: `bool`

  Whether nozzle blob detection is enabled.

#### Trait Implementations

##### `impl Clone for NozzleBlobDetectPayload`

- <span id="nozzleblobdetectpayload-clone"></span>`fn clone(&self) -> NozzleBlobDetectPayload` — [`NozzleBlobDetectPayload`](#nozzleblobdetectpayload)

##### `impl Debug for NozzleBlobDetectPayload`

- <span id="nozzleblobdetectpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for NozzleBlobDetectPayload`

- <span id="nozzleblobdetectpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PromptSoundPayload`

```rust
struct PromptSoundPayload {
    pub command: &'static str,
    pub sound_enable: bool,
    pub sequence_id: super::ClampedTaskId,
}
```

Turns prompt notification sounds on or off; BambuStudio's model profiles enable it on A1, A1 Mini and A2L, and a printer can report support itself.

See [`ModelQuirks::prompt_sound_support`](../../../quirks/index.md#modelquirks).
H2-series buzzer alerts use the separate `buzzer_ctrl` command — see [`BuzzerPayload`](#buzzerpayload).

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

### `SmartNozzleBlobDetectPayload`

```rust
struct SmartNozzleBlobDetectPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
    pub nozzle_blob_detect_v2: u8,
}
```

Sets the smart nozzle blob detection mode (off, on, or auto).

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"print_option"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

- **`nozzle_blob_detect_v2`**: `u8`

  The mode's code — see [`NozzleBlobDetectMode::code`](../../../types/control/index.md#nozzleblobdetectmode).

#### Trait Implementations

##### `impl Clone for SmartNozzleBlobDetectPayload`

- <span id="smartnozzleblobdetectpayload-clone"></span>`fn clone(&self) -> SmartNozzleBlobDetectPayload` — [`SmartNozzleBlobDetectPayload`](#smartnozzleblobdetectpayload)

##### `impl Debug for SmartNozzleBlobDetectPayload`

- <span id="smartnozzleblobdetectpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for SmartNozzleBlobDetectPayload`

- <span id="smartnozzleblobdetectpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `StoreSentFilesPayload`

```rust
struct StoreSentFilesPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
    pub config: bool,
}
```

Sets whether files sent from Bambu Studio, Bambu Handy and MakerWorld are kept on external storage.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"print_cache_set"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

- **`config`**: `bool`

  Whether sent files are kept.

#### Trait Implementations

##### `impl Clone for StoreSentFilesPayload`

- <span id="storesentfilespayload-clone"></span>`fn clone(&self) -> StoreSentFilesPayload` — [`StoreSentFilesPayload`](#storesentfilespayload)

##### `impl Debug for StoreSentFilesPayload`

- <span id="storesentfilespayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for StoreSentFilesPayload`

- <span id="storesentfilespayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `XcamControlPayload`

```rust
struct XcamControlPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
    pub module_name: &'static str,
    pub control: bool,
    pub enable: bool,
    pub print_halt: bool,
    pub halt_print_sensitivity: Option<&'static str>,
}
```

Turns one camera detector on or off, optionally with its halt sensitivity.

`enable` and `print_halt` are the old protocol's fields; BambuStudio still sends both, with
`print_halt` always `true` (`DevPrintOptions::command_xcam_control`).

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"xcam_control_set"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

- **`module_name`**: `&'static str`

  The detector addressed — see [`XcamModule`](../../../types/control/index.md#xcammodule).

- **`control`**: `bool`

  Whether the detector runs.

- **`enable`**: `bool`

  Old-protocol copy of `control`.

- **`print_halt`**: `bool`

  Old-protocol flag, always `true`.

- **`halt_print_sensitivity`**: `Option<&'static str>`

  The halt sensitivity — see [`XcamHaltSensitivity`](../../../types/control/index.md#xcamhaltsensitivity); omitted when `None`.

#### Trait Implementations

##### `impl Clone for XcamControlPayload`

- <span id="xcamcontrolpayload-clone"></span>`fn clone(&self) -> XcamControlPayload` — [`XcamControlPayload`](#xcamcontrolpayload)

##### `impl Debug for XcamControlPayload`

- <span id="xcamcontrolpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for XcamControlPayload`

- <span id="xcamcontrolpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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

### `AirPrintDetectRequest`

```rust
type AirPrintDetectRequest = super::Print<AirPrintDetectPayload>;
```

Enables or disables non-visual air-printing detection.

### `AirPurificationRequest`

```rust
type AirPurificationRequest = super::Print<AirPurificationPayload>;
```

Sets the end-of-print air purification mode.

### `AirductRequest`

```rust
type AirductRequest = super::Print<AirductPayload>;
```

Switches the enclosure airduct damper between cooling, heating, and laser modes.

### `AutoRecoveryRequest`

```rust
type AutoRecoveryRequest = super::Print<AutoRecoveryPayload>;
```

Enables or disables step-loss auto-recovery.

### `BuzzerRequest`

```rust
type BuzzerRequest = super::Print<BuzzerPayload>;
```

Controls the printer's buzzer alarm mode (silent, alarm, or chirp).

### `DoorOpenCheckRequest`

```rust
type DoorOpenCheckRequest = super::System<DoorOpenCheckPayload>;
```

Sets the door-open check mode (BambuStudio `MachineObject::command_set_door_open_check`).

### `FilamentBackupRequest`

```rust
type FilamentBackupRequest = super::Print<FilamentBackupPayload>;
```

Enables or disables AMS Filament Backup.

### `FilamentTangleDetectRequest`

```rust
type FilamentTangleDetectRequest = super::Print<FilamentTangleDetectPayload>;
```

Enables or disables filament tangle detection.

### `IdleHeatingProtectionRequest`

```rust
type IdleHeatingProtectionRequest = super::Print<IdleHeatingProtectionPayload>;
```

Turns idle heating protection on or off (BambuStudio `DevPrintOptions::command_set_against_continued_heating_mode`).

### `LedCtrlRequest`

```rust
type LedCtrlRequest = super::System<LedCtrlPayload>;
```

Turns chamber or toolhead LEDs on or off.

### `NozzleBlobDetectRequest`

```rust
type NozzleBlobDetectRequest = super::Print<NozzleBlobDetectPayload>;
```

Enables or disables nozzle blob detection.

### `PromptSoundRequest`

```rust
type PromptSoundRequest = super::Print<PromptSoundPayload>;
```

Enables or disables the printer's notification sounds.

### `SmartNozzleBlobDetectRequest`

```rust
type SmartNozzleBlobDetectRequest = super::Print<SmartNozzleBlobDetectPayload>;
```

Sets the smart nozzle blob detection mode.

### `StoreSentFilesRequest`

```rust
type StoreSentFilesRequest = super::System<StoreSentFilesPayload>;
```

Sets whether sent files are kept on external storage (BambuStudio `MachineObject::command_set_save_remote_print_file_to_storage`).

### `XcamControlRequest`

```rust
type XcamControlRequest = super::Xcam<XcamControlPayload>;
```

Turns a camera detector on or off (BambuStudio `DevPrintOptions::command_xcam_control`).

