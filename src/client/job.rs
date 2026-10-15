//! # Running job's plate and plate images
//!
//! A job's `.3mf` keys its plate and pick images on the plate number, and on P1/A1 printers
//! telemetry never says which plate is printing (`reference/03_mqtt_telemetry.md`, "P1 and A1
//! printers don't send `plate_idx`"). A wrong number gives a valid-looking wrong image and wrong
//! skip-objects ids, so the plate is resolved through a chain of sources, most exact first, and
//! is `None` when none of them knows. It is never a default.
//!
//! The chain, in order:
//!
//! 1. [`PlateSource::Telemetry`]: `plate_idx`, which H2/X1/P2S/X2D firmware sends.
//! 2. [`PlateSource::StartCommand`]: the plate in the `project_file` start command the printer
//!    echoed on the report topic, while this client was connected. Covers prints this client
//!    started, and those another client started while it was listening.
//! 3. [`PlateSource::ProjectFile`]: the job's `.3mf` holding G-code for exactly one plate
//!    ([`gcode_plates`](crate::ftps::project::gcode_plates)), read over FTPS.
//!
//! **Adding a source.** Give it a [`PlateSource`] variant (the enum is `#[non_exhaustive]`, so
//! that is not a breaking change). A source that needs no I/O goes in [`job_plate`]'s chain. One
//! that reads from the printer goes in [`read_job_images`], in priority order beside the FTPS
//! read, and if it only works on some models, it is gated by a `ModelQuirks` method, never a
//! model match. The port-6000 file service (GitHub issue #633) would be such a source: the
//! printer resolves the running job itself there, so it needs no path and reaches internal
//! storage FTPS can't.
//!
//! [`job_plate`]: super::PrinterClient::job_plate
//! [`read_job_images`]: super::PrinterClient::read_job_images

#[cfg(not(feature = "std"))]
use alloc::string::String;

use serde::Deserialize;

use crate::error::Error;
use crate::ftps::project::{
    front_members, pick_image_member, plate_image_member, plate_of_gcode_param,
};
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};
use crate::types::control::PrintStatus;
use crate::types::telemetry::PrinterTelemetry;

use super::PrinterClient;

/// Where a running job's plate number came from; see the module doc for the chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PlateSource {
    /// Telemetry's `plate_idx`.
    Telemetry,
    /// The `param` of the `project_file` start command the printer echoed for this job.
    StartCommand,
    /// The job's `.3mf`, which holds G-code for this plate only.
    ProjectFile,
}

/// A running job's plate number and the source that gave it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobPlate {
    /// The 1-based plate number, as the `.3mf`'s `Metadata/plate_<N>.png` names it.
    pub plate: u32,
    /// Which link of the chain answered.
    pub source: PlateSource,
}

/// A running job's plate and its images, from [`PrinterClient::read_job_images`].
#[derive(Debug)]
pub struct JobImages<'b> {
    /// The plate the images belong to; `None` when no source could tell, and then both images
    /// are `None` too.
    pub plate: Option<JobPlate>,
    /// `Metadata/plate_<N>.png`, or `None` when the file has none for this plate.
    pub plate_image: Option<&'b [u8]>,
    /// `Metadata/pick_<N>.png`, or `None` when the file has none for this plate. Decode its
    /// pixels with [`pick_identify_id`](crate::ftps::project::pick_identify_id).
    pub pick_image: Option<&'b [u8]>,
}

/// A `project_file` start command the printer echoed, kept for the job it started.
#[derive(Debug, Clone)]
pub(crate) struct JobStart {
    /// Last path segment of the echoed `file` (or `url`), matched against `gcode_file`'s.
    file: String,
    plate: u32,
    /// `PrinterClient::connection_generation` when it was seen: a job can end and another start
    /// while disconnected, so a start from an earlier connection is not trusted.
    generation: u32,
}

/// The parts of a `project_file` echo the chain reads.
#[derive(Deserialize)]
struct StartEcho {
    print: StartEchoFields,
}

#[derive(Deserialize)]
struct StartEchoFields {
    param: Option<String>,
    file: Option<String>,
    url: Option<String>,
}

/// The names of plate `plate`'s image and pick image, in that order.
fn plate_member_names(plate: u32) -> [String; 2] {
    [plate_image_member(plate), pick_image_member(plate)]
}

/// The last `/`-separated segment of a path or URL.
fn last_segment(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

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
    /// Returns the running job's plate from the sources that need no file read, or `None` when none of them knows it.
    ///
    /// Reads cached telemetry only. `None` is common on P1/A1 for a print started before this
    /// client connected; [`read_job_images`](Self::read_job_images) then falls back to the
    /// job's file. See the module doc for the chain.
    pub fn job_plate(&self) -> Option<JobPlate> {
        let cache = &self.core.cache;
        if let Some(plate) = cache.last_plate_idx {
            return Some(JobPlate {
                plate,
                source: PlateSource::Telemetry,
            });
        }
        let start = cache.last_job_start.as_ref()?;
        let running = cache.last_gcode_file.as_deref().map(last_segment)?;
        (start.generation == self.core.connection_generation && start.file == running).then_some(
            JobPlate {
                plate: start.plate,
                source: PlateSource::StartCommand,
            },
        )
    }

    /// Reads the running job's plate and pick images from the start of its `.3mf` at `remote_path`, into `buf`.
    ///
    /// The plate comes from [`job_plate`](Self::job_plate) when it knows, and otherwise from the
    /// file itself, in the same read: a file holding G-code for exactly one plate names it.
    /// With no answer from either, the result's `plate` and both images are `None`: the images
    /// of a guessed plate would look right and be wrong.
    ///
    /// `remote_path` is the job's file on the printer's storage. For a print sent over LAN to a
    /// P1S it was `/` followed by `gcode_file` (`reference/02_ftps.md`); mapping a job to its
    /// file in general is GitHub issue #630.
    ///
    /// One FTPS read, which usually ends the session: see
    /// [`FtpsClient::download_prefix`](crate::ftps::FtpsClient::download_prefix). `buf` needs room
    /// for the file's front section and, when the plate isn't known beforehand, its list of
    /// plate G-code after it: about 30–55 KB on the files measured.
    ///
    /// # Errors
    ///
    /// As [`FtpsClient::read_front_members`](crate::ftps::FtpsClient::read_front_members) and
    /// [`FtpsClient::read_job_prefix`](crate::ftps::FtpsClient::read_job_prefix), plus whatever
    /// connecting FTPS returns.
    pub async fn read_job_images<'b>(
        &mut self,
        remote_path: &str,
        buf: &'b mut [u8],
    ) -> Result<JobImages<'b>, Error> {
        if let Some(plate) = self.job_plate() {
            let names = plate_member_names(plate.plate);
            let [plate_image, pick_image] = self
                .ftps()
                .await?
                .read_front_members(remote_path, buf, [names[0].as_str(), names[1].as_str()])
                .await?;
            return Ok(JobImages {
                plate: Some(plate),
                plate_image,
                pick_image,
            });
        }

        let prefix = self.ftps().await?.read_job_prefix(remote_path, buf).await?;
        let [plate] = prefix.gcode_plates.as_slice() else {
            return Ok(JobImages {
                plate: None,
                plate_image: None,
                pick_image: None,
            });
        };
        let names = plate_member_names(*plate);
        let [plate_image, pick_image] =
            front_members(prefix.bytes, [names[0].as_str(), names[1].as_str()])?;
        Ok(JobImages {
            plate: Some(JobPlate {
                plate: *plate,
                source: PlateSource::ProjectFile,
            }),
            plate_image,
            pick_image,
        })
    }

    /// Records an accepted `project_file` echo as the start of the job it names.
    pub(super) fn record_job_start(&mut self, payload: &[u8]) {
        let Ok(echo) = serde_json::from_slice::<StartEcho>(payload) else {
            return;
        };
        let fields = echo.print;
        let (Some(plate), Some(file)) = (
            fields.param.as_deref().and_then(plate_of_gcode_param),
            fields.file.as_deref().or(fields.url.as_deref()),
        ) else {
            return;
        };
        self.core.cache.last_job_start = Some(JobStart {
            file: String::from(last_segment(file)),
            plate,
            generation: self.core.connection_generation,
        });
    }

    /// Updates the cached job identity a telemetry frame carries.
    ///
    /// A new `gcode_file` drops a `plate_idx` cached for the previous file, and the end of a
    /// job drops its recorded start, so neither can describe the next job.
    pub(super) fn update_job_cache(&mut self, print: &PrinterTelemetry) {
        let cache = &mut self.core.cache;
        if let Some(file) = &print.gcode_file
            && cache.last_gcode_file.as_ref() != Some(file)
        {
            cache.last_plate_idx = None;
            cache.last_gcode_file = Some(file.clone());
        }
        if let Some(plate) = print.plate_idx.and_then(|p| u32::try_from(p).ok())
            && plate > 0
        {
            cache.last_plate_idx = Some(plate);
        }
        let ended = print
            .gcode_state
            .as_deref()
            .map(PrintStatus::from_gcode_state)
            .is_some_and(|s| matches!(s, PrintStatus::Finished | PrintStatus::Failed));
        let was_busy = cache
            .last_gcode_state
            .as_deref()
            .map(PrintStatus::from_gcode_state)
            .is_some_and(PrintStatus::is_busy);
        if ended && was_busy {
            cache.last_job_start = None;
        }
    }
}
