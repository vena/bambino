#[cfg(not(feature = "std"))]
use alloc::string::ToString;

use crate::diagnostics::ExtrusionCaliGetResponse;
use crate::error::BambuError;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};
use crate::types::VersionInfo;

use super::PrinterClient;

/// Maximum documented drying-chamber temperature (°C) for an AMS-HT unit's built-in heater — confirmed via Bambu Lab's own wiki (`wiki.bambulab.com/en/ams-ht/Intr-to-ams-ht-workflow-and-features`), not this crate's `reference/` docs (no drying temperature ceiling is documented there).
/// This is a property of the physical AMS-HT hardware, not the host printer model.
pub(crate) const AMS_HT_DRY_TEMP_MAX: u32 = 85;

/// Maximum documented drying-chamber temperature (°C) for an AMS 2 Pro / standard-AMS unit's built-in heater — confirmed via Bambu Lab's own wiki (`wiki.bambulab.com/en/ams-2-pro/manual/drying-function`).
/// Property of the physical AMS 2 Pro hardware, not the host printer model.
pub(crate) const AMS_STANDARD_DRY_TEMP_MAX: u32 = 65;

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
    /// The wire's `target` field is derived internally rather than caller-supplied (BUG-116):
    /// confirmed against BambuStudio's `command_ams_change_filament`
    /// (`DeviceManager.cpp:1602-1638`) — `target` is `255` on unload, the `ams_id` itself for
    /// any AMS-HT/external-spool unit (`ams_id >= 16`), or the flat global tray ID
    /// (`ams_id*4 + slot_id`) for a standard unit. A caller-supplied `target` that didn't
    /// match this derivation was a real hardware misconfiguration risk (error `07FF_8012`
    /// class), not just a doc gap — `target` mirroring `slot_id` only coincidentally held for
    /// `ams_id: 0`, the sole worked example in the reference doc.
    pub async fn change_filament(
        &mut self,
        ams_id: i32,
        slot_id: i32,
        curr_temp: i32,
        tar_temp: i32,
    ) -> Result<u16, BambuError> {
        let ams_valid = (0..=3).contains(&ams_id)
            || (128..=135).contains(&ams_id)
            || ams_id == 254
            || ams_id == 255;
        let slot_valid = (0..=3).contains(&slot_id) || slot_id == 254 || slot_id == 255;
        if !ams_valid || !slot_valid {
            return Err(BambuError::ProtocolViolation(
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
                ams_id, slot_id, target, curr_temp, tar_temp, seq,
            )
        })
        .await
    }

    /// Initiates a dry-chamber heating cycle on an AMS-HT or AMS 2 Pro unit [REF-AMS-DRYER].
    ///
    /// * `ams_id`: Target AMS unit index. AMS-HT units use the `128..=135` bus ID range (see
    ///   `AMS_HT_ID_MIN`/`AMS_HT_ID_MAX` in `src/ams/parser.rs`); anything else is treated as
    ///   an AMS 2 Pro / standard-AMS drying unit.
    /// * `dry_temp`: Drying temperature in degrees Celsius. Clamped to this AMS unit's
    ///   documented ceiling — this is a property of the *attached AMS unit*, not the host
    ///   printer model: AMS-HT's built-in heater is rated to 85°C, AMS 2 Pro's to 65°C
    ///   (confirmed via Bambu Lab's own wiki, `wiki.bambulab.com/en/ams-ht/...` and
    ///   `wiki.bambulab.com/en/ams-2-pro/manual/drying-function` respectively — no per-printer
    ///   variation is documented, so this does not go through `ModelQuirks`).
    /// * `dry_time`: Duration in minutes (e.g., 480 for an 8-hour cycle). No documented
    ///   maximum duration was found to validate against — drying time appears to be
    ///   user-configurable with no firmware-enforced ceiling.
    /// * `rotate_tray`: Whether to rotate trays during the cycle.
    /// * `filament`: Filament type string (e.g., "PA-CF").
    pub async fn start_drying(
        &mut self,
        ams_id: i32,
        dry_temp: u32,
        dry_time: u32,
        rotate_tray: bool,
        filament: &str,
    ) -> Result<u16, BambuError> {
        let max_temp: u32 = if (128..=135).contains(&ams_id) {
            AMS_HT_DRY_TEMP_MAX
        } else {
            AMS_STANDARD_DRY_TEMP_MAX
        };
        let dry_temp = if dry_temp > max_temp {
            log::warn!(
                "AMS dry temperature {}°C exceeds maximum {}°C, clamping",
                dry_temp,
                max_temp
            );
            max_temp
        } else {
            dry_temp
        };

        self.dispatch(|seq| {
            crate::mqtt::AmsFilamentDryingRequest::new(
                ams_id,
                1,
                dry_temp,
                dry_time,
                rotate_tray,
                filament,
                seq,
            )
        })
        .await
    }

    /// Terminates an active dry-chamber heating cycle on an AMS unit [REF-AMS-DRYER].
    pub async fn stop_drying(&mut self, ams_id: i32) -> Result<u16, BambuError> {
        self.dispatch(|seq| {
            crate::mqtt::AmsFilamentDryingRequest::new(ams_id, 0, 0, 0, false, "", seq)
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
    pub async fn scan_rfid(&mut self, ams_id: i32, slot_id: i32) -> Result<u16, BambuError> {
        let ams_valid = (0..=3).contains(&ams_id) || (128..=135).contains(&ams_id);
        let slot_valid = (0..=3).contains(&slot_id);
        if !ams_valid || !slot_valid {
            return Err(BambuError::ProtocolViolation(
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
    ///   `tray_id: 254` (BUG-117).
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
    ) -> Result<u16, BambuError> {
        let ams_valid = (0..=3).contains(&ams_id)
            || (128..=135).contains(&ams_id)
            || ams_id == 254
            || ams_id == 255;
        let tray_valid = (0..=103).contains(&tray_id)
            || (128..=135).contains(&tray_id)
            || tray_id == 254
            || tray_id == 255;
        if !ams_valid || !tray_valid {
            return Err(BambuError::ProtocolViolation(
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
    pub async fn get_version(&mut self) -> Result<VersionInfo, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::GetVersionRequest::new(seq);
        self.publish_request(&req).await?;

        let expected_seq = seq.to_string();
        self.poll_until(|msg| {
            let v: serde_json::Value = serde_json::from_slice(&msg.payload).ok()?;
            let node = v.get("info").unwrap_or(&v);
            if node.get("command")?.as_str()? == "get_version" {
                let info: VersionInfo = serde_json::from_value(node.clone()).ok()?;
                if info.sequence_id == expected_seq {
                    Some(info)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .await
    }

    /// Requests a dump of the printer's stored K-profile calibration database [REF-DIAG-KPROF].
    ///
    /// Automatically sends a priming request on the first call after connection, because the
    /// firmware silently ignores the initial `extrusion_cali_get` command. Use
    /// `set_k_profile_primed(true)` to skip the automatic prime if you handle it yourself.
    pub async fn get_k_profiles(&mut self) -> Result<ExtrusionCaliGetResponse, BambuError> {
        if !self.k_profile_primed {
            let prime_seq = self.next_sequence_id();
            let prime_req = crate::diagnostics::ExtrusionCaliGetRequest::new(prime_seq);
            self.publish_request(&prime_req).await?;
            self.k_profile_primed = true;
        }

        let seq = self.next_sequence_id();
        let req = crate::diagnostics::ExtrusionCaliGetRequest::new(seq);
        self.publish_request(&req).await?;

        let expected_seq = seq.to_string();
        self.poll_until(|msg| {
            let resp: ExtrusionCaliGetResponse = serde_json::from_slice(&msg.payload).ok()?;
            if resp.print.command == "extrusion_cali_get" && resp.print.sequence_id == expected_seq
            {
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
