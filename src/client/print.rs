#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

#[cfg(feature = "std")]
use std::borrow::Cow;

#[cfg(not(feature = "std"))]
use alloc::borrow::Cow;

use crate::error::Error;
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
    /// Pauses the currently active print job [REF-MQTT-LIFECYCLE].
    pub async fn pause_print(&mut self) -> Result<u16, Error> {
        self.dispatch(|seq| StandardControlRequest::new("pause", seq))
            .await
    }

    /// Resumes a paused print job [REF-MQTT-LIFECYCLE].
    pub async fn resume_print(&mut self) -> Result<u16, Error> {
        self.dispatch(|seq| StandardControlRequest::new("resume", seq))
            .await
    }

    /// Aborts/cancels the currently running print job queue [REF-MQTT-LIFECYCLE].
    pub async fn stop_print(&mut self) -> Result<u16, Error> {
        self.dispatch(|seq| StandardControlRequest::new("stop", seq))
            .await
    }

    /// Clears active error codes from the printer's diagnostic fault register [REF-MQTT-LIFECYCLE].
    pub async fn clear_print_error(&mut self) -> Result<u16, Error> {
        self.dispatch(crate::mqtt::CleanPrintErrorRequest::new)
            .await
    }

    /// Dynamically scales maximum velocity and acceleration limits during an active print [REF-MQTT-LIFECYCLE].
    pub async fn set_print_speed(&mut self, level: PrintSpeed) -> Result<u16, Error> {
        let speed_str = match level {
            PrintSpeed::Silent => "1",
            PrintSpeed::Standard => "2",
            PrintSpeed::Sport => "3",
            PrintSpeed::Ludicrous => "4",
        };
        self.dispatch(|seq| crate::mqtt::commands::PrintSpeedRequest::new(speed_str, seq))
            .await
    }

    /// Bypasses rendering of specific objects within an active multi-model print job [REF-MQTT-LIFECYCLE].
    pub async fn skip_objects(&mut self, object_ids: Vec<u32>) -> Result<u16, Error> {
        self.dispatch(|seq| crate::mqtt::SkipObjectsRequest::new(object_ids, seq))
            .await
    }

    /// Triggers automated physical calibration routines on the printer chassis [REF-MQTT-LIFECYCLE].
    ///
    /// Use `CalibrationOption` flags combined with `|` to select routines:
    /// ```rust,ignore
    /// client.start_calibration(
    ///     CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION
    /// ).await?;
    /// ```
    ///
    /// The returned `u16` is the published command's `sequence_id`, not a completion signal, but
    /// the run is observable while it happens (verified on a P1S, firmware `01.10.00.00`):
    ///
    /// - [`print_progress()`](crate::client::PrinterClient::print_progress) tracks it.
    ///   `percent` ramps 0 to 100 and `remaining_secs` counts down. This is one aggregate bar
    ///   across the whole sweep — a single-routine run spans the same full range as a
    ///   multi-routine one, so per-routine progress cannot be derived from it.
    /// - Per-routine boundaries come from
    ///   [`current_stage()`](crate::types::telemetry::PrinterTelemetry::current_stage) and
    ///   [`stage_queue()`](crate::types::telemetry::PrinterTelemetry::stage_queue), which decode
    ///   `stg_cur`/`stg` into [`PrintStage`](crate::types::telemetry::PrintStage). Both wire
    ///   fields arrive in incremental pushes, so this tracks in real time.
    /// - A [`PrintStage::Idle`](crate::types::telemetry::PrintStage::Idle) mid-run is normal:
    ///   after the last queued stage finishes it reads idle for the rest of the run while
    ///   `percent` keeps climbing. Completion is `gcode_state`/`percent`, never the stage.
    /// - A calibration run is distinguishable from a user print by `print_type == "system"`
    ///   with `subtask_name == "auto_cali_for_user_param.gcode"`; `layer_num`/`total_layer_num`
    ///   stay 0 and are meaningless here.
    ///
    /// # Unsupported routines
    ///
    /// The firmware accepts every option bit, acknowledges the command `"result": "success"`,
    /// and silently queues nothing for a routine the hardware doesn't run — so the wire never
    /// reports the skip. This method masks the request against
    /// [`supported_calibration_mask()`](crate::quirks::QuirkStrategy::supported_calibration_mask)
    /// instead of trusting that ack: unsupported bits are dropped with a `log::warn!` and the
    /// remaining routines still run.
    ///
    /// # Errors
    ///
    /// [`Error::ModelMismatch`] when *none* of the requested routines are supported on this
    /// model, since that request would otherwise be a silent no-op reported as success. A
    /// partially-supported request is not an error — it proceeds with whatever the model runs.
    ///
    /// Wire observations are P1S firmware `01.10.00.00`. See `reference/03_mqtt_telemetry.md`
    /// for the wire detail and stage-ID mapping.
    pub async fn start_calibration(&mut self, options: CalibrationOption) -> Result<u16, Error> {
        // The firmware acks unsupported option bits as "success" and silently queues nothing for
        // them, so the wire cannot tell a caller their routine was skipped. Mask against what the
        // model actually runs and refuse only when nothing at all would execute — a partial
        // request still does useful work, so failing it outright would break a caller that ORs in
        // every flag defensively.
        let supported = self.identity.model.quirks().supported_calibration_mask();
        let effective = options.0 & supported;

        if effective == 0 {
            return Err(Error::ModelMismatch(Cow::Borrowed(
                "no requested calibration routine is supported on this model",
            )));
        }
        if effective != options.0 {
            log::warn!(
                "dropping calibration option bits unsupported on this model: {:#08b} (requested {:#08b}, running {:#08b})",
                options.0 & !supported,
                options.0,
                effective
            );
        }

        self.dispatch(|seq| crate::mqtt::CalibrationRequest::new(effective, seq))
            .await
    }

    /// Submits a `.3mf` print job from MicroSD storage for execution [REF-MQTT-LIFECYCLE].
    ///
    /// The model's quirks engine gates `nozzle_offset_cali`: it resolves the default when the
    /// config left it `None`, and forces it off on a single-nozzle model even if the caller set
    /// it explicitly.
    pub async fn start_print(&mut self, config: &PrintJobConfig) -> Result<u16, Error> {
        let model = self.identity.model;
        self.dispatch(|seq| crate::mqtt::ProjectFileRequest::from_config(config, seq, model))
            .await
    }
}
