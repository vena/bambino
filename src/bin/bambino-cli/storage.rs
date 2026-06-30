#![cfg(feature = "cli")]

//! # MicroSD Storage Traversal and File Transfer Subcommand
//!
//! Routes storage filesystem commands through `PrinterClient` with lazy FTPS
//! connection via `.with_ftps()`. The FTPS session is established on first use.

use std::fs;
use std::path::Path;

use bambino::error::BambuError;
use bambino::io::tokio::{
    TokioFtpDataStreamFactory, TokioTlsConnector, build_unsafe_client_config_with_options,
};
use clap::Subcommand;

use crate::connection::create_printer;

#[derive(Subcommand, Debug)]
pub enum FilesAction {
    /// Perform a UNIX directory listing traversal
    List {
        #[arg(default_value = "/")]
        remote_path: String,
    },
    /// Upload a local file to the remote card path
    Upload {
        local_path: String,
        remote_path: String,
    },
    /// Remove a file from the remote filesystem path
    Delete { remote_path: String },
    /// Query available MicroSD card capacity
    Space,
}

/// Dynamic calendar epoch helper converting UNIX timestamps to calendar date parts.
fn current_date_utc() -> (i32, u8, u8, u8, u8) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let secs_per_day = 86400;
    let days = now / secs_per_day;
    let seconds_of_day = now % secs_per_day;
    let hour = (seconds_of_day / 3600) as u8;
    let minute = ((seconds_of_day % 3600) / 60) as u8;

    let mut year = 1970;
    let mut remaining_days = days as i32;

    loop {
        let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year = if is_leap { 366 } else { 365 };
        if remaining_days >= days_in_year {
            remaining_days -= days_in_year;
            year += 1;
        } else {
            break;
        }
    }

    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let days_in_months = if is_leap {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    for days_in_m in days_in_months {
        if remaining_days >= days_in_m {
            remaining_days -= days_in_m;
            month += 1;
        } else {
            break;
        }
    }

    let day = (remaining_days + 1) as u8;
    (year, month as u8, day, hour, minute)
}

/// Dispatches a typed storage action over FTPS.
pub async fn run(
    ip: &str,
    serial: &str,
    access_code: &str,
    action: FilesAction,
) -> Result<(), BambuError> {
    let printer = create_printer(ip, serial, access_code)?;
    let model = printer.model();

    let ftps_config =
        build_unsafe_client_config_with_options(model.quirks().enforce_ftps_tls_1_2());
    let ftps_tls = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(ftps_config));

    let mut printer = printer.with_ftps(ftps_tls, TokioFtpDataStreamFactory);

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

            println!("\nDirectory listing: {}\n", remote_path);
            let mut table = crate::table::Table::new(vec!["Type", "Size", "Modified", "Name"]);
            for file in &files {
                let type_str = if file.is_dir { "DIR" } else { "FILE" };
                let size_str = if file.is_dir {
                    String::from("-")
                } else {
                    format_size(file.size)
                };
                let modified_str = format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}",
                    file.year, file.month, file.day, file.hour, file.minute
                );
                table.add_row(vec![type_str, &size_str, &modified_str, &file.name]);
            }
            table.print();
            println!();
        }
        FilesAction::Upload {
            local_path,
            remote_path,
        } => {
            let local = Path::new(&local_path);
            let metadata = fs::metadata(local).map_err(|_| {
                BambuError::ProtocolViolation("Target local file does not exist".into())
            })?;

            const MAX_UPLOAD_BYTES: u64 = 1_073_741_824;
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

fn format_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}
