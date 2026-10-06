#[cfg(not(feature = "std"))]
use alloc::format;

use crate::error::Error;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};

use super::{CommandHandle, PrinterClient};
use crate::mqtt::commands::AirductMode;
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
    pub async fn set_led(&mut self, node: LedNode, turn_on: bool) -> Result<CommandHandle, Error> {
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

    /// Configures whether the printer's speakers emit prompt notification sounds [REF-MQTT-LIFECYCLE].
    ///
    /// Supported on models with onboard speakers (A1, A1 Mini, A2L).
    pub async fn set_prompt_sound(&mut self, enable_sound: bool) -> Result<CommandHandle, Error> {
        require(self.quirks().supports_prompt_sound(), || {
            "prompt sound not available on this model".into()
        })?;
        self.dispatch(|seq| crate::mqtt::commands::PromptSoundRequest::new(enable_sound, seq))
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
