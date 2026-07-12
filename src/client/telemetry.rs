#[cfg(not(feature = "std"))]
use alloc::boxed::Box;
#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::ams::clean_stale_tray_data;
use crate::diagnostics::{
    DecodedHmsAlert, DecodedPrintError, decode_hms_alert, decode_print_error,
};
use crate::error::BambuError;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};
use crate::mqtt::MqttMessage;
use crate::quirks::decode_fan_percentage;
use crate::types::telemetry::{decode_bed_temperatures, decode_nozzle_temperatures};
use crate::types::{
    AmsStatusReport, DeviceTelemetry, HmsEntry, PrinterTelemetry, TelemetryReport, VirtualTray,
};

use super::PrinterClient;
use super::types::{PrintProgress, PrintSpeed, PrintStatus, TelemetryEvent};

/// Cached "last-observed" telemetry values, updated by `PrinterClient::poll_telemetry()`.
/// Each field independently keeps its most recently observed value — a telemetry message
/// that omits a field leaves the previously-cached value in place (see the accessor methods
/// on `PrinterClient` for the public read API over this cache).
#[derive(Debug, Clone, Default)]
pub(crate) struct TelemetryCache {
    pub(crate) last_home_flag: Option<u32>,
    pub(crate) last_gcode_state: Option<String>,
    pub(crate) last_door_open: Option<bool>,
    pub(crate) last_print_error: Option<u32>,
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
    /// Returns a [`TelemetryEvent::Report`] if the payload deserializes as a known
    /// telemetry structure, or [`TelemetryEvent::Unknown`] otherwise. Drains any
    /// internally buffered messages (from command-response round-trips) before
    /// reading from the wire.
    ///
    /// # Example
    ///
    /// ```ignore
    /// loop {
    ///     match printer.poll_telemetry().await? {
    ///         TelemetryEvent::Report(report, _raw) => {
    ///             if let Some(state) = &report.print.gcode_state {
    ///                 println!("Printer state: {}", state);
    ///             }
    ///         }
    ///         TelemetryEvent::Unknown(_) => {}
    ///     }
    /// }
    /// ```
    pub async fn poll_telemetry(&mut self) -> Result<TelemetryEvent, BambuError> {
        self.ensure_mqtt().await?;
        let msg = self
            .mqtt
            .as_mut()
            .unwrap()
            .poll_telemetry_with_timer(&self.timer)
            .await?;
        match serde_json::from_slice::<TelemetryReport>(&msg.payload) {
            Ok(report) => {
                self.update_telemetry_cache(&report);
                Ok(TelemetryEvent::Report(Box::new(report), msg))
            }
            Err(_) => Ok(TelemetryEvent::Unknown(msg)),
        }
    }

    /// Updates every `last_*` telemetry cache from a freshly-parsed report.
    /// A field only overwrites its cache when present in `report` — a message that omits a field leaves
    /// the previously-cached value in place (staleness is intentional; see the `last_*` field docs on
    /// the struct).
    fn update_telemetry_cache(&mut self, report: &TelemetryReport) {
        if let Some(device) = report.device() {
            // BUG-093: merge field-by-field rather than replacing wholesale — see
            // `DeviceTelemetry::merge_from`.
            match &mut self.cache.last_device {
                Some(cached) => cached.merge_from(device),
                None => self.cache.last_device = Some(device.clone()),
            }
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
    }

    fn update_state_cache(&mut self, print: &PrinterTelemetry) {
        if let Some(flag) = print.home_flag {
            self.cache.last_home_flag = Some(flag);
        }
        if let Some(state) = &print.gcode_state {
            self.cache.last_gcode_state = Some(state.clone());
        }
        if self.model.quirks().door_sensor_field_present(print) {
            self.cache.last_door_open = Some(self.model.quirks().is_door_open(print));
        }
        if let Some(print_error) = print.print_error {
            self.cache.last_print_error = Some(print_error);
        }
        if let Some(hms) = &print.hms {
            self.cache.last_hms = Some(hms.clone());
        }
    }

    fn update_progress_cache(&mut self, print: &PrinterTelemetry) {
        if let Some(percent) = print.mc_percent {
            self.cache.last_progress.percent = Some(percent);
        }
        if let Some(remaining) = print.mc_remaining_time {
            self.cache.last_progress.remaining_secs = Some(remaining);
        }
        if let Some(layer_num) = print.layer_num {
            self.cache.last_progress.layer_num = Some(layer_num);
        }
        if let Some(total_layers) = print.total_layers {
            self.cache.last_progress.total_layers = Some(total_layers);
        }
    }

    fn update_temperature_cache(&mut self, print: &PrinterTelemetry) {
        if let Some(bed_temper) = print.bed_temper {
            self.cache.last_bed_temper = Some(bed_temper);
        }
        if let Some(bed_target_temper) = print.bed_target_temper {
            self.cache.last_bed_target_temper = Some(bed_target_temper);
        }
        if let Some(nozzle_temper) = print.nozzle_temper {
            self.cache.last_nozzle_temper = Some(nozzle_temper);
        }
        if let Some(nozzle_target_temper) = print.nozzle_target_temper {
            self.cache.last_nozzle_target_temper = Some(nozzle_target_temper);
        }
        if let Some(chamber_temper) = print.chamber_temper {
            self.cache.last_chamber_temper = Some(chamber_temper);
        }
    }

    fn update_ams_cache(&mut self, print: &PrinterTelemetry) {
        if let Some(ams) = &print.ams {
            // BUG-091: merge field-by-field rather than replacing wholesale — a partial
            // `print.ams` push (confirmed via wire capture) can carry only a few fields with
            // the unit/tray array omitted entirely; see `AmsStatusReport::merge_from`.
            match &mut self.cache.last_ams {
                Some(cached) => cached.merge_from(ams),
                None => self.cache.last_ams = Some(ams.clone()),
            }
        }
        if let Some(vt_tray) = &print.vt_tray {
            self.cache.last_vt_tray = Some(vt_tray.clone());
        }
        if let Some(vir_slot) = &print.vir_slot {
            self.cache.last_vir_slot = Some(vir_slot.clone());
        }
    }

    fn update_fan_cache(&mut self, print: &PrinterTelemetry) {
        if let Some(v) = &print.cooling_fan_speed {
            self.cache.last_cooling_fan_speed = Some(v.clone());
        }
        if let Some(v) = &print.big_fan1_speed {
            self.cache.last_big_fan1_speed = Some(v.clone());
        }
        if let Some(v) = &print.big_fan2_speed {
            self.cache.last_big_fan2_speed = Some(v.clone());
        }
        if let Some(v) = &print.heatbreak_fan_speed {
            self.cache.last_heatbreak_fan_speed = Some(v.clone());
        }
    }

    fn update_speed_and_signal_cache(&mut self, print: &PrinterTelemetry) {
        if let Some(spd_lvl) = print.spd_lvl {
            self.cache.last_spd_lvl = Some(spd_lvl);
        }
        if let Some(spd_mag) = print.spd_mag {
            self.cache.last_spd_mag = Some(spd_mag);
        }
        if let Some(wifi_signal) = &print.wifi_signal {
            self.cache.last_wifi_signal = Some(wifi_signal.clone());
        }
    }

    /// Returns the printer's high-level activity classification as of the last-observed `gcode_state` telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    /// `None` means no telemetry carrying `gcode_state` has been observed yet.
    pub fn print_status(&self) -> Option<PrintStatus> {
        self.cache
            .last_gcode_state
            .as_deref()
            .map(PrintStatus::from_gcode_state)
    }

    /// Returns whether the door was open as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    ///
    /// Returns `None` on models without a door sensor (`ModelQuirks::has_door_sensor()`
    /// returns `false`, e.g. A1/A2), regardless of telemetry observed — distinct from
    /// `Some(false)`, which means a sensor-equipped model's telemetry confirms the door is
    /// closed. Also `None` before any telemetry carrying `print` has been observed.
    pub fn door_open(&self) -> Option<bool> {
        if !self.model.quirks().has_door_sensor() {
            return None;
        }
        self.cache.last_door_open
    }

    /// Returns the decoded active print-error fault as of the last-observed `print_error` telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    ///
    /// `None` covers both "no telemetry carrying `print_error` observed yet" and "the
    /// register reads 0 (no fault)" — both warrant the same caller action, so they are not
    /// distinguished here.
    pub fn active_fault(&self) -> Option<DecodedPrintError> {
        decode_print_error(self.cache.last_print_error?)
    }

    /// Returns the print progress snapshot as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    /// Each field independently tracks its own "last observed" value — see [`PrintProgress`]'s doc
    /// comment.
    pub fn print_progress(&self) -> PrintProgress {
        self.cache.last_progress
    }

    /// Returns the bed's (actual, target) temperatures in °C, decoded from the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    /// Returns `(0, 0)` before any telemetry carrying bed temperature has been observed.
    ///
    /// Shares its cross-model decode logic with
    /// [`TelemetryReport::bed_temperatures()`](crate::types::TelemetryReport::bed_temperatures) —
    /// use that method instead if you already have a fresh `TelemetryReport` in hand.
    pub fn bed_temperatures(&self) -> (u16, u16) {
        decode_bed_temperatures(
            self.cache.last_device.as_ref(),
            self.cache.last_bed_temper,
            self.cache.last_bed_target_temper,
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
    /// `is_exists`/`is_tray_info_ready()`-equivalent checks (`AmsTray::get_state()` here) and
    /// never scrubs the raw cache. This crate mirrors that design rather than
    /// [`clean_stale_tray_data`]'s proactive-clearing
    /// approach: wiring proactive clearing into this cache would make it *less* faithful to
    /// on-wire state than BambuStudio's own model. Two opt-in ways to get sanitized output
    /// without losing that raw fidelity:
    /// - Check [`AmsTray::get_state()`](crate::types::AmsTray::get_state) (or
    ///   [`evaluate_spool_presence`](crate::ams::evaluate_spool_presence)) before trusting a
    ///   tray's material fields — the same check-before-trust contract BambuStudio itself
    ///   relies on.
    /// - Call [`sanitized_ams()`](Self::sanitized_ams) for a cloned, scrubbed copy — mirrors
    ///   [`hms()`](Self::hms)/[`active_hms_alerts()`](Self::active_hms_alerts)'s raw-cache +
    ///   opt-in-decoded accessor split.
    pub fn ams(&self) -> Option<&AmsStatusReport> {
        self.cache.last_ams.as_ref()
    }

    /// Returns a cloned copy of the cached AMS status report with every tray's stale material
    /// fields cleared via [`clean_stale_tray_data`]
    /// (mirrors [`active_hms_alerts()`](Self::active_hms_alerts)'s raw-cache-decode-on-access
    /// shape). `None` under the same condition as [`ams()`](Self::ams) — no telemetry carrying
    /// `print.ams` observed yet. Does not mutate the underlying cache — [`ams()`](Self::ams)
    /// keeps returning the raw values; see its doc comment for why the raw cache is never
    /// proactively scrubbed.
    pub fn sanitized_ams(&self) -> Option<AmsStatusReport> {
        let mut sanitized = self.cache.last_ams.clone()?;
        for unit in &mut sanitized.ams {
            if let Some(trays) = &mut unit.tray {
                for tray in trays {
                    clean_stale_tray_data(tray);
                }
            }
        }
        Some(sanitized)
    }

    /// Returns the cached virtual/external spool holder state (single-nozzle models) as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    /// `None` means no telemetry carrying `print.vt_tray` has been observed yet — including on IDEX
    /// models, which send [`vir_slot()`](Self::vir_slot) instead.
    pub fn vt_tray(&self) -> Option<&VirtualTray> {
        self.cache.last_vt_tray.as_ref()
    }

    /// Returns the cached IDEX external spool holder array as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    /// `None` means no telemetry carrying `print.vir_slot` has been observed yet — including on
    /// single-nozzle models, which send [`vt_tray()`](Self::vt_tray) instead.
    pub fn vir_slot(&self) -> Option<&[VirtualTray]> {
        self.cache.last_vir_slot.as_deref()
    }

    /// Returns the nozzle temperatures as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)) as `(id, actual, target)` tuples in °C.
    /// Single-nozzle models return one entry (`id` 0); IDEX models return one entry per physical
    /// nozzle. See [`decode_nozzle_temperatures`] for the cross-model decode (including the
    /// undocumented IDEX flat-field routing quirk).
    pub fn nozzle_temperatures(&self) -> Vec<(u8, u16, u16)> {
        decode_nozzle_temperatures(
            self.cache.last_device.as_ref(),
            self.cache.last_nozzle_temper,
            self.cache.last_nozzle_target_temper,
        )
    }

    /// Returns the chamber's (actual, target) temperatures in °C, decoded from the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    ///
    /// Returns `None` on models without an active chamber temperature sensor/heater
    /// (`ModelQuirks::ignores_chamber_temperature()` returns `true`, e.g. A1/A1 Mini/A2L/P1P/
    /// P1S) — mirrors `door_open()`'s sensor-capability gate. `Some((0, 0))` before any
    /// telemetry carrying `chamber_temper` has been observed on a chamber-equipped model.
    pub fn chamber_temperature(&self) -> Option<(u16, u16)> {
        if self.model.quirks().ignores_chamber_temperature() {
            return None;
        }
        let raw = self.cache.last_chamber_temper.unwrap_or(0.0);
        Some(PrinterTelemetry::unpack_temperature(raw))
    }

    /// Returns the cached active hardware-alert (HMS) entries as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    /// `None` means no telemetry carrying `print.hms` has been observed yet.
    pub fn hms(&self) -> Option<&[HmsEntry]> {
        self.cache.last_hms.as_deref()
    }

    /// Returns every cached HMS entry decoded and filtered to genuine faults (mirrors `active_fault()`'s raw-cache-decode-on-access shape).
    /// Empty when nothing is cached or nothing currently decodes as a genuine fault — there's no caller
    /// action that would differ between those two cases.
    pub fn active_hms_alerts(&self) -> Vec<DecodedHmsAlert> {
        self.cache
            .last_hms
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|entry| decode_hms_alert(entry.attr, entry.code))
            .filter(|decoded| decoded.is_genuine_fault)
            .collect()
    }

    /// Returns the part-cooling fan speed (Port 1) as a percentage (0-100), decoded from the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    pub fn part_cooling_fan_speed(&self) -> Option<u8> {
        self.decode_fan_speed(self.cache.last_cooling_fan_speed.as_deref())
    }

    /// Returns the primary left-side auxiliary fan speed (Port 2) as a percentage (0-100).
    pub fn auxiliary_left_fan_speed(&self) -> Option<u8> {
        self.decode_fan_speed(self.cache.last_big_fan1_speed.as_deref())
    }

    /// Returns the chamber exhaust/filtration fan speed (Port 3) as a percentage (0-100).
    pub fn chamber_exhaust_fan_speed(&self) -> Option<u8> {
        self.decode_fan_speed(self.cache.last_big_fan2_speed.as_deref())
    }

    /// Returns the toolhead heatbreak fan speed as a percentage (0-100).
    /// Not independently controllable (no corresponding `FanTarget` variant/M106 port) — read-only
    /// telemetry.
    pub fn heatbreak_fan_speed(&self) -> Option<u8> {
        self.decode_fan_speed(self.cache.last_heatbreak_fan_speed.as_deref())
    }

    /// Returns the X2D/P2S secondary right-side auxiliary fan speed (Port 10, `FanTarget::AuxiliaryRight`) as a percentage (0-100).
    /// Reported at a different wire location than the other four fans —
    /// `device.airduct.parts[id=160].state` — already a direct percentage, no 0-15 step conversion
    /// [REF-CLIM-FANS].
    pub fn auxiliary_right_fan_speed(&self) -> Option<u8> {
        let state = self
            .cache
            .last_device
            .as_ref()?
            .airduct
            .as_ref()?
            .parts
            .iter()
            .find(|part| part.id == super::types::FAN_READ_PORT_AUXILIARY_RIGHT)?
            .state?;
        Some(state.clamp(0, 100) as u8)
    }

    fn decode_fan_speed(&self, raw: Option<&str>) -> Option<u8> {
        decode_fan_percentage(raw, self.model.quirks().auxiliary_fan_uses_percentage())
    }

    /// Returns the printer's current print-speed level as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    /// `None` before any telemetry carrying `spd_lvl` has been observed, or if the observed value is
    /// out of the known 1-4 range.
    pub fn print_speed(&self) -> Option<PrintSpeed> {
        PrintSpeed::from_level(self.cache.last_spd_lvl?)
    }

    /// Returns the printer's current print-speed magnitude (percentage of nominal feedrate) as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    pub fn print_speed_magnitude(&self) -> Option<u16> {
        self.cache.last_spd_mag
    }

    /// Returns the raw wireless signal strength string (e.g. `"-52dBm"`) as of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    pub fn wifi_signal(&self) -> Option<&str> {
        self.cache.last_wifi_signal.as_deref()
    }

    /// Returns whether the printer is on wired Ethernet, per the cached `wifi_signal` sentinel (mirrors `PrinterTelemetry::is_ethernet_active_via_wifi_signal()` but works between polls off the cached value, the same way [`is_all_axes_homed()`](Self::is_all_axes_homed) works off cached `home_flag`).
    pub fn is_ethernet_active_via_wifi_signal(&self) -> bool {
        self.cache.last_wifi_signal.as_deref() == Some("-90dBm")
    }

    /// Pulls the next raw MQTT message without deserialization.
    pub async fn poll_raw(&mut self) -> Result<MqttMessage, BambuError> {
        self.ensure_mqtt().await?;
        self.mqtt
            .as_mut()
            .unwrap()
            .poll_telemetry_with_timer(&self.timer)
            .await
    }
}
