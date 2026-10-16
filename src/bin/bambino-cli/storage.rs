#![cfg(feature = "cli")]

//! # MicroSD Storage Traversal and File Transfer Subcommand
//!
//! Routes storage filesystem commands through `PrinterClient` with lazy FTPS
//! connection via `.with_ftps()`. The FTPS session is established on first use.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Instant;

use bambino::Error;
use bambino::client::{FileKind, FileTime, PrinterClient, PrinterFile};
use bambino::ftps::CurrentDateTime;
use bambino::ftps::project::{front_members, pick_image_member, plate_image_member};
use bambino::io::TokioIo;
use bambino::io::tokio::TokioTlsConnector;
use bambino::io::tokio::{TokioRawStreamFactory, TokioTimer};
use clap::{Subcommand, ValueEnum};

use crate::connection::Target;
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
    /// --stop-after BYTES stops the transfer once that many bytes are written, leaving a partial
    /// file. The printer then closes the FTPS session, so the next command reconnects.
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
    /// falls back to the firmware build date on boot. `list_directory`'s year-rollover math
    /// can't be trusted whenever this is the case, since the raw `LIST` HH:MM timestamps come
    /// from the printer's own wrong clock. Unconfirmed on printers with more capable AP
    /// controllers (X1/H2 series).
    #[command(override_usage = "bambino-cli files <IP> <SERIAL> [ACCESS_CODE] clock-check")]
    ClockCheck,
    /// Query available MicroSD card capacity
    #[command(override_usage = "bambino-cli files <IP> <SERIAL> [ACCESS_CODE] space")]
    Space,
    /// List, download or delete print files or timelapses by kind, through PrinterClient::list_files
    ///
    /// Without get or rm, lists the files of KIND. --probe-clock reads the printer's clock first
    /// (an upload, MDTM and delete of /bambino_clock_probe.txt), so a year the listing leaves out
    /// is rebuilt against the printer's clock rather than the host's.
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
    /// Download a listed file by name, writing it as it arrives
    Get {
        /// The file's name or path, as the listing shows it
        name: String,
        local_path: String,
        /// Stop once this many bytes are written; the printer then closes the FTPS session
        #[arg(long, value_name = "BYTES")]
        stop_after: Option<u64>,
    },
    /// Download a listed timelapse's cover image by name
    Thumb {
        /// The timelapse's name or path, as the listing shows it
        name: String,
        local_path: String,
    },
    /// Delete a listed file by name
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

/// Writes a download to a local file as it arrives, stopping at `--stop-after` if given.
struct LocalSink {
    out: fs::File,
    written: u64,
    stop_after: Option<u64>,
    stopped: bool,
    write_error: Option<std::io::Error>,
}

impl LocalSink {
    fn create(local_path: &str, stop_after: Option<u64>) -> Result<Self, CliError> {
        Ok(Self {
            out: fs::File::create(local_path)?,
            written: 0,
            stop_after,
            stopped: false,
            write_error: None,
        })
    }

    /// The download sink: refuses the next read once `stop_after` bytes are written.
    fn accept(&mut self, chunk: &[u8]) -> Result<(), Error> {
        if self.stop_after.is_some_and(|limit| self.written >= limit) {
            self.stopped = true;
            return Err(Error::InvalidState("--stop-after reached".into()));
        }
        if let Err(e) = self.out.write_all(chunk) {
            self.write_error = Some(e);
            return Err(Error::InvalidState("local write failed".into()));
        }
        self.written += chunk.len() as u64;
        Ok(())
    }

    /// Reports the download's outcome; `session_closed` is whether FTPS ended with it.
    fn report(
        self,
        result: Result<u64, Error>,
        session_closed: bool,
        local_path: &str,
    ) -> Result<(), CliError> {
        if let Some(e) = self.write_error {
            return Err(e.into());
        }
        if !self.stopped {
            let n = result?;
            println!("Success: {n} bytes written to '{local_path}'.");
            return Ok(());
        }
        println!(
            "Stopped after {} bytes (--stop-after); '{local_path}' is partial.",
            self.written
        );
        if session_closed {
            println!("The data channel was reset; the printer has closed this FTPS session.");
        } else {
            println!("Unexpected: the FTPS session is still usable after the stop.");
        }
        Ok(())
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
                let mut sink = LocalSink::create(&local_path, stop_after)?;
                let started = Instant::now();
                let result = client
                    .download_file_each(&remote_path, |chunk| sink.accept(chunk))
                    .await;
                print_transfer_time("RETR", started);
                sink.report(result, client.is_poisoned(), &local_path)?;
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
            let mut sink = LocalSink::create(&local_path, stop_after)?;
            let started = Instant::now();
            let result = printer
                .download_file(&file, |chunk| sink.accept(chunk))
                .await;
            print_transfer_time("download_file", started);
            sink.report(result, !printer.is_ftps_connected(), &local_path)?;
        }
        Some(CatalogOp::Thumb { name, local_path }) => {
            let file = find(&name)?.clone();
            let mut sink = LocalSink::create(&local_path, None)?;
            match printer
                .read_thumbnail(&file, |chunk| sink.accept(chunk))
                .await
            {
                Ok(Some(n)) => println!("Success: {n} bytes written to '{local_path}'."),
                Ok(None) => {
                    drop(sink);
                    fs::remove_file(&local_path)?;
                    println!("'{name}' has no cover image.");
                }
                Err(e) => sink.report(Err(e), !printer.is_ftps_connected(), &local_path)?,
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
