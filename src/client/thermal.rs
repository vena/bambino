#[cfg(not(feature = "std"))]
use alloc::format;

use crate::error::BambuError;
use crate::ftps::FtpDataStreamFactory;
use crate::io::{AsyncIo, TimerProvider, TlsConnector};

use super::PrinterClient;

impl<IO, Timer, RawIO, Tls, Factory> PrinterClient<IO, Timer, RawIO, Tls, Factory>
where
    IO: AsyncIo,
    Timer: TimerProvider,
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: FtpDataStreamFactory<RawIO>,
{
    /// Sets the heated bed target temperature.
    ///
    /// Values exceeding the model's maximum (e.g. 120°C for X1C, 80°C for A1 Mini) are
    /// clamped automatically.
    ///
    /// # Example
    ///
    /// ```ignore
    /// printer.set_bed_temperature(60).await?;
    /// ```
    pub async fn set_bed_temperature(&mut self, target_temp: u16) -> Result<u16, BambuError> {
        let max = self.model.quirks().bed_temp_max();
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
