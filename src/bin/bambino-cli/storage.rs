#![cfg(feature = "cli")]

//! # MicroSD Storage Traversal and File Transfer Subcommand
//!
//! Routes storage filesystem commands through `PrinterClient` with lazy FTPS
//! connection via `.with_ftps()`. The FTPS session is established on first use.

use std::fs;
use std::path::Path;

use bambino::error::BambuError;
use bambino::io::tokio::{
    TokioRawStreamFactory, TokioTimer, TokioTlsConnector, build_unsafe_client_config_with_options,
};
use clap::Subcommand;

use crate::connection::create_printer;

/// Bytes per gibibyte — shared by the upload size ceiling and `format_size`'s unit conversion, which previously each hardcoded this same literal independently.
const BYTES_PER_GIB: u64 = 1_073_741_824;

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
    /// Remove a file from the remote filesystem path
    #[command(override_usage = "bambino-cli files <IP> <SERIAL> [ACCESS_CODE] delete <REMOTE_PATH>")]
    Delete { remote_path: String },
    /// Uploads a tiny probe file, diffs its printer-reported mtime against host wall-clock
    /// time, then deletes it — checks whether the printer's onboard clock is usably accurate.
    ///
    /// BUG-042: confirmed unreliable on ESP32/FreeRTOS-class printers (e.g. P1S) — no RTC
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
}

/// Dynamic calendar epoch helper converting the current wall-clock time to calendar date parts.
fn current_date_utc() -> (i32, u8, u8, u8, u8) {
    let now = time::OffsetDateTime::now_utc();
    (
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
    )
}

/// Dispatches a typed storage action over FTPS.
pub async fn run(
    ip: &str,
    serial: &str,
    access_code: &str,
    action: FilesAction,
    allow_unverified_tls_1_2: bool,
) -> Result<(), BambuError> {
    let printer = create_printer(ip, serial, access_code)?;
    let model = printer.model();

    let ftps_config =
        build_unsafe_client_config_with_options(model.quirks().enforce_ftps_tls_1_2());
    let ftps_tls = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(ftps_config));

    let mut printer = printer
        .with_ftps(ftps_tls, TokioRawStreamFactory, TokioTimer::new())
        .with_ftps_allow_unverified_tls_1_2(allow_unverified_tls_1_2);

    println!(
        "Connecting to implicitly secure FTPS server at {}:990...",
        ip
    );

    let client = printer.storage().await?;

    println!("FTPS connection authenticated. Executing operational action...\n");

    match action {
        FilesAction::List { remote_path } => {
            let (year, month, day, hour, min) = current_date_utc();

            println!("Traversing remote files on directory '{}'...", remote_path);
            let files = client
                .list_directory(&remote_path, year, month, day, hour, min)
                .await?;

            if files.is_empty() {
                println!("Directory is empty or path does not exist.");
                client.disconnect().await;
                return Ok(());
            }

            print_file_listing_table(&remote_path, &files);
        }
        FilesAction::Upload {
            local_path,
            remote_path,
        } => {
            let local = Path::new(&local_path);
            let metadata = fs::metadata(local).map_err(|_| {
                BambuError::ProtocolViolation("Target local file does not exist".into())
            })?;

            const MAX_UPLOAD_BYTES: u64 = BYTES_PER_GIB;
            if metadata.len() > MAX_UPLOAD_BYTES {
                return Err(BambuError::ProtocolViolation(
                    format!(
                        "File too large for upload: {} bytes (max {} MB)",
                        metadata.len(),
                        MAX_UPLOAD_BYTES / (1024 * 1024)
                    )
                    .into(),
                ));
            }

            println!("Reading source file '{}' into buffer...", local_path);
            let payload = fs::read(local).map_err(|_| {
                BambuError::ProtocolViolation("Failed to read local target file".into())
            })?;

            println!(
                "Uploading file ({} bytes) to remote path '{}'...",
                payload.len(),
                remote_path
            );
            println!(
                "Note: Under heavy write latency, standard SD card flushing may require up to 300 seconds [REF-FTPS-FLUSH]."
            );
            client.upload_file(&remote_path, &payload).await?;

            println!("Success: File uploaded and non-volatile write-buffers successfully flushed.");
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
            let space_mb = space_bytes as f64 / (1024.0 * 1024.0);
            let space_gb = space_mb / 1024.0;

            println!("\nStorage Capacity Status:");
            println!("  - Free Space (Bytes) : {}", space_bytes);
            println!("  - Free Space (MB)    : {:.2} MB", space_mb);
            println!("  - Free Space (GB)    : {:.2} GB\n", space_gb);
        }
    }

    client.disconnect().await;
    Ok(())
}

/// Uploads a tiny probe file, diffs its printer-reported mtime against host UTC wall-clock
/// time, then deletes it. Factored out of `run()`'s `ClockCheck` arm to keep that match
/// readable — see `FilesAction::ClockCheck`'s doc comment for why this exists (BUG-042:
/// LAN-mode NTP sync is unreliable, so `list_directory`'s year-rollover math can't be trusted
/// without checking the printer's clock first).
async fn run_clock_check<RawIO, Tls, Factory, FtpsTimer>(
    client: &mut bambino::ftps::BambuFtpsClient<RawIO, Tls, Factory, FtpsTimer>,
) -> Result<(), BambuError>
where
    RawIO: bambino::io::AsyncIo,
    Tls: bambino::io::TlsConnector<RawIO>,
    Factory: bambino::io::RawStreamFactory<RawIO>,
    FtpsTimer: bambino::io::TimerProvider,
{
    const PROBE_PATH: &str = "/bambino_clock_probe.txt";
    let payload = b"bambino clock probe";

    println!("Uploading clock probe file to '{}'...", PROBE_PATH);
    let before = time::OffsetDateTime::now_utc();
    client.upload_file(PROBE_PATH, payload).await?;
    let after = time::OffsetDateTime::now_utc();

    let (year, month, day, hour, min) = current_date_utc();
    let listing = client.list_directory("/", year, month, day, hour, min).await;

    // Always attempt cleanup, even if the listing failed — don't leave the probe file behind
    // on the printer's storage.
    let delete_result = client.delete_file(PROBE_PATH).await;

    let files = listing?;
    let probe = files.iter().find(|f| f.name == "bambino_clock_probe.txt");

    println!("\nHost UTC time before upload : {}", format_utc(before));
    println!("Host UTC time after upload   : {}", format_utc(after));

    match probe {
        Some(f) => {
            println!(
                "Printer-reported mtime       : {:04}-{:02}-{:02} {:02}:{:02}",
                f.year, f.month, f.day, f.hour, f.minute
            );
            report_clock_delta(after, f.year, f.month, f.day, f.hour, f.minute);
        }
        None => println!(
            "\nCould not locate the probe file in the directory listing — upload may have \
             failed, or the listing raced the SD card flush."
        ),
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
        if delta.is_negative() {
            "behind host"
        } else {
            "ahead of host"
        }
    );
    if days.abs() > 0 {
        println!(
            "Printer clock is off by more than a day — its onboard clock is not usably \
             synced (see BUG-042 in BACKLOG.md); don't trust `list_directory`'s inferred \
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
/// match readable — see `FilesAction::List`'s `year_is_inferred` handling (BUG-042) for why
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
            file.year, file.month, file.day, file.hour, file.minute, inferred_marker
        );
        table.add_row(vec![type_str, &size_str, &modified_str, &file.name]);
    }
    table.print();
    if any_inferred {
        println!(
            "\n* year inferred from host clock, not reported by the printer — its onboard \
             clock may be unsynced (see BUG-042 in BACKLOG.md); run `files clock-check` to \
             verify."
        );
    }
    println!();
}

fn format_size(bytes: u64) -> String {
    if bytes >= BYTES_PER_GIB {
        format!("{:.1} GB", bytes as f64 / BYTES_PER_GIB as f64)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}
