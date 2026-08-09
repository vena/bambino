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
| [`dummy`](#dummy) | mod | Zero-cost dummy implementations for [`PrinterClient`](super::PrinterClient)'s type parameters. |
| [`types`](#types) | mod | Client-facing enums and helper types (telemetry events, fan targets, print speed, calibration). |
| [`PrinterClient`](#printerclient) | struct | High-level client for controlling a Bambu Lab printer. |

## Modules

- [`dummy`](dummy/index.md#dummy) — Zero-cost dummy implementations for [`PrinterClient`](super::PrinterClient)'s type parameters.
- [`types`](types/index.md#types) — Client-facing enums and helper types (telemetry events, fan targets, print speed, calibration).


---

## Types

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

Cached print-progress snapshot as of the last-observed telemetry carrying any of these fields (via [`poll_telemetry()`](crate::client::PrinterClient::poll_telemetry)).

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

  Estimated remaining duration of the active layer sequence, in seconds.

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

Wraps an MQTT session (connected or lazy) and optionally a [`BambuFtpsClient`](../ftps/client/index.md#bambuftpsclient) for
SD card access. `MqttRawIO`/`MqttTls`/`MqttFactory` are MQTT's [`TlsConnector`](../io/index.md#tlsconnector)+
[`RawStreamFactory`](../io/index.md#rawstreamfactory) pair (mandatory — every `PrinterClient` needs MQTT);
`FtpsRawIO`/`FtpsTls`/`FtpsFactory` are FTPS's independent pair (defaulted, configured via
[`.with_ftps()`](Self::with_ftps)). Use [`PreConnected`] for both MQTT slots when wrapping
an already-connected [`MqttClient`](../mqtt/client/index.md#mqttclient) (see [`from_mqtt()`](Self::from_mqtt)), or a
platform's `TlsConnector`+`RawStreamFactory` pair (e.g. `TokioTlsConnector`+
`TokioRawStreamFactory`) for lazy connection via [`new()`](Self::new).

#### Implementations

- <span id="superprinterclient-change-filament"></span>`async fn change_filament(&mut self, ams_id: i32, slot_id: i32, curr_temp: i32, tar_temp: i32) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Triggers a filament load or unload sequence on a physical AMS unit or external spool [REF-AMS-MAP].

- <span id="superprinterclient-start-drying"></span>`async fn start_drying(&mut self, ams_id: i32, temp: u32, duration_hours: u32, humidity: u32, rotate_tray: bool, cooling_temp: i32, close_power_conflict: bool, filament: &str) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Initiates a dry-chamber heating cycle on an AMS-HT or AMS 2 Pro unit [REF-AMS-DRYER].

- <span id="superprinterclient-stop-drying"></span>`async fn stop_drying(&mut self, ams_id: i32) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Terminates an active dry-chamber heating cycle on an AMS unit [REF-AMS-DRYER].

- <span id="superprinterclient-scan-rfid"></span>`async fn scan_rfid(&mut self, ams_id: i32, slot_id: i32) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Scans proprietary RFID tag properties on a specific AMS tray [REF-AMS-MAP].

- <span id="superprinterclient-select-k-profile"></span>`async fn select_k_profile(&mut self, ams_id: i32, tray_id: i32, cali_idx: i32, filament_id: &str, nozzle_diameter: &str) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Binds a stored K-profile calibration entry to an AMS material slot [REF-AMS-MAP].

- <span id="superprinterclient-get-version"></span>`async fn get_version(&mut self) -> Result<VersionInfo, Error>` — [`VersionInfo`](../types/version/index.md#versioninfo), [`Error`](../error/index.md#error)

  Queries the printer's expansion bus version database and returns typed module info.

- <span id="superprinterclient-get-k-profiles"></span>`async fn get_k_profiles(&mut self) -> Result<ExtrusionCaliGetResponse, Error>` — [`ExtrusionCaliGetResponse`](../diagnostics/kprofile/index.md#extrusioncaligetresponse), [`Error`](../error/index.md#error)

  Requests a dump of the printer's stored K-profile calibration database [REF-DIAG-KPROF].

- <span id="superprinterclient-set-k-profile-primed"></span>`fn set_k_profile_primed(&mut self, primed: bool)`

  Controls whether `get_k_profiles()` sends an automatic priming request.

- <span id="superprinterclient-attach-camera"></span>`fn attach_camera(&mut self, camera: BambuBinaryCameraStream<<CameraTls as >::Stream>)` — [`BambuBinaryCameraStream`](../camera/binary/index.md#bambubinarycamerastream), [`TlsConnector`](../io/index.md#tlsconnector)

  Injects a pre-connected [`BambuBinaryCameraStream`](../camera/binary/index.md#bambubinarycamerastream) directly.

- <span id="superprinterclient-camera"></span>`async fn camera(&mut self) -> Result<&mut BambuBinaryCameraStream<<CameraTls as >::Stream>, Error>` — [`BambuBinaryCameraStream`](../camera/binary/index.md#bambubinarycamerastream), [`TlsConnector`](../io/index.md#tlsconnector), [`Error`](../error/index.md#error)

  Returns direct access to the underlying [`BambuBinaryCameraStream`](../camera/binary/index.md#bambubinarycamerastream), auto-connecting if needed.

- <span id="superprinterclient-read-camera-frame"></span>`async fn read_camera_frame(&mut self, frame_buf: &mut Vec<u8>) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Reads the next camera frame, auto-connecting (and authenticating) if needed.

- <span id="superprinterclient-disconnect-camera"></span>`async fn disconnect_camera(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Disconnects the camera session, if one exists, and clears it from the client.

- <span id="superprinterclient-connect-mqtt"></span>`async fn connect_mqtt(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Eagerly establishes the MQTT connection.

- <span id="superprinterclient-is-mqtt-connected"></span>`fn is_mqtt_connected(&self) -> bool`

  Returns whether the MQTT session is currently established.

- <span id="superprinterclient-attach-mqtt"></span>`fn attach_mqtt(&mut self, mqtt: MqttClient<<MqttTls as >::Stream>)` — [`MqttClient`](../mqtt/client/index.md#mqttclient), [`TlsConnector`](../io/index.md#tlsconnector)

  Injects a pre-connected [`MqttClient`](../mqtt/client/index.md#mqttclient) directly.

- <span id="superprinterclient-disconnect-mqtt"></span>`async fn disconnect_mqtt(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Disconnects the MQTT session, if one exists, and clears it from the client.

- <span id="superprinterclient-with-timer"></span>`fn with_timer<NewTimer: TimerProvider>(self, timer: NewTimer) -> PrinterClient<MqttRawIO, MqttTls, MqttFactory, NewTimer, FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer, CameraRawIO, CameraTls, CameraFactory>` — [`PrinterClient`](#printerclient)

  Sets a [`TimerProvider`](../io/index.md#timerprovider) for wall-clock command-response timeouts.

- <span id="superprinterclient-with-mqtt-port"></span>`fn with_mqtt_port(self, port: u16) -> Self`

  Overrides the default MQTT port (8883).

- <span id="superprinterclient-with-connect-timeout"></span>`fn with_connect_timeout(self, secs: u64) -> Self`

  Overrides the default connect-timeout deadline (10s) that bounds `ensure_mqtt()`/`ensure_ftps()`'s combined dial+TLS-connect sequence.

  Passing `0` disables the timeout entirely, matching `set_command_timeout`'s "0 disables"

  convention. Non-consuming — chain onto any construction path.

- <span id="superprinterclient-with-ftps"></span>`fn with_ftps<NewFtpsRawIO, NewFtpsTls, NewFtpsFactory, NewFtpsTimer>(self, tls: NewFtpsTls, factory: NewFtpsFactory, timer: NewFtpsTimer) -> PrinterClient<MqttRawIO, MqttTls, MqttFactory, Timer, NewFtpsRawIO, NewFtpsTls, NewFtpsFactory, NewFtpsTimer, CameraRawIO, CameraTls, CameraFactory>` — [`PrinterClient`](#printerclient)

  Configures FTPS for lazy connection on first storage method call.

- <span id="superprinterclient-with-ftps-port"></span>`fn with_ftps_port(self, port: u16) -> Self`

  Overrides the default FTPS port (990).

- <span id="superprinterclient-with-ftps-allow-unverified-tls-1-2"></span>`fn with_ftps_allow_unverified_tls_1_2(self, allow: bool) -> Self`

  Overrides the default `false` for `BambuFtpsClient`'s TLS-1.2-enforcement bypass.

- <span id="superprinterclient-connect-ftps"></span>`async fn connect_ftps(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Eagerly establishes the FTPS connection.

- <span id="superprinterclient-is-ftps-connected"></span>`fn is_ftps_connected(&self) -> bool`

  Returns whether the FTPS session is currently established.

- <span id="superprinterclient-connect-camera"></span>`async fn connect_camera(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Eagerly establishes the camera connection.

- <span id="superprinterclient-is-camera-connected"></span>`fn is_camera_connected(&self) -> bool`

  Returns whether the camera session is currently established.

- <span id="superprinterclient-with-camera"></span>`fn with_camera<NewCameraRawIO, NewCameraTls, NewCameraFactory>(self, tls: NewCameraTls, factory: NewCameraFactory) -> PrinterClient<MqttRawIO, MqttTls, MqttFactory, Timer, FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer, NewCameraRawIO, NewCameraTls, NewCameraFactory>` — [`PrinterClient`](#printerclient)

  Configures the binary-JPEG camera for lazy connection on first camera method call.

- <span id="superprinterclient-with-camera-port"></span>`fn with_camera_port(self, port: u16) -> Self`

  Overrides the default camera port (6000, binary-JPEG only).

- <span id="superprinterclient-with-camera-max-frame-size"></span>`fn with_camera_max_frame_size(self, bytes: usize) -> Self`

  Overrides the default maximum accepted camera frame size (see `BambuBinaryCameraStream::with_max_frame_size`).

- <span id="superprinterclient-set-fan-speed"></span>`async fn set_fan_speed(&mut self, fan_type: FanTarget, speed_percent: u8) -> Result<u16, Error>` — [`FanTarget`](types/index.md#fantarget), [`Error`](../error/index.md#error)

  Sets the speed of a targeted onboard fan as a percentage (0 to 100) [REF-CLIM-FANS].

- <span id="superprinterclient-set-led"></span>`async fn set_led(&mut self, node: &str, turn_on: bool) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Configures the active state of a targeted enclosure LED lighting node [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-set-airduct-mode"></span>`async fn set_airduct_mode(&mut self, mode: crate::mqtt::commands::AirductMode) -> Result<u16, Error>` — [`AirductMode`](../mqtt/commands/hardware/index.md#airductmode), [`Error`](../error/index.md#error)

  Configures the active climate airduct damper mode [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-set-prompt-sound"></span>`async fn set_prompt_sound(&mut self, enable_sound: bool) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Configures whether the printer's speakers emit prompt notification sounds [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-set-buzzer-mode"></span>`async fn set_buzzer_mode(&mut self, mode: BuzzerMode) -> Result<u16, Error>` — [`BuzzerMode`](types/index.md#buzzermode), [`Error`](../error/index.md#error)

  Modifies active alarm or attention chime parameters on the physical buzzer module [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-is-axis-homed"></span>`fn is_axis_homed(&self, axis: char) -> Option<bool>`

  Returns whether `axis` (`'X'`/`'Y'`/`'Z'`, case-insensitive) was homed as of the last-observed `home_flag` telemetry.

  `None` means no telemetry carrying `home_flag` has been observed yet (via

  [`poll_telemetry()`](Self::poll_telemetry)) — not "unhomed". Advisory only: the firmware does

  not reject motion on unhomed axes [REF-MOTO-HOME].

- <span id="superprinterclient-is-all-axes-homed"></span>`fn is_all_axes_homed(&self) -> Option<bool>`

  Returns whether X, Y, and Z were all homed as of the last-observed `home_flag` telemetry.

  `None` means no telemetry carrying `home_flag` has been observed yet.

- <span id="superprinterclient-send-gcode"></span>`async fn send_gcode(&mut self, gcode_line: &str) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Sends a G-code command with model-aware safety validation.

  

  Rejects commands that would be unsafe on the active model (e.g., partial-axis

  homing on bed-on-Z platforms). Use [`send_gcode_raw()`](Self::send_gcode_raw)

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

- <span id="superprinterclient-home-axes"></span>`async fn home_axes(&mut self, home_z_only_danger: bool) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Dispatches safe homing operations to prevent hardware collisions.

- <span id="superprinterclient-move-relative"></span>`async fn move_relative(&mut self, axis: char, distance: f32, feedrate: u32) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Dispatches a manual relative axis movement block.

- <span id="superprinterclient-extrude"></span>`async fn extrude(&mut self, length: f32, feedrate: u32) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Dispatches a manual relative extrusion command sequence [REF-GCODE-EXTRUDE].

- <span id="superprinterclient-wait-for-homing"></span>`async fn wait_for_homing(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Blocks until a `G28` homing cycle observed via telemetry has completed.

- <span id="superprinterclient-pause-print"></span>`async fn pause_print(&mut self) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Pauses the currently active print job [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-resume-print"></span>`async fn resume_print(&mut self) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Resumes a paused print job [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-stop-print"></span>`async fn stop_print(&mut self) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Aborts/cancels the currently running print job queue [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-clear-print-error"></span>`async fn clear_print_error(&mut self) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Clears active error codes from the printer's diagnostic fault register [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-set-print-speed"></span>`async fn set_print_speed(&mut self, level: PrintSpeed) -> Result<u16, Error>` — [`PrintSpeed`](types/index.md#printspeed), [`Error`](../error/index.md#error)

  Dynamically scales maximum velocity and acceleration limits during an active print [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-skip-objects"></span>`async fn skip_objects(&mut self, object_ids: Vec<u32>) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Bypasses rendering of specific objects within an active multi-model print job [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-start-calibration"></span>`async fn start_calibration(&mut self, options: CalibrationOption) -> Result<u16, Error>` — [`CalibrationOption`](types/index.md#calibrationoption), [`Error`](../error/index.md#error)

  Triggers automated physical calibration routines on the printer chassis [REF-MQTT-LIFECYCLE].

  

  Use `CalibrationOption` flags combined with `|` to select routines:

  ```rust,ignore

  client.start_calibration(

      CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION

  ).await?;

  ```

- <span id="superprinterclient-start-print"></span>`async fn start_print(&mut self, config: &PrintJobConfig) -> Result<u16, Error>` — [`PrintJobConfig`](../mqtt/commands/print_job/index.md#printjobconfig), [`Error`](../error/index.md#error)

  Submits a `.3mf` print job from MicroSD storage for execution [REF-MQTT-LIFECYCLE].

- <span id="superprinterclient-attach-storage"></span>`fn attach_storage(&mut self, ftps_client: BambuFtpsClient<FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer>)` — [`BambuFtpsClient`](../ftps/client/index.md#bambuftpsclient)

  Injects a pre-connected [`BambuFtpsClient`](../ftps/client/index.md#bambuftpsclient) directly.

- <span id="superprinterclient-storage"></span>`async fn storage(&mut self) -> Result<&mut BambuFtpsClient<FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer>, Error>` — [`BambuFtpsClient`](../ftps/client/index.md#bambuftpsclient), [`Error`](../error/index.md#error)

  Returns direct access to the underlying [`BambuFtpsClient`](../ftps/client/index.md#bambuftpsclient), auto-connecting if needed.

- <span id="superprinterclient-disconnect-storage"></span>`async fn disconnect_storage(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Disconnects the FTPS session, if one exists, and clears it from the client.

- <span id="superprinterclient-poll-telemetry"></span>`async fn poll_telemetry(&mut self) -> Result<TelemetryEvent, Error>` — [`TelemetryEvent`](types/index.md#telemetryevent), [`Error`](../error/index.md#error)

  Pulls the next telemetry event from the MQTT channel.

  

  Returns a [`Report`](https://docs.rs/std/latest/std/error/struct.Report.html) if the payload deserializes as a known

  telemetry structure, or [`TelemetryEvent::Unknown`] otherwise. A payload that

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

              if let Some(state) = &report.print.gcode_state {

                  println!("Printer state: {}", state);

              }

          }

          TelemetryEvent::Unknown(_) => {}

      }

  }

  ```

- <span id="superprinterclient-print-status"></span>`fn print_status(&self) -> Option<PrintStatus>` — [`PrintStatus`](types/index.md#printstatus)

  Returns the printer's high-level activity classification as of the last-observed `gcode_state` telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).

  `None` means no telemetry carrying `gcode_state` has been observed yet.

- <span id="superprinterclient-is-door-open"></span>`fn is_door_open(&self) -> Option<bool>`

  Returns whether the door was open as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).

- <span id="superprinterclient-active-fault"></span>`fn active_fault(&self) -> Option<DecodedPrintError>` — [`DecodedPrintError`](../diagnostics/hms/index.md#decodedprinterror)

  Returns the decoded active print-error fault as of the last-observed `print_error` telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).

- <span id="superprinterclient-print-progress"></span>`fn print_progress(&self) -> PrintProgress` — [`PrintProgress`](types/index.md#printprogress)

  Returns the print progress snapshot as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).

  Each field independently tracks its own "last observed" value — see [`PrintProgress`](types/index.md#printprogress)'s doc

  comment.

- <span id="superprinterclient-bed-temperatures"></span>`fn bed_temperatures(&self) -> (u16, u16)`

  Returns the bed's (actual, target) temperatures in °C, decoded from the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).

  Returns `(0, 0)` before any telemetry carrying bed temperature has been observed.

- <span id="superprinterclient-ams"></span>`fn ams(&self) -> Option<&AmsStatusReport>` — [`AmsStatusReport`](../types/telemetry/ams/index.md#amsstatusreport)

  Returns the cached AMS/tray status report as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).

  `None` means no telemetry carrying `print.ams` has been observed yet.

- <span id="superprinterclient-printing-tray-global-id"></span>`fn printing_tray_global_id(&self) -> Option<u8>`

  Returns the global tray ID of the spool currently feeding the active extruder, as of

  the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).

- <span id="superprinterclient-sanitized-ams"></span>`fn sanitized_ams(&self) -> Option<AmsStatusReport>` — [`AmsStatusReport`](../types/telemetry/ams/index.md#amsstatusreport)

  Returns a cloned copy of the cached AMS status report with every tray's stale material

  fields cleared via [`clean_stale_tray_data`](../ams/parser/index.md#clean-stale-tray-data)

  (mirrors [`active_hms_alerts()`](Self::active_hms_alerts)'s raw-cache-decode-on-access

  shape). `None` under the same condition as [`ams()`](Self::ams) — no telemetry carrying

  `print.ams` observed yet. Does not mutate the underlying cache — [`ams()`](Self::ams)

  keeps returning the raw values; see its doc comment for why the raw cache is never

  proactively scrubbed.

- <span id="superprinterclient-vt-tray"></span>`fn vt_tray(&self) -> Option<&VirtualTray>` — [`VirtualTray`](../types/telemetry/ams/index.md#virtualtray)

  Returns the cached virtual/external spool holder state (single-nozzle models) as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).

  `None` means no telemetry carrying `print.vt_tray` has been observed yet — including on IDEX

  models, which send [`vir_slot()`](Self::vir_slot) instead.

- <span id="superprinterclient-vir-slot"></span>`fn vir_slot(&self) -> Option<&[VirtualTray]>` — [`VirtualTray`](../types/telemetry/ams/index.md#virtualtray)

  Returns the cached IDEX external spool holder array as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).

  `None` means no telemetry carrying `print.vir_slot` has been observed yet — including on

  single-nozzle models, which send [`vt_tray()`](Self::vt_tray) instead.

- <span id="superprinterclient-nozzle-temperatures"></span>`fn nozzle_temperatures(&self) -> Vec<(u8, u16, u16)>`

  Returns the nozzle temperatures as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)) as `(id, actual, target)` tuples in °C.

  Single-nozzle models return one entry (`id` 0); IDEX models return one entry per physical

  nozzle. See [`decode_nozzle_temperatures`](../types/telemetry/index.md#decode-nozzle-temperatures) for the cross-model decode (including the

  undocumented IDEX flat-field routing quirk).

- <span id="superprinterclient-chamber-temperature"></span>`fn chamber_temperature(&self) -> Option<(u16, u16)>`

  Returns the chamber's (actual, target) temperatures in °C, decoded from the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).

- <span id="superprinterclient-hms"></span>`fn hms(&self) -> Option<&[HmsEntry]>` — [`HmsEntry`](../types/telemetry/diagnostics/index.md#hmsentry)

  Returns the cached active hardware-alert (HMS) entries as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).

  `None` means no telemetry carrying `print.hms` has been observed yet.

- <span id="superprinterclient-ipcam"></span>`fn ipcam(&self) -> Option<&IpcamTelemetry>` — [`IpcamTelemetry`](../types/telemetry/diagnostics/index.md#ipcamtelemetry)

  Returns the cached camera/recording state as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).

  `None` means no telemetry carrying `print.ipcam` has been observed yet.

- <span id="superprinterclient-active-hms-alerts"></span>`fn active_hms_alerts(&self) -> Vec<DecodedHmsAlert>` — [`DecodedHmsAlert`](../diagnostics/hms/index.md#decodedhmsalert)

  Returns every cached HMS entry decoded and filtered to genuine faults (mirrors `active_fault()`'s raw-cache-decode-on-access shape).

  Empty when nothing is cached or nothing currently decodes as a genuine fault — there's no caller

  action that would differ between those two cases.

- <span id="superprinterclient-part-cooling-fan-speed"></span>`fn part_cooling_fan_speed(&self) -> Option<u8>`

  Returns the part-cooling fan speed (Port 1) as a percentage (0-100), decoded from the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).

- <span id="superprinterclient-auxiliary-left-fan-speed"></span>`fn auxiliary_left_fan_speed(&self) -> Option<u8>`

  Returns the primary left-side auxiliary fan speed (Port 2) as a percentage (0-100).

- <span id="superprinterclient-chamber-exhaust-fan-speed"></span>`fn chamber_exhaust_fan_speed(&self) -> Option<u8>`

  Returns the chamber exhaust/filtration fan speed (Port 3) as a percentage (0-100).

- <span id="superprinterclient-heatbreak-fan-speed"></span>`fn heatbreak_fan_speed(&self) -> Option<u8>`

  Returns the toolhead heatbreak fan speed as a percentage (0-100).

  Not independently controllable (no corresponding `FanTarget` variant/M106 port) — read-only

  telemetry.

- <span id="superprinterclient-auxiliary-right-fan-speed"></span>`fn auxiliary_right_fan_speed(&self) -> Option<u8>`

  Returns the X2D/P2S secondary right-side auxiliary fan speed (Port 10, `FanTarget::AuxiliaryRight`) as a percentage (0-100).

  Reported at a different wire location than the other four fans —

  `device.airduct.parts[id=160].state` — already a direct percentage, no 0-15 step conversion

  [REF-CLIM-FANS].

- <span id="superprinterclient-print-speed"></span>`fn print_speed(&self) -> Option<PrintSpeed>` — [`PrintSpeed`](types/index.md#printspeed)

  Returns the printer's current print-speed level as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).

  `None` before any telemetry carrying `spd_lvl` has been observed, or if the observed value is

  out of the known 1-4 range.

- <span id="superprinterclient-print-speed-magnitude"></span>`fn print_speed_magnitude(&self) -> Option<u16>`

  Returns the printer's current print-speed magnitude (percentage of nominal feedrate) as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).

- <span id="superprinterclient-wifi-signal"></span>`fn wifi_signal(&self) -> Option<&str>`

  Returns the raw wireless signal strength string (e.g. `"-52dBm"`) as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).

- <span id="superprinterclient-is-ethernet-active-via-wifi-signal"></span>`fn is_ethernet_active_via_wifi_signal(&self) -> bool`

  Returns whether the printer is on wired Ethernet, per the cached `wifi_signal` sentinel (mirrors `PrinterTelemetry::is_ethernet_active_via_wifi_signal()` but works between polls off the cached value, the same way [`is_all_axes_homed()`](Self::is_all_axes_homed) works off cached `home_flag`).

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

  (`self.cache.last_home_flag`, bit 3 — see [`PrinterTelemetry::is_220v_power`](crate::types::PrinterTelemetry::is_220v_power));

  before any `home_flag` has been received (fresh connection, no `pushall` yet) the mains

  region is unknown and X1C conservatively clamps to 110°C.

  

  # Example

  

  ```rust,ignore

  printer.set_bed_temperature(60).await?;

  ```

- <span id="superprinterclient-set-nozzle-temperature"></span>`async fn set_nozzle_temperature(&mut self, nozzle_id: u8, target_temp: u16) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Sets the target temperature of a specific hotend/nozzle [REF-MOTO-GCODE].

- <span id="superprinterclient-set-chamber-temperature"></span>`async fn set_chamber_temperature(&mut self, target_temp: u16) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Sets the target temperature of the active heated chamber loop [REF-MOTO-GCODE].

- <span id="printerclient-new"></span>`fn new(tls: MqttTls, factory: MqttFactory, identity: PrinterIdentity) -> Self` — [`PrinterIdentity`](../identity/index.md#printeridentity)

  Creates a lazy client that defers MQTT connection until first use.

- <span id="printerclient-from-mqtt"></span>`fn from_mqtt(mqtt_client: MqttClient<IO>, model: PrinterModel) -> Self` — [`MqttClient`](../mqtt/client/index.md#mqttclient), [`PrinterModel`](../models/index.md#printermodel)

  Wraps an already-connected [`MqttClient`](../mqtt/client/index.md#mqttclient) in a `PrinterClient`.

- <span id="printerclient-next-sequence-id"></span>`fn next_sequence_id(&mut self) -> u64`

  Increments and returns the next transaction/sequence identifier tracking commands.

- <span id="printerclient-set-command-timeout"></span>`fn set_command_timeout(&mut self, secs: u64)`

  Sets the timeout (in seconds) used by command-response methods like [`get_version()`](Self::get_version) and [`get_k_profiles()`](Self::get_k_profiles).

- <span id="printerclient-request-pushall"></span>`async fn request_pushall(&mut self) -> Result<u16, Error>` — [`Error`](../error/index.md#error)

  Requests a full state dump from the printer [REF-MQTT-LIFECYCLE].

- <span id="printerclient-send-ping"></span>`async fn send_ping(&mut self) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Dispatches a PINGREQ keep-alive frame to maintain connection liveness.

- <span id="printerclient-serial"></span>`fn serial(&self) -> &str`

  Returns a reference to the printer's unique hardware serial number.

- <span id="printerclient-model"></span>`fn model(&self) -> PrinterModel` — [`PrinterModel`](../models/index.md#printermodel)

  Returns the resolved printer hardware model.

- <span id="printerclient-mqtt"></span>`async fn mqtt(&mut self) -> Result<&mut MqttClient<<MqttTls as >::Stream>, Error>` — [`MqttClient`](../mqtt/client/index.md#mqttclient), [`TlsConnector`](../io/index.md#tlsconnector), [`Error`](../error/index.md#error)

  Returns direct access to the underlying [`MqttClient`](../mqtt/client/index.md#mqttclient), auto-connecting if needed.

#### Trait Implementations

### `BuzzerMode`

```rust
enum BuzzerMode {
    Silent,
    Alarm,
    Chirp,
}
```

Buzzer alarm/attention chime mode for [`super::PrinterClient::set_buzzer_mode`] [REF-MQTT-LIFECYCLE].
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
    AuxiliaryRight,
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

- **`AuxiliaryRight`**

  Secondary right-side auxiliary fan (Port 10, supported on X2D and P2S).

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

  Classifies a raw `spd_lvl` telemetry value (`1`-`4`, matching the same wire values [`PrinterClient::set_print_speed()`](crate::client::PrinterClient::set_print_speed) sends).

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

  Classifies a raw `gcode_state` wire value (firmware casing: `"IDLE"`, `"PREPARE"`, `"RUNNING"`, `"PAUSE"`, `"FINISH"`, `"FAILED"` [REF-MQTT-IDLEBUG]).

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
available via [`into_raw`](TelemetryEvent::into_raw).

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

