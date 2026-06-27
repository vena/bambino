//! # Implicit FTPS Client Implementation
//!
//! Implements a secure, platform-agnostic, asynchronous FTPS client designed to execute
//! over our abstract `AsyncIo` boundaries. This client coordinates implicitly encrypted control channels
//! on Port 990, Passive port negotiation, TLS session wrapping (with A1-series plaintext bypass),
//! whitespace-insensitive UNIX listings parsing, and robust chunked uploads [REF-FTPS-CONN] [REF-FTPS-OPS].

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use embedded_io_async::Write;

use crate::error::BambuError;
use crate::ftps::parser::{FtpFile, parse_unix_listing};
use crate::io::{AsyncIo, SocketError, TlsConnector};
use crate::models::BambuModel;

use super::protocol::*;

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
        let mut control_stream = tls_connector
            .connect(ip, FTPS_IMPLICIT_PORT, raw_control)
            .await?;

        let mut buf = Vec::new();

        let (code, _) = read_response(&mut control_stream, &mut buf).await?;
        if code != FTP_GREETING {
            return Err(BambuError::ProtocolViolation(
                "Unexpected greeting from FTP server".into(),
            ));
        }

        write_command(&mut control_stream, "USER bblp").await?;
        let (code, _) = read_response(&mut control_stream, &mut buf).await?;
        if code != FTP_PASSWORD_NEEDED {
            return Err(BambuError::ProtocolViolation(
                "USER authentication phase rejected".into(),
            ));
        }

        let pass_cmd = format!("PASS {}", access_code);
        write_command(&mut control_stream, &pass_cmd).await?;
        let (code, _) = read_response(&mut control_stream, &mut buf).await?;
        if code != FTP_LOGIN_OK {
            return Err(BambuError::AccessDenied);
        }

        write_command(&mut control_stream, "PBSZ 0").await?;
        let (code, _) = read_response(&mut control_stream, &mut buf).await?;
        if code != FTP_COMMAND_OK {
            return Err(BambuError::ProtocolViolation(
                "PBSZ protection sizing configuration failed".into(),
            ));
        }

        // Handle model-specific TLS Protection constraints [REF-FTPS-CONN]
        if !model.quirks().uses_plaintext_ftps_data_channel() {
            write_command(&mut control_stream, "PROT P").await?;
            let (code, _) = read_response(&mut control_stream, &mut buf).await?;
            if code != FTP_COMMAND_OK {
                return Err(BambuError::ProtocolViolation(
                    "Failed to enable TLS data channel protection".into(),
                ));
            }
        }

        // Set binary transfer mode — RFC 959 defaults to ASCII which corrupts binary payloads.
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

        let list_cmd = format!("LIST {}", remote_path);
        write_command(&mut self.control_stream, &list_cmd).await?;

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
            drop(secure_data_socket);
        } else {
            let mut plain_data_socket = raw_data_socket;
            read_to_eof(&mut plain_data_socket, &mut listing_payload).await?;
            drop(plain_data_socket);
        }

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
            write_command(&mut self.control_stream, "STAT").await?;
            let (code, stat_text) = read_response(&mut self.control_stream, &mut buf).await?;
            if code == FTP_STAT_OK {
                let mut size_found = None;
                for word in stat_text.split_whitespace() {
                    if let Ok(val) = word.parse::<u64>() {
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
