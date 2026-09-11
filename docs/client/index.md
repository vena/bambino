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
  the secondary right-side auxiliary fan on models that have one (P2S, X2D, etc.).

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`capabilities`](#capabilities) | mod | # Client-Scoped Capabilities |
| [`dummy`](dummy/index.md) | mod | Zero-cost dummy implementations for [`PrinterClient`](#printerclient)'s type parameters. |
| [`types`](#types) | mod | Client-facing enums and helper types (telemetry events, fan targets, print speed, calibration). |
| [`PrinterClient`](#printerclient) | struct | High-level client for controlling a Bambu Lab printer. |

## Modules

- [`capabilities`](capabilities/index.md#capabilities) — # Client-Scoped Capabilities
- [`dummy`](dummy/index.md) — Zero-cost dummy implementations for [`PrinterClient`](#printerclient)'s type parameters.
- [`types`](types/index.md#types) — Client-facing enums and helper types (telemetry events, fan targets, print speed, calibration).


---

## Types

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

- <span id="capabilities-quirks"></span>`fn quirks(&self) -> &'static dyn ModelQuirks` — [`ModelQuirks`](../quirks/index.md#modelquirks)

  The underlying model quirks, for the capabilities that take no context.

- <span id="capabilities-supports-ams-remote-drying"></span>`fn supports_ams_remote_drying(&self) -> bool`

  Whether this printer honors `ams_filament_drying` sent over MQTT.

  Resolves the printer's reported `fun2` bit 5 against the model's own rules — never
  supported on A1/A1 Mini and P1P/P1S, firmware-gated on X1C/P2S/H2D/H2S/H2C, allowed
  elsewhere. See
  [`ModelQuirks::supports_ams_remote_drying`](../quirks/index.md#modelquirks)
  for the sourcing.

  **Gate UI on this rather than on a model check.** It is the same value
  [`start_drying`](#printerclient) tests, so a control offered on the
  strength of it will not then be refused.

  For a firmware-gated model this reads `false` until
  [`get_version()`](#printerclient) has been called — an unread version
  cannot be shown to meet a minimum. Call it once after connecting if you intend to ask.

#### Trait Implementations

##### `impl Clone for Capabilities<'a>`

- <span id="capabilities-clone"></span>`fn clone(&self) -> Capabilities<'a>` — [`Capabilities`](capabilities/index.md#capabilities)

##### `impl Copy for Capabilities<'a>`

##### `impl Debug for Capabilities<'_>`

- <span id="capabilities-debug-fmt"></span>`fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result`

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

#### Trait Implementations

##### `impl Clone for ConnectAllOutcome`

- <span id="connectalloutcome-clone"></span>`fn clone(&self) -> ConnectAllOutcome` — [`ConnectAllOutcome`](#connectalloutcome)

##### `impl Debug for ConnectAllOutcome`

- <span id="connectalloutcome-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

### `CalibrationOption`

```rust
struct CalibrationOption(u32);
```

Bitmask flags for selecting hardware calibration routines [REF-MQTT-LIFECYCLE].

Combine flags with bitwise OR to trigger multiple calibration routines simultaneously
(e.g., `CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION`).

#### Implementations

- <span id="calibrationoption-const-bed-leveling"></span>`const BED_LEVELING: Self`

- <span id="calibrationoption-const-vibration-compensation"></span>`const VIBRATION_COMPENSATION: Self`

- <span id="calibrationoption-const-motor-noise-cancellation"></span>`const MOTOR_NOISE_CANCELLATION: Self`

- <span id="calibrationoption-const-nozzle-height"></span>`const NOZZLE_HEIGHT: Self`

- <span id="calibrationoption-const-heatbed-thermal"></span>`const HEATBED_THERMAL: Self`

#### Trait Implementations

##### `impl BitOr for CalibrationOption`

- <span id="calibrationoption-bitor-type-output"></span>`type Output = CalibrationOption`

- <span id="calibrationoption-bitor"></span>`fn bitor(self, rhs: Self) -> Self`

##### `impl Clone for CalibrationOption`

- <span id="calibrationoption-clone"></span>`fn clone(&self) -> CalibrationOption` — [`CalibrationOption`](types/index.md#calibrationoption)

##### `impl Copy for CalibrationOption`

##### `impl Debug for CalibrationOption`

- <span id="calibrationoption-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for CalibrationOption`

##### `impl Hash for CalibrationOption`

- <span id="calibrationoption-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for CalibrationOption`

- <span id="calibrationoption-partialeq-eq"></span>`fn eq(&self, other: &CalibrationOption) -> bool` — [`CalibrationOption`](types/index.md#calibrationoption)

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

- <span id="superprinterclient-change-filament"></span>`async fn change_filament(&mut self, ams_id: i32, slot_id: i32, curr_temp: i32, tar_temp: i32, extruder_id: Option<u8>) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Triggers a filament load or unload sequence on a physical AMS unit or external spool [REF-AMS-MAP].

  * `ams_id`: AMS unit index (`0..=3`), AMS-HT unit bus ID (`128..=135`), or `254`/`255`
    for external spool (IDEX Ext-L/Ext-R or single-nozzle, respectively).
  * `slot_id`: Slot within the AMS (`0..=3`), `254` for a single-nozzle external-spool
    load, or `255` to unload/retract (see `ams_change_filament` examples in
    `reference/05_materials_ams.md` §5.3 [REF-AMS-MAP]).
  * `curr_temp` / `tar_temp`: Nozzle temperatures (`-1` = let firmware decide).

  The wire's `target` field is derived internally rather than caller-supplied —
  confirmed against BambuStudio's `command_ams_change_filament`
  (`DeviceManager.cpp:1602-1638`) — `target` is `255` on unload, the `ams_id` itself for
  any AMS-HT/external-spool unit (`ams_id >= 16`), or the flat global tray ID
  (`ams_id*4 + slot_id`) for a standard unit. A caller-supplied `target` that didn't
  match this derivation was a real hardware misconfiguration risk (error `07FF_8012`
  class), not just a doc gap — `target` mirroring `slot_id` only coincidentally held for
  `ams_id: 0`, the sole worked example in the reference doc.

  `extruder_id` names the hotend to feed — `Some(0)` for right/main, `Some(1)` for
  left/deputy. Pass `None` on any printer without a Filament Track Switch, where the
  firmware derives the hotend from the AMS's own extruder binding and the payload is
  byte-identical to the pre-FTS form. On a machine *with* a switch (an H2C, for example)
  every AMS reports its extruder as "not fixed" (`0xE`) and can reach either hotend
  through the switch, so a `None` here means the firmware has nothing to derive from and
  **discards the command in silence** — load and unload simply do nothing.

- <span id="superprinterclient-supports-ams-remote-drying"></span>`fn supports_ams_remote_drying(&self) -> bool`

  Whether this printer supports remote AMS drying — the printer-side half of the gate.

  Supplies the cached `fun2` to
  [`ModelQuirks::supports_ams_remote_drying`](../quirks/index.md#modelquirks),
  which resolves the printer's own answer against the model default. This is the call to
  gate a UI on: it is the identical value [`start_drying`](#printerclient) checks, so a
  control offered on the strength of it cannot then be refused.

  Shorthand for
  `capabilities().supports_ams_remote_drying()`;
  see there for how the answer is resolved and what has to be polled first.

- <span id="superprinterclient-start-drying"></span>`async fn start_drying(&mut self, ams_id: i32, temp: u32, duration_hours: u32, humidity: u32, rotate_tray: bool, cooling_temp: i32, close_power_conflict: bool, filament: &str) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Initiates a dry-chamber heating cycle on an AMS-HT or AMS 2 Pro unit [REF-AMS-DRYER].

  * `ams_id`: Target AMS unit index. AMS-HT units use the `128..=135` bus ID range (see
    `AMS_HT_ID_MIN`/`AMS_HT_ID_MAX` in `src/ams/parser.rs`). The address alone does **not**
    identify the unit: `0..=3` is shared by the original AMS, the AMS Lite and the AMS 2 Pro,
    and only the last of those has a heater — the unit model comes from cached telemetry,
    see Errors below.
  * `temp`: Drying temperature in degrees Celsius. Must fall inside the attached unit's
    [`AmsUnitModel::dry_temp_range`](../types/telemetry/ams/index.md#amsunitmodel) — `45..=65` for the AMS 2 Pro, `45..=85` for the
    AMS-HT. This is a property of the *attached AMS unit*, not the host printer model
    (confirmed via Bambu Lab's own wiki, `wiki.bambulab.com/en/ams-ht/...` and
    `wiki.bambulab.com/en/ams-2-pro/manual/drying-function` respectively — no per-printer
    variation is documented, so this does not go through `ModelQuirks`). **Both bounds are
    rejected, not clamped**: BambuStudio refuses a temperature below the floor exactly as it
    refuses one above the ceiling (`AMSDryControl.cpp:1186-1199`), and silently rewriting a
    caller's `0` into `45` would start a real heating cycle nobody asked for.
  * `duration_hours`: Duration in **hours** (e.g., `8` for an 8-hour cycle) —
    the wire field is `duration` in hours, not the old `dry_time` in minutes. No
    documented maximum duration was found to validate against.
  * `humidity`: Target humidity (`0` = firmware default / no target).
  * `rotate_tray`: Whether to rotate trays during the cycle.
  * `cooling_temp`: Cooling temperature applied after the drying cycle completes.
  * `close_power_conflict`: Whether to override the AMS unit's power-conflict interlock.
  * `filament`: Filament type string (e.g., "PA-CF").

  # Errors

  [`Error::ModelMismatch`](../error/index.md#error) on hosts where
  [`supports_ams_remote_drying()`](#printerclient) is `false` — the
  printer's own `fun2` bit 5 when it has reported one, else the P1P/P1S quirk. Such
  firmware acks this command `result: success` and silently discards it rather than
  actually driving the AMS heater; see `[REF-AMS-DRYER]`.

  [`Error::ModelMismatch`](../error/index.md#error) also when the addressed unit is one this crate can see has no
  drying chamber — an external-spool sentinel (`254`/`255`), or a cached
  [`AmsUnitModel`](../types/telemetry/ams/index.md#amsunitmodel) whose [`supports_drying`](../types/telemetry/ams/index.md#amsunitmodel) is `false`.
  These are two independent gates on purpose, matching the pair BambuStudio writes out
  longhand at `Widgets/AMSControl.cpp:348`: the printer must act on the command *and* the
  attached box must have a heater.

  [`Error::InvalidArgument`](../error/index.md#error) when `temp` falls outside the unit's
  [`dry_temp_range`](../types/telemetry/ams/index.md#amsunitmodel).

  The unit-model gate reads the **cached** AMS snapshot, so a unit this client has never
  observed passes through — same rule as [`skip_objects`](#printerclient), and for the
  same reason: an idle printer's incremental pushes frequently carry no `ams` block at all,
  and refusing there would break a caller that connects and commands without polling. Call
  [`poll_telemetry()`](#printerclient) first to arm the gate. When the unit is
  unobserved the temperature range falls back to the `ams_id`-derived ceiling this method
  used before, which is the best guess available from the address alone.

- <span id="superprinterclient-stop-drying"></span>`async fn stop_drying(&mut self, ams_id: i32) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Terminates an active dry-chamber heating cycle on an AMS unit [REF-AMS-DRYER].

  Mirrors BambuStudio's `CtrlAmsStopDrying` (`DevFilaSystemCtrl.cpp:40-53`) exactly —
  every field zeroed/defaulted, only `mode: 0` (`Off`) is meaningful.

- <span id="superprinterclient-scan-rfid"></span>`async fn scan_rfid(&mut self, ams_id: i32, slot_id: i32) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Scans proprietary RFID tag properties on a specific AMS tray [REF-AMS-MAP].

  * `ams_id`: AMS unit index (`0..=3`) or AMS-HT unit bus ID (`128..=135`). Only
    documented against a physical bus unit (`reference/03_mqtt_telemetry.md`
    `ams_get_rfid` example) — external spools have no RFID reader node, so no
    external-spool sentinel value applies here.
  * `slot_id`: Slot within the AMS (`0..=3`).

- <span id="superprinterclient-select-k-profile"></span>`async fn select_k_profile(&mut self, ams_id: i32, tray_id: i32, cali_idx: i32, filament_id: &str, nozzle_diameter: &str) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

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

  **Validation note:** the cheat-sheet above documents only the *external-spool* case.
  `reference/05_materials_ams.md` §5.3's own primary `extrusion_cali_sel` example binds a
  perfectly ordinary AMS slot (`"ams_id": 0, "tray_id": 1`) — `tray_id` there is the
  *global* tray ID (the same `(ams_id * 4) + slot_id` / `128..=135` AMS-HT composite the
  flat `ams_mapping` array uses, per §5.3's "Hardware Channel Identifiers"), not a
  per-unit slot index. The validation below therefore accepts the full documented
  address space — standard AMS units, AMS-HT units, and the external-spool sentinels —
  not just the two cheat-sheet pairs; restricting to only `(254,254)`/`(255,255)` (as an
  earlier draft of this check assumed) would incorrectly reject this exact primary example.

- <span id="superprinterclient-get-version"></span>`async fn get_version(&mut self) -> Result<VersionInfo, Error>` — [`VersionInfo`](../types/version/index.md#versioninfo), [`Error`](../error/index.md#error)

  Queries the printer's expansion bus version database and returns typed module info.

  Sends a `get_version` command and waits for the response, buffering any
  telemetry messages that arrive in the interim. Wrap in a platform-specific
  timeout if you need a shorter deadline than `command_timeout_secs`.

- <span id="superprinterclient-get-k-profiles"></span>`async fn get_k_profiles(&mut self, nozzle_diameter: Option<&str>) -> Result<ExtrusionCaliGetResponse, Error>` — [`ExtrusionCaliGetResponse`](../diagnostics/kprofile/index.md#extrusioncaligetresponse), [`Error`](../error/index.md#error)

  Requests a dump of the printer's stored K-profile calibration database [REF-DIAG-KPROF].

  Automatically sends a priming request on the first call after connection, because the
  firmware silently ignores the initial `extrusion_cali_get` command. Use
  `set_k_profile_primed(true)` to skip the automatic prime if you handle it yourself.

  **A response is the complete table for exactly one nozzle diameter.** `nozzle_diameter`
  scopes the query, and the reply echoes the diameter that was *requested* rather than
  reflecting installed hardware. Passing `None` sends the bare request, whose reply covers
  whichever single diameter the firmware picks — on a machine that can hold more than one,
  that is a partial table which looks complete to the caller. Call once per fitted diameter
  and merge the results.

- <span id="superprinterclient-set-k-profile-primed"></span>`fn set_k_profile_primed(&mut self, primed: bool)`

  Controls whether `get_k_profiles()` sends an automatic priming request.

  Set to `true` to skip the firmware priming quirk — useful if you handle priming
  yourself or target firmware that does not require it.

- <span id="superprinterclient-attach-camera"></span>`fn attach_camera(&mut self, camera: BinaryCameraStream<<CameraTls as >::Stream>)` — [`BinaryCameraStream`](../camera/binary/index.md#binarycamerastream), [`TlsConnector`](../io/index.md#tlsconnector)

  Injects a pre-connected [`BinaryCameraStream`](../camera/binary/index.md#binarycamerastream) directly.

  Use this for test mocks or Embassy where the caller manages the camera
  connection. For lazy connection, use [`.with_camera()`](#printerclient).

- <span id="superprinterclient-camera"></span>`async fn camera(&mut self) -> Result<&mut BinaryCameraStream<<CameraTls as >::Stream>, Error>` — [`BinaryCameraStream`](../camera/binary/index.md#binarycamerastream), [`TlsConnector`](../io/index.md#tlsconnector), [`Error`](../error/index.md#error)

  Returns direct access to the underlying [`BinaryCameraStream`](../camera/binary/index.md#binarycamerastream), auto-connecting if needed.

  Requires prior camera configuration via [`.with_camera()`](#printerclient) or
  [`.attach_camera()`](#printerclient). Returns `Error::ProtocolViolation`
  immediately for RTSPS models — see `ensure_camera()`'s doc
  comment.

- <span id="superprinterclient-read-camera-frame"></span>`async fn read_camera_frame(&mut self, frame_buf: &mut Vec<u8>) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Reads the next camera frame, auto-connecting (and authenticating) if needed.

  Bounds the read against `self.timer` (see
  `BinaryCameraStream::read_next_frame_with_timer`), mirroring
  [`poll_telemetry()`](#printerclient)'s relationship to
  [`.mqtt()`](#printerclient).

- <span id="superprinterclient-disconnect-camera"></span>`async fn disconnect_camera(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Disconnects the camera session, if one exists, and clears it from the client.

  Once `camera_config` is consumed by `ensure_camera()`, a dead
  stream (`ConnectionReset`, bad markers, etc.) would otherwise leave `self.camera`
  stuck `Some(...)` forever, since `ensure_camera()`'s `is_some()` short-circuit would
  keep handing back the same broken stream. There is no protocol-level teardown on
  `BinaryCameraStream` to call — this just clears the slot.

  Idempotent. Reconnecting requires a fresh [`.with_camera()`](#printerclient) on a
  new `PrinterClient`, the same caveat FTPS already documents for
  [`disconnect_storage()`](#printerclient).

- <span id="superprinterclient-connect-mqtt"></span>`async fn connect_mqtt(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Eagerly establishes the MQTT connection.

  Idempotent — returns `Ok(())` if already connected.

- <span id="superprinterclient-is-mqtt-connected"></span>`fn is_mqtt_connected(&self) -> bool`

  Returns whether the MQTT session is currently established.

- <span id="superprinterclient-attach-mqtt"></span>`fn attach_mqtt(&mut self, mqtt: MqttClient<<MqttTls as >::Stream>)` — [`MqttClient`](../mqtt/client/index.md#mqttclient), [`TlsConnector`](../io/index.md#tlsconnector)

  Injects a pre-connected [`MqttClient`](../mqtt/client/index.md#mqttclient) directly.

  Use this for test mocks or Embassy where the caller manages the MQTT connection,
  mirroring [`attach_camera()`](#printerclient)/
  [`attach_storage()`](#printerclient).

- <span id="superprinterclient-disconnect-mqtt"></span>`async fn disconnect_mqtt(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Disconnects the MQTT session, if one exists, and clears it from the client.

  There is no protocol-level teardown on `MqttClient` to call — this just clears
  the slot, mirroring `disconnect_camera()`. Without this, a dead stream (a
  [`tick_zombie_check()`](../mqtt/index.md)-detected
  zombie, a transport error) left `self.mqtt` stuck `Some(...)` forever, since
  `ensure_mqtt()`'s `is_some()` short-circuit kept handing back the same broken
  connection with no supported redial path.

  Idempotent. Reconnecting requires [`.attach_mqtt()`](#printerclient) with a fresh
  `MqttClient` for a [`from_mqtt()`](#printerclient)-built client — its
  `PreConnected` factory's `dial()` always errors, so `ensure_mqtt()`'s lazy-dial fallback
  only recovers a `connect()`-built client, never one built via `from_mqtt()`.

- <span id="superprinterclient-with-timer"></span>`fn with_timer<NewTimer: TimerProvider>(self, timer: NewTimer) -> PrinterClient<MqttRawIO, MqttTls, MqttFactory, NewTimer, FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer, CameraRawIO, CameraTls, CameraFactory>` — [`PrinterClient`](#printerclient)

  Sets a [`TimerProvider`](../io/index.md#timerprovider) for wall-clock command-response timeouts.

  Consuming builder — works on both [`new()`](#printerclient) and
  [`from_mqtt()`](#printerclient) construction paths.

- <span id="superprinterclient-with-mqtt-port"></span>`fn with_mqtt_port(self, port: u16) -> Self`

  Overrides the default MQTT port (8883).

- <span id="superprinterclient-with-connect-timeout"></span>`fn with_connect_timeout(self, secs: u64) -> Self`

  Overrides the default connect-timeout deadline (10s) that bounds `ensure_mqtt()`/`ensure_ftps()`'s combined dial+TLS-connect sequence.
  Passing `0` disables the timeout entirely, matching `set_command_timeout`'s "0 disables"
  convention. Non-consuming — chain onto any construction path.

  On ESP-IDF, this budget is structurally independent from `EspIdfTlsConnector`'s own
  internal handshake timeout (default 10s) — the connector is an opaque generic by the
  time it reaches `PrinterClient::new()`, so this outer setting can't see or influence it.
  Set `EspIdfTlsConnector::with_connect_timeout` directly and keep the two in sync,
  including the `0` case (both treat `0` as "disabled", but neither number implies the
  other). Not an issue on `tokio`/`embassy`, where the handshake is bounded solely by
  this outer race.

- <span id="superprinterclient-with-ftps"></span>`fn with_ftps<NewFtpsRawIO, NewFtpsTls, NewFtpsFactory, NewFtpsTimer>(self, tls: NewFtpsTls, factory: NewFtpsFactory, timer: NewFtpsTimer) -> PrinterClient<MqttRawIO, MqttTls, MqttFactory, Timer, NewFtpsRawIO, NewFtpsTls, NewFtpsFactory, NewFtpsTimer, CameraRawIO, CameraTls, CameraFactory>` — [`PrinterClient`](#printerclient)

  Configures FTPS for lazy connection on first storage method call.

  Consuming builder — changes the `FtpsRawIO`, `FtpsTls`, `FtpsFactory`, and `FtpsTimer`
  type parameters. The FTPS [`TlsConnector`](../io/index.md#tlsconnector) is independent from MQTT's (some models
  require different TLS settings for FTPS, e.g. `force_tls_1_2`). `timer` is
  constructed fresh by the caller (e.g. `TokioTimer::new()`) — `FtpsClient` owns it
  independently of `PrinterClient`'s own `Timer`, since `PrinterClient::storage()` hands
  out direct `&mut FtpsClient` access rather than mediating every FTPS call itself,
  so there's no call site to thread `self.timer` through the way MQTT/camera do.

  Must not be called on a client with an already-connected FTPS session — the existing
  connection is dropped (not explicitly disconnected) when the new struct is built.
  Functionally safe (LAN-only TCP/TLS, `Drop`-based teardown), but callers should
  disconnect first if they want an explicit, orderly teardown.

- <span id="superprinterclient-with-ftps-port"></span>`fn with_ftps_port(self, port: u16) -> Self`

  Overrides the default FTPS port (990).

- <span id="superprinterclient-with-ftps-allow-unverified-tls-1-2"></span>`fn with_ftps_allow_unverified_tls_1_2(self, allow: bool) -> Self`

  Overrides the default `false` for `FtpsClient`'s TLS-1.2-enforcement bypass.

  Only meaningful for the `embassy` feature talking to P2S/X2D, where no available TLS
  backend can honestly satisfy `require_tls_1_2_if_enforced`'s exact-version check —
  see `src/ftps/CLAUDE.md` and `src/io/CLAUDE.md`. On `tokio`/`esp-idf`, use
  `force_tls_1_2` on the `TlsConnector` instead, since those platforms can actually
  satisfy the check for real.
  Non-consuming — chain onto any construction path.

- <span id="superprinterclient-connect-ftps"></span>`async fn connect_ftps(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Eagerly establishes the FTPS connection.

  Idempotent — returns `Ok(())` if already connected.

- <span id="superprinterclient-is-ftps-connected"></span>`fn is_ftps_connected(&self) -> bool`

  Returns whether the FTPS session is currently established.

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
    [`Error::ProtocolViolation`](../error/index.md#error) on an RTSPS model: here an RTSPS camera is a channel
    that does not apply to this printer, not a failure, so reporting it as an error
    would hand every P2S/X2D consumer a guaranteed `Err` on an otherwise clean connect.
    Those models use `camera::rtsps::build_rtsps_url()` and have no client-managed
    connection to establish.

  # Timeouts

  `connect_timeout_secs` is applied **per channel**, matching the individual
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

  Must not be called on a client with an already-connected camera session — see
  `.with_ftps()`'s doc comment for why.

- <span id="superprinterclient-with-camera-port"></span>`fn with_camera_port(self, port: u16) -> Self`

  Overrides the default camera port (6000, binary-JPEG only).

- <span id="superprinterclient-with-camera-max-frame-size"></span>`fn with_camera_max_frame_size(self, bytes: usize) -> Self`

  Overrides the default maximum accepted camera frame size (see `BinaryCameraStream::with_max_frame_size`).

- <span id="superprinterclient-set-fan-speed"></span>`async fn set_fan_speed(&mut self, fan_type: FanTarget, speed_percent: u8) -> Result<u16, Error>` — [`FanTarget`](types/index.md#fantarget), [`Error`](../error/index.md#error)

  Sets the speed of a targeted onboard fan as a percentage (0 to 100) [REF-CLIM-FANS].

  Translates percentage input to standard PWM ranges (0 to 255) in the G-code envelope.
  For models with unique secondary cooling configurations (like the X2D), directs commands
  to the correct target port ID.

- <span id="superprinterclient-set-led"></span>`async fn set_led(&mut self, node: &str, turn_on: bool) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Configures the active state of a targeted enclosure LED lighting node [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-set-airduct-mode"></span>`async fn set_airduct_mode(&mut self, mode: crate::mqtt::commands::AirductMode) -> Result<u16, Error>` — [`AirductMode`](../mqtt/commands/hardware/index.md#airductmode), [`Error`](../error/index.md#error)

  Configures the active climate airduct damper mode [REF-MQTT-LIFECYCLE].

  Supported on models with controllable airduct dampers (H2 series, P2S, X2D).

- <span id="superprinterclient-set-prompt-sound"></span>`async fn set_prompt_sound(&mut self, enable_sound: bool) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Configures whether the printer's speakers emit prompt notification sounds [REF-MQTT-LIFECYCLE].

  Supported on models with onboard speakers (A1, A1 Mini, A2L).

- <span id="superprinterclient-set-buzzer-mode"></span>`async fn set_buzzer_mode(&mut self, mode: BuzzerMode) -> Result<u16, Error>` — [`BuzzerMode`](types/index.md#buzzermode), [`Error`](../error/index.md#error)

  Modifies active alarm or attention chime parameters on the physical buzzer module [REF-MQTT-LIFECYCLE].

  Supported on models with a physical fire alarm buzzer (H2 series).

- <span id="superprinterclient-is-axis-homed"></span>`fn is_axis_homed(&self, axis: char) -> Option<bool>`

  Returns whether `axis` (`'X'`/`'Y'`/`'Z'`, case-insensitive) was homed as of the last-observed `home_flag` telemetry.
  `None` means no telemetry carrying `home_flag` has been observed yet (via
  [`poll_telemetry()`](#printerclient)) — not "unhomed". Advisory only: the firmware does
  not reject motion on unhomed axes [REF-MOTO-HOME].

- <span id="superprinterclient-is-all-axes-homed"></span>`fn is_all_axes_homed(&self) -> Option<bool>`

  Returns whether X, Y, and Z were all homed as of the last-observed `home_flag` telemetry.
  `None` means no telemetry carrying `home_flag` has been observed yet.

- <span id="superprinterclient-send-gcode"></span>`async fn send_gcode(&mut self, gcode_line: &str) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Sends a G-code command with model-aware safety validation.

  Rejects commands that would be unsafe on the active model (e.g., partial-axis
  homing on bed-on-Z platforms). Use [`send_gcode_raw()`](#printerclient)
  to bypass validation when you need unchecked access.

  # Example

  ```rust,ignore
  // Turn on the part cooling fan at 100%
  printer.send_gcode("M106 P1 S255").await?;

  // This will be rejected on CoreXY printers (unsafe partial homing):
  // printer.send_gcode("G28 Z").await?;  // -> Err(ModelMismatch)
  ```

- <span id="superprinterclient-send-gcode-raw"></span>`async fn send_gcode_raw(&mut self, gcode_line: &str) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Dispatches a raw G-code string without model safety checks [REF-MOTO-GCODE].

  Returns the MQTT packet identifier assigned to track publication delivery status.

- <span id="superprinterclient-home-axes"></span>`async fn home_axes(&mut self, home_z_only_danger: bool) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Dispatches safe homing operations to prevent hardware collisions.

  **Z-Axis Homing Crash Hazards [REF-MOTO-GCODE]:**
  * **Bed-on-Z models** (X1, X2D, P1, H2, P2S series) must strictly be homed using a bare `G28`
    to execute the safe firmware-defined toolhead parking sequence. Specifying axis constraints
    (such as `G28 Z`) bypasses this and risks driving the bed directly into a misplaced toolhead.
  * **Bed-Slingers** (A1, A1 Mini, A2L) can handle targeted homing macros safely, but a bare `G28` is
    highly recommended for standard configurations.

- <span id="superprinterclient-move-relative"></span>`async fn move_relative(&mut self, axis: char, distance: f32, feedrate: u32) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Dispatches a manual relative axis movement block.

  **Relative Axis Movement Safety [REF-MOTO-GCODE]:**
  For relative movements on the Z-axis, this method wraps the move in a client-side
  `z_max` distance cap (bounding how far a single command can travel — not true
  position-aware crash prevention, since the printer reports no absolute axis position
  over MQTT) and safe reference-mode push/pop blocks (`M1002 push_ref_mode` /
  `M1002 pop_ref_mode`) to prevent frame shifting. `M211 S1` is also sent, but per real
  H2D hardware testing (bambuddy #2579, confirmed 2026-07-16) firmware does not enforce
  software travel limits on G-code received over MQTT regardless of `M211` state — it is
  not a source of crash protection here. X/Y moves get the same kind of client-side
  `x_max()`/`y_max()` distance cap — same limitation, not position-aware.

  A `distance` of exactly `0.0` is a no-op: no G-code is sent to the printer, and this
  returns `Ok(0)` (packet id `0` is reserved by the MQTT layer and never assigned to a
  real publish, so it unambiguously signals "nothing was sent"). This avoids surfacing
  the Z-axis travel-limit error for a request that isn't actually out of range.

- <span id="superprinterclient-extrude"></span>`async fn extrude(&mut self, length: f32, feedrate: u32) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Dispatches a manual relative extrusion command sequence [REF-GCODE-EXTRUDE].

  Configures the active extruder drive gear to relative mode (`M83`) and feeds
  the specified length of filament (in mm) at the designated feedrate (in mm/min).

- <span id="superprinterclient-wait-for-homing"></span>`async fn wait_for_homing(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Blocks until a `G28` homing cycle observed via telemetry has completed.

  Standalone — does not require this client to have issued [`home_axes()`](#printerclient).
  Resolves correctly whether homing was triggered by this client, the touchscreen, slicer
  software, or another `PrinterClient` instance, since it only relies on `home_flag`
  telemetry observed via [`poll_telemetry()`](#printerclient).

  Only resolves successfully after observing a not-all-homed `home_flag` reading
  followed by an all-homed reading: an already-homed printer at call time does not
  resolve instantly, and a call where nothing ever homes times out rather than
  returning early.

  Like `poll_until` (`src/client/mod.rs`), `wait_for_homing_inner`'s own
  wall-clock timeout (`HOMING_WAIT_TIMEOUT_SECS`) and message-count valve
  (`POLL_UNTIL_MAX_MESSAGES`) only run *after* each `poll_telemetry().await` below
  has already returned — neither protects against that single call stalling
  forever on a connection that stops delivering bytes mid-homing (printer powered
  off, network drop). That protection is a distinct, lower layer: the underlying
  `MqttClient::poll_wire()` (`src/mqtt/client/mod.rs`) races each low-level read
  step against `self.timer` internally, bounding a single call regardless of what
  this loop does above it.

- <span id="superprinterclient-pause-print"></span>`async fn pause_print(&mut self) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Pauses the currently active print job [REF-MQTT-LIFECYCLE].

  Deliberately **not** state-gated, unlike [`skip_objects`](#printerclient). Pausing an
  idle printer is a firmware no-op rather than a misdirected command, neither BambuStudio
  nor bambuddy gates this on `gcode_state`, and the CLI's `probe` sends it while idle on
  purpose to document what the firmware does. See `stop_print` for the staleness argument
  that applies to any cache-backed gate on this path.

- <span id="superprinterclient-resume-print"></span>`async fn resume_print(&mut self) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Resumes a paused print job [REF-MQTT-LIFECYCLE].

  Not state-gated, on the same terms as [`pause_print`](#printerclient).

- <span id="superprinterclient-stop-print"></span>`async fn stop_print(&mut self) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Aborts/cancels the currently running print job queue [REF-MQTT-LIFECYCLE].

  Deliberately **ungated**, unlike [`skip_objects`](#printerclient). A state gate reads
  the *cached* `gcode_state`, which is only as fresh as the last
  [`poll_telemetry()`](#printerclient); a caller that has not polled since before the
  job started holds a stale `IDLE`. Refusing an abort on a stale reading would leave the
  printer running while reporting the stop as rejected — the wrong direction to fail for
  the abort path. Stop is idempotent, so a no-op stop costs nothing on the other side.

- <span id="superprinterclient-clear-print-error"></span>`async fn clear_print_error(&mut self) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Clears active error codes from the printer's diagnostic fault register [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-set-print-speed"></span>`async fn set_print_speed(&mut self, level: PrintSpeed) -> Result<u16, Error>` — [`PrintSpeed`](types/index.md#printspeed), [`Error`](../error/index.md#error)

  Dynamically scales maximum velocity and acceleration limits during an active print [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-skip-objects"></span>`async fn skip_objects(&mut self, object_ids: Vec<u32>) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

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

- <span id="superprinterclient-start-calibration"></span>`async fn start_calibration(&mut self, options: CalibrationOption) -> Result<u16, Error>` — [`CalibrationOption`](types/index.md#calibrationoption), [`Error`](../error/index.md#error)

  Triggers automated physical calibration routines on the printer chassis [REF-MQTT-LIFECYCLE].

  Use `CalibrationOption` flags combined with `|` to select routines:
  ```rust,ignore
  client.start_calibration(
      CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION
  ).await?;
  ```

  The returned `u16` is the published command's `sequence_id`, not a completion signal, but
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
  [`supported_calibration_mask()`](../quirks/index.md)
  instead of trusting that ack: unsupported bits are dropped with a `log::warn!` and the
  remaining routines still run.

  # Errors

  [`Error::ModelMismatch`](../error/index.md#error) when *none* of the requested routines are supported on this
  model, since that request would otherwise be a silent no-op reported as success. A
  partially-supported request is not an error — it proceeds with whatever the model runs.

  Wire observations are P1S firmware `01.10.00.00`. See `reference/03_mqtt_telemetry.md`
  for the wire detail and stage-ID mapping.

- <span id="superprinterclient-start-print"></span>`async fn start_print(&mut self, config: &PrintJobConfig) -> Result<u16, Error>` — [`PrintJobConfig`](../mqtt/commands/print_job/index.md#printjobconfig), [`Error`](../error/index.md#error)

  Submits a `.3mf` print job from MicroSD storage for execution [REF-MQTT-LIFECYCLE].

  The model's quirks engine gates `nozzle_offset_cali`: it resolves the default when the
  config left it `None`, and forces it off on a single-nozzle model even if the caller set
  it explicitly.

- <span id="superprinterclient-attach-storage"></span>`fn attach_storage(&mut self, ftps_client: FtpsClient<FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer>)` — [`FtpsClient`](../ftps/client/index.md#ftpsclient)

  Injects a pre-connected [`FtpsClient`](../ftps/client/index.md#ftpsclient) directly.

  Use this for test mocks or Embassy where the caller manages the FTPS
  connection. For lazy connection, use [`.with_ftps()`](#printerclient).

- <span id="superprinterclient-storage"></span>`async fn storage(&mut self) -> Result<&mut FtpsClient<FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer>, Error>` — [`FtpsClient`](../ftps/client/index.md#ftpsclient), [`Error`](../error/index.md#error)

  Returns direct access to the underlying [`FtpsClient`](../ftps/client/index.md#ftpsclient), auto-connecting if needed.

  Requires prior FTPS configuration via [`.with_ftps()`](#printerclient) or
  [`.attach_storage()`](#printerclient).

- <span id="superprinterclient-disconnect-storage"></span>`async fn disconnect_storage(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Disconnects the FTPS session, if one exists, and clears it from the client.

  `FtpsClient::disconnect()` is `&mut self` (non-consuming) and always poisons
  itself on the way out (see its doc comment) — every subsequent call on that instance
  would fail with `ProtocolViolation`. Without this method, nothing ever resets
  `self.ftps` back to `None`, so a later [`storage()`](#printerclient) call would
  short-circuit `ensure_ftps()`'s `is_some()` check and hand back the now-poisoned
  client, surfacing a confusing low-level error instead of a clear one.

  `disconnect_storage()` takes `self.ftps`, disconnects it, and leaves the slot `None`.
  The next `storage()` call then falls through to `ensure_ftps()`'s existing "FTPS not
  configured" error (if `ftps_config` was already consumed by an earlier connect) rather
  than ever returning a poisoned client. Reconnecting still requires fresh FTPS
  configuration — [`.with_ftps()`](#printerclient) on a new `PrinterClient`, or
  [`.attach_storage()`](#printerclient) — since `ftps_config` is consumed on first
  connection.

  Idempotent — a no-op if no FTPS session is active. Always returns `Ok(())`; kept
  fallible for API symmetry with [`connect_ftps()`](#printerclient) and to leave room
  for a fallible teardown step in the future without a breaking signature change.

- <span id="superprinterclient-poll-telemetry"></span>`async fn poll_telemetry(&mut self) -> Result<TelemetryEvent, Error>` — [`TelemetryEvent`](types/index.md#telemetryevent), [`Error`](../error/index.md#error)

  Pulls the next telemetry event from the MQTT channel.

  Returns a [`Report`](https://docs.rs/std/latest/std/error/struct.Report.html) if the payload deserializes as a known
  telemetry structure, or [`TelemetryEvent::Unknown`](types/index.md#telemetryevent) otherwise. A payload that
  deserializes successfully but carries a `print.command` other than `"push_status"`/
  `"pushall"` is a command-echo response (e.g. `extrusion_cali_get`'s reply shares the
  `print` envelope and the `nozzle_diameter` field name with genuine telemetry) and is
  also routed to `Unknown` rather than misreported as a report. Drains any
  internally buffered messages (from command-response round-trips) before
  reading from the wire.

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
          TelemetryEvent::Unknown(_) => {}
      }
  }
  ```

- <span id="superprinterclient-print-status"></span>`fn print_status(&self) -> Option<PrintStatus>` — [`PrintStatus`](types/index.md#printstatus)

  Returns the printer's high-level activity classification as of the last-observed `gcode_state` telemetry (via [`poll_telemetry()`](#printerclient)).
  `None` means no telemetry carrying `gcode_state` has been observed yet.

- <span id="superprinterclient-is-door-open"></span>`fn is_door_open(&self) -> Option<bool>`

  Returns whether the door was open as of the last-observed telemetry (via [`poll_telemetry()`](#printerclient)).

  Returns `None` on models without a door sensor (`ModelQuirks::has_door_sensor()`
  returns `false`, e.g. A1/A2), regardless of telemetry observed — distinct from
  `Some(false)`, which means a sensor-equipped model's telemetry confirms the door is
  closed. Also `None` before any telemetry carrying `print` has been observed.

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

- <span id="superprinterclient-bed-temperatures"></span>`fn bed_temperatures(&self) -> (u16, u16)`

  Returns the bed's (actual, target) temperatures in °C, decoded from the last-observed telemetry (via [`poll_telemetry()`](#printerclient)).
  Returns `(0, 0)` before any telemetry carrying bed temperature has been observed.

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
  `is_exists`/`is_tray_info_ready()`-equivalent checks (`AmsTray::state()` here) and
  never scrubs the raw cache. This crate mirrors that design rather than
  [`clean_stale_tray_data`](../ams/parser/index.md#clean-stale-tray-data)'s proactive-clearing
  approach: wiring proactive clearing into this cache would make it *less* faithful to
  on-wire state than BambuStudio's own model. Two opt-in ways to get sanitized output
  without losing that raw fidelity:
  - Check `AmsTray::state()` (or
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

- <span id="superprinterclient-nozzle-temperatures"></span>`fn nozzle_temperatures(&self) -> Vec<(u8, u16, u16)>`

  Returns the nozzle temperatures as of the last-observed telemetry (via [`poll_telemetry()`](#printerclient)) as `(id, actual, target)` tuples in °C.
  Single-nozzle models return one entry (`id` 0); IDEX models return one entry per physical
  nozzle. See [`decode_nozzle_temperatures`](../types/telemetry/index.md#decode-nozzle-temperatures) for the cross-model decode (including the
  undocumented IDEX flat-field routing quirk).

- <span id="superprinterclient-chamber-temperature"></span>`fn chamber_temperature(&self) -> Option<(u16, u16)>`

  Returns the chamber's (actual, target) temperatures in °C, decoded from the last-observed telemetry (via [`poll_telemetry()`](#printerclient)).

  Returns `None` on models without an active chamber temperature sensor/heater
  (`ModelQuirks::ignores_chamber_temperature()` returns `true`, e.g. A1/A1 Mini/A2L/P1P/
  P1S) — mirrors `is_door_open()`'s sensor-capability gate. `Some((0, 0))` before any
  telemetry carrying `chamber_temper` has been observed on a chamber-equipped model.

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

- <span id="superprinterclient-part-cooling-fan-speed"></span>`fn part_cooling_fan_speed(&self) -> Option<u8>`

  Returns the part-cooling fan speed (Port 1) as a percentage (0-100), decoded from the last-observed telemetry (via [`poll_telemetry()`](#printerclient)).

- <span id="superprinterclient-auxiliary-left-fan-speed"></span>`fn auxiliary_left_fan_speed(&self) -> Option<u8>`

  Returns the primary left-side auxiliary fan speed (Port 2) as a percentage (0-100).

- <span id="superprinterclient-chamber-exhaust-fan-speed"></span>`fn chamber_exhaust_fan_speed(&self) -> Option<u8>`

  Returns the chamber exhaust/filtration fan speed (Port 3) as a percentage (0-100).

- <span id="superprinterclient-heatbreak-fan-speed"></span>`fn heatbreak_fan_speed(&self) -> Option<u8>`

  Returns the toolhead heatbreak fan speed as a percentage (0-100).
  Not independently controllable (no corresponding `FanTarget` variant/M106 port) — read-only
  telemetry.

- <span id="superprinterclient-auxiliary-left2-fan-speed"></span>`fn auxiliary_left2_fan_speed(&self) -> Option<u8>`

  Returns the X2D/P2S second left-side auxiliary fan speed (Port 10, `FanTarget::AuxiliaryLeft2`) as a percentage (0-100).
  Reported at a different wire location than the other four fans —
  `device.airduct.parts[id=160].state` — already a direct percentage, no 0-15 step conversion
  [REF-CLIM-FANS].

- <span id="superprinterclient-print-speed"></span>`fn print_speed(&self) -> Option<PrintSpeed>` — [`PrintSpeed`](types/index.md#printspeed)

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

- <span id="superprinterclient-poll-raw"></span>`async fn poll_raw(&mut self) -> Result<MqttMessage, Error>` — [`MqttMessage`](../mqtt/client/index.md#mqttmessage), [`Error`](../error/index.md#error)

  Pulls the next raw MQTT message without deserialization.

- <span id="superprinterclient-set-bed-temperature"></span>`async fn set_bed_temperature(&mut self, target_temp: u16) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Sets the heated bed target temperature.

  Values exceeding the model's maximum are clamped automatically. Most models have a flat
  per-model ceiling (e.g. 80°C for A1 Mini), but X1C's ceiling is voltage-dependent — 110°C
  on a 220V-region unit, 120°C on a 110V-region unit, per the official spec sheet. This is
  derived from the most recently observed `home_flag` telemetry
  (`self.cache.last_home_flag`, bit 3 — see `PrinterTelemetry::is_220v_power`);
  before any `home_flag` has been received (fresh connection, no `pushall` yet) the mains
  region is unknown and X1C conservatively clamps to 110°C.

  # Example

  ```rust,ignore
  printer.set_bed_temperature(60).await?;
  ```

- <span id="superprinterclient-set-nozzle-temperature"></span>`async fn set_nozzle_temperature(&mut self, nozzle_id: u8, target_temp: u16) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Sets the target temperature of a specific hotend/nozzle [REF-MOTO-GCODE].

  * `nozzle_id`: The carriage ID (usually `0` for primary/single, or `1` for secondary on
    IDEX). **Tool-changer exception (H2C):** per `reference/04_toolhead_thermal_motion.md`
    §4's "Nozzle & Carriage Kinematics", H2C addresses its dedicated fixed hotend as `0`
    (same `M104 T0` convention as every other model) but its 6 passive tool-changer rack
    slots as `16..=21` — NOT a simple `0..physical_nozzle_count()` linear index, despite
    `physical_nozzle_count()` returning `7` for this model. The reference doc only
    confirms `16..=21` for the rack slots' telemetry-side `stat` field, not that
    `M104 T16`-style writes are actually meaningful for a passively-stored (unmounted)
    tool — validation below is deliberately permissive on H2C for exactly that reason.

  Values exceeding the model's maximum nozzle temperature are clamped automatically.

- <span id="superprinterclient-set-chamber-temperature"></span>`async fn set_chamber_temperature(&mut self, target_temp: u16) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

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

- <span id="superprinterclient-preheat-chamber"></span>`async fn preheat_chamber(&mut self, target_temp: u16) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

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

  Returns the sequence ID of the `M141` when one is sent, or of the `set_airduct` command
  when `target_temp` is `0` on a flap-only model.

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
  `PreConnected` for both the MQTT `Tls` and `Factory` slots — `ensure_mqtt()`
  short-circuits on `self.mqtt.is_some()` before either is ever called, so
  `PreConnected`'s `RawStreamFactory::dial` (which returns
  [`SocketError::NotConnected`](../io/index.md#socketerror)) is unreachable in
  practice.

- <span id="printerclient-next-sequence-id"></span>`fn next_sequence_id(&mut self) -> u64`

  Increments and returns the next transaction/sequence identifier tracking commands.

  Wraps via `clamp_task_id()` (32-bit signed integer limit) to stay within firmware
  parsing constraints [REF-MQTT-ENV] — on overflow this continues as
  `(sequence_counter + 1) % TASK_ID_MAX` rather than resetting to
  `INITIAL_SEQUENCE_ID`, so a session never revisits the same starting value mid-flight.

- <span id="printerclient-set-command-timeout"></span>`fn set_command_timeout(&mut self, secs: u64)`

  Sets the timeout (in seconds) used by command-response methods like [`get_version()`](#printerclient) and [`get_k_profiles()`](#printerclient).

  Passing `0` disables the wall-clock timeout entirely — commands then rely solely on
  the 200-message safety valve (`POLL_UNTIL_MAX_MESSAGES`), not immediate timeout.

- <span id="printerclient-request-pushall"></span>`async fn request_pushall(&mut self) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Requests a full state dump from the printer [REF-MQTT-LIFECYCLE].

- <span id="printerclient-send-ping"></span>`async fn send_ping(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Dispatches a PINGREQ keep-alive frame to maintain connection liveness.

- <span id="printerclient-serial"></span>`fn serial(&self) -> &str`

  Returns a reference to the printer's unique hardware serial number.

- <span id="printerclient-model"></span>`fn model(&self) -> PrinterModel` — [`PrinterModel`](../models/index.md#printermodel)

  Returns the resolved printer hardware model.

- <span id="printerclient-quirks"></span>`fn quirks(&self) -> &'static dyn crate::quirks::ModelQuirks` — [`ModelQuirks`](../quirks/index.md#modelquirks)

  Returns the model quirks for this printer's resolved model.

  Equivalent to `client.model().quirks()` but skips the intermediate `model()` call —
  the single entry point for every model-level limit (`nozzle_temp_max()`, axis travel
  bounds, fan/AMS predicates, etc.). `bed_temp_max()` additionally needs the printer's
  mains region, which lives on the client, not the model — see
  [`is_220v_power()`](#printerclient).

  This is the static strategy object: it knows the model and nothing about what this
  printer has reported. Quirks whose answer depends on the machine's own report take a
  [`QuirkContext`](../quirks/index.md) and cannot be called from here without one
  — use [`capabilities()`](#printerclient) for those, which supplies it from the cache.

- <span id="printerclient-quirk-context"></span>`fn quirk_context(&self) -> crate::quirks::QuirkContext<'_>` — [`QuirkContext`](../quirks/context/index.md#quirkcontext)

  Builds a [`QuirkContext`](../quirks/index.md) from this client's cached state.

  A snapshot of whatever has been observed so far: `fun`/`fun2` from the last telemetry
  carrying them, and firmware from the last [`get_version()`](#printerclient). Fields
  never observed stay `None`, which quirks read as "the printer didn't say" rather than as
  a denial.

  [`QuirkContext::telemetry`](../quirks/index.md) is left `None` here.
  The client's cache stores extracted scalars rather than a whole `PrinterTelemetry`, so
  there is no live report to hand over; the state-reading quirks are fed the `print` object
  directly as it arrives, and their results are cached (see
  [`is_door_open()`](#printerclient)). Set it yourself when calling such a quirk against
  a report you hold.

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

### `BuzzerMode`

```rust
enum BuzzerMode {
    Silent,
    Alarm,
    Chirp,
}
```

Buzzer alarm/attention chime mode for [`super::PrinterClient::set_buzzer_mode`](#printerclient) [REF-MQTT-LIFECYCLE].
Supported on models with a physical fire alarm buzzer (H2 series).

#### Variants

- **`Silent`**

  Silent/disarmed.

- **`Alarm`**

  Alarm triggered.

- **`Chirp`**

  Beeping attention chime.

#### Trait Implementations

##### `impl Clone for BuzzerMode`

- <span id="buzzermode-clone"></span>`fn clone(&self) -> BuzzerMode` — [`BuzzerMode`](types/index.md#buzzermode)

##### `impl Copy for BuzzerMode`

##### `impl Debug for BuzzerMode`

- <span id="buzzermode-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for BuzzerMode`

##### `impl PartialEq for BuzzerMode`

- <span id="buzzermode-partialeq-eq"></span>`fn eq(&self, other: &BuzzerMode) -> bool` — [`BuzzerMode`](types/index.md#buzzermode)

### `FanTarget`

```rust
enum FanTarget {
    PartCooling,
    AuxiliaryLeft,
    ChamberExhaust,
    AuxiliaryLeft2,
}
```

Enumeration representing target onboard cooling fans [REF-CLIM-FANS].

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
  a second left-side auxiliary fan, distinct from [`AuxiliaryLeft`](types/index.md#fantarget)'s
  primary port-2 fan (`FAN_REMOTE_COOLING_0_IDX`, mirrored into `big_fan1_speed`).
  Confirmed against bambuddy's test suite, which titles this fan "P2S/X2D left auxiliary
  part cooling fan" throughout (issue #60).

#### Trait Implementations

##### `impl Clone for FanTarget`

- <span id="fantarget-clone"></span>`fn clone(&self) -> FanTarget` — [`FanTarget`](types/index.md#fantarget)

##### `impl Copy for FanTarget`

##### `impl Debug for FanTarget`

- <span id="fantarget-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for FanTarget`

##### `impl Hash for FanTarget`

- <span id="fantarget-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for FanTarget`

- <span id="fantarget-partialeq-eq"></span>`fn eq(&self, other: &FanTarget) -> bool` — [`FanTarget`](types/index.md#fantarget)

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

- <span id="printspeed-from-level"></span>`fn from_level(level: u8) -> Option<Self>`

  Classifies a raw `spd_lvl` telemetry value (`1`-`4`, matching the same wire values [`PrinterClient::set_print_speed()`](#printerclient) sends).
  Returns `None` for an out-of-range level.

#### Trait Implementations

##### `impl Clone for PrintSpeed`

- <span id="printspeed-clone"></span>`fn clone(&self) -> PrintSpeed` — [`PrintSpeed`](types/index.md#printspeed)

##### `impl Copy for PrintSpeed`

##### `impl Debug for PrintSpeed`

- <span id="printspeed-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for PrintSpeed`

##### `impl Hash for PrintSpeed`

- <span id="printspeed-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for PrintSpeed`

- <span id="printspeed-partialeq-eq"></span>`fn eq(&self, other: &PrintSpeed) -> bool` — [`PrintSpeed`](types/index.md#printspeed)

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

`Unknown` covers both an unrecognized wire value and a missing field — callers
needing to tell those apart should inspect the raw `gcode_state` string directly.

#### Variants

- **`Idle`**

  No print job active or loaded (wire: `"IDLE"`).

- **`Preparing`**

  Print preparing to start — homing, bed leveling, or priming, physical
  motion in progress (wire: `"PREPARE"`).

- **`Slicing`**

  Printer is slicing a job on-device, before any physical motion (wire: `"SLICING"`).
  
  Distinct from [`Preparing`](types/index.md#printstatus): nothing is moving yet. It is still a
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

  Unrecognized wire value, or `gcode_state` field missing entirely — see the enum's doc comment.

#### Implementations

- <span id="printstatus-from-gcode-state"></span>`fn from_gcode_state(state: &str) -> Self`

  Classifies a raw `gcode_state` wire value (firmware casing: `"IDLE"`, `"PREPARE"`, `"SLICING"`, `"RUNNING"`, `"PAUSE"`, `"FINISH"`, `"FAILED"` [REF-MQTT-IDLEBUG]).

#### Trait Implementations

##### `impl Clone for PrintStatus`

- <span id="printstatus-clone"></span>`fn clone(&self) -> PrintStatus` — [`PrintStatus`](types/index.md#printstatus)

##### `impl Copy for PrintStatus`

##### `impl Debug for PrintStatus`

- <span id="printstatus-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for PrintStatus`

##### `impl Hash for PrintStatus`

- <span id="printstatus-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for PrintStatus`

- <span id="printstatus-partialeq-eq"></span>`fn eq(&self, other: &PrintStatus) -> bool` — [`PrintStatus`](types/index.md#printstatus)

### `TelemetryEvent`

```rust
enum TelemetryEvent {
    Report(Box<crate::types::TelemetryReport>, crate::mqtt::MqttMessage),
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

- **`Unknown`**

  Payload that didn't match any known telemetry structure.

#### Implementations

- <span id="telemetryevent-into-raw"></span>`fn into_raw(self) -> MqttMessage` — [`MqttMessage`](../mqtt/client/index.md#mqttmessage)

  Consumes the event and returns the underlying raw MQTT message.

- <span id="telemetryevent-raw"></span>`fn raw(&self) -> &MqttMessage` — [`MqttMessage`](../mqtt/client/index.md#mqttmessage)

  Returns a reference to the underlying raw MQTT message.

- <span id="telemetryevent-report"></span>`fn report(&self) -> Option<&TelemetryReport>` — [`TelemetryReport`](../types/telemetry/index.md#telemetryreport)

  Returns the typed report if this is a `Report` variant.

#### Trait Implementations

##### `impl Clone for TelemetryEvent`

- <span id="telemetryevent-clone"></span>`fn clone(&self) -> TelemetryEvent` — [`TelemetryEvent`](types/index.md#telemetryevent)

##### `impl Debug for TelemetryEvent`

- <span id="telemetryevent-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

