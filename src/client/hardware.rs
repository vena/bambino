#[cfg(not(feature = "std"))]
use alloc::format;

use crate::error::Error;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};

use super::{CommandHandle, PrinterClient};
use crate::mqtt::commands::{AirductMode, PurifierGear, SntpRequest};
use crate::types::control::{BuzzerMode, FanTarget, LedNode};
#[cfg(not(feature = "std"))]
use alloc::string as alloc_string;
#[cfg(feature = "std")]
use std::string as alloc_string;

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
    /// Sets the speed of a targeted onboard fan as a percentage (0 to 100) [REF-CLIM-FANS].
    ///
    /// Translates the percentage to the 0-255 PWM range of `M106`, on the fan's own port
    /// ([`FanTarget::write_port`]).
    ///
    /// # Errors
    ///
    /// [`Error::ModelMismatch`] when this model doesn't have the fan.
    pub async fn set_fan_speed(
        &mut self,
        fan: FanTarget,
        speed_percent: u8,
    ) -> Result<CommandHandle, Error> {
        require(fan.is_supported_by(self.quirks()), || {
            format!("{fan:?} fan not available on this model")
        })?;
        if speed_percent > 100 {
            log::warn!(
                "Fan speed {}% exceeds maximum 100%, clamping",
                speed_percent
            );
        }
        let speed_clamped = core::cmp::min(speed_percent, 100);
        let pwm = ((speed_clamped as u32 * 255) / 100) as u16;
        let gcode = format!("M106 P{} S{}", fan.write_port(), pwm);
        self.send_gcode_raw(&gcode).await
    }

    /// Turns an LED fixture on or off [REF-MQTT-LIFECYCLE].
    ///
    /// # Errors
    ///
    /// [`Error::ModelMismatch`] when the printer's own `lights_report` shows it lacks `node`
    /// (see [`Capabilities::light_support`](crate::client::Capabilities::light_support)).
    /// Before a full status report has listed the printer's lights, every node is allowed.
    pub async fn set_led(&mut self, node: LedNode, turn_on: bool) -> Result<CommandHandle, Error> {
        require(
            self.capabilities().light_support(node).is_supported(),
            || format!("{node} light not available on this printer"),
        )?;
        self.dispatch(|seq| crate::mqtt::commands::LedCtrlRequest::new(node, turn_on, seq))
            .await
    }

    /// Configures the active climate airduct damper mode [REF-MQTT-LIFECYCLE].
    ///
    /// Supported on models with controllable airduct dampers (H2 series, P2S, X2D).
    pub async fn set_airduct_mode(&mut self, mode: AirductMode) -> Result<CommandHandle, Error> {
        require(self.quirks().supports_airduct_mode(), || {
            "airduct damper control not available on this model".into()
        })?;
        self.dispatch(|seq| crate::mqtt::commands::AirductRequest::new(mode, seq))
            .await
    }

    /// Sets the air purifier (APS) gear.
    ///
    /// Refused unless the printer has reported the accessory fitted (`print.aux` bit 37, see
    /// [`air_purifier_present()`](Self::air_purifier_present)), so poll telemetry after
    /// connecting first.
    ///
    /// # Errors
    ///
    /// [`Error::ModelMismatch`] when `aux` bit 37 is clear, or when no `aux` has been reported.
    pub async fn set_air_purifier_gear(
        &mut self,
        gear: PurifierGear,
    ) -> Result<CommandHandle, Error> {
        require(self.air_purifier_present() == Some(true), || {
            "air purifier not reported fitted by this printer".into()
        })?;
        self.dispatch(|seq| crate::mqtt::commands::PartControlRequest::air_purifier(gear, seq))
            .await
    }

    /// Sets the printer's clock, as BambuStudio does on every connection.
    ///
    /// The caller supplies the time, since `no_std` targets have no wall clock; `micros` is the
    /// part past `unix_seconds` and `utc_offset_minutes` the offset in effect at that time,
    /// daylight saving included. Not sent automatically on connect. Sends BambuStudio's `src`
    /// (`1`, the only documented value) and `path` `2` (LAN only).
    ///
    /// A LAN-only printer with no route to an NTP server otherwise keeps whatever clock it has,
    /// and that clock stamps `hms[].ts_unix`, which `reference/07_diagnostics_hms.md` §7.1 says
    /// is not guaranteed UTC.
    ///
    /// The command isn't ack-correlated: BambuStudio ignores an `err_code` in the reply
    /// (`MachineObject::parse_json`), so a printer that doesn't support it may answer with an
    /// error. Which firmware honours it is unrecorded.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidArgument`] when `micros` is 1,000,000 or more, or `utc_offset_minutes` is
    /// outside -12:00 to +14:00.
    pub async fn set_clock(
        &mut self,
        unix_seconds: u64,
        micros: u32,
        utc_offset_minutes: i32,
    ) -> Result<CommandHandle, Error> {
        // Built with a placeholder sequence id the closure replaces, so a bad argument is refused
        // before anything is sent.
        let mut request = SntpRequest::new(unix_seconds, micros, utc_offset_minutes, 0u64)?;
        self.dispatch(|seq| {
            request.system.sequence_id = seq.into();
            request
        })
        .await
    }

    /// Modifies active alarm or attention chime parameters on the physical buzzer module [REF-MQTT-LIFECYCLE].
    ///
    /// Supported on models with a physical fire alarm buzzer (H2 series).
    pub async fn set_buzzer_mode(&mut self, mode: BuzzerMode) -> Result<CommandHandle, Error> {
        require(self.quirks().has_buzzer(), || {
            "buzzer control not available on this model".into()
        })?;
        self.dispatch(|seq| crate::mqtt::commands::BuzzerRequest::new(mode, seq))
            .await
    }
}

/// Fails with [`Error::ModelMismatch`] carrying `message()` unless `supported`.
pub(crate) fn require(
    supported: bool,
    message: impl FnOnce() -> alloc_string::String,
) -> Result<(), Error> {
    if supported {
        Ok(())
    } else {
        Err(Error::ModelMismatch(message().into()))
    }
}
