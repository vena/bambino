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
//! that reads from the printer goes in [`read_running_job_images`], in priority order beside the
//! FTPS read, and if it only works on some models, it is gated by a `ModelQuirks` method, never
//! a model match.
//!
//! **Two routes to the images.** The FTPS route finds the job's file with [`job_file`] and reads
//! it with [`read_plate_images`], passing [`job_plate`]'s plate, so the running job's plate only
//! ever applies to the running job's file. [`read_running_job_images`] first asks the port-6000
//! file tunnel for the running job's members by name ([REF-FILESVC]), which needs no path and
//! reaches internal storage FTPS can't, and falls back to the FTPS route. The tunnel route needs
//! the plate from [`job_plate`]: it reads one plate's members and cannot tell which plate is
//! printing.
//!
//! [`job_plate`]: super::PrinterClient::job_plate
//! [`job_file`]: super::PrinterClient::job_file
//! [`read_plate_images`]: super::PrinterClient::read_plate_images
//! [`read_running_job_images`]: super::PrinterClient::read_running_job_images

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use core::ops::Range;

use serde::Deserialize;

use crate::error::Error;
use crate::file_tunnel::FileKind;
use crate::ftps::project::{
    pick_image_member, plate_image_member, plate_of_gcode_param, plate_small_image_member,
};
use crate::ftps::protocol::FTP_FILE_NOT_FOUND;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};
use crate::types::control::PrintStatus;
use crate::types::telemetry::PrinterTelemetry;

use super::PrinterClient;
use super::files::{FileTime, PrinterFile};

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

/// A running job's plate and its images, from [`PrinterClient::read_running_job_images`].
#[derive(Debug)]
pub struct JobImages<'b> {
    /// The plate the images belong to; `None` when no source could tell, and then every image
    /// is `None` too.
    pub plate: Option<JobPlate>,
    /// The job's file the images were read from on the FTPS route; `None` on the file tunnel
    /// route, which reads the running job's members without learning its file, and when no
    /// file was found.
    pub file: Option<PrinterFile>,
    /// `Metadata/plate_<N>.png`, or `None` when the file has none for this plate.
    pub plate_image: Option<&'b [u8]>,
    /// `Metadata/plate_<N>_small.png`, the 128×128 thumbnail, or `None` when the file has none
    /// for this plate.
    pub small_image: Option<&'b [u8]>,
    /// `Metadata/pick_<N>.png`, or `None` when the file has none for this plate. Decode its
    /// pixels with [`pick_identify_id`](crate::ftps::project::pick_identify_id).
    pub pick_image: Option<&'b [u8]>,
}

/// A `project_file` start command the printer echoed, kept for the job it started.
#[derive(Debug, Clone)]
pub(crate) struct JobStart {
    /// Last path segment of the echoed `file` (or `url`), matched against `gcode_file`'s.
    file: String,
    /// The card path an `ftp://` `url` names; `None` for any other scheme or no `url`.
    path: Option<String>,
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

/// The `url` scheme of a start command whose file is on the card (`ftp://<name>`, the card root).
const FTP_URL_SCHEME: &str = "ftp://";
/// The extension of a job file `job_file` accepts from a bare `gcode_file` name.
const PROJECT_FILE_EXTENSION: &str = ".3mf";

/// The names of plate `plate`'s image, thumbnail and pick image, in that order.
pub(super) fn plate_member_names(plate: u32) -> [String; 3] {
    [
        plate_image_member(plate),
        plate_small_image_member(plate),
        pick_image_member(plate),
    ]
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
    /// client connected; [`read_running_job_images`](Self::read_running_job_images) then falls
    /// back to the job's file. See the module doc for the chain.
    pub fn job_plate(&self) -> Option<JobPlate> {
        let cache = &self.core.cache;
        if let Some(plate) = cache.last_plate_idx {
            return Some(JobPlate {
                plate,
                source: PlateSource::Telemetry,
            });
        }
        self.running_job_start().map(|start| JobPlate {
            plate: start.plate,
            source: PlateSource::StartCommand,
        })
    }

    /// The recorded start command, when it started the job now running on this connection.
    fn running_job_start(&self) -> Option<&JobStart> {
        let cache = &self.core.cache;
        let start = cache.last_job_start.as_ref()?;
        let running = cache.last_gcode_file.as_deref().map(last_segment)?;
        (start.generation == self.core.connection_generation && start.file == running)
            .then_some(start)
    }

    /// Returns the running job's `.3mf` on the printer's card, or `None` when it can't be found exactly.
    ///
    /// The path comes from the `url` of the start command the printer echoed for this job, when
    /// this client saw one, and otherwise from `/` followed by `gcode_file`. A `SIZE` request
    /// confirms the file is there and gives its size; a file that isn't is `None`, not an error.
    ///
    /// That second rule covers every file Bambu Studio and OrcaSlicer send, which go to the card
    /// root (`url: "ftp://<name>"`), and a print started from the printer's screen of a file at
    /// the root. **A file placed in a folder by hand** and printed from the screen reports only
    /// its bare name (the P1S screen lists every folder but `cache` flattened into one list), so
    /// it gives `None`, unless a *different* file of the same name sits at the root: that one is
    /// returned, and its images are not the job's. `reference/02_ftps.md` [REF-FTPS-OPS] has the
    /// capture. A `gcode_file` that is a path rather than a bare `.3mf` name, as X1 firmware's
    /// `/data/Metadata/plate_1.gcode` on internal storage, is `None`.
    ///
    /// After `SIZE`, an `MDTM` request gives the file's modification time with an explicit year
    /// ([`FileTime::Calendar`] with `year_inferred: false`), or [`FileTime::Unknown`] on firmware
    /// without `MDTM`. No directory is listed. Pass the result to
    /// [`read_plate_images`](Self::read_plate_images) or any other call taking a [`PrinterFile`].
    ///
    /// # Errors
    ///
    /// Whatever connecting FTPS or the `SIZE` and `MDTM` requests return, except the `550` of a
    /// missing file from `SIZE`.
    pub async fn job_file(&mut self) -> Result<Option<PrinterFile>, Error> {
        let path = match self.running_job_start() {
            Some(start) => start.path.clone(),
            None => self
                .core
                .cache
                .last_gcode_file
                .as_deref()
                .filter(|name| {
                    !name.contains('/')
                        && name.len() > PROJECT_FILE_EXTENSION.len()
                        && name
                            .get(name.len() - PROJECT_FILE_EXTENSION.len()..)
                            .is_some_and(|ext| ext.eq_ignore_ascii_case(PROJECT_FILE_EXTENSION))
                })
                .map(|name| format!("/{name}")),
        };
        let Some(path) = path else {
            return Ok(None);
        };
        // Both requests are control-channel only and come before any front-of-file read, which
        // usually ends the session.
        let ftps = self.ftps().await?;
        let size = match ftps.get_file_size(&path).await {
            Ok(size) => size,
            Err(Error::FtpReply {
                code: FTP_FILE_NOT_FOUND,
                ..
            }) => return Ok(None),
            Err(e) => return Err(e),
        };
        let modified = match ftps.modification_time(&path).await? {
            Some(at) => FileTime::Calendar {
                at,
                year_inferred: false,
            },
            None => FileTime::Unknown,
        };
        Ok(Some(PrinterFile::from_ftps_path(
            path,
            size,
            modified,
            FileKind::Model,
        )))
    }

    /// Reads the running job's plate images into `buf`, through the file tunnel where the printer serves it and over FTPS otherwise.
    ///
    /// The tunnel route asks the printer for `Metadata/plate_<N>.png`,
    /// `Metadata/plate_<N>_small.png` and `Metadata/pick_<N>.png` of the running job by name
    /// (`mem:/16`, the read Bambu Studio's skip-objects dialog makes), so it needs no file path
    /// and reaches internal storage. It is taken when
    /// [`Capabilities::file_tunnel_support`](super::Capabilities::file_tunnel_support) allows,
    /// `.with_file_tunnel()` is configured, and [`job_plate`](Self::job_plate) knows the plate.
    /// The session is disconnected afterwards, so no TLS session lingers; that includes a session
    /// the caller had opened with [`file_tunnel()`](Self::file_tunnel). This route never learns
    /// the job's file, so [`JobImages::file`] is `None`.
    ///
    /// The FTPS route ([`job_file`](Self::job_file), then
    /// [`read_plate_images`](Self::read_plate_images) with [`job_plate`](Self::job_plate)'s
    /// plate) is used otherwise, and also when the tunnel fails to connect, fails mid-read, or has
    /// none of the images. A failed connect is remembered until the next MQTT connection, so a
    /// printer that ignores the tunnel costs its 8 s handshake timeout once rather than on every
    /// call. Without a plate from `job_plate`, the plate comes from the file when it holds G-code
    /// for exactly one plate ([`PlateSource::ProjectFile`]), and otherwise every image is `None`.
    /// With no job file on the card either, every image is `None`.
    ///
    /// The tunnel route has not been run against a printer that serves the tunnel; see
    /// `reference/08_file_service.md`.
    ///
    /// # Errors
    ///
    /// Those of the FTPS route, when it runs.
    pub async fn read_running_job_images<'b>(
        &mut self,
        buf: &'b mut [u8],
    ) -> Result<JobImages<'b>, Error> {
        let plate = self.job_plate();
        if let Some(plate) = plate
            && self.file_tunnel_worth_trying()
            && let Some(ranges) = self.read_tunnel_job_images(plate.plate, buf).await
            && ranges.iter().any(Option::is_some)
        {
            let buf: &'b [u8] = buf;
            let [plate_image, small_image, pick_image] = ranges.map(|r| r.map(|r| &buf[r]));
            return Ok(JobImages {
                plate: Some(plate),
                file: None,
                plate_image,
                small_image,
                pick_image,
            });
        }
        let Some(file) = self.job_file().await? else {
            return Ok(JobImages {
                plate,
                file: None,
                plate_image: None,
                small_image: None,
                pick_image: None,
            });
        };
        let images = self
            .read_plate_images(&file, plate.map(|p| p.plate), buf)
            .await?;
        let plate = if images.from_file {
            images.plate.map(|plate| JobPlate {
                plate,
                source: PlateSource::ProjectFile,
            })
        } else {
            plate
        };
        Ok(JobImages {
            plate,
            file: Some(file),
            plate_image: images.plate_image,
            small_image: images.small_image,
            pick_image: images.pick_image,
        })
    }

    /// Whether a routed operation should try the file tunnel before FTPS.
    fn file_tunnel_worth_trying(&self) -> bool {
        self.capabilities().file_tunnel_support().is_supported()
            && self.camera_config.is_some()
            && self.core.file_tunnel_failed_generation != Some(self.core.connection_generation)
    }

    /// Reads plate `plate`'s images through the file tunnel into `buf`, returning where each landed, or `None` when the tunnel couldn't be used.
    ///
    /// A member the printer refuses is `None` in the result: Studio treats every refusal of this
    /// read as "no such file". The client cancels the refused stream; if that fails the session
    /// is poisoned, and the next member reconnects.
    ///
    /// Studio is only known to ask `mem:/16` for `pick_<N>.png` (`reference/08_file_service.md`);
    /// asking for `plate_<N>.png` and `plate_<N>_small.png` the same way is inferred, so a refusal
    /// of either is an ordinary `None`.
    async fn read_tunnel_job_images(
        &mut self,
        plate: u32,
        buf: &mut [u8],
    ) -> Option<[Option<Range<usize>>; 3]> {
        let names = plate_member_names(plate);
        let mut ranges = [None, None, None];
        let mut used = 0;
        for (slot, name) in ranges.iter_mut().zip(names.iter()) {
            if let Err(e) = self.ensure_file_tunnel().await {
                log::debug!("file tunnel unavailable, using FTPS: {e}");
                self.core.file_tunnel_failed_generation = Some(self.core.connection_generation);
                self.disconnect_file_tunnel().await;
                return None;
            }
            let tunnel = self
                .file_tunnel
                .as_mut()
                .expect("ensure_file_tunnel() just connected the file tunnel");
            match tunnel
                .read_running_job_member_into(&self.timer, name, &mut buf[used..])
                .await
            {
                Ok(len) => {
                    *slot = Some(used..used + len);
                    used += len;
                }
                Err(Error::TunnelReply { result, .. }) => {
                    log::debug!("file tunnel has no {name} for the running job (result {result})");
                }
                Err(e) => {
                    log::debug!("file tunnel read of {name} failed, using FTPS: {e}");
                    self.disconnect_file_tunnel().await;
                    return None;
                }
            }
        }
        self.disconnect_file_tunnel().await;
        Some(ranges)
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
        let path = fields
            .url
            .as_deref()
            .and_then(|url| url.strip_prefix(FTP_URL_SCHEME))
            .map(|rest| format!("/{}", rest.trim_start_matches('/')));
        self.core.cache.last_job_start = Some(JobStart {
            file: String::from(last_segment(file)),
            path,
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
