#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::ToString;

use crate::diagnostics::ExtrusionCaliGetResponse;
use crate::error::Error;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};
use crate::types::VersionInfo;

use super::PrinterClient;

use crate::types::telemetry::AmsUnitModel;

/// Returns true if `ams_id` addresses a physical AMS bus unit.
///
/// That is a standard AMS unit (`0..=3`), an AMS-HT unit (`128..=135`), or an A2L-attached AMS
/// Lite under either spelling — the physical wire id
/// [`AMS_LITE_ON_A2L_PHYSICAL_ID`](crate::ams::parser::AMS_LITE_ON_A2L_PHYSICAL_ID) or the
/// [normalized id](crate::ams::parser::AMS_LITE_ON_A2L_NORMALIZED_ID) telemetry reports it as.
/// External-spool sentinels are excluded; see [`is_valid_ams_id`] for the set that includes them.
#[must_use]
pub(crate) fn is_valid_ams_bus_unit_id(ams_id: i32) -> bool {
    (0..=i32::from(crate::ams::parser::AMS_MAX_STANDARD_ID)).contains(&ams_id)
        || (i32::from(crate::ams::parser::AMS_HT_ID_MIN)
            ..=i32::from(crate::ams::parser::AMS_HT_ID_MAX))
            .contains(&ams_id)
        || ams_id == i32::from(crate::ams::parser::AMS_LITE_ON_A2L_PHYSICAL_ID)
        || ams_id == i32::from(crate::ams::parser::AMS_LITE_ON_A2L_NORMALIZED_ID)
}

/// Returns true if `ams_id` is a bus unit ([`is_valid_ams_bus_unit_id`]) or an external-spool
/// sentinel (`254`/`255`) — the full documented `ams_id` address space shared by
/// `change_filament()` and `select_k_profile()`.
#[must_use]
pub(crate) fn is_valid_ams_id(ams_id: i32) -> bool {
    is_valid_ams_bus_unit_id(ams_id)
        || ams_id == i32::from(crate::ams::parser::AMS_EXTERNAL_SPOOL_DEPUTY_ID)
        || ams_id == i32::from(crate::ams::parser::AMS_EXTERNAL_SPOOL_MAIN_ID)
}

/// Translates a caller-supplied `ams_id` into the id the wire carries.
///
/// Only the A2L-attached AMS Lite differs: telemetry normalizes its physical id 16 to 6, but
/// every per-unit command addresses it as 16 with a local `0..=3` slot — confirmed from the
/// firmware's own `ams_mapping2` (`{ams_id: 16, slot_id: 0-3}`, bambuddy `a2l_lite_wire_ids`,
/// `bambu_mqtt.py:142-163`), and BambuStudio sends `ams_get_rfid {ams_id: 16}` for the unit.
/// Callers may pass either spelling; every other id passes through untouched.
#[must_use]
pub(crate) fn wire_ams_id(ams_id: i32) -> i32 {
    if ams_id == i32::from(crate::ams::parser::AMS_LITE_ON_A2L_NORMALIZED_ID) {
        i32::from(crate::ams::parser::AMS_LITE_ON_A2L_PHYSICAL_ID)
    } else {
        ams_id
    }
}

/// Global tray ids of an A2L-attached AMS Lite accepted by tray-id addressing commands:
/// `AMS_LITE_ON_A2L_NORMALIZED_ID * AMS_SLOTS_PER_UNIT + slot`, i.e. `24..=27`.
///
/// This is BambuStudio's own tray id for the unit — `DevAms::GetTrayId` returns `24 + slot_id`
/// for `AMS_LITE_MIXED` (`DevFilaSystem.cpp:262-263`), and `extrusion_cali_sel`'s `tray_id` is
/// filled from it (`AMSMaterialsSetting.cpp:677`). bambuddy instead extrapolates `64..=67`
/// (`16 * 4 + slot`) and marks that as unconfirmed; BambuStudio wins the disagreement.
pub(crate) const AMS_LITE_ON_A2L_GLOBAL_TRAY_IDS: core::ops::RangeInclusive<i32> =
    (crate::ams::parser::AMS_LITE_ON_A2L_NORMALIZED_ID as i32
        * crate::ams::parser::AMS_SLOTS_PER_UNIT as i32)
        ..=(crate::ams::parser::AMS_LITE_ON_A2L_NORMALIZED_ID as i32
            * crate::ams::parser::AMS_SLOTS_PER_UNIT as i32
            + crate::ams::parser::AMS_SLOTS_PER_UNIT as i32
            - 1);

/// `ams.tray_now` value meaning no filament is loaded to the toolhead.
const AMS_TRAY_NOW_UNLOADED: &str = "255";

/// Highest standard-AMS global tray ID accepted by tray-id addressing commands:
/// `AMS_MAX_STANDARD_ID + 1` units × `AMS_SLOTS_PER_UNIT` slots, zero-indexed (i.e. `15`).
pub(crate) const STANDARD_AMS_MAX_GLOBAL_TRAY_ID: i32 =
    (crate::ams::parser::AMS_MAX_STANDARD_ID as i32 + 1)
        * crate::ams::parser::AMS_SLOTS_PER_UNIT as i32
        - 1;

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
    /// The wire's `target` field is derived internally rather than caller-supplied —
    /// confirmed against BambuStudio's `command_ams_change_filament`
    /// (`DeviceManager.cpp:1602-1638`) — `target` is `255` on unload, the `ams_id` itself for
    /// any AMS-HT/external-spool unit or the A2L AMS Lite (wire `ams_id >= 16`), or the flat global tray ID
    /// (`ams_id*4 + slot_id`) for a standard unit. A caller-supplied `target` that didn't
    /// match this derivation was a real hardware misconfiguration risk (error `07FF_8012`
    /// class), not just a doc gap — `target` mirroring `slot_id` only coincidentally held for
    /// `ams_id: 0`, the sole worked example in the reference doc.
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
        ams_id: i32,
        slot_id: i32,
        curr_temp: i32,
        tar_temp: i32,
        extruder_id: Option<u8>,
    ) -> Result<u16, Error> {
        let ams_valid = is_valid_ams_id(ams_id);
        let slot_valid = (0..=3).contains(&slot_id) || slot_id == 254 || slot_id == 255;
        // slot_id 254 is only meaningful as the external-spool load sentinel, so it is valid
        // only against an external-spool ams_id. `ams_id >= 16` was too loose: it also admits
        // the AMS-HT bus range 128..=135, and the reference documents an AMS-HT slot only as
        // `{"ams_id": ams_id, "slot_id": 0}` (`reference/05_materials_ams.md` §5.3). A pair like
        // (130, 254) therefore derived a correct `target` but shipped a nonsensical slot.
        let pair_valid = slot_id != 254
            || ams_id == i32::from(crate::ams::parser::AMS_EXTERNAL_SPOOL_DEPUTY_ID)
            || ams_id == i32::from(crate::ams::parser::AMS_EXTERNAL_SPOOL_MAIN_ID);
        if !ams_valid || !slot_valid || !pair_valid {
            return Err(Error::ProtocolViolation(
                "invalid AMS addressing parameters for change_filament".into(),
            ));
        }

        let ams_id = wire_ams_id(ams_id);
        let target = if slot_id == 255 {
            255
        } else if ams_id >= 16 {
            ams_id
        } else {
            ams_id * i32::from(crate::ams::parser::AMS_SLOTS_PER_UNIT) + slot_id
        };

        self.dispatch(|seq| {
            crate::mqtt::AmsChangeFilamentRequest::new(
                ams_id,
                slot_id,
                target,
                curr_temp,
                tar_temp,
                extruder_id,
                seq,
            )
        })
        .await
    }

    /// Whether this printer supports remote AMS drying — the printer-side half of the gate.
    ///
    /// Supplies this client's [`quirk_context()`](Self::quirk_context) to
    /// [`ModelQuirks::supports_ams_remote_drying`](crate::quirks::ModelQuirks::supports_ams_remote_drying),
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
    pub(crate) fn cached_ams_unit_model(&self, ams_id: i32) -> Option<AmsUnitModel> {
        let ams_id = u8::try_from(ams_id).map_or(ams_id, |id| {
            i32::from(crate::ams::parser::normalize_ams_unit_id(id))
        });
        self.ams()?
            .ams
            .iter()
            .find(|unit| unit.id.parse::<i32>() == Ok(ams_id))
            .and_then(crate::types::telemetry::AmsUnit::unit_model)
    }

    /// Configures a drying cycle for the unit at `ams_id`, to be sent with
    /// [`send()`](crate::client::DryingCycle::send).
    ///
    /// The way to start drying. Names each parameter at the call site instead of ordering nine
    /// of them, defaults the four most callers don't set, and lets
    /// [`material()`](crate::client::DryingCycle::material) fill temperature, duration and
    /// cooling temperature from one choice:
    ///
    /// ```rust,ignore
    /// client
    ///     .dry(0)
    ///     .material(DryingMaterial::Petg, AmsUnitModel::Ams2Pro)
    ///     .rotate_tray(true)
    ///     .send()
    ///     .await?;
    /// ```
    ///
    /// Nothing is published until [`send()`](crate::client::DryingCycle::send), which is where
    /// every gate runs — host capability, AMS addressing, the external-spool sentinels, the
    /// attached unit's model, and the temperature range [REF-AMS-DRYER].
    pub fn dry(
        &mut self,
        ams_id: i32,
    ) -> crate::client::DryingCycle<
        '_,
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
    > {
        crate::client::DryingCycle::new(self, ams_id)
    }

    /// Terminates an active dry-chamber heating cycle on an AMS unit [REF-AMS-DRYER].
    ///
    /// Mirrors BambuStudio's `CtrlAmsStopDrying` (`DevFilaSystemCtrl.cpp:40-53`) exactly —
    /// every field zeroed/defaulted, only `mode: 0` (`Off`) is meaningful.
    pub async fn stop_drying(&mut self, ams_id: i32) -> Result<u16, Error> {
        if !is_valid_ams_id(ams_id) {
            return Err(Error::ProtocolViolation(
                "invalid AMS addressing parameters for stop_drying".into(),
            ));
        }
        let ams_id = wire_ams_id(ams_id);
        self.dispatch(|seq| {
            crate::mqtt::AmsFilamentDryingRequest::new(ams_id, 0, "", 0, 0, 0, false, 0, false, seq)
        })
        .await
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
    /// **Two commands, chosen by protocol generation** — BambuStudio's selector
    /// (`StatusPanel.cpp:5376-5399`). A printer whose telemetry shows the new MQTT protocol
    /// ([`PrinterTelemetry::reports_new_protocol`](crate::types::PrinterTelemetry::reports_new_protocol))
    /// gets `ams_get_rfid`; one whose `push_status` frames don't gets the G-code
    /// `M620 R<global tray>` (`command_ams_refresh_rfid`, `DeviceManager.cpp:1738-1743`), since
    /// an old-protocol printer acks `ams_get_rfid` and does nothing. Before any telemetry has
    /// arrived the protocol is unknown and `ams_get_rfid` is sent — call
    /// [`poll_telemetry()`](Self::poll_telemetry) first on older firmware.
    ///
    /// **Refused while filament is loaded to the toolhead**, because the scan feeds filament to
    /// the reader: returns [`Error::InvalidState`] when the cached `ams.tray_now` is anything but
    /// `255` (unloaded), matching bambuddy (`bambu_mqtt.py:7601-7615`). BambuStudio refuses the
    /// same case with a dialog (`StatusPanel.cpp:5386-5391`). An unobserved `tray_now` passes.
    pub async fn scan_rfid(&mut self, ams_id: i32, slot_id: i32) -> Result<u16, Error> {
        let ams_valid = is_valid_ams_bus_unit_id(ams_id);
        let slot_valid = (0..=3).contains(&slot_id);
        if !ams_valid || !slot_valid {
            return Err(Error::ProtocolViolation(
                "invalid AMS addressing parameters for scan_rfid".into(),
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

        if self.cache.last_new_protocol == Some(false) {
            // Both ids are range-checked above, so the u8 conversions cannot fail.
            let global_tray = u8::try_from(ams_id)
                .ok()
                .zip(u8::try_from(slot_id).ok())
                .and_then(|(ams, slot)| {
                    crate::ams::resolve_global_tray_id(crate::ams::normalize_ams_unit_id(ams), slot)
                })
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

        let ams_id = wire_ams_id(ams_id);
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
    /// **Validation note:** the cheat-sheet above documents only the *external-spool* case.
    /// `reference/05_materials_ams.md` §5.3's own primary `extrusion_cali_sel` example binds a
    /// perfectly ordinary AMS slot (`"ams_id": 0, "tray_id": 1`) — `tray_id` there is the
    /// *global* tray ID (the same `(ams_id * 4) + slot_id` / `128..=135` AMS-HT composite the
    /// flat `ams_mapping` array uses, per §5.3's "Hardware Channel Identifiers"), not a
    /// per-unit slot index. The validation below therefore accepts the full documented
    /// address space — standard AMS units, AMS-HT units, and the external-spool sentinels —
    /// not just the two cheat-sheet pairs; restricting to only `(254,254)`/`(255,255)` (as an
    /// earlier draft of this check assumed) would incorrectly reject this exact primary example.
    pub async fn select_k_profile(
        &mut self,
        ams_id: i32,
        tray_id: i32,
        cali_idx: i32,
        filament_id: &str,
        nozzle_diameter: &str,
    ) -> Result<u16, Error> {
        let ams_valid = is_valid_ams_id(ams_id);
        let tray_valid = (0..=STANDARD_AMS_MAX_GLOBAL_TRAY_ID).contains(&tray_id)
            || AMS_LITE_ON_A2L_GLOBAL_TRAY_IDS.contains(&tray_id)
            || (128..=135).contains(&tray_id)
            || tray_id == 254
            || tray_id == 255;
        if !ams_valid || !tray_valid {
            return Err(Error::ProtocolViolation(
                "invalid ams_id/tray_id parameters for select_k_profile".into(),
            ));
        }

        let ams_id = wire_ams_id(ams_id);
        self.dispatch(|seq| {
            crate::diagnostics::ExtrusionCaliSelRequest::new(
                ams_id,
                tray_id,
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
    /// timeout if you need a shorter deadline than `command_timeout_secs`.
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
        let mut parse_failed = false;
        let result = self
            .poll_until(|msg| {
                let v: serde_json::Value = serde_json::from_slice(&msg.payload).ok()?;
                let node = v.get("info").unwrap_or(&v);
                if node.get("command")?.as_str()? == "get_version" {
                    match serde_json::from_value::<VersionInfo>(node.clone()) {
                        Ok(info) if info.sequence_id == expected_seq => Some(info),
                        Ok(_) => None,
                        Err(_) => {
                            parse_failed = true;
                            None
                        }
                    }
                } else {
                    None
                }
            })
            .await;

        match result {
            Err(_) if parse_failed => Err(Error::Serialization),
            Ok(info) => {
                // Cache the OTA version for the quirk context — several capabilities are gated
                // on it, and a caller should not have to re-query per check.
                if let Some(firmware) = info.firmware_version() {
                    self.cache.last_firmware = Some(firmware.to_string());
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
    /// **A response is the complete table for exactly one nozzle diameter.** `nozzle_diameter`
    /// scopes the query, and the reply echoes the diameter that was *requested* rather than
    /// reflecting installed hardware. Passing `None` sends the bare request, whose reply covers
    /// whichever single diameter the firmware picks — on a machine that can hold more than one,
    /// that is a partial table which looks complete to the caller. Call once per fitted diameter
    /// and merge the results.
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
        if !self.k_profile_primed {
            let prime_seq = self.next_sequence_id();
            let prime_req = crate::diagnostics::ExtrusionCaliGetRequest::new(
                filament_id,
                nozzle_diameter,
                prime_seq,
            );
            self.publish_request(&prime_req).await?;
            self.k_profile_primed = true;
        }

        let seq = self.next_sequence_id();
        let req =
            crate::diagnostics::ExtrusionCaliGetRequest::new(filament_id, nozzle_diameter, seq);
        self.publish_request(&req).await?;

        let expected_seq = seq.to_string();
        self.poll_until(|msg| {
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
        self.k_profile_primed = primed;
    }
}
