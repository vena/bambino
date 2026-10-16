//! Listing, downloading and deleting a printer's print files and timelapses, and reading timelapse covers and print files' plate images, whichever service holds them.
//!
//! Only FTPS backs these calls today, so every file is on external storage. The port-6000 file
//! tunnel (#636) will add a second [`FileAddress`] arm for internal storage without changing
//! anything public: a [`PrinterFile`] carries its own address, so a caller never sends one
//! service's path to the other.

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::error::Error;
use crate::file_tunnel::FileKind;
use crate::ftps::client::PrinterClockProbe;
use crate::ftps::parser::{FTP_MAX_LISTING_ENTRIES, advance_timestamp};
use crate::ftps::project::front_members;
use crate::ftps::protocol::FTP_FILE_NOT_FOUND;
use crate::ftps::{CurrentDateTime, FtpFile, FtpTimestamp, FtpsClient};
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};

use super::PrinterClient;
use super::job::plate_member_names;

/// Directories FTPS keeps timelapses in [REF-FTPS-OPS].
const TIMELAPSE_DIRS: &[&str] = &["/timelapse"];
/// Subdirectory of a timelapse's directory holding its cover, `<video name without extension>.jpg`.
const THUMBNAIL_SUBDIR: &str = "thumbnail";
/// Extension of a timelapse cover.
const THUMBNAIL_EXTENSION: &str = "jpg";
/// Directories FTPS keeps print files in, in listing order.
///
/// A slicer's LAN send puts a `.3mf` at the root, and `/cache` holds `.3mf` files too, the one
/// folder the printer's screen shows (P1S, 2026-10-16). ha-bambulab's
/// `Ftps990MediaSource.list_files` lists the same two.
const MODEL_DIRS: &[&str] = &["/cache", "/"];
/// Extension a listed print file must have, compared ignoring ASCII case.
const MODEL_EXTENSION: &[u8] = b".3mf";
/// The throwaway file the printer-clock probe uploads, reads the mtime of, and deletes.
const CLOCK_PROBE_PATH: &str = "/bambino_clock_probe.txt";
/// The probe file's contents.
const CLOCK_PROBE_PAYLOAD: &[u8] = b"bambino clock probe";
/// Earliest year a host clock is taken to be set; an embedded clock that never synced reads 1970.
#[cfg(feature = "std")]
const MIN_HOST_CLOCK_YEAR: i32 = 2000;
/// Reference for a listing with no clock: the last minute of a leap year.
///
/// No entry is later than it, so none rolls back a year, and a Feb 29 entry parses. The years
/// it supplies are never reported: those entries become [`FileTime::NoYear`].
const NO_REFERENCE: CurrentDateTime = CurrentDateTime::new(2024, 12, 31, 23, 59);

/// Where a [`PrinterFile`] is stored.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageKind {
    /// The removable card or USB stick: everything FTPS serves [REF-FTPS-OPS].
    External,
    /// The printer's internal storage, which only the port-6000 file tunnel reaches.
    ///
    /// No listing returns it yet; it arrives with the file tunnel's support in
    /// [`PrinterClient::list_files`] (#636).
    Internal,
}

/// When a [`PrinterFile`] was last modified, in the form the service that listed it gives.
///
/// Every form is the printer's own clock at the time it wrote the file, and Bambu printers in
/// LAN mode often run months behind real time (see [`CurrentDateTime`]).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileTime {
    /// A calendar time from FTPS: a listing's, with no seconds, or an `MDTM` reply's, to the second.
    Calendar {
        /// The time. `hour` and `minute` are 0 when a listing gave a year instead of a time.
        at: FtpTimestamp,
        /// `true` when the listing left the year out and it was rebuilt against a reference clock.
        ///
        /// Always `false` from `MDTM`, which carries an explicit four-digit year.
        ///
        /// The default reference is the host's clock, which can put such a file in the wrong
        /// year. Turn on
        /// [`with_printer_clock_probe`](PrinterClient::with_printer_clock_probe) to rebuild it
        /// against the printer's clock instead.
        year_inferred: bool,
    },
    /// A calendar time from an FTPS listing that left the year out, with no reference clock to rebuild it.
    ///
    /// Happens without `std`, where there is no host clock, or with a host clock that was never
    /// set (one reading before 2000, as an embedded clock that never synced does). Turn on
    /// [`with_printer_clock_probe`](PrinterClient::with_printer_clock_probe) to get the year
    /// from the printer's clock.
    NoYear {
        /// Calendar month (1-12).
        month: u8,
        /// Day of month (1-31).
        day: u8,
        /// Hour (0-23).
        hour: u8,
        /// Minute (0-59).
        minute: u8,
    },
    /// Unix seconds, as the file tunnel reports them.
    Unix(u64),
    /// The listing gave no time.
    Unknown,
}

/// One file from [`PrinterClient::list_files`], [`PrinterClient::list_files_each`] or [`PrinterClient::job_file`].
///
/// Pass it back to [`PrinterClient::download_file`], [`PrinterClient::delete_file`],
/// [`PrinterClient::read_thumbnail`] or [`PrinterClient::read_plate_images`]. It can't be built
/// by hand: it carries the service and address it was listed from, which aren't
/// public, so a file is always reached the way it was found.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterFile {
    /// The file name, without its directory.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// Modification time.
    pub modified: FileTime,
    /// What kind of file this is: the kind the listing asked for.
    pub kind: FileKind,
    /// Where the file is stored.
    pub storage: StorageKind,
    address: FileAddress,
}

/// How the service that listed a [`PrinterFile`] addresses it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum FileAddress {
    /// A full card path over FTPS.
    Ftps { path: String },
}

impl PrinterFile {
    /// The service's own path for the file, which tells apart two listed files with the same name.
    ///
    /// Over FTPS it is the full card path, such as `/cache/job.3mf`. It is for display and
    /// comparison; pass the `PrinterFile` itself back to reach the file.
    #[must_use]
    pub fn path(&self) -> &str {
        match &self.address {
            FileAddress::Ftps { path } => path,
        }
    }

    /// An FTPS listing entry from `dir`, its year rebuilt only when `has_reference`.
    fn from_ftps(dir: &str, file: FtpFile, kind: FileKind, has_reference: bool) -> Self {
        let modified = if file.year_is_inferred && !has_reference {
            FileTime::NoYear {
                month: file.modified.month,
                day: file.modified.day,
                hour: file.modified.hour,
                minute: file.modified.minute,
            }
        } else {
            FileTime::Calendar {
                at: file.modified,
                year_inferred: file.year_is_inferred,
            }
        };
        let path = if dir.ends_with('/') {
            format!("{dir}{}", file.name)
        } else {
            format!("{dir}/{}", file.name)
        };
        Self {
            name: file.name,
            size: file.size,
            modified,
            kind,
            storage: StorageKind::External,
            address: FileAddress::Ftps { path },
        }
    }

    /// A file found at FTPS `path` without a listing, from its `SIZE` and `MDTM` answers.
    pub(super) fn from_ftps_path(
        path: String,
        size: u64,
        modified: FileTime,
        kind: FileKind,
    ) -> Self {
        Self {
            name: String::from(path.rsplit('/').next().unwrap_or(&path)),
            size,
            modified,
            kind,
            storage: StorageKind::External,
            address: FileAddress::Ftps { path },
        }
    }
}

/// A plate's images from the front of a print file's `.3mf`, from [`PrinterClient::read_plate_images`].
#[derive(Debug)]
pub struct PlateImages<'b> {
    /// The 1-based plate read, or `None` when none could be told, and then every image is `None`.
    pub plate: Option<u32>,
    /// `true` when `plate` came from the file's own G-code list rather than the argument.
    pub from_file: bool,
    /// `Metadata/plate_<N>.png`, or `None` when the file has none for this plate.
    pub plate_image: Option<&'b [u8]>,
    /// `Metadata/plate_<N>_small.png`, the 128×128 thumbnail a file browser shows, or `None`
    /// when the file has none for this plate.
    pub small_image: Option<&'b [u8]>,
    /// `Metadata/pick_<N>.png`, or `None` when the file has none for this plate. Decode its
    /// pixels with [`pick_identify_id`](crate::ftps::project::pick_identify_id).
    pub pick_image: Option<&'b [u8]>,
}

impl PlateImages<'_> {
    /// No plate and no images.
    const NONE: Self = Self {
        plate: None,
        from_file: false,
        plate_image: None,
        small_image: None,
        pick_image: None,
    };
}

/// The FTPS path of a timelapse's cover: `<dir>/thumbnail/<name without extension>.jpg`.
fn ftps_thumbnail_path(video_path: &str) -> String {
    let (dir, name) = video_path.rsplit_once('/').unwrap_or(("", video_path));
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    format!("{dir}/{THUMBNAIL_SUBDIR}/{stem}.{THUMBNAIL_EXTENSION}")
}

/// Whether `name` ends in `.3mf`, ignoring ASCII case.
fn is_model_file(name: &str) -> bool {
    let name = name.as_bytes();
    name.len() >= MODEL_EXTENSION.len()
        && name[name.len() - MODEL_EXTENSION.len()..].eq_ignore_ascii_case(MODEL_EXTENSION)
}

/// The host's clock as a listing reference, or `None` when it has none or it was never set.
#[cfg(feature = "std")]
fn host_clock() -> Option<CurrentDateTime> {
    let now = CurrentDateTime::now_utc();
    (now.year >= MIN_HOST_CLOCK_YEAR).then_some(now)
}

/// The host's clock as a listing reference: none without `std`.
#[cfg(not(feature = "std"))]
fn host_clock() -> Option<CurrentDateTime> {
    None
}

/// Reads the printer's clock: uploads [`CLOCK_PROBE_PATH`], reads its mtime with `MDTM`, then deletes it.
///
/// Never fails: a failure is logged and recorded as [`PrinterClockProbe::Failed`]. The delete is
/// attempted whenever the session is still usable, even after a failed upload, which may have
/// left a partial file.
async fn probe_printer_clock<RawIO, Tls, Factory, Timer>(
    ftps: &mut FtpsClient<RawIO, Tls, Factory, Timer>,
) -> PrinterClockProbe
where
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: RawStreamFactory<RawIO>,
    Timer: TimerProvider,
{
    let upload = ftps
        .upload_file(CLOCK_PROBE_PATH, CLOCK_PROBE_PAYLOAD)
        .await;
    let read_at_ms = ftps.timer().now_millis();
    let mdtm = match upload {
        Ok(()) => ftps.modification_time(CLOCK_PROBE_PATH).await,
        Err(e) => Err(e),
    };
    if !ftps.is_poisoned()
        && let Err(e) = ftps.delete_file(CLOCK_PROBE_PATH).await
    {
        log::warn!("Printer clock probe: could not delete {CLOCK_PROBE_PATH}: {e}");
    }
    match mdtm {
        Ok(Some(at)) => PrinterClockProbe::Read { at, read_at_ms },
        Ok(None) => {
            log::warn!(
                "Printer clock probe: this firmware has no MDTM; listing against the host clock"
            );
            PrinterClockProbe::Failed
        }
        Err(e) => {
            log::warn!("Printer clock probe failed ({e}); listing against the host clock");
            PrinterClockProbe::Failed
        }
    }
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
    /// Has [`list_files`](Self::list_files) and [`list_files_each`](Self::list_files_each) read the printer's clock before an FTPS listing, so rebuilt years are the printer's.
    ///
    /// An FTPS listing leaves the year out for recent files, and the year can only be rebuilt
    /// against the printer's clock, which in LAN mode runs months behind and restarts on every
    /// boot. By default a listing uses the host's clock and flags each rebuilt year
    /// ([`FileTime::Calendar`]'s `year_inferred`), or reports [`FileTime::NoYear`] without one.
    ///
    /// When on, the first listing of each FTPS session uploads a throwaway file
    /// (`/bambino_clock_probe.txt`), reads its mtime with `MDTM`, and deletes it. The reading is
    /// kept for that session, advanced by the time since, and dropped when the session ends,
    /// since a reboot both ends the session and resets the clock. If the probe fails (no `MDTM`,
    /// a full or read-only card, any error), that session uses the default and the failure is
    /// logged; the listing itself goes ahead.
    ///
    /// Off by default because it writes to the card as a side effect of a listing: the file is
    /// briefly visible on the printer's screen, and every new FTPS session probes again, including
    /// the one after a stopped download ends a session. It runs only inside `list_files` and
    /// `list_files_each`, on the session the listing uses.
    #[must_use]
    pub fn with_printer_clock_probe(mut self, enabled: bool) -> Self {
        self.core.ftps_clock_probe = enabled;
        self
    }

    /// Lists the printer's files of one kind.
    ///
    /// Served over FTPS, so only external storage (card or USB stick) is listed; internal storage
    /// arrives with the port-6000 file tunnel (#636). One `LIST` per directory below, never
    /// recursive, so a file in a folder of your own isn't listed:
    ///
    /// - [`FileKind::Timelapse`]: the files in `/timelapse`, not its `thumbnail` covers; read a
    ///   cover with [`read_thumbnail`](Self::read_thumbnail).
    /// - [`FileKind::Model`]: the `.3mf` files in `/cache`, then at the card root. A name can
    ///   appear in both; [`PrinterFile::path`] tells them apart.
    /// - [`FileKind::Video`]: always empty. A P1S's `/ipcam` holds only an `index` file, and no
    ///   source shows recorded videos elsewhere on the card.
    ///
    /// **Which clock rebuilds a missing year.** A listing leaves the year out for recent files.
    /// By default it is rebuilt against the host's clock (with `std`) and flagged
    /// ([`FileTime::Calendar`]'s `year_inferred`); without `std`, or with a host clock that was
    /// never set (before 2000), it isn't rebuilt ([`FileTime::NoYear`]). The printer's clock is
    /// the right reference and the host's can put a file in the wrong year: turn on
    /// [`with_printer_clock_probe`](Self::with_printer_clock_probe) to read it, at the cost of an
    /// upload, an `MDTM` and a delete once per FTPS session.
    ///
    /// Returns at most [`FTP_MAX_LISTING_ENTRIES`] files. To place a listing's memory yourself,
    /// use [`list_files_each`](Self::list_files_each).
    ///
    /// # Errors
    ///
    /// [`Error::NotConfigured`] without FTPS, and whatever
    /// [`FtpsClient::list_directory`] returns.
    pub async fn list_files(&mut self, kind: FileKind) -> Result<Vec<PrinterFile>, Error> {
        let mut files = Vec::new();
        let mut capped = false;
        self.list_files_each(kind, |file| {
            if files.len() < FTP_MAX_LISTING_ENTRIES {
                files.push(file);
            } else if !capped {
                capped = true;
                log::warn!(
                    "more than {FTP_MAX_LISTING_ENTRIES} {kind:?} files; truncating the listing"
                );
            }
        })
        .await?;
        Ok(files)
    }

    /// Lists the printer's files of one kind like [`list_files`](Self::list_files), handing each file to `on_file` as it is listed.
    ///
    /// Nothing is collected, so the caller decides where files live, and can keep only the ones
    /// it needs. Directories, filters, order and clock reference are `list_files`'. No entry cap
    /// applies; every listed file is delivered.
    ///
    /// # Errors
    ///
    /// As for `list_files`. **Files delivered before an error are not a complete listing;
    /// discard them.** That includes files from `/cache` when listing the card root then fails,
    /// for [`FileKind::Model`].
    pub async fn list_files_each(
        &mut self,
        kind: FileKind,
        mut on_file: impl FnMut(PrinterFile),
    ) -> Result<(), Error> {
        let (dirs, keep): (&[&str], fn(&FtpFile) -> bool) = match kind {
            FileKind::Timelapse => (TIMELAPSE_DIRS, |f| !f.is_dir),
            FileKind::Model => (MODEL_DIRS, |f| !f.is_dir && is_model_file(&f.name)),
            FileKind::Video => return Ok(()),
        };
        let reference = self.listing_reference().await?;
        let ftps = self.ftps().await?;
        for &dir in dirs {
            ftps.list_directory_each(dir, reference.unwrap_or(NO_REFERENCE), |file| {
                if keep(&file) {
                    on_file(PrinterFile::from_ftps(dir, file, kind, reference.is_some()));
                }
            })
            .await?;
        }
        Ok(())
    }

    /// Downloads a listed file, passing each read to `sink` in order, and returns its size.
    ///
    /// Checked against the printer's `SIZE` once the transfer ends, so treat what `sink` received
    /// as provisional until this returns `Ok`. **An error from `sink` ends the FTPS session**
    /// (see [`FtpsClient::download_file_each`]); the next FTPS call reconnects.
    pub async fn download_file(
        &mut self,
        file: &PrinterFile,
        sink: impl FnMut(&[u8]) -> Result<(), Error>,
    ) -> Result<u64, Error> {
        match &file.address {
            FileAddress::Ftps { path } => self.ftps().await?.download_file_each(path, sink).await,
        }
    }

    /// Deletes a listed file. Deleting one that is already gone succeeds.
    pub async fn delete_file(&mut self, file: &PrinterFile) -> Result<(), Error> {
        match &file.address {
            FileAddress::Ftps { path } => self.ftps().await?.delete_file(path).await,
        }
    }

    /// Reads a listed timelapse's cover image, passing each read to `sink`, and returns its size, or `None` when it has none.
    ///
    /// Over FTPS the cover is `thumbnail/<video name without extension>.jpg` beside the video,
    /// as ha-bambulab's `_download_timelapse_thumbnail` reads it (P1S: one `.jpg` per `.avi`).
    /// The file tunnel (#636) will read it as Bambu Studio does, through the video's
    /// `#thumbnail` member. Only a [`FileKind::Timelapse`] file has a cover; any other kind
    /// returns `None` without a request. A print file's images are inside its `.3mf`.
    ///
    /// A cover the printer refuses with `550` before sending anything is `None`; FTPS can't tell
    /// a missing file from an unreadable one. A `550` after bytes reached `sink` (from the
    /// closing `SIZE` check) is an error, not `None`. The read and its `sink` behave as
    /// [`download_file`](Self::download_file)'s.
    pub async fn read_thumbnail(
        &mut self,
        file: &PrinterFile,
        mut sink: impl FnMut(&[u8]) -> Result<(), Error>,
    ) -> Result<Option<u64>, Error> {
        if file.kind != FileKind::Timelapse {
            return Ok(None);
        }
        match &file.address {
            FileAddress::Ftps { path } => {
                let cover = ftps_thumbnail_path(path);
                let mut delivered = false;
                let read = self
                    .ftps()
                    .await?
                    .download_file_each(&cover, |chunk| {
                        delivered = true;
                        sink(chunk)
                    })
                    .await;
                match read {
                    Ok(size) => Ok(Some(size)),
                    Err(Error::FtpReply {
                        code: FTP_FILE_NOT_FOUND,
                        ..
                    }) if !delivered => Ok(None),
                    Err(e) => Err(e),
                }
            }
        }
    }

    /// Reads one plate's image, thumbnail and pick image from the front of a print file's `.3mf`, into `buf`.
    ///
    /// `Some(n)` reads plate `n`'s images. `None` takes the plate from the file's own list of
    /// plate G-code when it holds G-code for exactly one plate, and otherwise returns no plate
    /// and no images: the images of a guessed plate would look right and be wrong. Either way the
    /// plate is never taken from the running job: for its images, use
    /// [`read_running_job_images`](Self::read_running_job_images).
    ///
    /// Only a [`FileKind::Model`] file has plate images; any other kind returns none without a
    /// request.
    ///
    /// One FTPS read, which usually ends the session: see
    /// [`FtpsClient::download_prefix`]. `buf` needs room for the file's front section and, when
    /// the plate isn't given, its list of plate G-code after it: about 30–55 KB on the files
    /// measured.
    ///
    /// # Errors
    ///
    /// As [`FtpsClient::read_front_members`] and [`FtpsClient::read_job_prefix`], plus whatever
    /// connecting FTPS returns.
    pub async fn read_plate_images<'b>(
        &mut self,
        file: &PrinterFile,
        plate: Option<u32>,
        buf: &'b mut [u8],
    ) -> Result<PlateImages<'b>, Error> {
        if file.kind != FileKind::Model {
            return Ok(PlateImages::NONE);
        }
        match &file.address {
            FileAddress::Ftps { path } => {
                if let Some(plate) = plate {
                    let names = plate_member_names(plate);
                    let [plate_image, small_image, pick_image] = self
                        .ftps()
                        .await?
                        .read_front_members(path, buf, names.each_ref().map(String::as_str))
                        .await?;
                    return Ok(PlateImages {
                        plate: Some(plate),
                        from_file: false,
                        plate_image,
                        small_image,
                        pick_image,
                    });
                }
                let prefix = self.ftps().await?.read_job_prefix(path, buf).await?;
                let [plate] = prefix.gcode_plates.as_slice() else {
                    return Ok(PlateImages::NONE);
                };
                let names = plate_member_names(*plate);
                let [plate_image, small_image, pick_image] =
                    front_members(prefix.bytes, names.each_ref().map(String::as_str))?;
                Ok(PlateImages {
                    plate: Some(*plate),
                    from_file: true,
                    plate_image,
                    small_image,
                    pick_image,
                })
            }
        }
    }

    /// The reference clock an FTPS listing rebuilds missing years against, or `None` for none.
    async fn listing_reference(&mut self) -> Result<Option<CurrentDateTime>, Error> {
        if !self.core.ftps_clock_probe {
            return Ok(host_clock());
        }
        let ftps = self.ftps().await?;
        if ftps.printer_clock == PrinterClockProbe::NotRun {
            ftps.printer_clock = probe_printer_clock(ftps).await;
            if ftps.is_poisoned() {
                // The probe ended the session. List on a fresh one, without probing it again:
                // a printer whose session the probe breaks would otherwise never list.
                self.ftps().await?.printer_clock = PrinterClockProbe::Failed;
            }
        }
        let ftps = self.ftps().await?;
        Ok(match ftps.printer_clock {
            PrinterClockProbe::Read { at, read_at_ms } => {
                let timer = ftps.timer();
                let elapsed_secs = if timer.has_real_clock() {
                    timer.now_millis().saturating_sub(read_at_ms) / 1000
                } else {
                    0
                };
                Some(advance_timestamp(at, elapsed_secs).into())
            }
            PrinterClockProbe::NotRun | PrinterClockProbe::Failed => host_clock(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_model_file() {
        assert!(is_model_file("job.gcode.3mf"));
        assert!(is_model_file("JOB.3MF"));
        assert!(!is_model_file("job.gcode"));
        assert!(!is_model_file("3mf"));
        assert!(!is_model_file("é.3m"));
    }

    #[test]
    fn test_ftps_thumbnail_path() {
        assert_eq!(
            ftps_thumbnail_path("/timelapse/video_2025-12-15_11-47-18.avi"),
            "/timelapse/thumbnail/video_2025-12-15_11-47-18.jpg"
        );
        assert_eq!(
            ftps_thumbnail_path("/timelapse/a.b.mp4"),
            "/timelapse/thumbnail/a.b.jpg"
        );
    }
}
