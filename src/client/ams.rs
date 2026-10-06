#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use crate::ams::ids::{
    AMS_SLOTS_PER_UNIT, SLOT_UNLOAD, VALID_AMS_IDS_TEXT, is_bus_unit_id, is_external_spool_id,
    is_unit_slot, is_valid_ams_id, normalize_ams_unit_id, wire_ams_id,
};
use crate::diagnostics::ExtrusionCaliGetResponse;
use crate::error::Error;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};
use crate::mqtt::commands::ChangeTemps;
use crate::types::VersionInfo;

use super::{CommandHandle, PrinterClient};

use crate::types::telemetry::AmsUnitModel;

/// `change_filament` slot meaning "load from the single-nozzle external spool".
const SLOT_EXTERNAL_LOAD: u8 = 254;

/// `ams.tray_now` value meaning no filament is loaded to the toolhead.
const AMS_TRAY_NOW_UNLOADED: &str = "255";

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
    /// Triggers a filament load or unload sequence on a physical AMS unit or external spool [REF-AMS-MAP].
    ///
    /// * `ams_id`: AMS unit index (`0..=3`), AMS-HT unit bus ID (`128..=135`), an A2L-attached
    ///   AMS Lite (`6` as telemetry reports it, or its physical `16`), or `254`/`255` for
    ///   external spool (IDEX Ext-L/Ext-R or single-nozzle, respectively).
    /// * `slot_id`: Slot within the AMS (`0..=3`), `254` for a single-nozzle external-spool
    ///   load, or `255` to unload/retract (see `ams_change_filament` examples in
    ///   `reference/05_materials_ams.md` §5.3 [REF-AMS-MAP]).
    /// * `curr_temp` / `tar_temp`: Nozzle temperatures (`-1` = let firmware decide).
    ///
    /// The wire's `target` field is derived, not caller-supplied — see
    /// [`AmsChangeFilamentRequest::load`](crate::mqtt::AmsChangeFilamentRequest::load).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidArgument`] for an address no unit answers to.
    ///
    /// `extruder_id` names the hotend to feed — `Some(0)` for right/main, `Some(1)` for
    /// left/deputy. Pass `None` on any printer without a Filament Track Switch, where the
    /// firmware derives the hotend from the AMS's own extruder binding and the payload is
    /// byte-identical to the pre-FTS form. On a machine *with* a switch (an H2C, for example)
    /// every AMS reports its extruder as "not fixed" (`0xE`) and can reach either hotend
    /// through the switch, so a `None` here means the firmware has nothing to derive from and
    /// **discards the command in silence** — load and unload simply do nothing.
    pub async fn change_filament(
        &mut self,
        ams_id: u8,
        slot_id: u8,
        curr_temp: i32,
        tar_temp: i32,
        extruder_id: Option<u8>,
    ) -> Result<CommandHandle, Error> {
        let slot_valid = match slot_id {
            SLOT_UNLOAD => true,
            // Only meaningful as the external-spool load sentinel. The reference documents an
            // AMS-HT slot only as `{"ams_id": ams_id, "slot_id": 0}`
            // (`reference/05_materials_ams.md` §5.3), so (130, 254) is not a load.
            SLOT_EXTERNAL_LOAD => is_external_spool_id(ams_id),
            // An external holder takes the local slot range too; a bus unit only its own slots,
            // which for a single-slot AMS-HT is just 0 (#357), matching `resolve_global_tray_id`.
            slot if is_external_spool_id(ams_id) => slot < AMS_SLOTS_PER_UNIT,
            slot => is_unit_slot(ams_id, slot),
        };
        if !is_valid_ams_id(ams_id) || !slot_valid {
            return Err(Error::InvalidArgument(
                format!(
                    "change_filament: ams_id {ams_id} slot_id {slot_id} is not a load or unload \
                     address (ams_id {VALID_AMS_IDS_TEXT}; slot 0..=3, 0 on AMS-HT, 254 for an \
                     external spool, 255 to unload)"
                )
                .into(),
            ));
        }

        let ams_id = wire_ams_id(ams_id);
        let temps = ChangeTemps {
            current: curr_temp,
            target: tar_temp,
        };
        self.dispatch(|seq| {
            if slot_id == SLOT_UNLOAD {
                crate::mqtt::AmsChangeFilamentRequest::unload(ams_id, temps, extruder_id, seq)
            } else {
                crate::mqtt::AmsChangeFilamentRequest::load(
                    ams_id,
                    slot_id,
                    temps,
                    extruder_id,
                    seq,
                )
            }
        })
        .await
    }

    /// Whether this printer supports remote AMS drying — the printer-side half of the gate.
    ///
    /// Supplies this client's [`quirk_context()`](Self::quirk_context) to
    /// [`ModelQuirks::ams_remote_drying_support`](crate::quirks::ModelQuirks::ams_remote_drying_support),
    /// which resolves the printer's own reported answer against the model's rules. This is the
    /// call to gate a UI on: it is the identical value a drying cycle's
    /// [`send()`](crate::client::DryingCycle::send) checks, so a control offered on the strength
    /// of it cannot then be refused.
    ///
    /// Shorthand for
    /// [`capabilities().supports_ams_remote_drying()`](crate::client::Capabilities::supports_ams_remote_drying);
    /// see there for how the answer is resolved and what has to be polled first.
    #[must_use]
    pub fn supports_ams_remote_drying(&self) -> bool {
        self.capabilities().supports_ams_remote_drying()
    }

    /// Looks up the cached [`AmsUnitModel`] for the unit at `ams_id`, if one has been observed.
    ///
    /// `None` covers three distinct cases that all mean the same thing to a caller — no AMS
    /// snapshot has arrived yet, no unit answers to this address, or the unit reports a type
    /// newer than this crate knows — and all three read as "don't assume a capability".
    ///
    /// Matches on the unit's own `id`, which is already normalized on deserialize (the A2L's AMS
    /// Lite reports physical `16` and is stored as `6`), so a caller-supplied physical `16` is
    /// normalized the same way before comparing.
    pub fn ams_unit_model(&self, ams_id: u8) -> Option<AmsUnitModel> {
        self.ams()?
            .unit(normalize_ams_unit_id(ams_id))?
            .unit_model()
    }

    /// Scans proprietary RFID tag properties on a specific AMS tray [REF-AMS-MAP].
    ///
    /// * `ams_id`: AMS unit index (`0..=3`), AMS-HT unit bus ID (`128..=135`), or an A2L-attached
    ///   AMS Lite (`6` or its physical `16`; sent as `16`). Only
    ///   documented against a physical bus unit (`reference/03_mqtt_telemetry.md`
    ///   `ams_get_rfid` example) — external spools have no RFID reader node, so no
    ///   external-spool sentinel value applies here.
    /// * `slot_id`: Slot within the AMS (`0..=3`).
    ///
    /// **Two commands, chosen by firmware payload format** — BambuStudio's selector
    /// (`StatusPanel.cpp:5376-5399`). The MQTT transport is the same either way. A printer whose
    /// telemetry shows BambuStudio's "np" format
    /// ([`PrinterTelemetry::reports_np_format`](crate::types::PrinterTelemetry::reports_np_format))
    /// gets `ams_get_rfid`; one whose `push_status` frames don't gets the G-code
    /// `M620 R<global tray>` (`command_ams_refresh_rfid`, `DeviceManager.cpp:1738-1743`), since
    /// older firmware acks `ams_get_rfid` and does nothing. Before any telemetry has arrived the
    /// format is unknown and `ams_get_rfid` is sent — call
    /// [`poll_telemetry()`](Self::poll_telemetry) first on older firmware.
    ///
    /// **Refused while filament is loaded to the toolhead**, because the scan feeds filament to
    /// the reader: returns [`Error::InvalidState`] when the cached `ams.tray_now` is anything but
    /// `255` (unloaded), matching bambuddy (`bambu_mqtt.py:7601-7615`). BambuStudio refuses the
    /// same case with a dialog (`StatusPanel.cpp:5386-5391`). An unobserved `tray_now` passes.
    pub async fn scan_rfid(&mut self, ams_id: u8, slot_id: u8) -> Result<CommandHandle, Error> {
        // `is_unit_slot` keeps AMS-HT to its single slot (#357); without that the np path
        // published a nonexistent slot while the legacy path's `resolve_global_tray_id` rejected
        // the same input.
        if !is_bus_unit_id(ams_id) || !is_unit_slot(ams_id, slot_id) {
            return Err(Error::InvalidArgument(
                format!(
                    "scan_rfid: ams_id {ams_id} slot_id {slot_id} is not an AMS slot (ams_id 0..=3, \
                     6 or 16, 128..=135; slot 0..=3, 0 on AMS-HT)"
                )
                .into(),
            ));
        }
        if self
            .ams()
            .and_then(|ams| ams.tray_now.as_deref())
            .is_some_and(|tray_now| tray_now != AMS_TRAY_NOW_UNLOADED)
        {
            return Err(Error::InvalidState(
                "filament is loaded to the toolhead — unload it before scanning RFID".into(),
            ));
        }

        if self.core.cache.last_np_format == Some(false) {
            let global_tray =
                crate::ams::resolve_global_tray_id(normalize_ams_unit_id(ams_id), slot_id)
                    .ok_or_else(|| {
                        Error::ProtocolViolation(
                            "AMS address has no global tray index for M620 R".into(),
                        )
                    })?;
            let gcode = format!("M620 R{global_tray}");
            return self
                .dispatch(|seq| crate::mqtt::GCodeRequest::new(&gcode, seq))
                .await;
        }

        let ams_id = i32::from(wire_ams_id(ams_id));
        let slot_id = i32::from(slot_id);
        self.dispatch(|seq| crate::mqtt::AmsGetRfidRequest::new(ams_id, slot_id, seq))
            .await
    }

    /// Binds a stored K-profile calibration entry to an AMS material slot [REF-AMS-MAP].
    ///
    /// **IDEX External-Spool Addressing Cheat-Sheet:** this command (`extrusion_cali_sel`)
    /// uses different `ams_id`/`tray_id` external-spool addressing than
    /// `ams_filament_setting` (filament configuration) — do not reuse one rule for both:
    /// * `extrusion_cali_sel` (this command) — Single-Nozzle Platforms: `ams_id: 254` /
    ///   `tray_id: 254`. Dual-Nozzle IDEX: Ext-L requires `ams_id: 254` / `tray_id: 254`;
    ///   Ext-R requires `ams_id: 255` / `tray_id: 255`. **Warning:** targeting the wrong
    ///   address for Ext-R on IDEX machines mis-routes the pressure advance profile to
    ///   the left carriage (Ext-L) EEPROM, leaving the primary right carriage completely
    ///   uncalibrated.
    /// * `ams_filament_setting` — Single-Nozzle Platforms: `ams_id: 255` / `tray_id: 254`.
    ///   Dual-Nozzle IDEX: both Ext-L (`ams_id: 254`) and Ext-R (`ams_id: 255`) require
    ///   `tray_id: 254`.
    ///
    /// Takes the unit and its **local** slot; the global `tray_id` the wire carries is derived by
    /// [`CaliSelAddress`](crate::diagnostics::kprofile::CaliSelAddress). Taking the global id
    /// from the caller used to let `(2, 1)` bind unit 0's tray 1 while claiming unit 2 (#397).
    pub async fn select_k_profile(
        &mut self,
        ams_id: u8,
        slot_id: u8,
        cali_idx: i32,
        filament_id: &str,
        nozzle_diameter: &str,
    ) -> Result<CommandHandle, Error> {
        let address = crate::diagnostics::kprofile::CaliSelAddress::new(ams_id, slot_id)?;
        self.dispatch(|seq| {
            crate::diagnostics::ExtrusionCaliSelRequest::new(
                address,
                cali_idx,
                filament_id,
                nozzle_diameter,
                seq,
            )
        })
        .await
    }

    /// Queries the printer's expansion bus version database and returns typed module info.
    ///
    /// Sends a `get_version` command and waits for the response, buffering any
    /// telemetry messages that arrive in the interim. Wrap in a platform-specific
    /// timeout if you need a shorter deadline than the command timeout.
    pub async fn get_version(&mut self) -> Result<VersionInfo, Error> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::GetVersionRequest::new(seq);
        self.publish_request(&req).await?;

        let expected_seq = seq.to_string();
        // Distinguishes "a get_version response arrived but failed to deserialize" from "not
        // my message" — without this, a malformed module (e.g. one missing a field that lacks
        // #[serde(default)]) silently falls through as a non-match and the caller sees whatever
        // error poll_until eventually surfaces (typically Error::Timeout, or a connection error
        // if the stream drops) with no indication a response ever arrived (issue #52).
        let mut parse_error: Option<String> = None;
        let result = self
            .poll_until_command_timeout(|msg| {
                let v: serde_json::Value = serde_json::from_slice(&msg.payload).ok()?;
                let node = v.get("info").unwrap_or(&v);
                if node.get("command")?.as_str()? == "get_version" {
                    match serde_json::from_value::<VersionInfo>(node.clone()) {
                        Ok(info) if info.sequence_id == expected_seq => Some(info),
                        Ok(_) => None,
                        Err(e) => {
                            // Keep the first failure: it names the field that didn't parse.
                            parse_error.get_or_insert_with(|| e.to_string());
                            None
                        }
                    }
                } else {
                    None
                }
            })
            .await;

        match result {
            Err(_) if parse_error.is_some() => Err(Error::Serialization(
                format!("get_version response: {}", parse_error.unwrap_or_default()).into(),
            )),
            Ok(info) => {
                // Cache the OTA version for the quirk context — several capabilities are gated
                // on it, and a caller should not have to re-query per check.
                if let Some(firmware) = info.firmware_version() {
                    self.core.cache.last_firmware = Some(firmware.to_string());
                    self.core.cache.last_firmware_generation =
                        Some(self.core.connection_generation);
                }
                Ok(info)
            }
            other => other,
        }
    }

    /// Requests a dump of the printer's stored K-profile calibration database [REF-DIAG-KPROF].
    ///
    /// Automatically sends a priming request on the first call after connection, because the
    /// firmware silently ignores the initial `extrusion_cali_get` command. Use
    /// `set_k_profile_primed(true)` to skip the automatic prime if you handle it yourself.
    ///
    /// `nozzle_diameter` scopes the query to one diameter; that reply is the complete table for
    /// the diameter *requested*, not a reflection of installed hardware. Passing `None` sends the
    /// bare request, which BambuStudio relies on to return the full table on multi-extruder and
    /// nozzle-rack machines. On other machines BambuStudio calls once per diameter the model
    /// supports and merges the results — see [REF-DIAG-KPROF].
    ///
    /// `filament_id` scopes the query the same way, to a single filament preset id. `None` omits
    /// the field entirely; `Some("")` is the "every filament" form `reference/07_diagnostics_hms.md`
    /// documents, and is distinct from omitting it. Exposed here because the alternative was to
    /// bypass this wrapper and hand-manage the priming quirk through
    /// [`set_k_profile_primed()`](Self::set_k_profile_primed), forfeiting the automatic priming
    /// this method exists to guarantee.
    pub async fn get_k_profiles(
        &mut self,
        filament_id: Option<&str>,
        nozzle_diameter: Option<&str>,
    ) -> Result<ExtrusionCaliGetResponse, Error> {
        if !self.core.k_profile_primed {
            let prime_seq = self.next_sequence_id();
            let prime_req = crate::diagnostics::ExtrusionCaliGetRequest::new(
                filament_id,
                nozzle_diameter,
                prime_seq,
            );
            self.publish_request(&prime_req).await?;
            self.core.k_profile_primed = true;
        }

        let seq = self.next_sequence_id();
        let req =
            crate::diagnostics::ExtrusionCaliGetRequest::new(filament_id, nozzle_diameter, seq);
        self.publish_request(&req).await?;

        let expected_seq = seq.to_string();
        self.poll_until_command_timeout(|msg| {
            let mut resp: ExtrusionCaliGetResponse = match serde_json::from_slice(&msg.payload) {
                Ok(resp) => resp,
                Err(e) => {
                    // Most frames on this topic are unrelated telemetry, so a parse failure is
                    // normally just "not our message" and must stay silent. A payload that
                    // *names* this command and still fails to parse is different: it makes
                    // poll_until run to its timeout with no diagnostic at all, which is how a
                    // single unexpected field shape reads to a caller as an unexplained hang.
                    if msg.payload.windows(18).any(|w| w == b"extrusion_cali_get") {
                        log::debug!("extrusion_cali_get reply failed to deserialize: {e}");
                    }
                    return None;
                }
            };
            if resp.print.command == "extrusion_cali_get" && resp.print.sequence_id == expected_seq
            {
                // Single-nozzle firmware omits nozzle_diameter per-entry, setting it only at
                // the envelope level — see KProfileEntry::nozzle_diameter's doc comment.
                let envelope_diameter = resp.print.nozzle_diameter.clone();
                for entry in &mut resp.print.filaments {
                    if entry.nozzle_diameter.is_none() {
                        entry.nozzle_diameter = envelope_diameter.clone();
                    }
                }
                Some(resp)
            } else {
                None
            }
        })
        .await
    }

    /// Controls whether `get_k_profiles()` sends an automatic priming request.
    ///
    /// Set to `true` to skip the firmware priming quirk — useful if you handle priming
    /// yourself or target firmware that does not require it.
    pub fn set_k_profile_primed(&mut self, primed: bool) {
        self.core.k_profile_primed = primed;
    }
}
