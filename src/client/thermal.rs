#[cfg(not(feature = "std"))]
use alloc::format;

use crate::error::Error;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};
use crate::types::telemetry::report::POWER_220V_BITMASK;

use super::PrinterClient;

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
    /// Sets the heated bed target temperature.
    ///
    /// Values exceeding the model's maximum are clamped automatically. Most models have a flat
    /// per-model ceiling (e.g. 80°C for A1 Mini), but X1C's ceiling is voltage-dependent — 110°C
    /// on a 220V-region unit, 120°C on a 110V-region unit, per the official spec sheet. This is
    /// derived from the most recently observed `home_flag` telemetry
    /// (`self.cache.last_home_flag`, bit 3 — see [`PrinterTelemetry::is_220v_power`](crate::types::PrinterTelemetry::is_220v_power));
    /// before any `home_flag` has been received (fresh connection, no `pushall` yet) the mains
    /// region is unknown and X1C conservatively clamps to 110°C.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// printer.set_bed_temperature(60).await?;
    /// ```
    pub async fn set_bed_temperature(&mut self, target_temp: u16) -> Result<u16, Error> {
        let mains_220v = self
            .cache
            .last_home_flag
            .map(|flag| flag & POWER_220V_BITMASK != 0);
        let max = self.identity.model.quirks().bed_temp_max(mains_220v);
        let target_temp = super::clamp_temp(target_temp, max, "Bed");
        let gcode = format!("M140 S{}", target_temp);
        self.send_gcode_raw(&gcode).await
    }

    /// Sets the target temperature of a specific hotend/nozzle [REF-MOTO-GCODE].
    ///
    /// * `nozzle_id`: The carriage ID (usually `0` for primary/single, or `1` for secondary on
    ///   IDEX). **Tool-changer exception (H2C):** per `reference/04_toolhead_thermal_motion.md`
    ///   §4's "Nozzle & Carriage Kinematics", H2C addresses its dedicated fixed hotend as `0`
    ///   (same `M104 T0` convention as every other model) but its 6 passive tool-changer rack
    ///   slots as `16..=21` — NOT a simple `0..physical_nozzle_count()` linear index, despite
    ///   `physical_nozzle_count()` returning `7` for this model. The reference doc only
    ///   confirms `16..=21` for the rack slots' telemetry-side `stat` field, not that
    ///   `M104 T16`-style writes are actually meaningful for a passively-stored (unmounted)
    ///   tool — validation below is deliberately permissive on H2C for exactly that reason.
    ///
    /// Values exceeding the model's maximum nozzle temperature are clamped automatically.
    pub async fn set_nozzle_temperature(
        &mut self,
        nozzle_id: u8,
        target_temp: u16,
    ) -> Result<u16, Error> {
        // Rack-slot addressing is a quirks *predicate*, not something to infer from the
        // nozzle count — `uses_nozzle_rack()` is passed explicitly by the H2 macro precisely
        // so a future variant has to state whether it racks its hotends (see
        // `quirks/models/h2.rs`), and `mqtt/commands/print_job.rs` already dispatches on it.
        let quirks = self.identity.model.quirks();
        if quirks.uses_nozzle_rack() {
            // Tool changer: fixed hotend (0) or a rack slot (16..=21) — see doc comment.
            if nozzle_id != 0 && !(16..=21).contains(&nozzle_id) {
                return Err(Error::ModelMismatch(
                    "nozzle_id must be the fixed hotend (0) or a rack slot (16..=21) on this model"
                        .into(),
                ));
            }
        } else if nozzle_id >= quirks.physical_nozzle_count() {
            return Err(Error::ModelMismatch(
                "nozzle_id exceeds this model's physical nozzle count".into(),
            ));
        }

        let max = self.identity.model.quirks().nozzle_temp_max();
        let target_temp = super::clamp_temp(target_temp, max, "Nozzle");
        let gcode = format!("M104 T{} S{}", nozzle_id, target_temp);
        self.send_gcode_raw(&gcode).await
    }

    /// Sets the target temperature of the active heated chamber loop [REF-MOTO-GCODE].
    ///
    /// **Chamber Temperature Safety Check [REF-THER-DECODE]:**
    /// Only supported on models with active PTC chamber heaters (X1E, X2D, H2 series).
    /// Models with passive chamber sensors but no heater (X1C, P2S) will return a capability
    /// mismatch error — their firmware silently ignores M141.
    pub async fn set_chamber_temperature(&mut self, target_temp: u16) -> Result<u16, Error> {
        let Some(max) = self.identity.model.quirks().active_chamber_heater_max_temp_c() else {
            return Err(Error::ModelMismatch(
                "active chamber heater not available on this model".into(),
            ));
        };
        let target_temp = super::clamp_temp(target_temp, max, "Chamber");
        let gcode = format!("M141 S{}", target_temp);
        self.send_gcode_raw(&gcode).await
    }
}
