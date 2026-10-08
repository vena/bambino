#[cfg(not(feature = "std"))]
use alloc::boxed::Box;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::ams::clean_stale_tray_data;
use crate::diagnostics::{
    DecodedHmsAlert, DecodedPrintError, decode_hms_alert, decode_print_error,
};
use crate::error::Error;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};
use crate::mqtt::MqttMessage;
use crate::types::control::FanTarget;
use crate::types::telemetry::merge::{keep_new, merge_keyed, merge_opt};
use crate::types::telemetry::{
    FULL_REPORT_MIN_KEYS, FanStrings, HeaterTemps, NozzleTemps, bits, decode_bed_temperatures,
    decode_fan_percent, decode_nozzle_temperatures, print_key_count, unpack_temperature,
};
use crate::types::{
    AmsStatusReport, DeviceTelemetry, HmsEntry, IpcamTelemetry, PrinterTelemetry, SdcardState,
    TelemetryReport, VirtualTray, XcamTelemetry,
};

use super::PrinterClient;
use super::command::{
    AckExpectation, CommandHandle, CommandOutcome, CommandResolution, PUSH_STATUS_COMMAND,
    decode_verdict, is_telemetry_command, parse_command_echo,
};
use super::print_options::ReplySettings;
use super::types::{PrintProgress, TelemetryEvent};
use crate::mqtt::commands::PromptSoundRequest;
use crate::types::control::{PrintSpeed, PrintStatus};

/// Cached "last-observed" telemetry values, updated by `PrinterClient::poll_telemetry()`.
/// Each field independently keeps its most recently observed value — a telemetry message
/// that omits a field leaves the previously-cached value in place (see the accessor methods
/// on `PrinterClient` for the public read API over this cache).
#[derive(Debug, Clone, Default)]
pub(crate) struct TelemetryCache {
    pub(crate) last_home_flag: Option<u32>,
    /// `PrinterClient::connection_generation` in effect when `last_home_flag` was last
    /// written. The homing accessors refuse a flag stamped with a stale generation; the
    /// mains-region read (`is_220v_power`) deliberately does not, because mains wiring is a
    /// fixed property of the physical printer and cannot change across a reconnect to the
    /// same unit — letting it go cold would fall back to the conservative 110 °C bed clamp
    /// for no gain. The two share a field but not a fate; do not collapse them.
    pub(crate) last_home_flag_generation: Option<u32>,
    pub(crate) last_gcode_state: Option<String>,
    pub(crate) last_door_open: Option<bool>,
    pub(crate) last_print_error: Option<u32>,
    // Echoed back by the error-dialog commands (`ignore`, error-aware `resume`/`stop`), as
    // BambuStudio's `command_hms_*` do; `job_id` is sparse in telemetry, so it is cached.
    pub(crate) last_job_id: Option<String>,
    pub(crate) last_subtask_name: Option<String>,
    pub(crate) last_nozzle_diameter: Option<String>,
    pub(crate) last_nozzle_type: Option<String>,
    pub(crate) last_sdcard: Option<SdcardState>,
    pub(crate) last_progress: PrintProgress,
    pub(crate) last_bed_temper: Option<f64>,
    pub(crate) last_bed_target_temper: Option<f64>,
    pub(crate) last_device: Option<DeviceTelemetry>,
    pub(crate) last_ams: Option<AmsStatusReport>,
    pub(crate) last_vt_tray: Option<VirtualTray>,
    pub(crate) last_vir_slot: Option<Vec<VirtualTray>>,
    pub(crate) last_nozzle_temper: Option<f64>,
    pub(crate) last_nozzle_target_temper: Option<f64>,
    pub(crate) last_chamber_temper: Option<f64>,
    pub(crate) last_hms: Option<Vec<HmsEntry>>,
    pub(crate) last_cooling_fan_speed: Option<String>,
    pub(crate) last_big_fan1_speed: Option<String>,
    pub(crate) last_big_fan2_speed: Option<String>,
    pub(crate) last_heatbreak_fan_speed: Option<String>,
    pub(crate) last_spd_lvl: Option<u8>,
    pub(crate) last_spd_mag: Option<u16>,
    pub(crate) last_wifi_signal: Option<String>,
    // Caches print.net.conf so PrinterClient::is_ethernet_active() (the confirmed-
    // authoritative source) is reachable through the cached client pipeline, not
    // only via a raw TelemetryReport parse.
    pub(crate) last_net_conf: Option<u32>,
    pub(crate) last_ipcam: Option<IpcamTelemetry>,
    // Cached because `print.xcam` is pushall-only: without this, every accessor would read
    // `None` on the incremental frames that make up the bulk of the stream.
    pub(crate) last_xcam: Option<XcamTelemetry>,
    // The `fun2` capability bitfield arrives on pushall and is absent from most incremental
    // frames, so it needs the same caching `xcam` does to be readable on a command path.
    pub(crate) last_fun2: Option<String>,
    // `fun`, cached like `fun2`: its `print_option` support bits feed `quirk_context()`.
    pub(crate) last_fun: Option<String>,
    // `home_flag` as trusted for settings and capability bits. Once a printer has sent `cfg`,
    // only from full status reports (`FULL_REPORT_MIN_KEYS`): those families (H2D among them)
    // also send heartbeat frames with a partial `home_flag`. P1 and A1 send no `cfg`, and their
    // diff frames carry a complete one (`tests/mocks/P1S_print_sequence.ndjson`), so every
    // status frame counts there. The `print_option` getters and support bits read this, never
    // `last_home_flag`. The getters ignore the generation, since a setting persists across a
    // reconnect; `quirk_context()` honors it, as for firmware.
    pub(crate) last_settings_home_flag: Option<u32>,
    pub(crate) last_settings_home_flag_generation: Option<u32>,
    // Setting values from accepted `print_option` replies, cleared by the next status frame
    // carrying `cfg` or a trusted `home_flag` — see `ReplySettings`.
    pub(crate) reply_settings: ReplySettings,
    // The top-level `print.cfg` settings bitmask. Kept across reconnects: whether a printer sends
    // `cfg` at all is a fixed property of the printer, and once it has, the `print_option`
    // getters read `cfg` alone.
    pub(crate) last_cfg: Option<String>,
    // The OTA firmware version, from a `get_version` round trip rather than from telemetry —
    // several capabilities are gated on it, and a caller should not have to re-query per check.
    pub(crate) last_firmware: Option<String>,
    // `connection_generation` when `last_firmware` was fetched. An OTA update reboots the
    // printer and forces a reconnect, so a version from an earlier connection is not trusted.
    pub(crate) last_firmware_generation: Option<u32>,
    // Whether the firmware uses BambuStudio's "np" payload format (`PrinterTelemetry::reports_np_format`).
    // `Some(true)` is sticky: a partial frame lacking the probe fields must not downgrade it.
    // `Some(false)` is set only from a `push_status` frame while nothing better is known.
    pub(crate) last_np_format: Option<bool>,
}

impl<
    MqttRawIO,
    MqttTls,
    MqttFactory,
    Timer,
    FtpsRawIO,
    FtpsTls,
    FtpsFactory,
    FtpsTimer,
    CameraRawIO,
    CameraTls,
    CameraFactory,
>
    PrinterClient<
        MqttRawIO,
        MqttTls,
        MqttFactory,
        Timer,
        FtpsRawIO,
        FtpsTls,
        FtpsFactory,
        FtpsTimer,
        CameraRawIO,
        CameraTls,
        CameraFactory,
    >
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
    CameraFactory: RawStreamFactory<CameraRawIO>,
{
    /// Pulls the next telemetry event from the MQTT channel.
    ///
    /// Returns, in order of precedence:
    ///
    /// - [`TelemetryEvent::Command`] for a command outcome that needs no message — a command
    ///   past its deadline, or one lost to a disconnect — before touching the wire.
    /// - [`TelemetryEvent::Command`] for an echo answering a command this client published,
    ///   with the printer's verdict decoded.
    /// - [`TelemetryEvent::Unknown`] for any other command echo, under any wrapper (`print`,
    ///   `system`, `info`): another client's command, or a response a request method such as
    ///   [`get_version()`](Self::get_version) did not claim. An echo shares envelopes and field
    ///   names with telemetry (`extrusion_cali_get`'s reply carries `nozzle_diameter`), so it is
    ///   never read as a report.
    /// - [`TelemetryEvent::Report`] if the payload deserializes as telemetry, else `Unknown`.
    ///
    /// Drains any internally buffered messages (from command-response round-trips) before
    /// reading from the wire. A timeout is noticed on the next call, so it is delivered late by
    /// however long this call blocks on the wire — at most `MQTT_READ_TIMEOUT_SECS` (30s) on a
    /// completely silent link, and in practice far sooner, since the printer pushes telemetry
    /// continuously and this call sends a keepalive every 20s.
    ///
    /// Cancellation-safe in a `select!`: outcome bookkeeping happens only after the wire read
    /// has returned, never across an await.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// loop {
    ///     match printer.poll_telemetry().await? {
    ///         TelemetryEvent::Report(report, _raw) => {
    ///             // `report.print` is an Option — absent on a report that carries only
    ///             // top-level `device` data.
    ///             if let Some(print) = &report.print {
    ///                 println!("Printer state: {:?}", print.gcode_state);
    ///             }
    ///         }
    ///         TelemetryEvent::Command(resolution, _raw) => {
    ///             println!("{}: {:?}", resolution.handle.command(), resolution.outcome);
    ///         }
    ///         TelemetryEvent::Unknown(_) => {}
    ///     }
    /// }
    /// ```
    pub async fn poll_telemetry(&mut self) -> Result<TelemetryEvent, Error> {
        // Messages a command-response wait already read come first: one may be the echo of a
        // command whose deadline has since passed, and expiring that command before its buffered
        // echo is seen would report a command the printer answered in time as TimedOut (#350).
        if let Some(msg) = self.mqtt.as_mut().and_then(|mqtt| mqtt.take_pending()) {
            return Ok(self.classify_message(msg));
        }
        if let Some(event) = self.next_unanswered_outcome() {
            return Ok(event);
        }
        self.ensure_mqtt().await?;
        // A lazy reconnect inside `ensure_mqtt()` just resolved every pending command.
        if let Some(event) = self.next_unanswered_outcome() {
            return Ok(event);
        }
        let msg = self
            .mqtt
            .as_mut()
            .unwrap()
            .poll_telemetry(&self.timer)
            .await?;
        Ok(self.classify_message(msg))
    }

    /// Waits for the outcome of one command this client published, reading the wire until its echo arrives or its time runs out.
    ///
    /// The inline counterpart to receiving [`TelemetryEvent::Command`] from
    /// [`poll_telemetry()`](Self::poll_telemetry), for scripts that send one command and act on
    /// the answer. Messages read while waiting are buffered and still delivered by later
    /// `poll_telemetry()` calls, and the outcome returned here is not delivered again as an
    /// event.
    ///
    /// - A command that never echoes returns [`CommandOutcome::SettledOnPublish`] at once.
    /// - A command whose outcome is already known returns it at once — including one the
    ///   caller's event loop has already received, for the most recent 32 outcomes.
    /// - Otherwise waits up to [`set_command_timeout()`](Self::set_command_timeout) from this
    ///   call and returns [`CommandOutcome::TimedOut`] if no echo arrives. Without a real clock
    ///   the wait is bounded only by the 200-message safety valve, which also ends a wait early
    ///   on a busy link.
    ///
    /// **Blocks the caller's event loop for up to the timeout.** A UI should consume
    /// `TelemetryEvent::Command` from its existing `poll_telemetry()` loop instead.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidArgument`] for a handle whose outcome this client no longer holds — it
    /// was delivered more than 32 outcomes ago, or already returned by an earlier `await_ack`.
    /// Transport errors from reading the wire are returned as-is; the command then resolves as
    /// [`CommandOutcome::ConnectionLost`] once the session is re-established.
    pub async fn await_ack(&mut self, handle: &CommandHandle) -> Result<CommandOutcome, Error> {
        if handle.ack() == AckExpectation::SettlesOnPublish {
            return Ok(CommandOutcome::SettledOnPublish);
        }
        // No `ensure_mqtt()` before these: a known outcome (e.g. `ConnectionLost` after
        // `disconnect_mqtt()`) needs no connection, and `poll_until` connects on its own (#351).
        if let Some(outcome) = self.core.commands.take_known(handle) {
            return Ok(outcome);
        }
        if !self.core.commands.is_pending(handle) {
            return Err(Error::InvalidArgument(
                "no outcome held for this command handle".into(),
            ));
        }
        let result = self
            .poll_until_command_timeout(|msg| {
                let echo = parse_command_echo(&msg.payload)?;
                handle.is_answered_by(&echo).then(|| decode_verdict(&echo))
            })
            .await;
        match result {
            Ok(outcome) => {
                self.core.commands.forget(handle);
                Ok(outcome)
            }
            Err(Error::Timeout) => {
                self.core.commands.forget(handle);
                Ok(CommandOutcome::TimedOut)
            }
            Err(e) => Err(e),
        }
    }

    /// Pops a command outcome that needs no message, recording it as delivered.
    fn next_unanswered_outcome(&mut self) -> Option<TelemetryEvent> {
        let now_ms = self.timer.has_real_clock().then(|| self.timer.now_millis());
        let resolution = self.core.commands.next_unanswered(now_ms)?;
        self.core.commands.remember(&resolution);
        Some(TelemetryEvent::Command(resolution, None))
    }

    /// Turns one wire message into the event it represents — see [`poll_telemetry()`](Self::poll_telemetry).
    fn classify_message(&mut self, msg: MqttMessage) -> TelemetryEvent {
        let report = serde_json::from_slice::<TelemetryReport>(&msg.payload).ok();
        // The bulk of traffic is telemetry pushes, which say so in `print.command`; only other
        // frames pay for the second, echo-shaped read.
        let is_push = report.as_ref().is_some_and(|report| {
            report
                .print
                .as_ref()
                .and_then(|print| print.command.as_deref())
                .is_some_and(is_telemetry_command)
        });
        if !is_push && let Some(echo) = parse_command_echo(&msg.payload) {
            if echo.command() == PromptSoundRequest::COMMAND
                && matches!(decode_verdict(&echo), CommandOutcome::Accepted)
            {
                self.apply_print_option_reply(&msg.payload);
            }
            return match self.core.commands.take_answered(&echo) {
                Some(handle) => {
                    let resolution = CommandResolution {
                        handle,
                        outcome: decode_verdict(&echo),
                    };
                    self.core.commands.remember(&resolution);
                    TelemetryEvent::Command(resolution, Some(msg))
                }
                None => TelemetryEvent::Unknown(msg),
            };
        }
        match report {
            Some(report) => {
                // See `last_settings_home_flag`: a printer that sends `cfg` needs a full
                // report, one that doesn't (P1, A1) doesn't.
                let settings_home_flag = report
                    .print
                    .as_ref()
                    .and_then(|print| print.home_flag)
                    .filter(|_| {
                        self.core.cache.last_cfg.is_none()
                            || print_key_count(&msg.payload)
                                .is_some_and(|keys| keys > FULL_REPORT_MIN_KEYS)
                    });
                let carries_settings = settings_home_flag.is_some()
                    || report
                        .print
                        .as_ref()
                        .is_some_and(|print| print.cfg.is_some());
                if carries_settings {
                    self.core.cache.reply_settings = ReplySettings::default();
                }
                if let Some(flag) = settings_home_flag {
                    self.core.cache.last_settings_home_flag = Some(flag);
                    self.core.cache.last_settings_home_flag_generation =
                        Some(self.core.connection_generation);
                }
                self.update_telemetry_cache(&report);
                TelemetryEvent::Report(Box::new(report), msg)
            }
            None => TelemetryEvent::Unknown(msg),
        }
    }

    /// Updates every `last_*` telemetry cache from a freshly-parsed report.
    /// A field only overwrites its cache when present in `report` — a message that omits a field leaves
    /// the previously-cached value in place (staleness is intentional; see the `last_*` field docs on
    /// the struct).
    fn update_telemetry_cache(&mut self, report: &TelemetryReport) {
        // Merge field-by-field rather than replacing wholesale — see `DeviceTelemetry`'s
        // `Mergeable` impl.
        merge_opt(&mut self.core.cache.last_device, &report.device().cloned());
        // Read before the `print` early-return: both accessors check the top level too.
        if let Some(fun2) = report.fun2() {
            self.core.cache.last_fun2 = Some(fun2.to_string());
        }
        if let Some(fun) = report.fun() {
            self.core.cache.last_fun = Some(fun.to_string());
        }
        let Some(print) = report.print.as_ref() else {
            return;
        };
        self.update_state_cache(print);
        self.update_progress_cache(print);
        self.update_temperature_cache(print);
        self.update_ams_cache(print);
        self.update_fan_cache(print);
        self.update_speed_and_signal_cache(print);
        self.update_ipcam_cache(print);
        self.update_xcam_cache(print);
    }

    fn update_state_cache(&mut self, print: &PrinterTelemetry) {
        if let Some(flag) = print.home_flag {
            self.core.cache.last_home_flag = Some(flag);
            self.core.cache.last_home_flag_generation = Some(self.core.connection_generation);
        }
        keep_new(&mut self.core.cache.last_gcode_state, &print.gcode_state);
        keep_new(&mut self.core.cache.last_cfg, &print.cfg);
        // A frame without the door field leaves the last observed state in place.
        if let Some(open) = print.door_state(self.quirks().door_sensor()) {
            self.core.cache.last_door_open = Some(open);
        }
        keep_new(&mut self.core.cache.last_print_error, &print.print_error);
        keep_new(&mut self.core.cache.last_job_id, &print.job_id);
        keep_new(&mut self.core.cache.last_subtask_name, &print.subtask_name);
        keep_new(
            &mut self.core.cache.last_nozzle_diameter,
            &print.nozzle_diameter,
        );
        keep_new(&mut self.core.cache.last_nozzle_type, &print.nozzle_type);
        keep_new(&mut self.core.cache.last_sdcard, &print.sdcard_status());
        keep_new(&mut self.core.cache.last_hms, &print.hms);
        if print.reports_np_format() {
            self.core.cache.last_np_format = Some(true);
        } else if self.core.cache.last_np_format.is_none()
            && print.command.as_deref() == Some(PUSH_STATUS_COMMAND)
        {
            self.core.cache.last_np_format = Some(false);
        }
    }

    fn update_progress_cache(&mut self, print: &PrinterTelemetry) {
        keep_new(
            &mut self.core.cache.last_progress.percent,
            &print.mc_percent,
        );
        // `mc_remaining_time` is in minutes on the wire, not seconds — both BambuStudio
        // (`DeviceManager.cpp:3081-3086`) and bambuddy (`notification_service.py:1163-1169`)
        // multiply by 60 to reach a seconds value. Convert here so `remaining_secs` is honest
        // about its own name; storing the raw value understated every ETA by 60x.
        if let Some(remaining) = print.mc_remaining_time {
            self.core.cache.last_progress.remaining_secs = Some(remaining.saturating_mul(60));
        }
        keep_new(
            &mut self.core.cache.last_progress.layer_num,
            &print.layer_num,
        );
        // Some firmware (confirmed on P1S) resets total_layer_num to 0 in the end-of-print
        // frame — only positive values are real, so 0 must not clobber the last known total.
        if let Some(total_layers) = print.total_layers
            && total_layers > 0
        {
            self.core.cache.last_progress.total_layers = Some(total_layers);
        }
    }

    fn update_temperature_cache(&mut self, print: &PrinterTelemetry) {
        let cache = &mut self.core.cache;
        keep_new(&mut cache.last_bed_temper, &print.bed_temper);
        keep_new(&mut cache.last_bed_target_temper, &print.bed_target_temper);
        keep_new(&mut cache.last_nozzle_temper, &print.nozzle_temper);
        keep_new(
            &mut cache.last_nozzle_target_temper,
            &print.nozzle_target_temper,
        );
        keep_new(&mut cache.last_chamber_temper, &print.chamber_temper);
    }

    fn update_ams_cache(&mut self, print: &PrinterTelemetry) {
        let cache = &mut self.core.cache;
        // Merged field-by-field rather than replaced wholesale: a partial `print.ams` push
        // (confirmed via wire capture) can carry only a few fields with the unit/tray array
        // omitted entirely, and a partial id-only `vt_tray` push must not wipe cached
        // tray_type/tray_color/etc.
        merge_opt(&mut cache.last_ams, &print.ams);
        merge_opt(&mut cache.last_vt_tray, &print.vt_tray);
        // Per element by id, so a partial array carrying only one IDEX extruder's entry doesn't
        // drop the other cached entry.
        if let Some(incoming) = &print.vir_slot {
            let cached = cache.last_vir_slot.get_or_insert_with(Vec::new);
            merge_keyed(cached, incoming, |slot| slot.id.clone());
        }
    }

    fn update_fan_cache(&mut self, print: &PrinterTelemetry) {
        let cache = &mut self.core.cache;
        keep_new(&mut cache.last_cooling_fan_speed, &print.cooling_fan_speed);
        keep_new(&mut cache.last_big_fan1_speed, &print.big_fan1_speed);
        keep_new(&mut cache.last_big_fan2_speed, &print.big_fan2_speed);
        keep_new(
            &mut cache.last_heatbreak_fan_speed,
            &print.heatbreak_fan_speed,
        );
    }

    fn update_speed_and_signal_cache(&mut self, print: &PrinterTelemetry) {
        let cache = &mut self.core.cache;
        keep_new(&mut cache.last_spd_lvl, &print.spd_lvl);
        keep_new(&mut cache.last_spd_mag, &print.spd_mag);
        keep_new(&mut cache.last_wifi_signal, &print.wifi_signal);
        keep_new(
            &mut cache.last_net_conf,
            &print.net.as_ref().and_then(|net| net.conf),
        );
    }

    fn update_ipcam_cache(&mut self, print: &PrinterTelemetry) {
        merge_opt(&mut self.core.cache.last_ipcam, &print.ipcam);
    }

    fn update_xcam_cache(&mut self, print: &PrinterTelemetry) {
        merge_opt(&mut self.core.cache.last_xcam, &print.xcam);
    }

    /// Returns the printer's high-level activity classification as of the last-observed `gcode_state` telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    /// `None` means no telemetry carrying `gcode_state` has been observed yet.
    pub fn print_status(&self) -> Option<PrintStatus> {
        self.core
            .cache
            .last_gcode_state
            .as_deref()
            .map(PrintStatus::from_gcode_state)
    }

    /// Returns the active job's name (`subtask_name`) as of the last-observed telemetry; `None` before any carried it.
    pub fn subtask_name(&self) -> Option<&str> {
        self.core.cache.last_subtask_name.as_deref()
    }

    /// Returns the legacy single-nozzle `(nozzle_diameter, nozzle_type)` strings as of the last-observed telemetry.
    ///
    /// Pre-IDEX models report the fitted nozzle this way; newer ones report per-nozzle entries
    /// under [`device()`](Self::device) instead.
    pub fn legacy_nozzle(&self) -> (Option<&str>, Option<&str>) {
        (
            self.core.cache.last_nozzle_diameter.as_deref(),
            self.core.cache.last_nozzle_type.as_deref(),
        )
    }

    /// Returns the SD-card state as of the last-observed telemetry that carried one — see [`PrinterTelemetry::sdcard_status`].
    pub fn sdcard_status(&self) -> Option<SdcardState> {
        self.core.cache.last_sdcard
    }

    /// Returns the merged `device` telemetry (nozzles, extruders, airduct, chamber controller) as of the last-observed telemetry.
    ///
    /// `None` before any telemetry carried a `device` object, from either wire location.
    pub fn device(&self) -> Option<&DeviceTelemetry> {
        self.core.cache.last_device.as_ref()
    }

    /// Returns whether the door was open as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    ///
    /// Returns `None` on models without a door sensor (`ModelQuirks::door_sensor()` is
    /// `DoorSensor::None`, e.g. A1/A2) — distinct from `Some(false)`, which means a
    /// sensor-equipped model's telemetry confirms the door is closed. Also `None` before any
    /// telemetry carrying the model's door field has been observed.
    pub fn is_door_open(&self) -> Option<bool> {
        self.core.cache.last_door_open
    }

    /// Returns the printer's mains region as of the last-observed `home_flag` telemetry.
    /// `None` means no telemetry carrying `home_flag` has been observed yet — distinct from
    /// `Some(false)`. Feed this straight into
    /// [`ModelQuirks::bed_temp_max`](crate::quirks::ModelQuirks::bed_temp_max) (reachable via
    /// [`PrinterClient::quirks()`](super::PrinterClient::quirks)) to read a printer's bed
    /// ceiling before issuing a command; `set_bed_temperature` uses this same accessor.
    pub fn is_220v_power(&self) -> Option<bool> {
        self.core.cache.last_home_flag.map(bits::is_220v)
    }

    /// Returns the decoded active print-error fault as of the last-observed `print_error` telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    ///
    /// `None` covers both "no telemetry carrying `print_error` observed yet" and "the
    /// register reads 0 (no fault)" — both warrant the same caller action, so they are not
    /// distinguished here.
    pub fn active_fault(&self) -> Option<DecodedPrintError> {
        decode_print_error(self.core.cache.last_print_error?)
    }

    /// Returns the print progress snapshot as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    /// Each field independently tracks its own "last observed" value — see [`PrintProgress`]'s doc
    /// comment.
    pub fn print_progress(&self) -> PrintProgress {
        self.core.cache.last_progress
    }

    /// Returns the bed's temperatures as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)); `None` before any telemetry carrying them.
    ///
    /// Shares its cross-model decode logic with
    /// [`TelemetryReport::bed_temperatures()`](crate::types::TelemetryReport::bed_temperatures) —
    /// use that method instead if you already have a fresh `TelemetryReport` in hand.
    pub fn bed_temperatures(&self) -> Option<HeaterTemps> {
        decode_bed_temperatures(
            self.core.cache.last_device.as_ref(),
            self.core.cache.last_bed_temper,
            self.core.cache.last_bed_target_temper,
        )
    }

    /// Returns the cached AMS/tray status report as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    /// `None` means no telemetry carrying `print.ams` has been observed yet.
    ///
    /// This is the **raw** merged cache — every field independently keeps its most recently
    /// observed value ([`AmsStatusReport::merge_from`](crate::types::telemetry::ams::AmsStatusReport)-level
    /// detail), but stale per-tray material fields (`tray_type`, `tray_color`, `remain`, etc.)
    /// are **not** proactively cleared when a slot empties — confirmed against BambuStudio's
    /// own `DevFilaSystem.cpp`, whose structural equivalent (`DevAmsTray::reset()`) is dead
    /// code with zero call sites in its own current codebase; the shipped BambuStudio/
    /// OrcaSlicer UI instead gates every read of a tray's material fields on
    /// `is_exists`/`is_tray_info_ready()`-equivalent checks (`AmsTray::is_loaded()` here) and
    /// never scrubs the raw cache. This crate mirrors that design rather than
    /// [`clean_stale_tray_data`]'s proactive-clearing
    /// approach: wiring proactive clearing into this cache would make it *less* faithful to
    /// on-wire state than BambuStudio's own model. Two opt-in ways to get sanitized output
    /// without losing that raw fidelity:
    /// - Check [`AmsTray::is_loaded()`](crate::types::AmsTray::is_loaded) (or
    ///   [`evaluate_spool_presence`](crate::ams::evaluate_spool_presence)) before trusting a
    ///   tray's material fields — the same check-before-trust contract BambuStudio itself
    ///   relies on.
    /// - Call [`sanitized_ams()`](Self::sanitized_ams) for a cloned, scrubbed copy — mirrors
    ///   [`hms()`](Self::hms)/[`active_hms_alerts()`](Self::active_hms_alerts)'s raw-cache +
    ///   opt-in-decoded accessor split.
    pub fn ams(&self) -> Option<&AmsStatusReport> {
        self.core.cache.last_ams.as_ref()
    }

    /// Returns the global tray ID of the spool currently feeding the active extruder, as of
    /// the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    ///
    /// Prefers `device.extruder.info[active].snow`, BambuStudio's own preferred resolution
    /// method (`DevExterSystem::ParseV2_0`, `DevExtderSystem.cpp:318-386`) — no
    /// `ams_extruder_map` inversion needed, since `snow` self-identifies both the AMS unit and
    /// slot directly. `None` when `device.extruder` telemetry hasn't been observed yet (common
    /// on single-nozzle models, which may not populate this sub-object at all) or the active
    /// extruder's `snow` is the unmapped sentinel.
    pub fn printing_tray_global_id(&self) -> Option<u8> {
        let extruder = self.core.cache.last_device.as_ref()?.extruder.as_ref()?;
        let active_idx = extruder.active_extruder_index();
        let info = extruder
            .info
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .find(|e| e.id == active_idx)?;
        let (ams_id, slot_id) = info.current_ams_slot()?;
        crate::ams::resolve_global_tray_id(ams_id, slot_id)
    }

    /// Returns a cloned copy of the cached AMS status report with every tray's stale material
    /// fields cleared via [`clean_stale_tray_data`]
    /// (mirrors [`active_hms_alerts()`](Self::active_hms_alerts)'s raw-cache-decode-on-access
    /// shape). `None` under the same condition as [`ams()`](Self::ams) — no telemetry carrying
    /// `print.ams` observed yet. Does not mutate the underlying cache — [`ams()`](Self::ams)
    /// keeps returning the raw values; see its doc comment for why the raw cache is never
    /// proactively scrubbed.
    pub fn sanitized_ams(&self) -> Option<AmsStatusReport> {
        let mut sanitized = self.core.cache.last_ams.clone()?;
        for unit in &mut sanitized.ams {
            // A non-numeric id can't be placed on the bus, so there is no rule to clean it by;
            // treating it as unit 0 used to apply standard-AMS rules to whatever it was.
            let Some(ams_id) = unit.ams_id() else {
                continue;
            };
            if let Some(trays) = &mut unit.tray {
                for tray in trays {
                    clean_stale_tray_data(tray, ams_id);
                }
            }
        }
        Some(sanitized)
    }

    /// Returns the cached virtual/external spool holder state (single-nozzle models) as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    /// `None` means no telemetry carrying `print.vt_tray` has been observed yet — including on IDEX
    /// models, which send [`vir_slot()`](Self::vir_slot) instead.
    pub fn vt_tray(&self) -> Option<&VirtualTray> {
        self.core.cache.last_vt_tray.as_ref()
    }

    /// Returns the cached IDEX external spool holder array as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    /// `None` means no telemetry carrying `print.vir_slot` has been observed yet — including on
    /// single-nozzle models, which send [`vt_tray()`](Self::vt_tray) instead.
    pub fn vir_slot(&self) -> Option<&[VirtualTray]> {
        self.core.cache.last_vir_slot.as_deref()
    }

    /// Returns the nozzle temperatures as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)), one entry per nozzle; empty before any telemetry carrying them.
    ///
    /// Single-nozzle models return one entry (`id` 0); IDEX models return one entry per physical
    /// nozzle. Same decode as
    /// [`TelemetryReport::nozzle_temperatures()`](crate::types::TelemetryReport::nozzle_temperatures),
    /// including the undocumented IDEX flat-field routing quirk.
    pub fn nozzle_temperatures(&self) -> Vec<NozzleTemps> {
        decode_nozzle_temperatures(
            self.core.cache.last_device.as_ref(),
            self.core.cache.last_nozzle_temper,
            self.core.cache.last_nozzle_target_temper,
        )
    }

    /// Returns the chamber's temperatures as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    ///
    /// `None` on models without a chamber temperature sensor
    /// (`ModelQuirks::has_chamber_temperature_sensor()` is `false`, e.g. A1/A1 Mini/A2L/P1P/
    /// P1S), and before any telemetry carrying `chamber_temper` has been observed.
    pub fn chamber_temperature(&self) -> Option<HeaterTemps> {
        if !self.quirks().has_chamber_temperature_sensor() {
            return None;
        }
        self.core.cache.last_chamber_temper.map(unpack_temperature)
    }

    /// Returns the cached active hardware-alert (HMS) entries as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    /// `None` means no telemetry carrying `print.hms` has been observed yet.
    pub fn hms(&self) -> Option<&[HmsEntry]> {
        self.core.cache.last_hms.as_deref()
    }

    /// Returns the cached camera/recording state as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    /// `None` means no telemetry carrying `print.ipcam` has been observed yet.
    pub fn ipcam(&self) -> Option<&IpcamTelemetry> {
        self.core.cache.last_ipcam.as_ref()
    }

    /// Returns the cached AI-detection and print-option settings as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    ///
    /// `None` means no telemetry carrying `print.xcam` has been observed yet. Because `xcam`
    /// appears to be pushall-only, that can persist for a long stretch of incremental frames — it
    /// is not evidence the model lacks these settings. Use
    /// [`XcamTelemetry::supports_ai_monitoring`] for that question instead.
    pub fn xcam(&self) -> Option<&XcamTelemetry> {
        self.core.cache.last_xcam.as_ref()
    }

    /// Returns every cached HMS entry decoded and filtered to genuine faults (mirrors `active_fault()`'s raw-cache-decode-on-access shape).
    /// Empty when nothing is cached or nothing currently decodes as a genuine fault — there's no caller
    /// action that would differ between those two cases.
    pub fn active_hms_alerts(&self) -> Vec<DecodedHmsAlert> {
        self.core
            .cache
            .last_hms
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|entry| decode_hms_alert(entry.attr, entry.code))
            .filter(|decoded| decoded.is_genuine_fault)
            .collect()
    }

    /// Returns `fan`'s speed as a percentage (0-100), decoded from the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)); `None` before any telemetry carrying it.
    ///
    /// [`FanTarget::AuxiliaryLeft2`] (X2D/P2S, port 10) reports at a different wire location
    /// than the other three — `device.airduct.parts[id=160].state`, already a percentage
    /// [REF-CLIM-FANS] — which this handles.
    pub fn fan_speed(&self, fan: FanTarget) -> Option<u8> {
        let cache = &self.core.cache;
        let strings = FanStrings {
            part_cooling: cache.last_cooling_fan_speed.as_deref(),
            aux_left: cache.last_big_fan1_speed.as_deref(),
            chamber_exhaust: cache.last_big_fan2_speed.as_deref(),
        };
        decode_fan_percent(fan, strings, cache.last_device.as_ref())
    }

    /// Returns the toolhead heatbreak fan speed as a percentage (0-100).
    ///
    /// Not independently controllable (no corresponding `FanTarget` variant/M106 port) — read-only
    /// telemetry.
    pub fn heatbreak_fan_speed(&self) -> Option<u8> {
        crate::quirks::decode_fan_percentage(self.core.cache.last_heatbreak_fan_speed.as_deref())
    }

    /// Returns the printer's current print-speed level as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    /// `None` before any telemetry carrying `spd_lvl` has been observed, or if the observed value is
    /// out of the known 1-4 range.
    pub fn print_speed(&self) -> Option<PrintSpeed> {
        PrintSpeed::from_level(self.core.cache.last_spd_lvl?)
    }

    /// Returns the printer's current print-speed magnitude (percentage of nominal feedrate) as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    pub fn print_speed_magnitude(&self) -> Option<u16> {
        self.core.cache.last_spd_mag
    }

    /// Returns the raw wireless signal strength string (e.g. `"-52dBm"`) as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    pub fn wifi_signal(&self) -> Option<&str> {
        self.core.cache.last_wifi_signal.as_deref()
    }

    /// Returns whether the printer is on wired Ethernet, per the cached `wifi_signal` sentinel (mirrors `PrinterTelemetry::is_ethernet_active_via_wifi_signal()` but works between polls off the cached value, the same way [`is_all_axes_homed()`](Self::is_all_axes_homed) works off cached `home_flag`).
    pub fn is_ethernet_active_via_wifi_signal(&self) -> bool {
        self.core
            .cache
            .last_wifi_signal
            .as_deref()
            .is_some_and(bits::is_wired_wifi_signal)
    }

    /// Returns whether the printer is on wired Ethernet, per the cached `print.net.conf` bit 0
    /// (mirrors `PrinterTelemetry::is_ethernet_active()`, the documented-preferred,
    /// confirmed-authoritative source) but works between polls off the cached
    /// value. `false` before any telemetry carrying `print.net.conf` has been observed; prefer
    /// `is_ethernet_active_via_wifi_signal()` as a fallback for firmware that doesn't send it.
    pub fn is_ethernet_active(&self) -> bool {
        self.core.cache.last_net_conf.is_some_and(bits::is_wired)
    }

    /// Polls telemetry until `done` holds for this client's cache, or `timeout` passes; returns whether `done` was reached.
    ///
    /// `done` is checked before each poll, so an already-satisfied condition returns at once
    /// without touching the wire. Events read along the way update the cache as
    /// [`poll_telemetry()`](Self::poll_telemetry) always does, and are otherwise dropped — use
    /// `poll_telemetry()` directly to see them.
    ///
    /// `timeout` is measured on this client's timer and checked between messages, so on a link
    /// that goes silent the wait can overrun it by up to one read deadline (30s). Without a real
    /// clock ([`with_timer()`](Self::with_timer)) the elapsed time can't be measured and the
    /// wait ends after the same 200-message backstop `get_version()` uses.
    pub async fn poll_telemetry_until(
        &mut self,
        timeout: core::time::Duration,
        mut done: impl FnMut(&Self) -> bool,
    ) -> Result<bool, Error> {
        let mut wait = super::WaitBudget::start(&self.timer, Some(timeout));
        loop {
            if done(self) {
                return Ok(true);
            }
            self.poll_telemetry().await?;
            if let Err(Error::Timeout) = wait.after_message(&self.timer) {
                return Ok(done(self));
            }
        }
    }

    /// Requests a full state dump and waits until the cache holds a `gcode_state`, or `timeout` passes; returns whether it does.
    ///
    /// A `pushall` reply carries `gcode_state`, so this waits for the reply on a cold cache; on a
    /// cache that already holds one it returns at once without waiting for the new dump. For
    /// a field that only some frames carry (an AMS unit's type, say), follow this with
    /// [`poll_telemetry_until()`](Self::poll_telemetry_until) on that field.
    pub async fn refresh_state(&mut self, timeout: core::time::Duration) -> Result<bool, Error> {
        self.request_pushall().await?;
        self.poll_telemetry_until(timeout, |client| client.print_status().is_some())
            .await
    }

    /// Pulls the next raw MQTT message without deserialization.
    ///
    /// Bypasses command-outcome tracking: an echo read here is not matched to its command, so
    /// that command later resolves as [`CommandOutcome::TimedOut`] instead. Don't mix this with
    /// [`poll_telemetry()`](Self::poll_telemetry) while commands are outstanding.
    pub async fn poll_raw(&mut self) -> Result<MqttMessage, Error> {
        self.ensure_mqtt().await?;
        self.mqtt
            .as_mut()
            .unwrap()
            .poll_telemetry(&self.timer)
            .await
    }
}
