#![cfg(feature = "cli")]

//! # MicroSD Storage Traversal and File Transfer Subcommand
//!
//! Routes storage filesystem commands through `PrinterClient` with lazy FTPS
//! connection via `.with_ftps()`. The FTPS session is established on first use.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use bambino::Error;
use bambino::PrinterModel;
use bambino::ams::{AmsMappingResult, MaterialSource};
use bambino::client::{
    CommandOutcome, FileKind, FileTime, ModelCheck, PrintStatus, PrinterClient, PrinterFile,
    StorageKind, UploadSource,
};
use bambino::ftps::CurrentDateTime;
use bambino::ftps::project::{
    PlateFilament, front_members, pick_image_member, plate_image_member, read_plate_filaments,
    read_sliced_for,
};
use bambino::io::TokioIo;
use bambino::io::tokio::TokioTlsConnector;
use bambino::io::tokio::{TokioRawStreamFactory, TokioTimer};
use bambino::mqtt::PrintJobConfig;
use bambino::resolve_model;
use clap::{Subcommand, ValueEnum};

use crate::connection::{RESPONSE_TIMEOUT_SECS, Target, unix_now_secs};
use crate::error::CliError;
use crate::trust::build_cli_tls_connector;

const BYTES_PER_KIB: u64 = 1024;
const BYTES_PER_MIB: u64 = 1024 * BYTES_PER_KIB;
const BYTES_PER_GIB: u64 = 1024 * BYTES_PER_MIB;

#[derive(Subcommand, Debug)]
pub enum FilesAction {
    /// Perform a UNIX directory listing traversal
    #[command(override_usage = "bambino-cli files <IP> <SERIAL> [ACCESS_CODE] list [REMOTE_PATH]")]
    List {
        #[arg(default_value = "/")]
        remote_path: String,
    },
    /// Upload a local file to the remote card path
    #[command(
        override_usage = "bambino-cli files <IP> <SERIAL> [ACCESS_CODE] upload <LOCAL_PATH> <REMOTE_PATH>"
    )]
    Upload {
        local_path: String,
        remote_path: String,
    },
    /// Download a remote card file to a local path, writing it as it arrives
    ///
    /// The file is written to LOCAL_PATH.part and renamed to LOCAL_PATH once the download
    /// succeeds; a failed download leaves LOCAL_PATH as it was. --stop-after BYTES stops the
    /// transfer once that many bytes are written and keeps the partial file. The printer then
    /// closes the FTPS session, so the next command reconnects.
    #[command(
        override_usage = "bambino-cli files <IP> <SERIAL> [ACCESS_CODE] download <REMOTE_PATH> <LOCAL_PATH> [--stop-after <BYTES>]"
    )]
    Download {
        remote_path: String,
        local_path: String,
        #[arg(long, value_name = "BYTES")]
        stop_after: Option<u64>,
    },
    /// Download only the first BYTES of a remote card file, then stop the transfer
    ///
    /// Stopping early ends the FTPS session (the printer closes it), so this is one command per
    /// connection. A file shorter than BYTES is downloaded whole.
    #[command(
        override_usage = "bambino-cli files <IP> <SERIAL> [ACCESS_CODE] head <REMOTE_PATH> <LOCAL_PATH> [BYTES]"
    )]
    Head {
        remote_path: String,
        local_path: String,
        #[arg(default_value_t = 65536)]
        bytes: usize,
    },
    /// Read a plate's image and pick image from the front of a job's .3mf, then stop the transfer
    ///
    /// Writes plate_<PLATE>.png and pick_<PLATE>.png into OUT_DIR. BYTES caps how much of the
    /// file may be read. Stopping early ends the FTPS session, as with head.
    #[command(
        override_usage = "bambino-cli files <IP> <SERIAL> [ACCESS_CODE] front <REMOTE_PATH> <PLATE> <OUT_DIR> [BYTES]"
    )]
    Front {
        remote_path: String,
        plate: u32,
        out_dir: String,
        #[arg(default_value_t = 262_144)]
        bytes: usize,
    },
    /// Find which plate a job's .3mf holds G-code for, and read that plate's images
    ///
    /// Reads the start of the file only. Exactly one plate with G-code is the plate a print of
    /// this file runs; its plate_<N>.png and pick_<N>.png go into OUT_DIR. Several, or none,
    /// write nothing. Stopping early ends the FTPS session, as with head.
    #[command(
        override_usage = "bambino-cli files <IP> <SERIAL> [ACCESS_CODE] job <REMOTE_PATH> <OUT_DIR> [BYTES]"
    )]
    Job {
        remote_path: String,
        out_dir: String,
        #[arg(default_value_t = 262_144)]
        bytes: usize,
    },
    /// Create a directory on the remote card
    #[command(override_usage = "bambino-cli files <IP> <SERIAL> [ACCESS_CODE] mkdir <REMOTE_PATH>")]
    Mkdir { remote_path: String },
    /// Remove an empty directory from the remote card
    #[command(override_usage = "bambino-cli files <IP> <SERIAL> [ACCESS_CODE] rmdir <REMOTE_PATH>")]
    Rmdir { remote_path: String },
    /// Remove a file from the remote filesystem path
    #[command(
        override_usage = "bambino-cli files <IP> <SERIAL> [ACCESS_CODE] delete <REMOTE_PATH>"
    )]
    Delete { remote_path: String },
    /// Uploads a tiny probe file, diffs its printer-reported mtime against host wall-clock
    /// time, then deletes it — checks whether the printer's onboard clock is usably accurate.
    ///
    /// Confirmed unreliable on ESP32/FreeRTOS-class printers (e.g. P1S) — no RTC
    /// battery, and absent a successful LAN-mode NTP sync (observed unreliable), the clock
    /// restarts from a fixed base on every boot (on a P1S, about two months before its
    /// firmware's release; reference/02_ftps.md, "Absolute Timestamps via MDTM").
    /// `list_directory`'s year-rollover math can't be trusted whenever this is the case, since
    /// the raw `LIST` HH:MM timestamps come from the printer's own wrong clock. Unconfirmed on
    /// printers with more capable AP controllers (X1/H2 series).
    #[command(override_usage = "bambino-cli files <IP> <SERIAL> [ACCESS_CODE] clock-check")]
    ClockCheck,
    /// Query available MicroSD card capacity
    #[command(override_usage = "bambino-cli files <IP> <SERIAL> [ACCESS_CODE] space")]
    Space,
    /// List, download or delete print files or timelapses by kind, through PrinterClient::list_files
    ///
    /// Without get, thumb or rm, lists the files of KIND, by path. get, thumb and rm take a name
    /// or, where a name is in both /cache and the root, a path. --probe-clock reads the
    /// printer's clock first (an upload, MDTM and delete of /bambino_clock_probe.txt), so a year
    /// the listing leaves out is rebuilt against the printer's clock rather than the host's.
    #[command(
        override_usage = "bambino-cli files <IP> <SERIAL> [ACCESS_CODE] catalog <KIND> [--probe-clock]\n       bambino-cli files <IP> <SERIAL> [ACCESS_CODE] catalog <KIND> get <NAME> <LOCAL_PATH> [--stop-after <BYTES>]\n       bambino-cli files <IP> <SERIAL> [ACCESS_CODE] catalog <KIND> rm <NAME>\n       bambino-cli files <IP> <SERIAL> [ACCESS_CODE] catalog timelapse thumb <NAME> <LOCAL_PATH>"
    )]
    Catalog {
        kind: CatalogKind,
        /// Read the printer's clock before listing (with_printer_clock_probe)
        #[arg(long)]
        probe_clock: bool,
        #[command(subcommand)]
        op: Option<CatalogOp>,
    },
    /// Derive an AMS mapping for a local .3mf from the printer's loaded trays, and optionally print it
    ///
    /// Reads plate PLATE's filaments from the start and end of LOCAL_3MF (the file is never
    /// loaded whole), asks the printer for its state, and matches each filament to a loaded tray
    /// the way BambuStudio does. Prints a table of the matches and the `ams_mapping2` array.
    /// Without --print the only message sent to the printer is a status request (`pushall`).
    ///
    /// With --print the file is uploaded to the printer's card in chunks under its own name and
    /// the job is started with the derived mapping. This starts a real print, so it asks for
    /// confirmation first. It refuses if the printer is busy, and if any filament found no
    /// loaded tray unless --allow-unmatched is given: on a single-nozzle printer a mapping with
    /// nothing matched prints from the external spool.
    ///
    /// The table is headed by the printer type id the file was sliced for and the printer's own
    /// model. With --print it also refuses a file sliced for a model the printer doesn't accept,
    /// or that records none, unless --allow-other-model is given: an A1 file's start G-code drove
    /// a P1S's carriage into the frame (#657).
    #[command(
        override_usage = "bambino-cli files <IP> <SERIAL> [ACCESS_CODE] map <LOCAL_3MF> [--plate <N>] [--print --bed-type <TYPE>] [--allow-unmatched] [--allow-other-model]"
    )]
    Map {
        /// The sliced .3mf on this machine; its file name is the name on the printer
        local_3mf: String,
        /// The plate to map and print, counted from 1
        #[arg(long, default_value_t = 1, value_name = "N", value_parser = clap::value_parser!(u32).range(1..))]
        plate: u32,
        /// Upload the file and start the job after deriving the mapping
        #[arg(long, requires = "bed_type")]
        print: bool,
        /// Bed plate type sent with the job, passed through unchecked
        ///
        /// Bambu Studio sends the plate the file was sliced for: textured_plate, hot_plate (smooth
        /// PEI), cool_plate, eng_plate or supertack_plate. bambuddy sends auto.
        #[arg(long, value_name = "TYPE", requires = "print")]
        bed_type: Option<String>,
        /// With --print, start the job even if some filament matched no loaded tray
        #[arg(long, requires = "print")]
        allow_unmatched: bool,
        /// With --print, start the job even if the file was sliced for another printer model
        ///
        /// Its start G-code can drive the carriage into the frame: an A1 file did on a P1S (#657).
        #[arg(long, requires = "print")]
        allow_other_model: bool,
    },
}

/// Which files `catalog` works on.
#[derive(Clone, Copy, ValueEnum, Debug)]
pub enum CatalogKind {
    /// Print files (.3mf)
    Model,
    /// Timelapse videos
    Timelapse,
    /// Recorded videos (none over FTPS)
    Video,
}

impl From<CatalogKind> for FileKind {
    fn from(kind: CatalogKind) -> Self {
        match kind {
            CatalogKind::Model => FileKind::Model,
            CatalogKind::Timelapse => FileKind::Timelapse,
            CatalogKind::Video => FileKind::Video,
        }
    }
}

/// What `catalog` does with one listed file.
#[derive(Subcommand, Debug)]
pub enum CatalogOp {
    /// Download a listed file, writing it as it arrives
    ///
    /// The file is written to LOCAL_PATH.part and renamed to LOCAL_PATH once the download
    /// succeeds or --stop-after stops it; a failed download leaves LOCAL_PATH as it was.
    Get {
        /// The file's name or path, as the listing shows it
        name: String,
        local_path: String,
        /// Stop once this many bytes are written; the printer then closes the FTPS session
        #[arg(long, value_name = "BYTES")]
        stop_after: Option<u64>,
    },
    /// Download a listed timelapse's cover image
    Thumb {
        /// The timelapse's name or path, as the listing shows it
        name: String,
        local_path: String,
    },
    /// Delete a listed file
    Rm {
        /// The file's name or path, as the listing shows it
        name: String,
    },
}

/// `PrinterClient` as `run` configures it, with FTPS.
type FtpsPrinter = PrinterClient<
    TokioIo<::tokio::net::TcpStream>,
    TokioTlsConnector,
    TokioRawStreamFactory,
    TokioTimer,
    TokioIo<::tokio::net::TcpStream>,
    TokioTlsConnector,
    TokioRawStreamFactory,
    TokioTimer,
>;

/// Writes a download to `<LOCAL_PATH>.part` as it arrives, stopping at `--stop-after` if given.
///
/// The file takes `LOCAL_PATH`'s name only once the download succeeds or `--stop-after` stops it,
/// so a download that fails (a missing remote file, a broken transfer, a `SIZE` mismatch) never
/// replaces a file already at `LOCAL_PATH`. The library leaves this choice to its consumer: the
/// sink is where the bytes go, and this is the CLI's.
struct LocalSink {
    local_path: String,
    part_path: String,
    /// Opened on the first chunk, so a request that sends nothing creates nothing.
    out: Option<fs::File>,
    written: u64,
    stop_after: Option<u64>,
    stopped: bool,
    write_error: Option<std::io::Error>,
}

/// How a [`LocalSink`] download ended, once its file is in place.
enum Outcome {
    /// The whole file, this many bytes, is at `LOCAL_PATH`.
    Written(u64),
    /// `--stop-after` stopped it; this many bytes are at `LOCAL_PATH`.
    Stopped(u64),
    /// There was nothing to download, and nothing was written.
    Nothing,
}

impl LocalSink {
    fn new(local_path: &str, stop_after: Option<u64>) -> Self {
        Self {
            local_path: local_path.to_string(),
            part_path: format!("{local_path}.part"),
            out: None,
            written: 0,
            stop_after,
            stopped: false,
            write_error: None,
        }
    }

    /// The download sink: refuses the next read once `stop_after` bytes are written.
    fn accept(&mut self, chunk: &[u8]) -> Result<(), Error> {
        if self.stop_after.is_some_and(|limit| self.written >= limit) {
            self.stopped = true;
            return Err(Error::InvalidState("--stop-after reached".into()));
        }
        let written = match &mut self.out {
            Some(out) => out.write_all(chunk),
            None => fs::File::create(&self.part_path).and_then(|mut out| {
                let written = out.write_all(chunk);
                self.out = Some(out);
                written
            }),
        };
        if let Err(e) = written {
            self.write_error = Some(e);
            return Err(Error::InvalidState("local write failed".into()));
        }
        self.written += chunk.len() as u64;
        Ok(())
    }

    /// Moves the file into place or removes it, given the download's result (`None`: nothing to download).
    fn finish(mut self, result: Result<Option<u64>, Error>) -> Result<Outcome, CliError> {
        let had_part = self.out.take().is_some();
        let discard = |part_path: &str| {
            if had_part {
                let _ = fs::remove_file(part_path);
            }
        };
        if let Some(e) = self.write_error {
            discard(&self.part_path);
            return Err(e.into());
        }
        let outcome = match result {
            _ if self.stopped => Outcome::Stopped(self.written),
            Ok(Some(n)) => Outcome::Written(n),
            Ok(None) => {
                discard(&self.part_path);
                return Ok(Outcome::Nothing);
            }
            Err(e) => {
                discard(&self.part_path);
                return Err(e.into());
            }
        };
        if had_part {
            fs::rename(&self.part_path, &self.local_path)?;
        } else {
            // An empty file, or a stop before the first chunk.
            fs::File::create(&self.local_path)?;
        }
        Ok(outcome)
    }
}

/// Prints how a download ended; `session_closed` is whether FTPS ended with it.
fn report_download(outcome: Outcome, local_path: &str, session_closed: bool) {
    match outcome {
        Outcome::Written(n) => println!("Success: {n} bytes written to '{local_path}'."),
        Outcome::Stopped(n) => {
            println!("Stopped after {n} bytes (--stop-after); '{local_path}' is partial.");
            if session_closed {
                println!("The data channel was reset; the printer has closed this FTPS session.");
            } else {
                println!("Unexpected: the FTPS session is still usable after the stop.");
            }
        }
        Outcome::Nothing => println!("Nothing to download; nothing written."),
    }
}

/// Prints how long a transfer call took, from the command through the final `226` and any
/// `SIZE` check the call makes.
///
/// Exists to measure whether the `226` arrives promptly after the data channel's TLS
/// `close_notify` (GitHub issue #322): a server waiting on a reply to that alert would show
/// here as a transfer that takes most of the control-channel read deadline.
fn print_transfer_time(command: &str, started: Instant) {
    println!(
        "{command} completed in {:.2}s",
        started.elapsed().as_secs_f64()
    );
}

/// Dispatches a typed storage action over FTPS.
pub async fn run(
    target: &Target,
    action: FilesAction,
    allow_unverified_tls_1_2: bool,
) -> Result<(), CliError> {
    let printer = target.printer()?;
    let model = printer.model();

    let ftps_tls = build_cli_tls_connector(model.quirks().ftps_tls_versions())?;

    let mut printer = printer
        .with_ftps(ftps_tls, TokioRawStreamFactory, TokioTimer::new())
        .with_ftps_allow_unverified_tls_1_2(allow_unverified_tls_1_2);

    // `map` is MQTT-first and opens FTPS only to upload, so it skips the FTPS login below.
    let action = match action {
        FilesAction::Map {
            local_3mf,
            plate,
            print,
            bed_type,
            allow_unmatched,
            allow_other_model,
        } => {
            let result = run_map(
                &mut printer,
                &target.ip,
                &local_3mf,
                plate,
                bed_type.filter(|_| print),
                allow_unmatched,
                allow_other_model,
            )
            .await;
            let _ = printer.disconnect_ftps().await;
            printer.disconnect_mqtt().await;
            return result;
        }
        other => other,
    };

    println!(
        "Connecting to implicitly secure FTPS server at {} port {}...",
        target.ip,
        bambino::ftps::FTPS_PORT
    );

    let action = match action {
        FilesAction::Catalog {
            kind,
            probe_clock,
            op,
        } => {
            let mut printer = printer.with_printer_clock_probe(probe_clock);
            let result = run_catalog(&mut printer, kind.into(), op).await;
            printer.disconnect_ftps().await;
            return result;
        }
        other => other,
    };

    let client = printer.ftps().await?;

    println!("FTPS connection authenticated. Executing operational action...\n");

    // Every arm below used to propagate via `?` directly out of `run()`, bypassing
    // the `disconnect_ftps()` at the bottom on any error — skipping FTPS's graceful `QUIT`
    // on every failure path except the empty-listing early return. Capturing the dispatch
    // result in a variable instead lets `disconnect_ftps()` run unconditionally before the error
    // (if any) is propagated.
    let result: Result<(), CliError> = async {
        match action {
            FilesAction::List { remote_path } => {
                let now = CurrentDateTime::now_utc();

                println!("Traversing remote files on directory '{}'...", remote_path);
                let started = Instant::now();
                let files = client
                    .list_directory(&remote_path, now)
                    .await?;
                print_transfer_time("LIST", started);

                if files.is_empty() {
                    println!("Directory is empty or path does not exist.");
                } else {
                    print_file_listing_table(&remote_path, &files);
                }
            }
            FilesAction::Upload {
                local_path,
                remote_path,
            } => {
                let local = Path::new(&local_path);
                let metadata = fs::metadata(local)?;

                // The library's own transfer cap, checked before reading the file in: anything
                // larger couldn't be downloaded again.
                let max = bambino::ftps::MAX_TRANSFER_BYTES as u64;
                if metadata.len() > max {
                    return Err(CliError::InvalidArgs(format!(
                        "File too large for upload: {} (max {})",
                        format_size(metadata.len()),
                        format_size(max)
                    )));
                }

                println!("Reading source file '{}' into buffer...", local_path);
                let payload = fs::read(local)?;

                println!(
                    "Uploading file ({} bytes) to remote path '{}'...",
                    payload.len(),
                    remote_path
                );
                println!(
                    "Note: Under heavy write latency, standard SD card flushing may require up to 300 seconds [REF-FTPS-FLUSH]."
                );
                let started = Instant::now();
                client.upload_file(&remote_path, &payload).await?;
                print_transfer_time("STOR", started);

                println!(
                    "Success: File uploaded and non-volatile write-buffers successfully flushed."
                );
            }
            FilesAction::Download {
                remote_path,
                local_path,
                stop_after,
            } => {
                println!("Downloading remote file '{}'...", remote_path);
                let mut sink = LocalSink::new(&local_path, stop_after);
                let started = Instant::now();
                let result = client
                    .download_file_each(&remote_path, |chunk| sink.accept(chunk))
                    .await;
                print_transfer_time("RETR", started);
                let outcome = sink.finish(result.map(Some))?;
                report_download(outcome, &local_path, client.is_poisoned());
            }
            FilesAction::Head {
                remote_path,
                local_path,
                bytes,
            } => {
                if bytes == 0 || bytes > bambino::ftps::MAX_TRANSFER_BYTES {
                    return Err(CliError::InvalidArgs(format!(
                        "BYTES must be between 1 and {}",
                        bambino::ftps::MAX_TRANSFER_BYTES
                    )));
                }
                println!("Reading the first {bytes} bytes of '{remote_path}'...");
                let mut buf = vec![0u8; bytes];
                let started = Instant::now();
                let n = client.download_prefix(&remote_path, &mut buf).await?;
                print_transfer_time("RETR (prefix)", started);
                fs::write(&local_path, &buf[..n])?;
                println!("Success: {n} bytes written to '{local_path}'.");
                if n == bytes {
                    println!("Transfer stopped early; the printer has closed this FTPS session.");
                }
            }
            FilesAction::Front {
                remote_path,
                plate,
                out_dir,
                bytes,
            } => {
                if bytes == 0 || bytes > bambino::ftps::MAX_TRANSFER_BYTES {
                    return Err(CliError::InvalidArgs(format!(
                        "BYTES must be between 1 and {}",
                        bambino::ftps::MAX_TRANSFER_BYTES
                    )));
                }
                let names = [plate_image_member(plate), pick_image_member(plate)];
                println!("Reading {} and {} from '{remote_path}'...", names[0], names[1]);
                let mut buf = vec![0u8; bytes];
                let base = buf.as_ptr() as usize;
                let started = Instant::now();
                let found = client
                    .read_front_members(&remote_path, &mut buf, [names[0].as_str(), names[1].as_str()])
                    .await?;
                print_transfer_time("RETR (front members)", started);
                for (name, member) in names.iter().zip(found) {
                    let file_name = name.trim_start_matches("Metadata/");
                    match member {
                        Some(data) => {
                            let end = data.as_ptr() as usize - base + data.len();
                            let local = Path::new(&out_dir).join(file_name);
                            fs::write(&local, data)?;
                            println!(
                                "{name}: {} bytes, ends at file offset {end}, written to '{}'",
                                data.len(),
                                local.display()
                            );
                        }
                        None => println!("{name}: not in the file's front section"),
                    }
                }
                if client.is_poisoned() {
                    println!("Transfer stopped early; the printer has closed this FTPS session.");
                }
            }
            FilesAction::Job {
                remote_path,
                out_dir,
                bytes,
            } => {
                if bytes == 0 || bytes > bambino::ftps::MAX_TRANSFER_BYTES {
                    return Err(CliError::InvalidArgs(format!(
                        "BYTES must be between 1 and {}",
                        bambino::ftps::MAX_TRANSFER_BYTES
                    )));
                }
                println!("Reading the plate G-code list of '{remote_path}'...");
                let mut buf = vec![0u8; bytes];
                let started = Instant::now();
                let prefix = client.read_job_prefix(&remote_path, &mut buf).await?;
                print_transfer_time("RETR (job prefix)", started);
                println!(
                    "Read {} bytes; plates with G-code: {:?}",
                    prefix.bytes.len(),
                    prefix.gcode_plates
                );
                match prefix.gcode_plates.as_slice() {
                    [plate] => {
                        let names = [plate_image_member(*plate), pick_image_member(*plate)];
                        let found = front_members(
                            prefix.bytes,
                            [names[0].as_str(), names[1].as_str()],
                        )?;
                        for (name, member) in names.iter().zip(found) {
                            let file_name = name.trim_start_matches("Metadata/");
                            match member {
                                Some(data) => {
                                    let local = Path::new(&out_dir).join(file_name);
                                    fs::write(&local, data)?;
                                    println!(
                                        "{name}: {} bytes, written to '{}'",
                                        data.len(),
                                        local.display()
                                    );
                                }
                                None => println!("{name}: not in the file's front section"),
                            }
                        }
                    }
                    _ => println!("Not exactly one plate: the file can't say which one prints."),
                }
                if client.is_poisoned() {
                    println!("Transfer stopped early; the printer has closed this FTPS session.");
                }
            }
            FilesAction::Mkdir { remote_path } => {
                client.create_directory(&remote_path).await?;
                println!("Success: directory '{remote_path}' created.");
            }
            FilesAction::Rmdir { remote_path } => {
                client.remove_directory(&remote_path).await?;
                println!("Success: directory '{remote_path}' removed.");
            }
            FilesAction::Delete { remote_path } => {
                println!("Deleting file from remote path '{}'...", remote_path);
                client.delete_file(&remote_path).await?;
                println!("Success: Target file successfully removed.");
            }
            FilesAction::ClockCheck => {
                run_clock_check(client).await?;
            }
            FilesAction::Space => {
                println!("Querying hardware storage space evaluations...");
                let space_bytes = client.get_available_space().await?;
                println!("\nStorage Capacity Status:");
                println!("  - Free Space (Bytes) : {}", space_bytes);
                println!("  - Free Space         : {}\n", format_size(space_bytes));
            }
            // Dispatched to `run_catalog` above, before this session was opened.
            FilesAction::Catalog { .. } => unreachable!("catalog runs through run_catalog"),
            FilesAction::Map { .. } => unreachable!("map runs through run_map"),
        }
        Ok(())
    }
    .await;

    let _ = printer.disconnect_ftps().await;
    result
}

/// Lists files of `kind` through `PrinterClient::list_files`, then downloads or deletes one by name if `op` says to.
async fn run_catalog(
    printer: &mut FtpsPrinter,
    kind: FileKind,
    op: Option<CatalogOp>,
) -> Result<(), CliError> {
    let started = Instant::now();
    let files = printer.list_files(kind).await?;
    print_transfer_time("list_files", started);
    // A path names one file; a name may name one in /cache and one at the root.
    let find = |name: &str| -> Result<&PrinterFile, CliError> {
        if let Some(file) = files.iter().find(|f| f.path() == name) {
            return Ok(file);
        }
        let mut named = files.iter().filter(|f| f.name == name);
        match (named.next(), named.next()) {
            (Some(file), None) => Ok(file),
            (Some(_), Some(_)) => Err(CliError::InvalidArgs(format!(
                "more than one {kind:?} file is named '{name}'; pass its path instead"
            ))),
            (None, _) => Err(CliError::InvalidArgs(format!(
                "no {kind:?} file named '{name}'"
            ))),
        }
    };
    match op {
        None if files.is_empty() => println!("No {kind:?} files."),
        None => print_catalog_table(&files),
        Some(CatalogOp::Get {
            name,
            local_path,
            stop_after,
        }) => {
            let file = find(&name)?.clone();
            println!("Downloading '{name}'...");
            let mut sink = LocalSink::new(&local_path, stop_after);
            let started = Instant::now();
            let result = printer
                .download_file(&file, |chunk| sink.accept(chunk))
                .await;
            print_transfer_time("download_file", started);
            let outcome = sink.finish(result.map(Some))?;
            report_download(outcome, &local_path, !printer.is_ftps_connected());
        }
        Some(CatalogOp::Thumb { name, local_path }) => {
            let file = find(&name)?.clone();
            let mut sink = LocalSink::new(&local_path, None);
            let result = printer
                .read_thumbnail(&file, |chunk| sink.accept(chunk))
                .await;
            match sink.finish(result)? {
                Outcome::Nothing => println!("'{name}' has no cover image."),
                outcome => report_download(outcome, &local_path, !printer.is_ftps_connected()),
            }
        }
        Some(CatalogOp::Rm { name }) => {
            let file = find(&name)?.clone();
            printer.delete_file(&file).await?;
            println!("Success: '{name}' deleted.");
        }
    }
    Ok(())
}

/// Prints a `catalog` listing; a rebuilt year is marked `*`, a missing one `????`.
fn print_catalog_table(files: &[PrinterFile]) {
    let mut table = crate::table::Table::new(vec!["Size", "Modified", "Storage", "Path"]);
    let mut any_inferred = false;
    for file in files {
        let modified = match file.modified {
            FileTime::Calendar { at, year_inferred } => {
                any_inferred |= year_inferred;
                format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}{}",
                    at.year,
                    at.month,
                    at.day,
                    at.hour,
                    at.minute,
                    if year_inferred { " *" } else { "" }
                )
            }
            FileTime::NoYear {
                month,
                day,
                hour,
                minute,
            } => format!("????-{month:02}-{day:02} {hour:02}:{minute:02}"),
            FileTime::Unix(secs) => format!("unix {secs}"),
            _ => String::from("-"),
        };
        let storage = format!("{:?}", file.storage);
        table.add_row(vec![
            &format_size(file.size),
            &modified,
            &storage,
            file.path(),
        ]);
    }
    table.print();
    if any_inferred {
        println!(
            "\n* year rebuilt against a reference clock: the printer's with --probe-clock when \
             its probe succeeds, otherwise the host's."
        );
    }
    println!();
}

/// Uploads a tiny probe file, diffs its printer-reported mtime against host UTC wall-clock
/// time, then deletes it. Factored out of `run()`'s `ClockCheck` arm to keep that match
/// readable — see `FilesAction::ClockCheck`'s doc comment for why this exists:
/// LAN-mode NTP sync is unreliable, so `list_directory`'s year-rollover math can't be trusted
/// without checking the printer's clock first.
async fn run_clock_check(client: &mut bambino::io::tokio::TokioFtpsClient) -> Result<(), Error> {
    const PROBE_PATH: &str = "/bambino_clock_probe.txt";
    let payload = b"bambino clock probe";

    println!("Uploading clock probe file to '{}'...", PROBE_PATH);
    let before = time::OffsetDateTime::now_utc();
    client.upload_file(PROBE_PATH, payload).await?;
    let after = time::OffsetDateTime::now_utc();

    // Ask MDTM first. When the firmware implements it, the reply carries an explicit four-digit
    // year, so the printer's clock is read outright instead of reconstructed against ours, which
    // is the whole ambiguity this check exists to expose. A failure here isn't fatal: the LIST
    // path below is both the fallback and what a no-MDTM firmware is stuck with anyway.
    let mdtm = client.modification_time(PROBE_PATH).await;

    let now = CurrentDateTime::now_utc();
    let listing = client.list_directory("/", now).await;

    // Always attempt cleanup, even if the listing failed — don't leave the probe file behind
    // on the printer's storage.
    let delete_result = client.delete_file(PROBE_PATH).await;

    let files = match listing {
        Ok(files) => files,
        Err(listing_err) => {
            // The listing error takes priority (it's why the check failed), but
            // don't let a simultaneous cleanup-delete failure go completely unreported —
            // the probe file would otherwise be silently left behind on the printer's
            // storage with no operator-visible signal.
            if let Err(delete_err) = delete_result {
                eprintln!(
                    "Warning: failed to clean up probe file after listing error: {delete_err}"
                );
            }
            return Err(listing_err);
        }
    };
    let probe = files.iter().find(|f| f.name == "bambino_clock_probe.txt");

    println!("\nHost UTC time before upload : {}", format_utc(before));
    println!("Host UTC time after upload   : {}", format_utc(after));

    match &mdtm {
        Ok(Some(t)) => println!(
            "Printer MDTM mtime (absolute): {:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            t.year, t.month, t.day, t.hour, t.minute, t.second
        ),
        Ok(None) => println!("Printer MDTM mtime (absolute): not implemented by this firmware"),
        Err(e) => println!("Printer MDTM mtime (absolute): query failed ({e})"),
    }

    match probe {
        Some(f) => {
            println!(
                "Printer LIST mtime{}: {:04}-{:02}-{:02} {:02}:{:02}",
                if f.year_is_inferred {
                    " (year reconstructed)"
                } else {
                    " (year from wire)  "
                },
                f.modified.year,
                f.modified.month,
                f.modified.day,
                f.modified.hour,
                f.modified.minute
            );
        }
        None => println!(
            "\nCould not locate the probe file in the directory listing — upload may have \
             failed, or the listing raced the SD card flush."
        ),
    }

    // Report the delta against MDTM when it's available: it's the only reading here whose year
    // didn't come from this host's clock, so a delta computed from it is the only one that can
    // legitimately span years. Falling back to the LIST entry keeps the check useful on firmware
    // without MDTM, but that delta is capped at a year by construction; see `CurrentDateTime`.
    match (&mdtm, probe) {
        (Ok(Some(t)), _) => report_clock_delta(after, t.year, t.month, t.day, t.hour, t.minute),
        (_, Some(f)) => {
            report_clock_delta(
                after,
                f.modified.year,
                f.modified.month,
                f.modified.day,
                f.modified.hour,
                f.modified.minute,
            );
            if f.year_is_inferred {
                println!(
                    "Note: MDTM was unavailable, so the year above was reconstructed from this \
                     host's clock. A printer more than a year out of sync cannot be detected \
                     as such by this fallback."
                );
            }
        }
        (_, None) => {}
    }

    delete_result?;
    println!("\nProbe file removed.");
    Ok(())
}

/// Prints the delta between `after` (host UTC at upload completion) and the printer's
/// reported mtime, plus a warning if it exceeds a day (see `run_clock_check`).
fn report_clock_delta(
    after: time::OffsetDateTime,
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
) {
    let Some(printer_dt) = printer_mtime_as_utc(year, month, day, hour, minute) else {
        println!("\nCould not interpret the printer-reported mtime as a valid date.");
        return;
    };
    let delta = after - printer_dt;
    let (days, hours, minutes) = split_duration(delta);
    println!(
        "\nDelta (host UTC - printer)   : {} day(s), {} hour(s), {} minute(s) {}",
        days.abs(),
        hours.abs(),
        minutes.abs(),
        // `delta` is host-minus-printer: positive means the host is later, i.e. the printer's
        // clock lags. These two arms were swapped, which reported a P1S running months stale
        // as "ahead of host", the exact reading this check exists to make legible.
        if delta.is_negative() {
            "ahead of host"
        } else {
            "behind host"
        }
    );
    if days.abs() > 0 {
        println!(
            "Printer clock is off by more than a day — its onboard clock is not usably \
             synced; don't trust `list_directory`'s inferred \
             year near a calendar boundary."
        );
    }
}

/// Formats an `OffsetDateTime` as `YYYY-MM-DD HH:MM:SS UTC` for `run_clock_check`'s output.
fn format_utc(dt: time::OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        dt.year(),
        dt.month() as u8,
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    )
}

/// Interprets a `list_directory`-reported `(year, month, day, hour, minute)` tuple as a UTC
/// `OffsetDateTime`, returning `None` if the components don't form a valid calendar date
/// (e.g. an out-of-range month from a malformed/corrupted `LIST` reply).
fn printer_mtime_as_utc(
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
) -> Option<time::OffsetDateTime> {
    let month = time::Month::try_from(month).ok()?;
    let date = time::Date::from_calendar_date(year, month, day).ok()?;
    let time = time::Time::from_hms(hour, minute, 0).ok()?;
    Some(date.with_time(time).assume_utc())
}

/// Splits a `time::Duration` into whole days/hours/minutes components (each signed, matching
/// the sign of `duration`) for `run_clock_check`'s human-readable delta report.
fn split_duration(duration: time::Duration) -> (i64, i64, i64) {
    let days = duration.whole_days();
    let hours = duration.whole_hours() % 24;
    let minutes = duration.whole_minutes() % 60;
    (days, hours, minutes)
}

/// Renders `files list`'s output table. Factored out of `run()`'s `List` arm to keep that
/// match readable — see `FilesAction::List`'s `year_is_inferred` handling for why
/// this isn't a trivial print loop: every row whose year was inferred (not reported by the
/// printer) gets marked, since a per-row plausibility threshold can never actually detect
/// printer clock skew (see `FtpFile::year_is_inferred`'s doc comment).
fn print_file_listing_table(remote_path: &str, files: &[bambino::ftps::FtpFile]) {
    println!("\nDirectory listing: {}\n", remote_path);
    let mut table = crate::table::Table::new(vec!["Type", "Size", "Modified", "Name"]);
    let mut any_inferred = false;
    for file in files {
        let type_str = if file.is_dir { "DIR" } else { "FILE" };
        let size_str = if file.is_dir {
            String::from("-")
        } else {
            format_size(file.size)
        };
        let inferred_marker = if file.year_is_inferred {
            any_inferred = true;
            " *"
        } else {
            ""
        };
        let modified_str = format!(
            "{:04}-{:02}-{:02} {:02}:{:02}{}",
            file.modified.year,
            file.modified.month,
            file.modified.day,
            file.modified.hour,
            file.modified.minute,
            inferred_marker
        );
        table.add_row(vec![type_str, &size_str, &modified_str, &file.name]);
    }
    table.print();
    if any_inferred {
        println!(
            "\n* year inferred from host clock, not reported by the printer — its onboard \
             clock may be unsynced; run `files clock-check` to \
             verify."
        );
    }
    println!();
}

/// A local file read by offset as an [`UploadSource`], so an upload sends it in chunks without loading it.
struct FileSource {
    file: fs::File,
    len: u64,
    /// The file's name, for read errors.
    name: String,
}

impl FileSource {
    fn open(path: &Path) -> std::io::Result<Self> {
        let file = fs::File::open(path)?;
        let len = file.metadata()?.len();
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        Ok(Self { file, len, name })
    }
}

impl UploadSource for FileSource {
    fn len(&self) -> u64 {
        self.len
    }

    async fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, Error> {
        read_file_at(&self.file, offset, buf).map_err(|e| {
            Error::InvalidState(format!("reading {} at offset {offset}: {e}", self.name).into())
        })
    }
}

/// Reads up to `buf.len()` bytes of `file` starting at `offset`; a read at or past the end returns 0.
#[cfg(unix)]
fn read_file_at(file: &fs::File, offset: u64, buf: &mut [u8]) -> std::io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, buf, offset)
}

/// Reads up to `buf.len()` bytes of `file` starting at `offset`; a read at or past the end returns 0.
#[cfg(not(unix))]
fn read_file_at(file: &fs::File, offset: u64, buf: &mut [u8]) -> std::io::Result<usize> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = file;
    file.seek(SeekFrom::Start(offset))?;
    file.read(buf)
}

/// A file's `printer_model_id` as an operator reads it: the id and the model it names when this crate knows it.
fn describe_sliced_for(printer_model_id: Option<&str>) -> String {
    match printer_model_id {
        None => String::from("(no printer_model_id in the file)"),
        Some(id) => match resolve_model("", Some(id)) {
            PrinterModel::Unknown => format!("{id} (unrecognized model)"),
            model => format!("{id} ({model})"),
        },
    }
}

/// Where a matched filament will be fed from, as an operator reads it: "AMS 0 slot 2", "AMS-HT 128", "external left".
fn tray_label(source: MaterialSource) -> String {
    match source {
        MaterialSource::StandardAms { ams_id, slot_id } => format!("AMS {ams_id} slot {slot_id}"),
        MaterialSource::AmsHt { ams_id } => format!("AMS-HT {ams_id}"),
        MaterialSource::AmsLite { slot_id } => format!("AMS Lite slot {slot_id}"),
        MaterialSource::ExternalSpool => String::from("external spool"),
        MaterialSource::ExternalSpoolLeft => String::from("external left"),
        MaterialSource::ExternalSpoolRight => String::from("external right"),
        MaterialSource::Unmapped => String::from("unmapped"),
    }
}

/// Prints one row per filament: what the file asks for and the tray it matched, or "NO MATCH".
fn print_mapping_table(filaments: &[PlateFilament], result: &AmsMappingResult) {
    let mut table = crate::table::Table::new(vec![
        "Filament",
        "Type",
        "Colour",
        "tray_info_idx",
        "Matched tray",
        "Distance",
    ]);
    for (filament, found) in filaments.iter().zip(&result.matches) {
        let id = filament.id.to_string();
        let idx = if filament.tray_info_idx.is_empty() {
            "-"
        } else {
            &filament.tray_info_idx
        };
        let (tray, distance) = match (found.source, found.distance) {
            (Some(source), Some(distance)) => (tray_label(source), format!("{distance:.1}")),
            (Some(source), None) => (tray_label(source), String::from("-")),
            (None, _) => (String::from("NO MATCH"), String::from("-")),
        };
        table.add_row(vec![
            &id,
            &filament.filament_type,
            &filament.color,
            idx,
            &tray,
            &distance,
        ]);
    }
    table.print();
}

/// Refuses unless the printer is known not to be busy: no state yet, an unrecognized one, or a busy one is a refusal.
fn refuse_if_busy(status: Option<PrintStatus>) -> Result<(), CliError> {
    match status {
        Some(status) if status.is_busy() => Err(CliError::Other(format!(
            "printer is busy (gcode_state={status:?}); not starting a print"
        ))),
        Some(PrintStatus::Unknown) | None => Err(CliError::Other(format!(
            "no recognized gcode_state received within {RESPONSE_TIMEOUT_SECS}s of a pushall; \
             cannot confirm the printer is idle, not starting a print"
        ))),
        Some(_) => Ok(()),
    }
}

/// Polls telemetry until the cache holds AMS data (and a print state, if `need_state`) or `RESPONSE_TIMEOUT_SECS` pass; returns whether AMS data arrived.
async fn wait_for_ams(printer: &mut FtpsPrinter, need_state: bool) -> Result<bool, CliError> {
    let deadline = Instant::now() + Duration::from_secs(RESPONSE_TIMEOUT_SECS);
    while printer.ams().is_none() || (need_state && printer.print_status().is_none()) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match ::tokio::time::timeout(remaining, printer.poll_telemetry()).await {
            Ok(event) => {
                event?;
            }
            Err(_) => break,
        }
    }
    Ok(printer.ams().is_some())
}

/// Derives an AMS mapping for `local_3mf` plate `plate` and, with `bed_type`, uploads the file and starts the job.
async fn run_map(
    printer: &mut FtpsPrinter,
    ip: &str,
    local_3mf: &str,
    plate: u32,
    bed_type: Option<String>,
    allow_unmatched: bool,
    allow_other_model: bool,
) -> Result<(), CliError> {
    let path = Path::new(local_3mf);

    let plate_filaments = read_plate_filaments(FileSource::open(path)?, plate).await?;
    let sliced_for = read_sliced_for(FileSource::open(path)?).await?;
    let printer_model = printer.model();
    println!(
        "Plate {plate} of '{local_3mf}' uses {} filament(s).",
        plate_filaments.filaments.len()
    );
    println!("Sliced for: {}", describe_sliced_for(sliced_for.as_deref()));
    println!("Printer model: {printer_model}");

    eprintln!("Connecting to {ip} port {}...", bambino::mqtt::MQTTS_PORT);
    printer.connect_mqtt().await?;
    eprintln!("Connected.");
    printer.request_pushall().await?;
    let have_ams = wait_for_ams(printer, bed_type.is_some()).await?;
    if !have_ams {
        eprintln!(
            "No AMS data within {RESPONSE_TIMEOUT_SECS}s of the status request; continuing, so \
             every filament will be unmatched."
        );
    }

    let result = printer.derive_ams_mapping(&plate_filaments);
    println!();
    print_mapping_table(&plate_filaments.filaments, &result);
    let mapping_json = serde_json::to_string(&result.mapping2)
        .map_err(|e| CliError::Other(format!("failed to serialize mapping2: {e}")))?;
    println!("\nmapping2: {mapping_json}");

    let Some(bed_type) = bed_type else {
        return Ok(());
    };

    if !allow_other_model {
        let accepted = sliced_for
            .as_deref()
            .is_some_and(|id| printer_model.quirks().accepts_sliced_for(id));
        if !accepted {
            return Err(CliError::InvalidArgs(format!(
                "'{local_3mf}' was sliced for {}, which this printer ({printer_model}) does not \
                 accept. Its start G-code can drive the carriage into the frame (#657). Slice it \
                 for this printer, or pass --allow-other-model to start anyway.",
                describe_sliced_for(sliced_for.as_deref())
            )));
        }
    }
    if plate_filaments.filaments.is_empty() {
        return Err(CliError::InvalidArgs(format!(
            "plate {plate} lists no filaments in the file; not starting a print"
        )));
    }
    if !result.is_complete() && !allow_unmatched {
        return Err(CliError::InvalidArgs(String::from(
            "some filaments matched no loaded tray. On a single-nozzle printer a mapping with \
             nothing matched prints from the external spool, and a partly matched one is untested \
             on hardware. Load matching filament, or pass --allow-unmatched to start anyway.",
        )));
    }
    refuse_if_busy(printer.print_status())?;

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| CliError::InvalidArgs(format!("'{local_3mf}' has no usable file name")))?;
    let stem = path
        .file_stem()
        .and_then(|n| n.to_str())
        .ok_or_else(|| CliError::InvalidArgs(format!("'{local_3mf}' has no usable file name")))?;

    let prompt = format!(
        "This uploads '{file_name}' to the printer and STARTS A REAL PRINT.\n  sliced for: {}\n  \
         printer model: {printer_model}\n  plate: {plate}\n  bed type: {bed_type}\n  \
         mapping2: {mapping_json}\nType 'yes' to proceed: ",
        describe_sliced_for(sliced_for.as_deref())
    );
    if !crate::prompt::confirm(&prompt)? {
        return Ok(());
    }

    println!("Uploading '{file_name}'...");
    let started = Instant::now();
    let file = printer
        .upload_file(
            StorageKind::External,
            file_name,
            FileSource::open(path)?,
            if allow_other_model {
                ModelCheck::Skip
            } else {
                ModelCheck::Enforce
            },
        )
        .await?;
    print_transfer_time("upload_file", started);
    println!("Uploaded {} ({}).", file.name, format_size(file.size));

    // The firmware clamps task ids to 32 bits signed (reference/03_mqtt_telemetry.md, "Task
    // Tracking & Sequence Boundaries"); seconds since the epoch is unique per run and in range.
    let subtask_id = unix_now_secs() % 2_147_483_647;
    let config = PrintJobConfig::new(
        &file.name,
        &format!("Metadata/plate_{plate}.gcode"),
        stem,
        subtask_id,
        &bed_type,
    )
    .with_ams_mapping2(result.mapping2);
    let config = if allow_other_model {
        config.model_unchecked()
    } else {
        config.sliced_for(file.sliced_for.clone())
    };
    let handle = printer.start_print(&config).await?;
    println!(
        "project_file published (sequence_id {}); waiting for the printer's echo...",
        handle.sequence_id()
    );
    let outcome = printer.await_ack(&handle).await?;
    println!("Outcome: {outcome:?}");
    println!("An accepted command is a received one; watch the printer to see the job start.");
    match outcome {
        CommandOutcome::Refused(_) => Err(CliError::Other(String::from(
            "the printer refused project_file",
        ))),
        _ => Ok(()),
    }
}

fn format_size(bytes: u64) -> String {
    if bytes >= BYTES_PER_GIB {
        format!("{:.1} GB", bytes as f64 / BYTES_PER_GIB as f64)
    } else if bytes >= BYTES_PER_MIB {
        format!("{:.1} MB", bytes as f64 / BYTES_PER_MIB as f64)
    } else if bytes >= BYTES_PER_KIB {
        format!("{:.1} KB", bytes as f64 / BYTES_PER_KIB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod map_tests {
    use super::*;

    /// Writes `bytes` to a uniquely named file in the temp dir and returns its path.
    fn temp_file(tag: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "bambino_cli_map_test_{}_{tag}.3mf",
            std::process::id()
        ));
        fs::write(&path, bytes).unwrap();
        path
    }

    #[tokio::test]
    async fn test_file_source_reads_at_offsets_with_a_short_final_read() {
        let path = temp_file("offsets", b"0123456789");
        let mut source = FileSource::open(&path).unwrap();
        assert_eq!(UploadSource::len(&source), 10);

        let mut buf = [0u8; 4];
        assert_eq!(source.read_at(0, &mut buf).await.unwrap(), 4);
        assert_eq!(&buf, b"0123");
        assert_eq!(source.read_at(3, &mut buf).await.unwrap(), 4);
        assert_eq!(&buf, b"3456");

        // Only two bytes remain: a short final read, then nothing.
        assert_eq!(source.read_at(8, &mut buf).await.unwrap(), 2);
        assert_eq!(&buf[..2], b"89");
        assert_eq!(source.read_at(10, &mut buf).await.unwrap(), 0);
        assert_eq!(source.read_at(500, &mut buf).await.unwrap(), 0);
        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn test_file_source_of_an_empty_file_is_empty() {
        let path = temp_file("empty", b"");
        let mut source = FileSource::open(&path).unwrap();
        assert!(UploadSource::is_empty(&source));
        let mut buf = [0u8; 4];
        assert_eq!(source.read_at(0, &mut buf).await.unwrap(), 0);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_file_source_open_fails_for_a_missing_file() {
        let path = std::env::temp_dir().join("bambino_cli_map_test_does_not_exist.3mf");
        assert!(FileSource::open(&path).is_err());
    }

    #[test]
    fn test_tray_label_names_every_source_readably() {
        let cases = [
            (
                MaterialSource::StandardAms {
                    ams_id: 0,
                    slot_id: 2,
                },
                "AMS 0 slot 2",
            ),
            (MaterialSource::AmsHt { ams_id: 128 }, "AMS-HT 128"),
            (MaterialSource::AmsLite { slot_id: 3 }, "AMS Lite slot 3"),
            (MaterialSource::ExternalSpool, "external spool"),
            (MaterialSource::ExternalSpoolLeft, "external left"),
            (MaterialSource::ExternalSpoolRight, "external right"),
            (MaterialSource::Unmapped, "unmapped"),
        ];
        for (source, label) in cases {
            assert_eq!(tray_label(source), label);
        }
    }

    #[test]
    fn test_refuse_if_busy_refuses_busy_unknown_and_missing_states() {
        for status in [
            PrintStatus::Preparing,
            PrintStatus::Slicing,
            PrintStatus::Running,
            PrintStatus::Paused,
            PrintStatus::Unknown,
        ] {
            assert!(refuse_if_busy(Some(status)).is_err(), "{status:?}");
        }
        assert!(refuse_if_busy(None).is_err());
        for status in [
            PrintStatus::Idle,
            PrintStatus::Finished,
            PrintStatus::Failed,
        ] {
            assert!(refuse_if_busy(Some(status)).is_ok(), "{status:?}");
        }
    }
}
