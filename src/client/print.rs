#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::error::BambuError;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};
use crate::mqtt::{PrintJobConfig, StandardControlRequest};

use super::PrinterClient;
use super::types::{CalibrationOption, PrintSpeed};

impl<
    MqttRawIO,
    MqttTls,
    MqttFactory,
    Timer,
    FtpsRawIO,
    FtpsTls,
    FtpsFactory,
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
    CameraRawIO: AsyncIo,
    CameraTls: TlsConnector<CameraRawIO>,
    CameraFactory: RawStreamFactory<CameraRawIO>,
{
    /// Pauses the currently active print job [REF-MQTT-LIFECYCLE].
    pub async fn pause_print(&mut self) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = StandardControlRequest::new("pause", seq);
        self.publish_request(&req).await
    }

    /// Resumes a paused print job [REF-MQTT-LIFECYCLE].
    pub async fn resume_print(&mut self) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = StandardControlRequest::new("resume", seq);
        self.publish_request(&req).await
    }

    /// Aborts/cancels the currently running print job queue [REF-MQTT-LIFECYCLE].
    pub async fn stop_print(&mut self) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = StandardControlRequest::new("stop", seq);
        self.publish_request(&req).await
    }

    /// Clears active error codes from the printer's diagnostic fault register [REF-MQTT-LIFECYCLE].
    pub async fn clear_print_error(&mut self) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::CleanPrintErrorRequest::new(seq);
        self.publish_request(&req).await
    }

    /// Dynamically scales maximum velocity and acceleration limits during an active print [REF-MQTT-LIFECYCLE].
    pub async fn set_print_speed(&mut self, level: PrintSpeed) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let speed_str = match level {
            PrintSpeed::Silent => "1",
            PrintSpeed::Standard => "2",
            PrintSpeed::Sport => "3",
            PrintSpeed::Ludicrous => "4",
        };
        let req = crate::mqtt::commands::PrintSpeedRequest::new(speed_str, seq);
        self.publish_request(&req).await
    }

    /// Bypasses rendering of specific objects within an active multi-model print job [REF-MQTT-LIFECYCLE].
    pub async fn skip_objects(&mut self, object_ids: Vec<u32>) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::SkipObjectsRequest::new(object_ids, seq);
        self.publish_request(&req).await
    }

    /// Triggers automated physical calibration routines on the printer chassis [REF-MQTT-LIFECYCLE].
    ///
    /// Use `CalibrationOption` flags combined with `|` to select routines:
    /// ```ignore
    /// client.start_calibration(
    ///     CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION
    /// ).await?;
    /// ```
    pub async fn start_calibration(
        &mut self,
        options: CalibrationOption,
    ) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::CalibrationRequest::new(options.0, seq);
        self.publish_request(&req).await
    }

    /// Submits a `.3mf` print job from MicroSD storage for execution [REF-MQTT-LIFECYCLE].
    ///
    /// When `nozzle_offset_cali` is `None`, the model's quirks engine resolves the default.
    pub async fn start_print(&mut self, config: &PrintJobConfig) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::ProjectFileRequest::from_config(config, seq, self.model);
        self.publish_request(&req).await
    }
}
