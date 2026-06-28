use crate::diagnostics::ExtrusionCaliGetResponse;
use crate::error::BambuError;
use crate::ftps::FtpDataStreamFactory;
use crate::io::{AsyncIo, SecureConnect, TimerProvider, TlsConnector};
use crate::types::VersionInfo;

use super::PrinterClient;

impl<Conn, Timer, RawIO, Tls, Factory> PrinterClient<Conn, Timer, RawIO, Tls, Factory>
where
    Conn: SecureConnect,
    Timer: TimerProvider,
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: FtpDataStreamFactory<RawIO>,
{
    /// Triggers a filament load or unload sequence on a physical AMS unit or external spool [REF-AMS-MAP].
    ///
    /// * `ams_id`: AMS unit index (0-3), or `255` for external spool.
    /// * `slot_id`: Slot within the AMS (0-3), or `254` for external spool.
    /// * `target`: Load destination (`1` = toolhead load, `255` = unload/retract).
    /// * `curr_temp` / `tar_temp`: Nozzle temperatures (`-1` = let firmware decide).
    pub async fn change_filament(
        &mut self,
        ams_id: i32,
        slot_id: i32,
        target: i32,
        curr_temp: i32,
        tar_temp: i32,
    ) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::AmsChangeFilamentRequest::new(
            ams_id, slot_id, target, curr_temp, tar_temp, seq,
        );
        self.publish_request(&req).await
    }

    /// Initiates a dry-chamber heating cycle on an AMS-HT or AMS 2 Pro unit [REF-AMS-DRYER].
    ///
    /// * `ams_id`: Target AMS unit index.
    /// * `dry_temp`: Drying temperature in degrees Celsius.
    /// * `dry_time`: Duration in minutes (e.g., 480 for an 8-hour cycle).
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
        let seq = self.next_sequence_id();
        let req = crate::mqtt::AmsFilamentDryingRequest::new(
            ams_id,
            1,
            dry_temp,
            dry_time,
            rotate_tray,
            filament,
            seq,
        );
        self.publish_request(&req).await
    }

    /// Terminates an active dry-chamber heating cycle on an AMS unit [REF-AMS-DRYER].
    pub async fn stop_drying(&mut self, ams_id: i32) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::AmsFilamentDryingRequest::new(ams_id, 0, 0, 0, false, "", seq);
        self.publish_request(&req).await
    }

    /// Scans proprietary RFID tag properties on a specific AMS tray [REF-AMS-MAP].
    pub async fn scan_rfid(&mut self, ams_id: i32, slot_id: i32) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::AmsGetRfidRequest::new(ams_id, slot_id, seq);
        self.publish_request(&req).await
    }

    /// Binds a stored K-profile calibration entry to an AMS material slot [REF-AMS-MAP].
    pub async fn select_k_profile(
        &mut self,
        ams_id: i32,
        tray_id: i32,
        cali_idx: i32,
        filament_id: &str,
        nozzle_diameter: &str,
    ) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::diagnostics::ExtrusionCaliSelRequest::new(
            ams_id,
            tray_id,
            cali_idx,
            filament_id,
            nozzle_diameter,
            seq,
        );
        self.publish_request(&req).await
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

        self.poll_until(|msg| {
            let v: serde_json::Value = serde_json::from_slice(&msg.payload).ok()?;
            let node = v.get("info").unwrap_or(&v);
            if node.get("command")?.as_str()? == "get_version" {
                serde_json::from_value::<VersionInfo>(node.clone()).ok()
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

        self.poll_until(|msg| {
            let resp: ExtrusionCaliGetResponse = serde_json::from_slice(&msg.payload).ok()?;
            if resp.print.command == "extrusion_cali_get" {
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
