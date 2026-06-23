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

use crate::discovery::BambuModel;
use crate::error::BambuError;
use crate::ftps::parser::{parse_unix_listing, FtpFile};
use crate::io::{AsyncIo, SocketError, TlsConnector};

/// Factory trait used to establish standard TCP socket connections to passive ports.
///
/// Under FTPS, passive transfers open fresh data socket connections back to the printer.
/// This abstract boundary permits safe standard, ESP-IDF, and bare-metal Embassy bindings.
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
            .connect(ip, 990, raw_control)
            .await
            .map_err(BambuError::NetworkError)?;

        let mut buf = Vec::new();

        // Read Server Greeting (220)
        let (code, _) = read_response(&mut control_stream, &mut buf).await?;
        if code != 220 {
            return Err(BambuError::ProtocolViolation(
                "Unexpected greeting from FTP server",
            ));
        }

        // Login USER bblp
        write_command(&mut control_stream, "USER bblp").await?;
        let (code, _) = read_response(&mut control_stream, &mut buf).await?;
        if code != 331 {
            return Err(BambuError::ProtocolViolation(
                "USER authentication phase rejected",
            ));
        }

        // Login PASS <access_code>
        let pass_cmd = format!("PASS {}", access_code);
        write_command(&mut control_stream, &pass_cmd).await?;
        let (code, _) = read_response(&mut control_stream, &mut buf).await?;
        if code != 230 {
            return Err(BambuError::AccessDenied);
        }

        // Request Protection Buffer Size (PBSZ 0)
        write_command(&mut control_stream, "PBSZ 0").await?;
        let (code, _) = read_response(&mut control_stream, &mut buf).await?;
        if code != 200 {
            return Err(BambuError::ProtocolViolation(
                "PBSZ protection sizing configuration failed",
            ));
        }

        // Handle model-specific TLS Protection constraints [REF-FTPS-CONN]
        if !model.quirks().uses_plaintext_ftps_data_channel() {
            // Standard lines protect passive channels via PROT P (Private/TLS)
            write_command(&mut control_stream, "PROT P").await?;
            let (code, _) = read_response(&mut control_stream, &mut buf).await?;
            if code != 200 {
                return Err(BambuError::ProtocolViolation(
                    "Failed to enable TLS data channel protection",
                ));
            }
        } else {
            // A1 series does not support TLS on passive channels due to ESP32 constraints.
            // Bypassing PROT P keeps the data channel in PROT C (Clear/Plaintext) while commands remain secure.
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
        let raw_data_socket = self
            .data_factory
            .create_data_stream(&self.ip, port)
            .await
            .map_err(BambuError::NetworkError)?;

        // Command the control socket to list files
        let list_cmd = format!("LIST {}", remote_path);
        write_command(&mut self.control_stream, &list_cmd).await?;

        // Immediately read transient opening response on control channel
        let mut ctrl_buf = Vec::new();
        let (code, _) = read_response(&mut self.control_stream, &mut ctrl_buf).await?;
        if code != 150 && code != 125 {
            return Err(BambuError::ProtocolViolation(
                "LIST transfer initialization failed",
            ));
        }

        // Wrap passive data socket if secure channel is required [REF-FTPS-CONN]
        let mut listing_payload = Vec::new();
        if !self.model.quirks().uses_plaintext_ftps_data_channel() {
            let mut secure_data_socket = self
                .tls_connector
                .connect(&self.ip, port, raw_data_socket)
                .await
                .map_err(BambuError::NetworkError)?;
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
        if code != 226 {
            return Err(BambuError::ProtocolViolation(
                "LIST transfer confirmation aborted",
            ));
        }

        let payload_str = core::str::from_utf8(&listing_payload)
            .map_err(|_| BambuError::ProtocolViolation("Non-UTF8 directory listings response"))?;

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
        if code != 213 {
            return Err(BambuError::ProtocolViolation(
                "SIZE query rejected by storage server",
            ));
        }

        text.parse::<u64>()
            .map_err(|_| BambuError::ProtocolViolation("Invalid file size parameter returned"))
    }

    /// Removes a targeted file from non-volatile storage.
    pub async fn delete_file(&mut self, remote_path: &str) -> Result<(), BambuError> {
        let dele_cmd = format!("DELE {}", remote_path);
        write_command(&mut self.control_stream, &dele_cmd).await?;

        let mut buf = Vec::new();
        let (code, _) = read_response(&mut self.control_stream, &mut buf).await?;

        // Code 250 represents successful deletion. Code 550 indicates the file is absent.
        // Both represent terminal success for cleanup operations [REF-FTPS-OPS].
        if code == 250 || code == 550 {
            Ok(())
        } else {
            Err(BambuError::ProtocolViolation(
                "DELE file removal request failed",
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
        let raw_data_socket = self
            .data_factory
            .create_data_stream(&self.ip, port)
            .await
            .map_err(BambuError::NetworkError)?;

        let stor_cmd = format!("STOR {}", remote_path);
        write_command(&mut self.control_stream, &stor_cmd).await?;

        let mut ctrl_buf = Vec::new();
        let (code, _) = read_response(&mut self.control_stream, &mut ctrl_buf).await?;
        if code != 150 && code != 125 {
            return Err(BambuError::ProtocolViolation(
                "STOR upload negotiation rejected",
            ));
        }

        if !self.model.quirks().uses_plaintext_ftps_data_channel() {
            let mut secure_data_socket = self
                .tls_connector
                .connect(&self.ip, port, raw_data_socket)
                .await
                .map_err(BambuError::NetworkError)?;

            // Chunked upload sequence
            let mut offset = 0;
            while offset < data.len() {
                let chunk_size = core::cmp::min(65536, data.len() - offset);
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
                let chunk_size = core::cmp::min(65536, data.len() - offset);
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

        // Block-wait for transfer acknowledgment on the control socket
        let res = read_response(&mut self.control_stream, &mut ctrl_buf).await;
        match res {
            Ok((226, _)) | Ok((426, _)) => {
                // Verify the remote file size matches the expected upload length unconditionally.
                // The 426 path handles the TLS 1.3 close-notify race on P2S/X2D models, but SIZE
                // verification after 226 also catches silent SD card write truncation on all models.
                let remote_size = self.get_file_size(remote_path).await?;
                if remote_size == data.len() as u64 {
                    Ok(())
                } else {
                    Err(BambuError::DiskWriteFailure)
                }
            }
            _ => Err(BambuError::DiskWriteFailure),
        }
    }

    /// Queries the available capacity of the MicroSD card, in bytes.
    pub async fn get_available_space(&mut self) -> Result<u64, BambuError> {
        write_command(&mut self.control_stream, "AVBL").await?;

        let mut buf = Vec::new();
        let (code, text) = read_response(&mut self.control_stream, &mut buf).await?;

        if code == 213 {
            text.parse::<u64>()
                .map_err(|_| BambuError::ProtocolViolation("Malformed AVBL numeric response"))
        } else {
            // AVBL is unrecognized on older firmware targets. Fallback to STAT query [REF-FTPS-OPS].
            write_command(&mut self.control_stream, "STAT").await?;
            let (code, stat_text) = read_response(&mut self.control_stream, &mut buf).await?;
            if code == 211 {
                // Parse free bytes out of stat description lines. Real-world physical dumps vary,
                // but usually report standard numeric sizing metrics.
                let mut size_found = None;
                for word in stat_text.split_whitespace() {
                    if let Ok(val) = word.parse::<u64>() {
                        // High heuristic sizing boundary (> 100MB) to ensure we avoid smaller indexes
                        if val > 100_000_000 {
                            size_found = Some(val);
                        }
                    }
                }
                size_found.ok_or(BambuError::ProtocolViolation(
                    "No valid sizing fields parsed in STAT",
                ))
            } else {
                Err(BambuError::ProtocolViolation(
                    "Hardware capacity queries rejected",
                ))
            }
        }
    }

    /// Issues `PASV` over control channel and extracts passive connection port details.
    async fn negotiate_passive_port(&mut self) -> Result<u16, BambuError> {
        write_command(&mut self.control_stream, "PASV").await?;

        let mut buf = Vec::new();
        let (code, text) = read_response(&mut self.control_stream, &mut buf).await?;
        if code != 227 {
            return Err(BambuError::ProtocolViolation(
                "PASV port negotiation rejected",
            ));
        }

        // Extract (IP_1,IP_2,IP_3,IP_4,PORT_1,PORT_2) components
        let start = text
            .find('(')
            .ok_or(BambuError::ProtocolViolation("Invalid PASV format"))?;
        let end = text
            .find(')')
            .ok_or(BambuError::ProtocolViolation("Invalid PASV format"))?;
        let inner = &text[start + 1..end];
        let mut parts = inner.split(',');

        let _ = parts.next();
        let _ = parts.next();
        let _ = parts.next();
        let _ = parts.next();

        let p1 = parts.next().and_then(|p| p.parse::<u16>().ok()).ok_or(
            BambuError::ProtocolViolation("Failed to parse PORT_1 in PASV"),
        )?;
        let p2 = parts.next().and_then(|p| p.parse::<u16>().ok()).ok_or(
            BambuError::ProtocolViolation("Failed to parse PORT_2 in PASV"),
        )?;

        Ok(p1 * 256 + p2)
    }
}

// ============================================================================
// Internal Command IO Handlers
// ============================================================================

/// Sends a formatted ASCII FTP command string cleanly terminated with CRLF boundaries.
async fn write_command<IO: AsyncIo>(stream: &mut IO, cmd: &str) -> Result<(), BambuError> {
    let mut payload = String::from(cmd);
    payload.push_str("\r\n");

    stream
        .write_all(payload.as_bytes())
        .await
        .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
    stream
        .flush()
        .await
        .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;

    Ok(())
}

/// Reads a line-by-line buffer stream incrementally up to the terminating LF character.
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
///   XYZ-Another line\r\n
///   XYZ Termination line\r\n
///   ```
/// This helper keeps reading line-buffers until the final terminal signature is parsed.
async fn read_response<IO: AsyncIo>(
    stream: &mut IO,
    line_buf: &mut Vec<u8>,
) -> Result<(u16, String), BambuError> {
    loop {
        read_line_raw(stream, line_buf).await?;
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
            // Found final terminal line for the response code
            let text = core::str::from_utf8(&line_buf[4..])
                .unwrap_or("")
                .trim()
                .to_string();
            return Ok((code, text));
        } else if separator == b'-' {
            // Multi-line continuation column. Keep looping.
            continue;
        }
    }
}

/// Utility capturing passive stream data up to socket EOF bounds.
async fn read_to_eof<IO: AsyncIo>(stream: &mut IO, out: &mut Vec<u8>) -> Result<(), BambuError> {
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk).await {
            Ok(0) => break, // Socket closed gracefully (EOF reached)
            Ok(n) => out.extend_from_slice(&chunk[..n]),
            Err(_) => return Err(BambuError::NetworkError(SocketError::ConnectionAborted)),
        }
    }
    Ok(())
}
