#[cfg(not(feature = "std"))]
use alloc::format;

use crate::error::Error;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};

use super::{CommandHandle, PrinterClient};

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
    pub async fn set_bed_temperature(&mut self, target_temp: u16) -> Result<CommandHandle, Error> {
        let mains_220v = self.is_220v_power();
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
    ) -> Result<CommandHandle, Error> {
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
    ///
    /// **This does not manage the airduct flap, and on a model that has one the target will not
    /// be reached without it.** `M141` and the flap are independent: the flap stays wherever it
    /// was last left, and its default cooling position actively vents the chamber, so a
    /// chamber-heat request issued with the flap in cooling never converges — the heater is
    /// fighting an open exhaust, and this method still returns `Ok`. Use
    /// [`preheat_chamber()`](Self::preheat_chamber) to drive both together, or call
    /// [`set_airduct_mode()`](Self::set_airduct_mode) yourself.
    pub async fn set_chamber_temperature(
        &mut self,
        target_temp: u16,
    ) -> Result<CommandHandle, Error> {
        let Some(max) = self
            .identity
            .model
            .quirks()
            .active_chamber_heater_max_temp_c()
        else {
            return Err(Error::ModelMismatch(
                "active chamber heater not available on this model".into(),
            ));
        };
        let target_temp = super::clamp_temp(target_temp, max, "Chamber");
        let gcode = format!("M141 S{}", target_temp);
        self.send_gcode_raw(&gcode).await
    }

    /// Sets the chamber target *and* the airduct flap that has to agree with it.
    ///
    /// [`set_chamber_temperature()`](Self::set_chamber_temperature) is the primitive: it emits
    /// `M141` and nothing else. That is not enough on any model fitted with the cooling/heating
    /// flap (H2C, H2D, H2D Pro, H2S, X2D, and P2S for the cooling direction only, having the
    /// flap but no active chamber heater). The flap is independent of `M141` and **persists**
    /// across jobs, and its default cooling position actively vents the chamber — so a heat
    /// request with the flap left in cooling never converges.
    ///
    /// This method sets the flap to [`AirductMode::Heating`] before raising the target, and back
    /// to [`AirductMode::Cooling`] when `target_temp` is `0`. The second half is not optional:
    /// a PLA job following an ABS job on the same machine would otherwise inherit the heating
    /// flap and overheat.
    ///
    /// On a model with no flap ([`ModelQuirks::supports_airduct_mode`] false), this is exactly
    /// `set_chamber_temperature`. On a model with a flap but no heater (P2S), a non-zero
    /// `target_temp` still returns the same `ModelMismatch` the primitive would, and the flap is
    /// left alone — the caller wanted heat this model cannot make.
    ///
    /// Returns the handle of the `M141` when one is sent, or of the `set_airduct` command when
    /// `target_temp` is `0` on a flap-only model. When both are sent, only the `M141`'s handle is
    /// returned.
    ///
    /// [`AirductMode::Heating`]: crate::mqtt::commands::AirductMode::Heating
    /// [`AirductMode::Cooling`]: crate::mqtt::commands::AirductMode::Cooling
    /// [`ModelQuirks::supports_airduct_mode`]: crate::quirks::ModelQuirks::supports_airduct_mode
    pub async fn preheat_chamber(&mut self, target_temp: u16) -> Result<CommandHandle, Error> {
        use crate::mqtt::commands::AirductMode;

        let quirks = self.identity.model.quirks();
        let has_flap = quirks.supports_airduct_mode();
        let has_heater = quirks.active_chamber_heater_max_temp_c().is_some();

        // A model with a flap but no heater can still be asked to stop venting-for-cooling.
        // A model with neither is nothing but the primitive.
        if has_flap && (has_heater || target_temp == 0) {
            let mode = if target_temp > 0 {
                AirductMode::Heating
            } else {
                AirductMode::Cooling
            };
            let seq = self.set_airduct_mode(mode).await?;
            if !has_heater {
                return Ok(seq);
            }
        }

        self.set_chamber_temperature(target_temp).await
    }
}
