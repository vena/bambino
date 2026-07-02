#[cfg(not(feature = "std"))]
use alloc::format;

use crate::error::BambuError;
use crate::ftps::FtpDataStreamFactory;
use crate::io::{AsyncIo, SecureConnect, TimerProvider, TlsConnector};
use crate::types::telemetry::report::POWER_220V_BITMASK;

use super::PrinterClient;

impl<Conn, Timer, RawIO, Tls, Factory> PrinterClient<Conn, Timer, RawIO, Tls, Factory>
where
    Conn: SecureConnect,
    Timer: TimerProvider,
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: FtpDataStreamFactory<RawIO>,
{
    /// Sets the heated bed target temperature.
    ///
    /// Values exceeding the model's maximum are clamped automatically. Most models have a flat
    /// per-model ceiling (e.g. 80°C for A1 Mini), but X1C's ceiling is voltage-dependent — 110°C
    /// on a 220V-region unit, 120°C on a 110V-region unit, per the official spec sheet. This is
    /// derived from the most recently observed `home_flag` telemetry
    /// (`self.last_home_flag`, bit 3 — see [`PrinterTelemetry::is_220v_power`](crate::types::PrinterTelemetry::is_220v_power));
    /// before any `home_flag` has been received (fresh connection, no `pushall` yet) the mains
    /// region is unknown and X1C conservatively clamps to 110°C.
    ///
    /// # Example
    ///
    /// ```ignore
    /// printer.set_bed_temperature(60).await?;
    /// ```
    pub async fn set_bed_temperature(&mut self, target_temp: u16) -> Result<u16, BambuError> {
        let mains_220v = self
            .last_home_flag
            .map(|flag| flag & POWER_220V_BITMASK != 0);
        let max = self.model.quirks().bed_temp_max(mains_220v);
        let target_temp = if target_temp > max {
            log::warn!(
                "Bed temperature {}°C exceeds model max {}°C, clamping",
                target_temp,
                max
            );
            max
        } else {
            target_temp
        };
        let gcode = format!("M140 S{}", target_temp);
        self.send_gcode_raw(&gcode).await
    }

    /// Sets the target temperature of a specific hotend/nozzle [REF-MOTO-GCODE].
    ///
    /// * `nozzle_id`: The carriage ID (usually `0` for primary/single, or `1` for secondary on IDEX).
    ///
    /// Values exceeding the model's maximum nozzle temperature are clamped automatically.
    pub async fn set_nozzle_temperature(
        &mut self,
        nozzle_id: u8,
        target_temp: u16,
    ) -> Result<u16, BambuError> {
        let max = self.model.quirks().nozzle_temp_max();
        let target_temp = if target_temp > max {
            log::warn!(
                "Nozzle temperature {}°C exceeds model max {}°C, clamping",
                target_temp,
                max
            );
            max
        } else {
            target_temp
        };
        let gcode = format!("M104 T{} S{}", nozzle_id, target_temp);
        self.send_gcode_raw(&gcode).await
    }

    /// Sets the target temperature of the active heated chamber loop [REF-MOTO-GCODE].
    ///
    /// **Chamber Temperature Safety Check [REF-THER-DECODE]:**
    /// Only supported on models with active PTC chamber heaters (X1E, X2D, H2 series).
    /// Models with passive chamber sensors but no heater (X1C, P2S) will return a capability
    /// mismatch error — their firmware silently ignores M141.
    pub async fn set_chamber_temperature(&mut self, target_temp: u16) -> Result<u16, BambuError> {
        if !self.model.quirks().has_active_chamber_heater() {
            return Err(BambuError::ModelMismatch(
                "active chamber heater not available on this model".into(),
            ));
        }
        let max = self.model.quirks().chamber_temp_max();
        let target_temp = if target_temp > max {
            log::warn!(
                "Chamber temperature {}°C exceeds model max {}°C, clamping",
                target_temp,
                max
            );
            max
        } else {
            target_temp
        };
        let gcode = format!("M141 S{}", target_temp);
        self.send_gcode_raw(&gcode).await
    }
}
