#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::ToString;

use crate::diagnostics::ExtrusionCaliGetResponse;
use crate::error::Error;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};
use crate::types::VersionInfo;

use super::PrinterClient;

// The drying-chamber temperature bounds moved to `types::telemetry::ams` alongside
// `AmsUnitModel`, which is where the rest of the AMS *accessory* facts now live — the ceilings
// are a property of the attached unit, not of anything on this client. Re-imported rather than
// re-declared so the two cannot drift.
use crate::types::telemetry::AmsUnitModel;
use crate::types::telemetry::ams::{
    AMS_DRY_TEMP_MIN, AMS_HT_DRY_TEMP_MAX, AMS_STANDARD_DRY_TEMP_MAX,
};

/// Returns true if `ams_id` addresses a standard AMS unit (`0..=3`), an AMS-HT unit
/// (`128..=135`), or an external-spool sentinel (`254`/`255`) — the full documented
/// `ams_id` address space shared by `change_filament()` and `select_k_profile()`.
#[must_use]
fn is_valid_ams_id(ams_id: i32) -> bool {
    (0..=3).contains(&ams_id) || (128..=135).contains(&ams_id) || ams_id == 254 || ams_id == 255
}

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
    /// * `ams_id`: AMS unit index (`0..=3`), AMS-HT unit bus ID (`128..=135`), or `254`/`255`
    ///   for external spool (IDEX Ext-L/Ext-R or single-nozzle, respectively).
    /// * `slot_id`: Slot within the AMS (`0..=3`), `254` for a single-nozzle external-spool
    ///   load, or `255` to unload/retract (see `ams_change_filament` examples in
    ///   `reference/05_materials_ams.md` §5.3 [REF-AMS-MAP]).
    /// * `curr_temp` / `tar_temp`: Nozzle temperatures (`-1` = let firmware decide).
    ///
    /// The wire's `target` field is derived internally rather than caller-supplied —
    /// confirmed against BambuStudio's `command_ams_change_filament`
    /// (`DeviceManager.cpp:1602-1638`) — `target` is `255` on unload, the `ams_id` itself for
    /// any AMS-HT/external-spool unit (`ams_id >= 16`), or the flat global tray ID
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
    /// Supplies the cached `fun2` to
    /// [`ModelQuirks::supports_ams_remote_drying`](crate::quirks::ModelQuirks::supports_ams_remote_drying),
    /// which resolves the printer's own answer against the model default. This is the call to
    /// gate a UI on: it is the identical value [`start_drying`](Self::start_drying) checks, so a
    /// control offered on the strength of it cannot then be refused.
    ///
    /// `fun2` arrives on pushall, so a client that has never called
    /// [`poll_telemetry()`](Self::poll_telemetry) holds `None` here and gets the model default —
    /// on a P1 that means a `false` its firmware may no longer deserve. Poll first if the
    /// distinction matters.
    #[must_use]
    pub fn supports_ams_remote_drying(&self) -> bool {
        self.identity
            .model
            .quirks()
            .supports_ams_remote_drying(self.cache.last_fun2.as_deref())
    }

    /// Looks up the cached [`AmsUnitModel`] for the unit at `ams_id`, if one has been observed.
    ///
    /// `None` covers three distinct cases that all mean the same thing to a caller — no AMS
    /// snapshot has arrived yet, no unit answers to this address, or the unit reports a type
    /// newer than this crate knows — and all three read as "don't assume a capability".
    ///
    /// Matches on the unit's own `id`, which is already normalized on deserialize (the A2L's AMS
    /// Lite reports physical `16` and is stored as `6`), so this compares against the same
    /// address space `is_valid_ams_id` accepts.
    fn cached_ams_unit_model(&self, ams_id: i32) -> Option<AmsUnitModel> {
        self.ams()?
            .ams
            .iter()
            .find(|unit| unit.id.parse::<i32>() == Ok(ams_id))
            .and_then(crate::types::telemetry::AmsUnit::unit_model)
    }

    /// Initiates a dry-chamber heating cycle on an AMS-HT or AMS 2 Pro unit [REF-AMS-DRYER].
    ///
    /// * `ams_id`: Target AMS unit index. AMS-HT units use the `128..=135` bus ID range (see
    ///   `AMS_HT_ID_MIN`/`AMS_HT_ID_MAX` in `src/ams/parser.rs`). The address alone does **not**
    ///   identify the unit: `0..=3` is shared by the original AMS, the AMS Lite and the AMS 2 Pro,
    ///   and only the last of those has a heater — the unit model comes from cached telemetry,
    ///   see Errors below.
    /// * `temp`: Drying temperature in degrees Celsius. Must fall inside the attached unit's
    ///   [`AmsUnitModel::dry_temp_range`] — `45..=65` for the AMS 2 Pro, `45..=85` for the
    ///   AMS-HT. This is a property of the *attached AMS unit*, not the host printer model
    ///   (confirmed via Bambu Lab's own wiki, `wiki.bambulab.com/en/ams-ht/...` and
    ///   `wiki.bambulab.com/en/ams-2-pro/manual/drying-function` respectively — no per-printer
    ///   variation is documented, so this does not go through `ModelQuirks`). **Both bounds are
    ///   rejected, not clamped**: BambuStudio refuses a temperature below the floor exactly as it
    ///   refuses one above the ceiling (`AMSDryControl.cpp:1186-1199`), and silently rewriting a
    ///   caller's `0` into `45` would start a real heating cycle nobody asked for.
    /// * `duration_hours`: Duration in **hours** (e.g., `8` for an 8-hour cycle) —
    ///   the wire field is `duration` in hours, not the old `dry_time` in minutes. No
    ///   documented maximum duration was found to validate against.
    /// * `humidity`: Target humidity (`0` = firmware default / no target).
    /// * `rotate_tray`: Whether to rotate trays during the cycle.
    /// * `cooling_temp`: Cooling temperature applied after the drying cycle completes.
    /// * `close_power_conflict`: Whether to override the AMS unit's power-conflict interlock.
    /// * `filament`: Filament type string (e.g., "PA-CF").
    ///
    /// # Errors
    ///
    /// [`Error::ModelMismatch`] on hosts where
    /// [`supports_ams_remote_drying()`](Self::supports_ams_remote_drying) is `false` — the
    /// printer's own `fun2` bit 5 when it has reported one, else the P1P/P1S quirk. Such
    /// firmware acks this command `result: success` and silently discards it rather than
    /// actually driving the AMS heater; see `[REF-AMS-DRYER]`.
    ///
    /// [`Error::ModelMismatch`] also when the addressed unit is one this crate can see has no
    /// drying chamber — an external-spool sentinel (`254`/`255`), or a cached
    /// [`AmsUnitModel`] whose [`supports_drying`](AmsUnitModel::supports_drying) is `false`.
    /// These are two independent gates on purpose, matching the pair BambuStudio writes out
    /// longhand at `Widgets/AMSControl.cpp:348`: the printer must act on the command *and* the
    /// attached box must have a heater.
    ///
    /// [`Error::InvalidArgument`] when `temp` falls outside the unit's
    /// [`dry_temp_range`](AmsUnitModel::dry_temp_range).
    ///
    /// The unit-model gate reads the **cached** AMS snapshot, so a unit this client has never
    /// observed passes through — same rule as [`skip_objects`](Self::skip_objects), and for the
    /// same reason: an idle printer's incremental pushes frequently carry no `ams` block at all,
    /// and refusing there would break a caller that connects and commands without polling. Call
    /// [`poll_telemetry()`](Self::poll_telemetry) first to arm the gate. When the unit is
    /// unobserved the temperature range falls back to the `ams_id`-derived ceiling this method
    /// used before, which is the best guess available from the address alone.
    #[allow(clippy::too_many_arguments)]
    pub async fn start_drying(
        &mut self,
        ams_id: i32,
        temp: u32,
        duration_hours: u32,
        humidity: u32,
        rotate_tray: bool,
        cooling_temp: i32,
        close_power_conflict: bool,
        filament: &str,
    ) -> Result<u16, Error> {
        if !self.supports_ams_remote_drying() {
            return Err(Error::ModelMismatch(
                "AMS drying is screen-only on this host printer — firmware acks this command but does not act on it".into(),
            ));
        }
        if !is_valid_ams_id(ams_id) {
            return Err(Error::ProtocolViolation(
                "invalid AMS addressing parameters for start_drying".into(),
            ));
        }
        // An external spool is a holder on a bracket, not a box with a heater — the one place
        // the address *does* settle the capability, since 254/255 never appear in the `ams`
        // array for the cached lookup below to find.
        if ams_id == 254 || ams_id == 255 {
            return Err(Error::ModelMismatch(
                "external spool has no drying chamber — start_drying needs an AMS 2 Pro or AMS-HT"
                    .into(),
            ));
        }

        let unit_model = self.cached_ams_unit_model(ams_id);
        if let Some(model) = unit_model
            && !model.supports_drying()
        {
            return Err(Error::ModelMismatch(
                "attached AMS unit has no drying chamber — only the AMS 2 Pro and AMS-HT can dry"
                    .into(),
            ));
        }

        // `dry_temp_range()` is the authority when the unit is known. Unobserved, fall back to
        // the address-derived ceiling — wrong for an original AMS or an AMS Lite at `0..=3`, but
        // that is exactly the case the gate above cannot rule on either.
        let (min_temp, max_temp) = unit_model.and_then(AmsUnitModel::dry_temp_range).unwrap_or(
            if (128..=135).contains(&ams_id) {
                (AMS_DRY_TEMP_MIN, AMS_HT_DRY_TEMP_MAX)
            } else {
                (AMS_DRY_TEMP_MIN, AMS_STANDARD_DRY_TEMP_MAX)
            },
        );
        if temp < min_temp || temp > max_temp {
            return Err(Error::InvalidArgument(
                format!(
                    "AMS dry temperature {temp}°C outside this unit's {min_temp}-{max_temp}°C range"
                )
                .into(),
            ));
        }

        self.dispatch(|seq| {
            crate::mqtt::AmsFilamentDryingRequest::new(
                ams_id,
                1,
                filament,
                temp,
                duration_hours,
                humidity,
                rotate_tray,
                cooling_temp,
                close_power_conflict,
                seq,
            )
        })
        .await
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
        self.dispatch(|seq| {
            crate::mqtt::AmsFilamentDryingRequest::new(ams_id, 0, "", 0, 0, 0, false, 0, false, seq)
        })
        .await
    }

    /// Scans proprietary RFID tag properties on a specific AMS tray [REF-AMS-MAP].
    ///
    /// * `ams_id`: AMS unit index (`0..=3`) or AMS-HT unit bus ID (`128..=135`). Only
    ///   documented against a physical bus unit (`reference/03_mqtt_telemetry.md`
    ///   `ams_get_rfid` example) — external spools have no RFID reader node, so no
    ///   external-spool sentinel value applies here.
    /// * `slot_id`: Slot within the AMS (`0..=3`).
    pub async fn scan_rfid(&mut self, ams_id: i32, slot_id: i32) -> Result<u16, Error> {
        let ams_valid = (0..=3).contains(&ams_id) || (128..=135).contains(&ams_id);
        let slot_valid = (0..=3).contains(&slot_id);
        if !ams_valid || !slot_valid {
            return Err(Error::ProtocolViolation(
                "invalid AMS addressing parameters for scan_rfid".into(),
            ));
        }

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
            || (128..=135).contains(&tray_id)
            || tray_id == 254
            || tray_id == 255;
        if !ams_valid || !tray_valid {
            return Err(Error::ProtocolViolation(
                "invalid ams_id/tray_id parameters for select_k_profile".into(),
            ));
        }

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
    pub async fn get_k_profiles(
        &mut self,
        nozzle_diameter: Option<&str>,
    ) -> Result<ExtrusionCaliGetResponse, Error> {
        if !self.k_profile_primed {
            let prime_seq = self.next_sequence_id();
            let prime_req =
                crate::diagnostics::ExtrusionCaliGetRequest::new(None, nozzle_diameter, prime_seq);
            self.publish_request(&prime_req).await?;
            self.k_profile_primed = true;
        }

        let seq = self.next_sequence_id();
        let req = crate::diagnostics::ExtrusionCaliGetRequest::new(None, nozzle_diameter, seq);
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
