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
  - [`NonVisualAirPrintDetectPayload`](#nonvisualairprintdetectpayload)
  - [`NozzleBlobDetectPayload`](#nozzleblobdetectpayload)
  - [`PartControlPayload`](#partcontrolpayload)
  - [`PromptSoundPayload`](#promptsoundpayload)
  - [`SmartNozzleBlobDetectPayload`](#smartnozzleblobdetectpayload)
  - [`SntpPayload`](#sntppayload)
  - [`StoreSentFilesPayload`](#storesentfilespayload)
  - [`XcamControlPayload`](#xcamcontrolpayload)
  - [`AirductMode`](#airductmode)
  - [`PurifierGear`](#purifiergear)
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
  - [`NonVisualAirPrintDetectRequest`](#nonvisualairprintdetectrequest)
  - [`NozzleBlobDetectRequest`](#nozzleblobdetectrequest)
  - [`PartControlRequest`](#partcontrolrequest)
  - [`PromptSoundRequest`](#promptsoundrequest)
  - [`SmartNozzleBlobDetectRequest`](#smartnozzleblobdetectrequest)
  - [`SntpRequest`](#sntprequest)
  - [`StoreSentFilesRequest`](#storesentfilesrequest)
  - [`XcamControlRequest`](#xcamcontrolrequest)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`AirPrintDetectPayload`](#airprintdetectpayload) | struct | Turns non-visual air-printing detection on or off. |
| [`AirPurificationPayload`](#airpurificationpayload) | struct | Sets where chamber air is purified at the end of a print. |
| [`AirductPayload`](#airductpayload) | struct | Selects the airduct mode. |
| [`AutoRecoveryPayload`](#autorecoverypayload) | struct | Turns step-loss auto-recovery on or off. |
| [`BuzzerPayload`](#buzzerpayload) | struct | Modifies active alarm or attention chime parameters on the printer cabinet buzzer module. |
| [`DoorOpenCheckPayload`](#dooropencheckpayload) | struct | Sets what the printer does when its door opens mid-print. |
| [`FilamentBackupPayload`](#filamentbackuppayload) | struct | Turns AMS Filament Backup (auto-refill from a matching spool) on or off. |
| [`FilamentTangleDetectPayload`](#filamenttangledetectpayload) | struct | Turns filament tangle detection on or off. |
| [`FlashTiming`](#flashtiming) | struct | Flash cycle timing for [`LedCtrlRequest::new_flashing`](#ledctrlrequest); every field is in milliseconds except `loops`. |
| [`IdleHeatingProtectionPayload`](#idleheatingprotectionpayload) | struct | Turns idle heating protection on or off. |
| [`LedCtrlPayload`](#ledctrlpayload) | struct | Chamber illumination and toolhead LED control configurations. |
| [`NonVisualAirPrintDetectPayload`](#nonvisualairprintdetectpayload) | struct | Turns the second non-visual air-printing detection on or off. |
| [`NozzleBlobDetectPayload`](#nozzleblobdetectpayload) | struct | Turns nozzle blob detection (the original, on/off form) on or off. |
| [`PartControlPayload`](#partcontrolpayload) | struct | Sets a printer part's state; here the air purifier's gear. |
| [`PromptSoundPayload`](#promptsoundpayload) | struct | Turns prompt notification sounds on or off; BambuStudio's model profiles enable it on A1, A1 Mini and A2L, and a printer can report support itself. |
| [`SmartNozzleBlobDetectPayload`](#smartnozzleblobdetectpayload) | struct | Sets the smart nozzle blob detection mode (off, on, or auto). |
| [`SntpPayload`](#sntppayload) | struct | Sets the printer's clock. |
| [`StoreSentFilesPayload`](#storesentfilespayload) | struct | Sets whether files sent from Bambu Studio, Bambu Handy and MakerWorld are kept on external storage. |
| [`XcamControlPayload`](#xcamcontrolpayload) | struct | Turns one camera detector on or off, optionally with its halt sensitivity. |
| [`AirductMode`](#airductmode) | enum | Airduct operating mode, the `modeId` of `set_airduct` [REF-MQTT-LIFECYCLE]. |
| [`PurifierGear`](#purifiergear) | enum | Air purifier (APS) fan speed, the `action` of `part_control` for module `aps` and the decoded `device.aps.gear`. |
| [`AirPrintDetectRequest`](#airprintdetectrequest) | type | Enables or disables non-visual air-printing detection. |
| [`AirPurificationRequest`](#airpurificationrequest) | type | Sets the end-of-print air purification mode. |
| [`AirductRequest`](#airductrequest) | type | Switches the airduct between its modes ([`AirductMode`](#airductmode)). |
| [`AutoRecoveryRequest`](#autorecoveryrequest) | type | Enables or disables step-loss auto-recovery. |
| [`BuzzerRequest`](#buzzerrequest) | type | Controls the printer's buzzer alarm mode (silent, alarm, or chirp). |
| [`DoorOpenCheckRequest`](#dooropencheckrequest) | type | Sets the door-open check mode (BambuStudio `MachineObject::command_set_door_open_check`). |
| [`FilamentBackupRequest`](#filamentbackuprequest) | type | Enables or disables AMS Filament Backup. |
| [`FilamentTangleDetectRequest`](#filamenttangledetectrequest) | type | Enables or disables filament tangle detection. |
| [`IdleHeatingProtectionRequest`](#idleheatingprotectionrequest) | type | Turns idle heating protection on or off (BambuStudio `DevPrintOptions::command_set_against_continued_heating_mode`). |
| [`LedCtrlRequest`](#ledctrlrequest) | type | Turns chamber or toolhead LEDs on or off. |
| [`NonVisualAirPrintDetectRequest`](#nonvisualairprintdetectrequest) | type | Enables or disables the second non-visual air-printing detection (`air_printing_detection`). |
| [`NozzleBlobDetectRequest`](#nozzleblobdetectrequest) | type | Enables or disables nozzle blob detection. |
| [`PartControlRequest`](#partcontrolrequest) | type | Sets the air purifier's gear (BambuStudio `DevAirPurifier`, `part_control` with module `aps`). |
| [`PromptSoundRequest`](#promptsoundrequest) | type | Enables or disables the printer's notification sounds. |
| [`SmartNozzleBlobDetectRequest`](#smartnozzleblobdetectrequest) | type | Sets the smart nozzle blob detection mode. |
| [`SntpRequest`](#sntprequest) | type | Sets the printer's clock (BambuStudio `MachineObject::command_set_sntp`). |
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

Selects the airduct mode.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"set_airduct"`.

- **`mode_id`**: `i32`

  Airduct mode: 0 cooling, 1 heating, 2 exhaust, 3 full cooling ([`AirductMode`](#airductmode)) [REF-MQTT-LIFECYCLE].

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

### `NonVisualAirPrintDetectPayload`

```rust
struct NonVisualAirPrintDetectPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
    pub air_printing_detection: bool,
}
```

Turns the second non-visual air-printing detection on or off.

Not [`AirPrintDetectPayload`](#airprintdetectpayload)'s `air_print_detect` setting (`home_flag` bit 28) and not the
camera's AI air-printing detector. BambuStudio captions it "Detects air printing caused by
nozzle clogging, filament tangling, or filament grinding"
(`DevPrintOptions::command_non_visual_airprinting_detection`). Single source (BambuStudio);
no capture shows `fun2` bit 32 set.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"print_option"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

- **`air_printing_detection`**: `bool`

  Whether the detection is enabled.

#### Trait Implementations

##### `impl Clone for NonVisualAirPrintDetectPayload`

- <span id="nonvisualairprintdetectpayload-clone"></span>`fn clone(&self) -> NonVisualAirPrintDetectPayload` — [`NonVisualAirPrintDetectPayload`](#nonvisualairprintdetectpayload)

##### `impl Debug for NonVisualAirPrintDetectPayload`

- <span id="nonvisualairprintdetectpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for NonVisualAirPrintDetectPayload`

- <span id="nonvisualairprintdetectpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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

### `PartControlPayload`

```rust
struct PartControlPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
    pub module: &'static str,
    pub action: &'static str,
}
```

Sets a printer part's state; here the air purifier's gear.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"part_control"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

- **`module`**: `&'static str`

  The part addressed; `"aps"` for the air purifier.

- **`action`**: `&'static str`

  The setting, a decimal string: a JSON string, not a number — see [`PurifierGear::as_wire`](#purifiergear).

#### Trait Implementations

##### `impl Clone for PartControlPayload`

- <span id="partcontrolpayload-clone"></span>`fn clone(&self) -> PartControlPayload` — [`PartControlPayload`](#partcontrolpayload)

##### `impl Debug for PartControlPayload`

- <span id="partcontrolpayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for PartControlPayload`

- <span id="partcontrolpayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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

### `SntpPayload`

```rust
struct SntpPayload {
    pub command: &'static str,
    pub sequence_id: super::ClampedTaskId,
    pub second: u64,
    pub us: u32,
    pub tz: String,
    pub src: u8,
    pub path: u8,
}
```

Sets the printer's clock.

#### Fields

- **`command`**: `&'static str`

  Wire command name, always `"sntp"`.

- **`sequence_id`**: `super::ClampedTaskId`

  Request sequence ID, serialized as a string on the wire.

- **`second`**: `u64`

  Unix time, whole seconds.

- **`us`**: `u32`

  Microseconds past `second`, below 1,000,000.

- **`tz`**: `String`

  The UTC offset in effect now, as `"UTC+HH:MM"` or `"UTC-HH:MM"`.

- **`src`**: `u8`

  Sender kind, always `1`: BambuStudio's value, the only one documented.

- **`path`**: `u8`

  Connection path, always `2` (LAN only).

#### Trait Implementations

##### `impl Clone for SntpPayload`

- <span id="sntppayload-clone"></span>`fn clone(&self) -> SntpPayload` — [`SntpPayload`](#sntppayload)

##### `impl Debug for SntpPayload`

- <span id="sntppayload-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Serialize for SntpPayload`

- <span id="sntppayload-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

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
    Exhaust,
    FullCooling,
}
```

Airduct operating mode, the `modeId` of `set_airduct` [REF-MQTT-LIFECYCLE].

Values and names follow BambuStudio's `AIR_DUCT` enum (`DeviceCore/DevFan.h`), and the
variant docs quote its descriptions (`Widgets/FanControl.cpp`). The printer lists the modes it
offers in `device.airduct.modeList`.

#### Variants

- **`Cooling`**

  "Suitable for printing PLA/PETG/TPU materials and filters the chamber air."

- **`Heating`**

  "Suitable for printing ABS/ASA/PC/PA materials and circulates filters the chamber air."

- **`Exhaust`**

  Exhaust mode, which BambuStudio describes only by name.
  
  BambuStudio leaves it out of its mode list even when the printer reports it
  (`DevFan.cpp`, `/*STUDIO-12796*/`), for a reason it does not give. Sendable here, but a
  consumer offering modes to a user should consider hiding it as BambuStudio does.

- **`FullCooling`**

  "Strong cooling mode is suitable for printing PLA/TPU materials. In this mode, the printouts will be fully cooled."

#### Trait Implementations

##### `impl Clone for AirductMode`

- <span id="airductmode-clone"></span>`fn clone(&self) -> AirductMode` — [`AirductMode`](#airductmode)

##### `impl Copy for AirductMode`

##### `impl Debug for AirductMode`

- <span id="airductmode-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for AirductMode`

##### `impl PartialEq for AirductMode`

- <span id="airductmode-partialeq-eq"></span>`fn eq(&self, other: &AirductMode) -> bool` — [`AirductMode`](#airductmode)

### `PurifierGear`

```rust
enum PurifierGear {
    Off,
    Silent,
    Standard,
    Max,
    Auto,
}
```

Air purifier (APS) fan speed, the `action` of `part_control` for module `aps` and the decoded `device.aps.gear`.

Values follow BambuStudio's `AIR_PURIFIER_GEAR` (`DeviceCore/DevAirPurifier.hpp`);
ha-bambulab's `pybambu/air_purifier.py` sends the same codes.

#### Variants

- **`Off`**

  Off (`0`).

- **`Silent`**

  Silent, low speed (`1`).

- **`Standard`**

  Standard, mid speed (`2`).

- **`Max`**

  Max, high speed (`3`).

- **`Auto`**

  Automatic (`15`).

#### Implementations

- <span id="purifiergear-code"></span>`const fn code(self) -> u8`

  The gear's numeric code, as reported in `device.aps.gear`.

- <span id="purifiergear-from-code"></span>`const fn from_code(code: i32) -> Option<Self>`

  Decodes a `device.aps.gear` value; `None` for a code BambuStudio doesn't define.

- <span id="purifiergear-as-wire"></span>`const fn as_wire(self) -> &'static str`

  The `action` string sent on the wire: the decimal code, as BambuStudio's `std::to_string(gear)`.

#### Trait Implementations

##### `impl Clone for PurifierGear`

- <span id="purifiergear-clone"></span>`fn clone(&self) -> PurifierGear` — [`PurifierGear`](#purifiergear)

##### `impl Copy for PurifierGear`

##### `impl Debug for PurifierGear`

- <span id="purifiergear-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for PurifierGear`

##### `impl Hash for PurifierGear`

- <span id="purifiergear-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for PurifierGear`

- <span id="purifiergear-partialeq-eq"></span>`fn eq(&self, other: &PurifierGear) -> bool` — [`PurifierGear`](#purifiergear)

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

Switches the airduct between its modes ([`AirductMode`](#airductmode)).

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

### `NonVisualAirPrintDetectRequest`

```rust
type NonVisualAirPrintDetectRequest = super::Print<NonVisualAirPrintDetectPayload>;
```

Enables or disables the second non-visual air-printing detection (`air_printing_detection`).

### `NozzleBlobDetectRequest`

```rust
type NozzleBlobDetectRequest = super::Print<NozzleBlobDetectPayload>;
```

Enables or disables nozzle blob detection.

### `PartControlRequest`

```rust
type PartControlRequest = super::Print<PartControlPayload>;
```

Sets the air purifier's gear (BambuStudio `DevAirPurifier`, `part_control` with module `aps`).

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

### `SntpRequest`

```rust
type SntpRequest = super::System<SntpPayload>;
```

Sets the printer's clock (BambuStudio `MachineObject::command_set_sntp`).

A LAN-only printer with no route to an NTP server keeps whatever clock it has, and that clock
stamps `hms[].ts_unix`, which `reference/07_diagnostics_hms.md` §7.1 says is not guaranteed
UTC. Sending this is how BambuStudio sets it.

BambuStudio ignores an `err_code` in the reply (`MachineObject::parse_json`), so a printer
that doesn't support the command may answer with an error. Which firmware honours it is not
recorded. Not in `ACK_CORRELATED_COMMANDS`.

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

