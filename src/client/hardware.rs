#[cfg(not(feature = "std"))]
use alloc::format;

use crate::error::BambuError;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};

use super::PrinterClient;
use super::types::FanTarget;

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
    /// Translates percentage input to standard PWM ranges (0 to 255) in the G-code envelope.
    /// For models with unique secondary cooling configurations (like the X2D), directs commands
    /// to the correct target port ID.
    pub async fn set_fan_speed(
        &mut self,
        fan_type: FanTarget,
        speed_percent: u8,
    ) -> Result<u16, BambuError> {
        if speed_percent > 100 {
            log::warn!(
                "Fan speed {}% exceeds maximum 100%, clamping",
                speed_percent
            );
        }
        let speed_clamped = core::cmp::min(speed_percent, 100);
        let pwm = ((speed_clamped as u32 * 255) / 100) as u16;

        let port_id = match fan_type {
            FanTarget::PartCooling => 1,
            FanTarget::AuxiliaryLeft => {
                if !self.model.quirks().supports_auxiliary_left_fan() {
                    return Err(BambuError::ModelMismatch(
                        "auxiliary left fan not available on this model".into(),
                    ));
                }
                2
            }
            FanTarget::ChamberExhaust => {
                if !self.model.quirks().has_chamber_exhaust_fan() {
                    return Err(BambuError::ModelMismatch(
                        "chamber exhaust fan not available on this model".into(),
                    ));
                }
                3
            }
            FanTarget::AuxiliaryRight => {
                if !self.model.quirks().supports_auxiliary_right_fan() {
                    return Err(BambuError::ModelMismatch(
                        "auxiliary right fan not available on this model".into(),
                    ));
                }
                10
            }
        };

        let gcode = format!("M106 P{} S{}", port_id, pwm);
        self.send_gcode_raw(&gcode).await
    }

    /// Configures the active state of a targeted enclosure LED lighting node [REF-MQTT-LIFECYCLE].
    pub async fn set_led(&mut self, node: &str, turn_on: bool) -> Result<u16, BambuError> {
        self.dispatch(|seq| crate::mqtt::commands::LedCtrlRequest::new(node, turn_on, seq))
            .await
    }

    /// Configures the active climate airduct damper mode [REF-MQTT-LIFECYCLE].
    ///
    /// Supported on models with controllable airduct dampers (H2 series, P2S, X2D).
    pub async fn set_airduct_mode(
        &mut self,
        mode: crate::mqtt::commands::AirductMode,
    ) -> Result<u16, BambuError> {
        if !self.model.quirks().supports_airduct_mode() {
            return Err(BambuError::ModelMismatch(
                "airduct damper control not available on this model".into(),
            ));
        }
        self.dispatch(|seq| crate::mqtt::commands::AirductRequest::new(mode, seq))
            .await
    }

    /// Configures whether the printer's speakers emit prompt notification sounds [REF-MQTT-LIFECYCLE].
    ///
    /// Supported on models with onboard speakers (A1, A1 Mini, A2L).
    pub async fn set_prompt_sound(&mut self, enable_sound: bool) -> Result<u16, BambuError> {
        if !self.model.quirks().supports_prompt_sound() {
            return Err(BambuError::ModelMismatch(
                "prompt sound not available on this model".into(),
            ));
        }
        self.dispatch(|seq| crate::mqtt::commands::PromptSoundRequest::new(enable_sound, seq))
            .await
    }

    /// Modifies active alarm or attention chime parameters on the physical buzzer module [REF-MQTT-LIFECYCLE].
    ///
    /// Buzzer mode codes map to: `0` (Silent/disarmed), `1` (Alarm triggered), `2` (Beeping attention).
    /// Supported on models with a physical fire alarm buzzer (H2 series).
    pub async fn set_buzzer_mode(&mut self, mode_code: i32) -> Result<u16, BambuError> {
        if !self.model.quirks().supports_buzzer() {
            return Err(BambuError::ModelMismatch(
                "buzzer control not available on this model".into(),
            ));
        }
        self.dispatch(|seq| crate::mqtt::commands::BuzzerRequest::new(mode_code, seq))
            .await
    }
}
