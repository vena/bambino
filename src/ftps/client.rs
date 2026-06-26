//! # Implicit FTPS Client Implementation
//!
//! Implements a secure, platform-agnostic, asynchronous FTPS client designed to execute
//! over our abstract `AsyncIo` boundaries. This client coordinates implicitly encrypted control channels
//! on Port 990, Passive port negotiation, TLS session wrapping (with A1-series plaintext bypass),
//! whitespace-insensitive UNIX listings parsing, and robust chunked uploads [REF-FTPS-CONN] [REF-FTPS-OPS].

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use embedded_io_async::Write;

use crate::error::BambuError;
use crate::ftps::parser::{FtpFile, parse_unix_listing};
use crate::io::{AsyncIo, SocketError, TlsConnector};
use crate::models::BambuModel;

// FTP response codes (RFC 959)
pub(crate) const FTP_GREETING: u16 = 220;
pub(crate) const FTP_TRANSFER_STARTING: u16 = 125;
pub(crate) const FTP_TRANSFER_OPENING: u16 = 150;
pub(crate) const FTP_SIZE_OK: u16 = 213;
pub(crate) const FTP_STAT_OK: u16 = 211;
pub(crate) const FTP_TRANSFER_COMPLETE: u16 = 226;
pub(crate) const FTP_PASSIVE_MODE: u16 = 227;
pub(crate) const FTP_LOGIN_OK: u16 = 230;
pub(crate) const FTP_FILE_ACTION_OK: u16 = 250;
pub(crate) const FTP_PATHNAME_CREATED: u16 = 257;
pub(crate) const FTP_PASSWORD_NEEDED: u16 = 331;
pub(crate) const FTP_RENAME_PENDING: u16 = 350;
pub(crate) const FTP_TRANSFER_ABORTED: u16 = 426;
pub(crate) const FTP_FILE_NOT_FOUND: u16 = 550;
pub(crate) const FTP_COMMAND_OK: u16 = 200;

pub(crate) const FTPS_IMPLICIT_PORT: u16 = 990;
pub(crate) const FTPS_UPLOAD_CHUNK_SIZE: usize = 65536;
pub(crate) const FTPS_DATA_READ_BUF_SIZE: usize = 4096;
pub(crate) const FTPS_AVBL_SIZE_HEURISTIC_THRESHOLD: u64 = 100_000_000;
pub(crate) const FTPS_PASV_PORT_MULTIPLIER: u16 = 256;
pub(crate) const FTP_MAX_RESPONSE_LINE_BYTES: usize = 4096;
pub(crate) const FTP_MAX_RESPONSE_LINES: usize = 100;

/// Factory trait used to establish standard TCP socket connections to passive ports.
///
/// Under FTPS, passive transfers open fresh data socket connections back to the printer.
/// This abstract boundary permits safe standard, ESP-IDF, and bare-metal Embassy bindings.
#[allow(async_fn_in_trait)]
pub trait FtpDataStreamFactory<RawIO: AsyncIo> {
    /// Connects a raw, un-encrypted socket to the designated host and port.
    async fn create_data_stream(&self, host: &str, port: u16) -> Result<RawIO, SocketError>;
}

/// Lightweight, high-reliability implicit FTPS client running on top of abstract I/O traits.
pub struct BambuFtpsClient<RawIO, Tls, Factory>
where
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: FtpDataStreamFactory<RawIO>,
{
    control_stream: Tls::Stream,
    tls_connector: Tls,
    data_factory: Factory,
    model: BambuModel,
    ip: String,
}

impl<RawIO, Tls, Factory> BambuFtpsClient<RawIO, Tls, Factory>
where
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: FtpDataStreamFactory<RawIO>,
{
    /// Establishes the secure control channel, performs login handshakes, and configures security properties.
    ///
    /// **Implicit Security Handshake:**
    /// Prior to issuing or evaluating any standard text commands, the raw connection socket must be
    /// wrapped in a secure TLS session immediately upon establishment. Explicit handshakes (such as `AUTH TLS`)
    /// are not utilized.
    pub async fn connect(
        raw_control: RawIO,
        tls_connector: Tls,
        data_factory: Factory,
        model: BambuModel,
        ip: &str,
        access_code: &str,
    ) -> Result<Self, BambuError> {
        // Immediately wrap the control stream in TLS prior to reading greetings [REF-FTPS-CONN]
        let mut control_stream = tls_connector
            .connect(ip, FTPS_IMPLICIT_PORT, raw_control)
            .await?;

        let mut buf = Vec::new();

        // Read Server Greeting (220)
        let (code, _) = read_response(&mut control_stream, &mut buf).await?;
        if code != FTP_GREETING {
            return Err(BambuError::ProtocolViolation(
                "Unexpected greeting from FTP server".into(),
            ));
        }

        // Login USER bblp
        write_command(&mut control_stream, "USER bblp").await?;
        let (code, _) = read_response(&mut control_stream, &mut buf).await?;
        if code != FTP_PASSWORD_NEEDED {
            return Err(BambuError::ProtocolViolation(
                "USER authentication phase rejected".into(),
            ));
        }

        // Login PASS <access_code>
        let pass_cmd = format!("PASS {}", access_code);
        write_command(&mut control_stream, &pass_cmd).await?;
        let (code, _) = read_response(&mut control_stream, &mut buf).await?;
        if code != FTP_LOGIN_OK {
            return Err(BambuError::AccessDenied);
        }

        // Request Protection Buffer Size (PBSZ 0)
        write_command(&mut control_stream, "PBSZ 0").await?;
        let (code, _) = read_response(&mut control_stream, &mut buf).await?;
        if code != FTP_COMMAND_OK {
            return Err(BambuError::ProtocolViolation(
                "PBSZ protection sizing configuration failed".into(),
            ));
        }

        // Handle model-specific TLS Protection constraints [REF-FTPS-CONN]
        if !model.quirks().uses_plaintext_ftps_data_channel() {
            // Standard lines protect passive channels via PROT P (Private/TLS)
            write_command(&mut control_stream, "PROT P").await?;
            let (code, _) = read_response(&mut control_stream, &mut buf).await?;
            if code != FTP_COMMAND_OK {
                return Err(BambuError::ProtocolViolation(
                    "Failed to enable TLS data channel protection".into(),
                ));
            }
        } else {
            // A1 series does not support TLS on passive channels due to ESP32 constraints.
            // Bypassing PROT P keeps the data channel in PROT C (Clear/Plaintext) while commands remain secure.
        }

        // Set binary transfer mode. RFC 959 defaults to ASCII which translates
        // line endings, corrupting binary payloads like .3mf and .gcode files.
        write_command(&mut control_stream, "TYPE I").await?;
        let (code, _) = read_response(&mut control_stream, &mut buf).await?;
        if code != FTP_COMMAND_OK {
            return Err(BambuError::ProtocolViolation(
                "TYPE I binary mode configuration failed".into(),
            ));
        }

        Ok(Self {
            control_stream,
            tls_connector,
            data_factory,
            model,
            ip: String::from(ip),
        })
    }

    /// Queries the storage server for raw directory listings and parses their structures.
    pub async fn list_directory(
        &mut self,
        remote_path: &str,
        current_year: i32,
        current_month: u8,
        current_day: u8,
        current_hour: u8,
        current_minute: u8,
    ) -> Result<Vec<FtpFile>, BambuError> {
        let port = self.negotiate_passive_port().await?;
        let raw_data_socket = self.data_factory.create_data_stream(&self.ip, port).await?;

        // Command the control socket to list files
        let list_cmd = format!("LIST {}", remote_path);
        write_command(&mut self.control_stream, &list_cmd).await?;

        // Immediately read transient opening response on control channel
        let mut ctrl_buf = Vec::new();
        let (code, _) = read_response(&mut self.control_stream, &mut ctrl_buf).await?;
        if code != FTP_TRANSFER_OPENING && code != FTP_TRANSFER_STARTING {
            return Err(BambuError::ProtocolViolation(
                "LIST transfer initialization failed".into(),
            ));
        }

        // Wrap passive data socket if secure channel is required [REF-FTPS-CONN]
        let mut listing_payload = Vec::new();
        if !self.model.quirks().uses_plaintext_ftps_data_channel() {
            let mut secure_data_socket = self
                .tls_connector
                .connect(&self.ip, port, raw_data_socket)
                .await?;
            read_to_eof(&mut secure_data_socket, &mut listing_payload).await?;
            // Abruptly terminate the data stream socket prior to parsing to avoid hangs [REF-FTPS-FLUSH]
            drop(secure_data_socket);
        } else {
            let mut plain_data_socket = raw_data_socket;
            read_to_eof(&mut plain_data_socket, &mut listing_payload).await?;
            drop(plain_data_socket);
        }

        // Await transfer confirmation on control channel
        let (code, _) = read_response(&mut self.control_stream, &mut ctrl_buf).await?;
        if code != FTP_TRANSFER_COMPLETE {
            return Err(BambuError::ProtocolViolation(
                "LIST transfer confirmation aborted".into(),
            ));
        }

        let payload_str = core::str::from_utf8(&listing_payload).map_err(|_| {
            BambuError::ProtocolViolation("Non-UTF8 directory listings response".into())
        })?;

        Ok(parse_unix_listing(
            payload_str,
            current_year,
            current_month,
            current_day,
            current_hour,
            current_minute,
        ))
    }

    /// Queries the exact size of a file stored on the printer's MicroSD card.
    pub async fn get_file_size(&mut self, remote_path: &str) -> Result<u64, BambuError> {
        let size_cmd = format!("SIZE {}", remote_path);
        write_command(&mut self.control_stream, &size_cmd).await?;

        let mut buf = Vec::new();
        let (code, text) = read_response(&mut self.control_stream, &mut buf).await?;
        if code != FTP_SIZE_OK {
            return Err(BambuError::ProtocolViolation(
                "SIZE query rejected by storage server".into(),
            ));
        }

        text.parse::<u64>().map_err(|_| {
            BambuError::ProtocolViolation("Invalid file size parameter returned".into())
        })
    }

    /// Removes a targeted file from non-volatile storage.
    pub async fn delete_file(&mut self, remote_path: &str) -> Result<(), BambuError> {
        let dele_cmd = format!("DELE {}", remote_path);
        write_command(&mut self.control_stream, &dele_cmd).await?;

        let mut buf = Vec::new();
        let (code, _) = read_response(&mut self.control_stream, &mut buf).await?;

        // Code 250 represents successful deletion. Code 550 indicates the file is absent.
        // Both represent terminal success for cleanup operations [REF-FTPS-OPS].
        if code == FTP_FILE_ACTION_OK || code == FTP_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(BambuError::ProtocolViolation(
                "DELE file removal request failed".into(),
            ))
        }
    }

    /// Uploads a binary payload directly to MicroSD card storage.
    ///
    /// **Flush and Close Race Mitigation:**
    /// 1. Immediately drop the passive data channel upon finishing transmissions. This prevents
    ///    standard TLS graceful shutdown waits which would trigger indefinite hangs on physical vsFTPd.
    /// 2. Wait up to 300 seconds for the `226` transfer confirmation to print. Issuing downstream
    ///    print commands prior to this confirmation halts the printer due to microSD write latency exceptions [REF-FTPS-FLUSH].
    /// 3. If a TLS 1.3 session close race triggers a transient 426 on P2S/X2D models, verify the uploaded size
    ///    via the `SIZE` command to ensure data integrity [REF-FTPS-CONN].
    pub async fn upload_file(&mut self, remote_path: &str, data: &[u8]) -> Result<(), BambuError> {
        let port = self.negotiate_passive_port().await?;
        let raw_data_socket = self.data_factory.create_data_stream(&self.ip, port).await?;

        let stor_cmd = format!("STOR {}", remote_path);
        write_command(&mut self.control_stream, &stor_cmd).await?;

        let mut ctrl_buf = Vec::new();
        let (code, _) = read_response(&mut self.control_stream, &mut ctrl_buf).await?;
        if code != FTP_TRANSFER_OPENING && code != FTP_TRANSFER_STARTING {
            return Err(BambuError::ProtocolViolation(
                "STOR upload negotiation rejected".into(),
            ));
        }

        if !self.model.quirks().uses_plaintext_ftps_data_channel() {
            let mut secure_data_socket = self
                .tls_connector
                .connect(&self.ip, port, raw_data_socket)
                .await?;

            // Chunked upload sequence
            let mut offset = 0;
            while offset < data.len() {
                let chunk_size = core::cmp::min(FTPS_UPLOAD_CHUNK_SIZE, data.len() - offset);
                secure_data_socket
                    .write_all(&data[offset..offset + chunk_size])
                    .await
                    .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
                offset += chunk_size;
            }
            secure_data_socket
                .flush()
                .await
                .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;

            // Abruptly drop passive channel to complete transfer cleanly without hang negotiations
            drop(secure_data_socket);
        } else {
            let mut plain_data_socket = raw_data_socket;
            let mut offset = 0;
            while offset < data.len() {
                let chunk_size = core::cmp::min(FTPS_UPLOAD_CHUNK_SIZE, data.len() - offset);
                plain_data_socket
                    .write_all(&data[offset..offset + chunk_size])
                    .await
                    .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
                offset += chunk_size;
            }
            plain_data_socket
                .flush()
                .await
                .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
            drop(plain_data_socket);
        }

        // Block-wait for transfer acknowledgment on the control socket.
        // The 226 path is the standard success. The 426 path handles the TLS 1.3
        // close-notify race on P2S/X2D models [REF-FTPS-CONN]. In both cases,
        // verify via SIZE to catch silent SD card write truncation.
        let res = read_response(&mut self.control_stream, &mut ctrl_buf).await;
        match res {
            Ok((FTP_TRANSFER_COMPLETE, _)) | Ok((FTP_TRANSFER_ABORTED, _)) => {
                let remote_size = self.get_file_size(remote_path).await?;
                if remote_size == data.len() as u64 {
                    Ok(())
                } else {
                    Err(BambuError::DiskWriteFailure)
                }
            }
            Ok((_, _)) => Err(BambuError::DiskWriteFailure),
            Err(e) => Err(e),
        }
    }

    /// Downloads the contents of a remote file from MicroSD storage via the RETR command.
    ///
    /// Negotiates a passive data channel, retrieves the binary payload, and returns the raw bytes.
    pub async fn download_file(&mut self, remote_path: &str) -> Result<Vec<u8>, BambuError> {
        let port = self.negotiate_passive_port().await?;
        let raw_data_socket = self.data_factory.create_data_stream(&self.ip, port).await?;

        let retr_cmd = format!("RETR {}", remote_path);
        write_command(&mut self.control_stream, &retr_cmd).await?;

        let mut ctrl_buf = Vec::new();
        let (code, _) = read_response(&mut self.control_stream, &mut ctrl_buf).await?;
        if code != FTP_TRANSFER_OPENING && code != FTP_TRANSFER_STARTING {
            return Err(BambuError::ProtocolViolation(
                "RETR transfer initialization failed".into(),
            ));
        }

        let mut file_payload = Vec::new();
        if !self.model.quirks().uses_plaintext_ftps_data_channel() {
            let mut secure_data_socket = self
                .tls_connector
                .connect(&self.ip, port, raw_data_socket)
                .await?;
            read_to_eof(&mut secure_data_socket, &mut file_payload).await?;
            drop(secure_data_socket);
        } else {
            let mut plain_data_socket = raw_data_socket;
            read_to_eof(&mut plain_data_socket, &mut file_payload).await?;
            drop(plain_data_socket);
        }

        let (code, _) = read_response(&mut self.control_stream, &mut ctrl_buf).await?;
        if code != FTP_TRANSFER_COMPLETE {
            return Err(BambuError::ProtocolViolation(
                "RETR transfer confirmation aborted".into(),
            ));
        }

        Ok(file_payload)
    }

    /// Creates a directory on the printer's MicroSD storage.
    pub async fn create_directory(&mut self, path: &str) -> Result<(), BambuError> {
        let mkd_cmd = format!("MKD {}", path);
        write_command(&mut self.control_stream, &mkd_cmd).await?;

        let mut buf = Vec::new();
        let (code, _) = read_response(&mut self.control_stream, &mut buf).await?;
        if code != FTP_PATHNAME_CREATED {
            return Err(BambuError::ProtocolViolation(
                "MKD directory creation failed".into(),
            ));
        }
        Ok(())
    }

    /// Removes a directory from the printer's MicroSD storage.
    ///
    /// Returns success for both `250` (deleted) and `550` (already absent),
    /// matching the idempotent cleanup semantics of `delete_file`.
    pub async fn remove_directory(&mut self, path: &str) -> Result<(), BambuError> {
        let rmd_cmd = format!("RMD {}", path);
        write_command(&mut self.control_stream, &rmd_cmd).await?;

        let mut buf = Vec::new();
        let (code, _) = read_response(&mut self.control_stream, &mut buf).await?;
        if code == FTP_FILE_ACTION_OK || code == FTP_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(BambuError::ProtocolViolation(
                "RMD directory removal request failed".into(),
            ))
        }
    }

    /// Renames a file or directory on the printer's MicroSD storage.
    ///
    /// Executes the standard FTP two-step rename sequence: `RNFR` (rename from)
    /// followed by `RNTO` (rename to).
    pub async fn rename_file(&mut self, from: &str, to: &str) -> Result<(), BambuError> {
        let rnfr_cmd = format!("RNFR {}", from);
        write_command(&mut self.control_stream, &rnfr_cmd).await?;

        let mut buf = Vec::new();
        let (code, _) = read_response(&mut self.control_stream, &mut buf).await?;
        if code != FTP_RENAME_PENDING {
            return Err(BambuError::ProtocolViolation(
                "RNFR rename source path rejected".into(),
            ));
        }

        let rnto_cmd = format!("RNTO {}", to);
        write_command(&mut self.control_stream, &rnto_cmd).await?;

        let (code, _) = read_response(&mut self.control_stream, &mut buf).await?;
        if code != FTP_FILE_ACTION_OK {
            return Err(BambuError::ProtocolViolation(
                "RNTO rename destination path rejected".into(),
            ));
        }
        Ok(())
    }

    /// Queries the available capacity of the MicroSD card, in bytes.
    pub async fn get_available_space(&mut self) -> Result<u64, BambuError> {
        write_command(&mut self.control_stream, "AVBL").await?;

        let mut buf = Vec::new();
        let (code, text) = read_response(&mut self.control_stream, &mut buf).await?;

        if code == FTP_SIZE_OK {
            text.parse::<u64>().map_err(|_| {
                BambuError::ProtocolViolation("Malformed AVBL numeric response".into())
            })
        } else {
            // AVBL is unrecognized on older firmware targets. Fallback to STAT query [REF-FTPS-OPS].
            write_command(&mut self.control_stream, "STAT").await?;
            let (code, stat_text) = read_response(&mut self.control_stream, &mut buf).await?;
            if code == FTP_STAT_OK {
                // Parse free bytes out of stat description lines. Real-world physical dumps vary,
                // but usually report standard numeric sizing metrics.
                let mut size_found = None;
                for word in stat_text.split_whitespace() {
                    if let Ok(val) = word.parse::<u64>() {
                        // High heuristic sizing boundary (> 100MB) to ensure we avoid smaller indexes
                        if val > FTPS_AVBL_SIZE_HEURISTIC_THRESHOLD {
                            size_found = Some(val);
                        }
                    }
                }
                size_found.ok_or(BambuError::ProtocolViolation(
                    "No valid sizing fields parsed in STAT".into(),
                ))
            } else {
                Err(BambuError::ProtocolViolation(
                    "Hardware capacity queries rejected".into(),
                ))
            }
        }
    }

    /// Issues `PASV` over control channel and extracts passive connection port details.
    async fn negotiate_passive_port(&mut self) -> Result<u16, BambuError> {
        write_command(&mut self.control_stream, "PASV").await?;

        let mut buf = Vec::new();
        let (code, text) = read_response(&mut self.control_stream, &mut buf).await?;
        if code != FTP_PASSIVE_MODE {
            return Err(BambuError::ProtocolViolation(
                "PASV port negotiation rejected".into(),
            ));
        }

        parse_pasv_port(&text)
    }

    /// Sends a QUIT command and cleanly terminates the FTP session.
    ///
    /// Best-effort: errors during QUIT are silently ignored since the
    /// connection is being torn down regardless.
    pub async fn disconnect(&mut self) {
        let _ = write_command(&mut self.control_stream, "QUIT").await;
        let mut buf = Vec::new();
        let _ = read_response(&mut self.control_stream, &mut buf).await;
    }
}

// ============================================================================
// Internal Command IO Handlers
// ============================================================================

/// Sends a formatted ASCII FTP command string cleanly terminated with CRLF boundaries.
async fn write_command<IO: AsyncIo>(stream: &mut IO, cmd: &str) -> Result<(), BambuError> {
    stream
        .write_all(cmd.as_bytes())
        .await
        .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
    stream
        .write_all(b"\r\n")
        .await
        .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
    stream
        .flush()
        .await
        .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;

    Ok(())
}

/// Reads a line-by-line buffer stream incrementally up to the terminating LF character.
///
/// Enforces a maximum line length to prevent OOM from malformed server output.
async fn read_line_raw<IO: AsyncIo>(
    stream: &mut IO,
    line_buf: &mut Vec<u8>,
) -> Result<(), BambuError> {
    line_buf.clear();
    let mut byte = [0u8; 1];
    loop {
        stream
            .read_exact(&mut byte)
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionReset))?;
        let b = byte[0];
        line_buf.push(b);
        if b == b'\n' {
            break;
        }
        if line_buf.len() >= FTP_MAX_RESPONSE_LINE_BYTES {
            return Err(BambuError::ProtocolViolation(
                "FTP response line exceeds maximum length".into(),
            ));
        }
    }
    Ok(())
}

/// Parses multi-line and single-line command channel response arrays returned by FTP servers.
///
/// Under RFC-959, standard command responses take the shape:
/// * Single-Line: `XYZ Response text\r\n`
/// * Multi-Line:
///   ```text
///   XYZ-Header description line\r\n
///    Intermediate content lines\r\n
///   XYZ Termination line\r\n
///   ```
/// Accumulates all response text across lines so multi-line body content (e.g., from STAT)
/// is preserved in the returned string.
async fn read_response<IO: AsyncIo>(
    stream: &mut IO,
    line_buf: &mut Vec<u8>,
) -> Result<(u16, String), BambuError> {
    let mut accumulated = String::new();
    let mut lines_read: usize = 0;

    loop {
        read_line_raw(stream, line_buf).await?;
        lines_read += 1;
        if lines_read > FTP_MAX_RESPONSE_LINES {
            return Err(BambuError::ProtocolViolation(
                "FTP response exceeded maximum line count".into(),
            ));
        }
        if line_buf.len() < 4 {
            continue;
        }
        let code_str = match core::str::from_utf8(&line_buf[0..3]) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let code = match code_str.parse::<u16>() {
            Ok(c) => c,
            Err(_) => continue,
        };
        let separator = line_buf[3];

        if separator == b' ' {
            let text = core::str::from_utf8(&line_buf[4..]).unwrap_or("").trim();
            if accumulated.is_empty() {
                return Ok((code, text.to_string()));
            }
            if !text.is_empty() {
                accumulated.push('\n');
                accumulated.push_str(text);
            }
            return Ok((code, accumulated));
        } else if separator == b'-' {
            let line_text = core::str::from_utf8(&line_buf[4..]).unwrap_or("").trim();
            if !accumulated.is_empty() {
                accumulated.push('\n');
            }
            accumulated.push_str(line_text);
        }
    }
}

/// Extracts the passive port number from a PASV response text.
///
/// Parses the `(IP_1,IP_2,IP_3,IP_4,PORT_1,PORT_2)` tuple and computes
/// the port as `PORT_1 * 256 + PORT_2`.
pub(crate) fn parse_pasv_port(text: &str) -> Result<u16, BambuError> {
    let start = text
        .find('(')
        .ok_or(BambuError::ProtocolViolation("Invalid PASV format".into()))?;
    let end = text
        .find(')')
        .ok_or(BambuError::ProtocolViolation("Invalid PASV format".into()))?;
    let inner = &text[start + 1..end];
    let mut parts = inner.split(',');

    let _ = parts.next();
    let _ = parts.next();
    let _ = parts.next();
    let _ = parts.next();

    let p1 =
        parts
            .next()
            .and_then(|p| p.parse::<u16>().ok())
            .ok_or(BambuError::ProtocolViolation(
                "Failed to parse PORT_1 in PASV".into(),
            ))?;
    let p2 =
        parts
            .next()
            .and_then(|p| p.parse::<u16>().ok())
            .ok_or(BambuError::ProtocolViolation(
                "Failed to parse PORT_2 in PASV".into(),
            ))?;

    let port = (p1 as u32) * (FTPS_PASV_PORT_MULTIPLIER as u32) + (p2 as u32);
    if port > u16::MAX as u32 {
        return Err(BambuError::ProtocolViolation(
            "PASV port value out of range".into(),
        ));
    }
    Ok(port as u16)
}

/// Utility capturing passive stream data up to socket EOF bounds.
async fn read_to_eof<IO: AsyncIo>(stream: &mut IO, out: &mut Vec<u8>) -> Result<(), BambuError> {
    let mut chunk = [0u8; FTPS_DATA_READ_BUF_SIZE];
    loop {
        match stream.read(&mut chunk).await {
            Ok(0) => break, // Socket closed gracefully (EOF reached)
            Ok(n) => out.extend_from_slice(&chunk[..n]),
            Err(_) => return Err(BambuError::NetworkError(SocketError::ConnectionAborted)),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_pasv_response() {
        // Port = 192 * 256 + 168 = 49320
        let port =
            parse_pasv_port("Entering Passive Mode (127,0,0,1,192,168).").expect("valid PASV");
        assert_eq!(port, 49320);
    }

    #[test]
    fn test_pasv_port_zero() {
        let port = parse_pasv_port("Entering Passive Mode (127,0,0,1,0,21).").expect("valid PASV");
        assert_eq!(port, 21);
    }

    #[test]
    fn test_pasv_missing_parentheses() {
        let result = parse_pasv_port("227 No parentheses here");
        assert!(
            matches!(result, Err(BambuError::ProtocolViolation(_))),
            "Expected ProtocolViolation for missing parentheses"
        );
    }

    #[test]
    fn test_pasv_non_numeric_port() {
        let result = parse_pasv_port("(127,0,0,1,abc,168)");
        assert!(
            matches!(result, Err(BambuError::ProtocolViolation(_))),
            "Expected ProtocolViolation for non-numeric PORT_1"
        );
    }

    #[test]
    fn test_pasv_incomplete_components() {
        let result = parse_pasv_port("(127,0,0,1,192)");
        assert!(
            matches!(result, Err(BambuError::ProtocolViolation(_))),
            "Expected ProtocolViolation for missing PORT_2"
        );
    }

    #[test]
    fn test_pasv_empty_parens() {
        let result = parse_pasv_port("()");
        assert!(
            matches!(result, Err(BambuError::ProtocolViolation(_))),
            "Expected ProtocolViolation for empty parentheses"
        );
    }

    #[test]
    fn test_pasv_port_overflow() {
        let result = parse_pasv_port("(127,0,0,1,256,0)");
        assert!(
            matches!(result, Err(BambuError::ProtocolViolation(_))),
            "Expected ProtocolViolation for port exceeding u16::MAX"
        );
    }
}
