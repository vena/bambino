**bambino > client**

# Module: client

## Contents

**Modules**

- [`dummy`](#dummy) - Zero-cost dummy implementations for [`PrinterClient`](super::PrinterClient)'s type parameters.
- [`types`](#types) - Client-facing enums and helper types (telemetry events, fan targets, print speed, calibration).

**Structs**

- [`PrinterClient`](#printerclient) - High-level client for controlling a Bambu Lab printer.

---

## bambino::client::PrinterClient

*Struct*

High-level client for controlling a Bambu Lab printer.

Wraps an MQTT session (connected or lazy) and optionally a [`BambuFtpsClient`] for
SD card access. `MqttRawIO`/`MqttTls`/`MqttFactory` are MQTT's [`TlsConnector`]+
[`RawStreamFactory`] pair (mandatory — every `PrinterClient` needs MQTT);
`FtpsRawIO`/`FtpsTls`/`FtpsFactory` are FTPS's independent pair (defaulted, configured via
[`.with_ftps()`](Self::with_ftps)). Use [`PreConnected`] for both MQTT slots when wrapping
an already-connected [`BambuMqttClient`] (see [`from_mqtt()`](Self::from_mqtt)), or a
platform's `TlsConnector`+`RawStreamFactory` pair (e.g. `TokioTlsConnector`+
`TokioRawStreamFactory`) for lazy connection via [`new()`](Self::new).

**Generic Parameters:**
- MqttRawIO
- MqttTls
- MqttFactory
- Timer
- FtpsRawIO
- FtpsTls
- FtpsFactory
- FtpsTimer
- CameraRawIO
- CameraTls
- CameraFactory

**Methods:**

- `fn new(tls: MqttTls, factory: MqttFactory, ip: &str, serial: &str, access_code: &str, model: BambuModel) -> Self` - Creates a lazy client that defers MQTT connection until first use.
- `fn connect_mqtt(self: & mut Self) -> Result<(), BambuError>` - Eagerly establishes the MQTT connection.
- `fn mqtt_connected(self: &Self) -> bool` - Returns whether the MQTT session is currently established.
- `fn with_timer<NewTimer>(self: Self, timer: NewTimer) -> PrinterClient<MqttRawIO, MqttTls, MqttFactory, NewTimer, FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer, CameraRawIO, CameraTls, CameraFactory>` - Sets a [`TimerProvider`] for wall-clock command-response timeouts.
- `fn with_mqtt_port(self: Self, port: u16) -> Self` - Overrides the default MQTT port (8883).
- `fn with_connect_timeout(self: Self, secs: u64) -> Self` - Overrides the default connect-timeout deadline (10s) that bounds
- `fn with_ftps<NewFtpsRawIO, NewFtpsTls, NewFtpsFactory, NewFtpsTimer>(self: Self, tls: NewFtpsTls, factory: NewFtpsFactory, timer: NewFtpsTimer) -> PrinterClient<MqttRawIO, MqttTls, MqttFactory, Timer, NewFtpsRawIO, NewFtpsTls, NewFtpsFactory, NewFtpsTimer, CameraRawIO, CameraTls, CameraFactory>` - Configures FTPS for lazy connection on first storage method call.
- `fn with_ftps_port(self: Self, port: u16) -> Self` - Overrides the default FTPS port (990).
- `fn connect_ftps(self: & mut Self) -> Result<(), BambuError>` - Eagerly establishes the FTPS connection.
- `fn ftps_connected(self: &Self) -> bool` - Returns whether the FTPS session is currently established.
- `fn connect_camera(self: & mut Self) -> Result<(), BambuError>` - Eagerly establishes the camera connection.
- `fn camera_connected(self: &Self) -> bool` - Returns whether the camera session is currently established.
- `fn with_camera<NewCameraRawIO, NewCameraTls, NewCameraFactory>(self: Self, tls: NewCameraTls, factory: NewCameraFactory) -> PrinterClient<MqttRawIO, MqttTls, MqttFactory, Timer, FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer, NewCameraRawIO, NewCameraTls, NewCameraFactory>` - Configures the binary-JPEG camera for lazy connection on first camera method call.
- `fn with_camera_port(self: Self, port: u16) -> Self` - Overrides the default camera port (6000, binary-JPEG only).
- `fn with_camera_max_frame_size(self: Self, bytes: usize) -> Self` - Overrides the default maximum accepted camera frame size (see
- `fn poll_telemetry(self: & mut Self) -> Result<TelemetryEvent, BambuError>` - Pulls the next telemetry event from the MQTT channel.
- `fn print_status(self: &Self) -> Option<PrintStatus>` - Returns the printer's high-level activity classification as of the
- `fn door_open(self: &Self) -> Option<bool>` - Returns whether the door was open as of the last-observed telemetry (via
- `fn active_fault(self: &Self) -> Option<DecodedPrintError>` - Returns the decoded active print-error fault as of the last-observed `print_error`
- `fn print_progress(self: &Self) -> PrintProgress` - Returns the print progress snapshot as of the last-observed telemetry (via
- `fn bed_temperatures(self: &Self) -> (u16, u16)` - Returns the bed's (actual, target) temperatures in °C, decoded from the last-observed
- `fn ams(self: &Self) -> Option<&AmsStatusReport>` - Returns the cached AMS/tray status report as of the last-observed telemetry (via
- `fn vt_tray(self: &Self) -> Option<&VirtualTray>` - Returns the cached virtual/external spool holder state (single-nozzle models) as of
- `fn vir_slot(self: &Self) -> Option<&[VirtualTray]>` - Returns the cached IDEX external spool holder array as of the last-observed telemetry
- `fn nozzle_temperatures(self: &Self) -> Vec<(u8, u16, u16)>` - Returns the nozzle temperatures as of the last-observed telemetry (via
- `fn chamber_temperature(self: &Self) -> Option<(u16, u16)>` - Returns the chamber's (actual, target) temperatures in °C, decoded from the
- `fn hms(self: &Self) -> Option<&[HmsEntry]>` - Returns the cached active hardware-alert (HMS) entries as of the last-observed
- `fn active_hms_alerts(self: &Self) -> Vec<DecodedHmsAlert>` - Returns every cached HMS entry decoded and filtered to genuine faults (mirrors
- `fn part_cooling_fan_speed(self: &Self) -> Option<u8>` - Returns the part-cooling fan speed (Port 1) as a percentage (0-100), decoded from the
- `fn auxiliary_left_fan_speed(self: &Self) -> Option<u8>` - Returns the primary left-side auxiliary fan speed (Port 2) as a percentage (0-100).
- `fn chamber_exhaust_fan_speed(self: &Self) -> Option<u8>` - Returns the chamber exhaust/filtration fan speed (Port 3) as a percentage (0-100).
- `fn heatbreak_fan_speed(self: &Self) -> Option<u8>` - Returns the toolhead heatbreak fan speed as a percentage (0-100). Not independently
- `fn auxiliary_right_fan_speed(self: &Self) -> Option<u8>` - Returns the X2D/P2S secondary right-side auxiliary fan speed (Port 10,
- `fn print_speed(self: &Self) -> Option<PrintSpeed>` - Returns the printer's current print-speed level as of the last-observed telemetry (via
- `fn print_speed_magnitude(self: &Self) -> Option<u16>` - Returns the printer's current print-speed magnitude (percentage of nominal feedrate) as
- `fn wifi_signal(self: &Self) -> Option<&str>` - Returns the raw wireless signal strength string (e.g. `"-52dBm"`) as of the
- `fn is_ethernet_active_via_wifi_signal(self: &Self) -> bool` - Returns whether the printer is on wired Ethernet, per the cached `wifi_signal` sentinel
- `fn poll_raw(self: & mut Self) -> Result<MqttMessage, BambuError>` - Pulls the next raw MQTT message without deserialization.
- `fn attach_camera(self: & mut Self, camera: BambuBinaryCameraStream<<CameraTls as >::Stream>)` - Injects a pre-connected [`BambuBinaryCameraStream`] directly.
- `fn camera(self: & mut Self) -> Result<& mut BambuBinaryCameraStream<<CameraTls as >::Stream>, BambuError>` - Returns direct access to the underlying [`BambuBinaryCameraStream`], auto-connecting
- `fn read_camera_frame(self: & mut Self, frame_buf: & mut Vec<u8>) -> Result<(), BambuError>` - Reads the next camera frame, auto-connecting (and authenticating) if needed.
- `fn disconnect_camera(self: & mut Self) -> Result<(), BambuError>` - Disconnects the camera session, if one exists, and clears it from the client.
- `fn set_fan_speed(self: & mut Self, fan_type: FanTarget, speed_percent: u8) -> Result<u16, BambuError>` - Sets the speed of a targeted onboard fan as a percentage (0 to 100) [REF-CLIM-FANS].
- `fn set_led(self: & mut Self, node: &str, turn_on: bool) -> Result<u16, BambuError>` - Configures the active state of a targeted enclosure LED lighting node [REF-MQTT-LIFECYCLE].
- `fn set_airduct_mode(self: & mut Self, mode: crate::mqtt::commands::AirductMode) -> Result<u16, BambuError>` - Configures the active climate airduct damper mode [REF-MQTT-LIFECYCLE].
- `fn set_prompt_sound(self: & mut Self, enable_sound: bool) -> Result<u16, BambuError>` - Configures whether the printer's speakers emit prompt notification sounds [REF-MQTT-LIFECYCLE].
- `fn set_buzzer_mode(self: & mut Self, mode: BuzzerMode) -> Result<u16, BambuError>` - Modifies active alarm or attention chime parameters on the physical buzzer module [REF-MQTT-LIFECYCLE].
- `fn is_axis_homed(self: &Self, axis: char) -> Option<bool>` - Returns whether `axis` (`'X'`/`'Y'`/`'Z'`, case-insensitive) was homed as of the
- `fn is_all_axes_homed(self: &Self) -> Option<bool>` - Returns whether X, Y, and Z were all homed as of the last-observed `home_flag`
- `fn send_gcode(self: & mut Self, gcode_line: &str) -> Result<u16, BambuError>` - Sends a G-code command with model-aware safety validation.
- `fn send_gcode_raw(self: & mut Self, gcode_line: &str) -> Result<u16, BambuError>` - Dispatches a raw G-code string without model safety checks [REF-MOTO-GCODE].
- `fn home_axes(self: & mut Self, home_z_only_danger: bool) -> Result<u16, BambuError>` - Dispatches safe homing operations to prevent hardware collisions.
- `fn move_relative(self: & mut Self, axis: char, distance: f32, feedrate: u32) -> Result<u16, BambuError>` - Dispatches a manual relative axis movement block.
- `fn extrude(self: & mut Self, length: f32, feedrate: u32) -> Result<u16, BambuError>` - Dispatches a manual relative extrusion command sequence [REF-GCODE-EXTRUDE].
- `fn wait_for_homing(self: & mut Self) -> Result<(), BambuError>` - Blocks until a `G28` homing cycle observed via telemetry has completed.
- `fn next_sequence_id(self: & mut Self) -> u64` - Increments and returns the next transaction/sequence identifier tracking commands.
- `fn set_command_timeout(self: & mut Self, secs: u64)` - Sets the timeout (in seconds) used by command-response methods like
- `fn request_pushall(self: & mut Self) -> Result<u16, BambuError>` - Requests a full state dump from the printer [REF-MQTT-LIFECYCLE].
- `fn send_ping(self: & mut Self) -> Result<(), BambuError>` - Dispatches a PINGREQ keep-alive frame to maintain connection liveness.
- `fn serial(self: &Self) -> &str` - Returns a reference to the printer's unique hardware serial number.
- `fn model(self: &Self) -> BambuModel` - Returns the resolved printer hardware model.
- `fn mqtt(self: & mut Self) -> Result<& mut BambuMqttClient<<MqttTls as >::Stream>, BambuError>` - Returns direct access to the underlying [`BambuMqttClient`], auto-connecting
- `fn pause_print(self: & mut Self) -> Result<u16, BambuError>` - Pauses the currently active print job [REF-MQTT-LIFECYCLE].
- `fn resume_print(self: & mut Self) -> Result<u16, BambuError>` - Resumes a paused print job [REF-MQTT-LIFECYCLE].
- `fn stop_print(self: & mut Self) -> Result<u16, BambuError>` - Aborts/cancels the currently running print job queue [REF-MQTT-LIFECYCLE].
- `fn clear_print_error(self: & mut Self) -> Result<u16, BambuError>` - Clears active error codes from the printer's diagnostic fault register [REF-MQTT-LIFECYCLE].
- `fn set_print_speed(self: & mut Self, level: PrintSpeed) -> Result<u16, BambuError>` - Dynamically scales maximum velocity and acceleration limits during an active print [REF-MQTT-LIFECYCLE].
- `fn skip_objects(self: & mut Self, object_ids: Vec<u32>) -> Result<u16, BambuError>` - Bypasses rendering of specific objects within an active multi-model print job [REF-MQTT-LIFECYCLE].
- `fn start_calibration(self: & mut Self, options: CalibrationOption) -> Result<u16, BambuError>` - Triggers automated physical calibration routines on the printer chassis [REF-MQTT-LIFECYCLE].
- `fn start_print(self: & mut Self, config: &PrintJobConfig) -> Result<u16, BambuError>` - Submits a `.3mf` print job from MicroSD storage for execution [REF-MQTT-LIFECYCLE].
- `fn set_bed_temperature(self: & mut Self, target_temp: u16) -> Result<u16, BambuError>` - Sets the heated bed target temperature.
- `fn set_nozzle_temperature(self: & mut Self, nozzle_id: u8, target_temp: u16) -> Result<u16, BambuError>` - Sets the target temperature of a specific hotend/nozzle [REF-MOTO-GCODE].
- `fn set_chamber_temperature(self: & mut Self, target_temp: u16) -> Result<u16, BambuError>` - Sets the target temperature of the active heated chamber loop [REF-MOTO-GCODE].
- `fn attach_storage(self: & mut Self, ftps_client: BambuFtpsClient<FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer>)` - Injects a pre-connected [`BambuFtpsClient`] directly.
- `fn storage(self: & mut Self) -> Result<& mut BambuFtpsClient<FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer>, BambuError>` - Returns direct access to the underlying [`BambuFtpsClient`], auto-connecting
- `fn disconnect_storage(self: & mut Self) -> Result<(), BambuError>` - Disconnects the FTPS session, if one exists, and clears it from the client.
- `fn from_mqtt(mqtt_client: BambuMqttClient<IO>, serial: &str, model: BambuModel) -> Self` - Wraps an already-connected [`BambuMqttClient`] in a `PrinterClient`.
- `fn change_filament(self: & mut Self, ams_id: i32, slot_id: i32, target: i32, curr_temp: i32, tar_temp: i32) -> Result<u16, BambuError>` - Triggers a filament load or unload sequence on a physical AMS unit or external spool [REF-AMS-MAP].
- `fn start_drying(self: & mut Self, ams_id: i32, dry_temp: u32, dry_time: u32, rotate_tray: bool, filament: &str) -> Result<u16, BambuError>` - Initiates a dry-chamber heating cycle on an AMS-HT or AMS 2 Pro unit [REF-AMS-DRYER].
- `fn stop_drying(self: & mut Self, ams_id: i32) -> Result<u16, BambuError>` - Terminates an active dry-chamber heating cycle on an AMS unit [REF-AMS-DRYER].
- `fn scan_rfid(self: & mut Self, ams_id: i32, slot_id: i32) -> Result<u16, BambuError>` - Scans proprietary RFID tag properties on a specific AMS tray [REF-AMS-MAP].
- `fn select_k_profile(self: & mut Self, ams_id: i32, tray_id: i32, cali_idx: i32, filament_id: &str, nozzle_diameter: &str) -> Result<u16, BambuError>` - Binds a stored K-profile calibration entry to an AMS material slot [REF-AMS-MAP].
- `fn get_version(self: & mut Self) -> Result<VersionInfo, BambuError>` - Queries the printer's expansion bus version database and returns typed module info.
- `fn get_k_profiles(self: & mut Self) -> Result<ExtrusionCaliGetResponse, BambuError>` - Requests a dump of the printer's stored K-profile calibration database [REF-DIAG-KPROF].
- `fn set_k_profile_primed(self: & mut Self, primed: bool)` - Controls whether `get_k_profiles()` sends an automatic priming request.



## Module: dummy

Zero-cost dummy implementations for [`PrinterClient`](super::PrinterClient)'s type parameters.

These let you create an MQTT-only `PrinterClient` without specifying concrete FTPS,
TLS, or timer types. They're the defaults — you'll never need to reference them directly
unless you're building a fully custom client configuration.



## Module: types

Client-facing enums and helper types (telemetry events, fan targets, print speed, calibration).



