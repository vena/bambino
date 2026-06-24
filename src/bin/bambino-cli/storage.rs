#![cfg(feature = "std")]

//! # MicroSD Storage Traversal and File Transfer Subcommand
//!
//! Handles established implicitly secure FTPS sessions on Port 990 [REF-FTPS-CONN],
//! negotiating passive data channels with custom TLS verification bypass.
//!
//! Provides directory listings, space checks, deletions, and chunked upload dispatches.

use std::fs;
use std::path::Path;
use tokio::net::TcpStream;

use bambino::discovery::resolve_model;
use bambino::error::BambuError;
use bambino::ftps::{BambuFtpsClient, FtpDataStreamFactory};
use bambino::io::tokio::{TokioTlsConnector, build_unsafe_client_config, to_socket_error};
use bambino::io::{SocketError, TokioIo};

/// Concrete implementation of the passive data connection factory for the Tokio runtime.
struct TokioDataStreamFactory;

impl FtpDataStreamFactory<TokioIo<TcpStream>> for TokioDataStreamFactory {
    async fn create_data_stream(
        &self,
        host: &str,
        port: u16,
    ) -> Result<TokioIo<TcpStream>, SocketError> {
        let stream = TcpStream::connect(format!("{}:{}", host, port))
            .await
            .map_err(to_socket_error)?;
        Ok(TokioIo(stream))
    }
}

/// Dynamic calendar epoch helper converting UNIX timestamps to calendar date parts.
///
/// **Why this is used:** Bypasses massive dependency additions (such as `chrono` or `time`)
/// inside a standard embedded-friendly repository.
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

/// Parses and dispatches storage filesystem commands over FTPS.
pub async fn run(
    ip: &str,
    serial: &str,
    access_code: &str,
    action_args: &[String],
) -> Result<(), BambuError> {
    if action_args.is_empty() {
        return Err(BambuError::ProtocolViolation(
            "Missing storage action identifier".into(),
        ));
    }

    let action = action_args[0].to_lowercase();
    let model = resolve_model(serial, None);

    println!(
        "Connecting to implicitly secure FTPS server at {}:990...",
        ip
    );

    // 1. Setup secure TLS socket context
    let config = build_unsafe_client_config();
    let connector = tokio_rustls::TlsConnector::from(config);
    let tls_connector = TokioTlsConnector::new(connector);

    let tcp_stream = TcpStream::connect(format!("{}:990", ip))
        .await
        .map_err(to_socket_error)?;
    let raw_control = TokioIo(tcp_stream);

    // 2. Perform connection, login, and security policy selection [REF-FTPS-CONN]
    let mut client = BambuFtpsClient::connect(
        raw_control,
        tls_connector,
        TokioDataStreamFactory,
        model,
        ip,
        access_code,
    )
    .await?;

    println!("FTPS connection authenticated. Executing operational action...\n");

    match action.as_str() {
        "list" => {
            let path = action_args.get(1).map(|s| s.as_str()).unwrap_or("/");
            let (year, month, day, hour, min) = current_date_utc();

            println!("Traversing remote files on directory '{}'...", path);
            let files = client
                .list_directory(path, year, month, day, hour, min)
                .await?;

            if files.is_empty() {
                println!("Directory is empty or path does not exist.");
                return Ok(());
            }

            println!("\nDirectory listing: {}\n", path);
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
        "upload" => {
            if action_args.len() < 3 {
                return Err(BambuError::ProtocolViolation(
                    "Usage: files <ip> <serial> <access_code> upload <local_path> <remote_path>"
                        .into(),
                ));
            }
            let local_path_str = &action_args[1];
            let remote_path_str = &action_args[2];

            let local_path = Path::new(local_path_str);
            if !local_path.exists() {
                return Err(BambuError::ProtocolViolation(
                    "Target local file does not exist".into(),
                ));
            }

            println!("Reading source file '{}' into buffer...", local_path_str);
            let payload = fs::read(local_path).map_err(|_| {
                BambuError::ProtocolViolation("Failed to read local target file".into())
            })?;

            println!(
                "Uploading file ({} bytes) to remote path '{}'...",
                payload.len(),
                remote_path_str
            );
            println!(
                "Note: Under heavy write latency, standard SD card flushing may require up to 300 seconds [REF-FTPS-FLUSH]."
            );
            client.upload_file(remote_path_str, &payload).await?;

            println!("Success: File uploaded and non-volatile write-buffers successfully flushed.");
        }
        "delete" => {
            if action_args.len() < 2 {
                return Err(BambuError::ProtocolViolation(
                    "Usage: files <ip> <serial> <access_code> delete <remote_path>".into(),
                ));
            }
            let remote_path_str = &action_args[1];

            println!("Deleting file from remote path '{}'...", remote_path_str);
            client.delete_file(remote_path_str).await?;
            println!("Success: Target file successfully removed.");
        }
        "space" => {
            println!("Querying hardware storage space evaluations...");
            let space_bytes = client.get_available_space().await?;
            let space_mb = space_bytes as f64 / (1024.0 * 1024.0);
            let space_gb = space_mb / 1024.0;

            println!("\nStorage Capacity Status:");
            println!("  - Free Space (Bytes) : {}", space_bytes);
            println!("  - Free Space (MB)    : {:.2} MB", space_mb);
            println!("  - Free Space (GB)    : {:.2} GB\n", space_gb);
        }
        other => {
            return Err(BambuError::ProtocolViolation(
                format!("Unrecognized storage action identifier '{}'", other).into(),
            ));
        }
    }

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
