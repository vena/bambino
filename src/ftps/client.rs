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
use crate::io::{AsyncIo, RawStreamFactory, SocketError, TlsConnector, TlsVersion};
use crate::models::BambuModel;

use super::protocol::*;

/// Lightweight, high-reliability implicit FTPS client running on top of abstract I/O traits.
///
/// **Poisoning invariant:** if a data-channel transfer (`list_directory`/`upload_file`/
/// `download_file`) fails after the server has sent its `150`/`125` "opening data connection"
/// reply but before the matching final reply (`226`/etc.) has been read off the control channel,
/// the control channel is left mid-response. Reusing the client at that point risks a later,
/// unrelated command silently reading the stale trailing reply instead of its own. To make this
/// safe, the client sets `poisoned = true` on every such error path (and unconditionally in
/// `disconnect()`); every public method checks the flag first and returns
/// [`BambuError::ProtocolViolation`] immediately if set. A poisoned client must be discarded —
/// reconnect via a fresh [`BambuFtpsClient::connect`] call instead of reusing the instance.
pub struct BambuFtpsClient<RawIO, Tls, Factory>
where
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: RawStreamFactory<RawIO>,
{
    control_stream: Tls::Stream,
    tls_connector: Tls,
    data_factory: Factory,
    model: BambuModel,
    ip: String,
    /// Set once a control-channel desync is possible (see struct doc comment). Checked by every
    /// public method; once `true` the client must be discarded and reconnected.
    poisoned: bool,
    /// `read_line_raw`'s leftover-byte carry buffer (review/ftps.md Phase 6), threaded through
    /// every `read_response` call made against `control_stream` for the life of this client —
    /// not reset per method call. This must live at least as long as `control_stream` itself:
    /// FTP servers may write two logically separate replies to one command (e.g. `150`
    /// immediately followed by `226`) without waiting for the client to finish reading the
    /// first, so a single socket read can contain bytes belonging to a reply a *later* method
    /// call is expecting. Scoping this buffer to a single `read_response` call instead (an
    /// earlier version of this fix did) silently dropped those bytes and desynced the next
    /// read — confirmed via `tests/ftps_test.rs::test_ftps_download_file` failing with a
    /// spurious `ConnectionReset` when scoped too narrowly.
    control_fill_buf: Vec<u8>,
}

impl<RawIO, Tls, Factory> BambuFtpsClient<RawIO, Tls, Factory>
where
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: RawStreamFactory<RawIO>,
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
        let mut control_stream = tls_connector.connect(ip, raw_control).await?;

        Self::require_tls_1_2_if_enforced(&tls_connector, &control_stream, model)?;

        let mut buf = Vec::new();
        // Persists across every read_response call in this login sequence, and is carried
        // forward into `Self` below — see `control_fill_buf`'s doc comment on the struct.
        let mut fill_buf = Vec::new();

        let (code, _) = read_response(&mut control_stream, &mut buf, &mut fill_buf).await?;
        if code != FTP_GREETING {
            return Err(BambuError::ProtocolViolation(
                "Unexpected greeting from FTP server".into(),
            ));
        }

        write_command(&mut control_stream, "USER bblp").await?;
        let (code, text) = read_response(&mut control_stream, &mut buf, &mut fill_buf).await?;
        log::debug!("FTPS USER response: code={code} text={text:?}");
        if code != FTP_PASSWORD_NEEDED {
            return Err(BambuError::ProtocolViolation(
                "USER authentication phase rejected".into(),
            ));
        }

        let pass_cmd = format!("PASS {}", access_code);
        write_command(&mut control_stream, &pass_cmd).await?;
        let (code, text) = read_response(&mut control_stream, &mut buf, &mut fill_buf).await?;
        log::debug!("FTPS PASS response: code={code} text={text:?}");

        if code != FTP_LOGIN_OK {
            return Err(BambuError::AccessDenied);
        }

        write_command(&mut control_stream, "PBSZ 0").await?;
        let (code, text) = read_response(&mut control_stream, &mut buf, &mut fill_buf).await?;
        log::debug!("FTPS PBSZ response: code={code} text={text:?}");
        if code != FTP_COMMAND_OK {
            return Err(BambuError::ProtocolViolation(
                "PBSZ protection sizing configuration failed".into(),
            ));
        }

        // Handle model-specific TLS Protection constraints [REF-FTPS-CONN]
        if !model.quirks().uses_plaintext_ftps_data_channel() {
            write_command(&mut control_stream, "PROT P").await?;
            let (code, text) = read_response(&mut control_stream, &mut buf, &mut fill_buf).await?;
            log::debug!("FTPS PROT P response: code={code} text={text:?}");
            if code != FTP_COMMAND_OK {
                return Err(BambuError::ProtocolViolation(
                    "Failed to enable TLS data channel protection".into(),
                ));
            }
        }

        // Set binary transfer mode — RFC 959 defaults to ASCII which corrupts binary payloads.
        write_command(&mut control_stream, "TYPE I").await?;
        let (code, text) = read_response(&mut control_stream, &mut buf, &mut fill_buf).await?;
        log::debug!("FTPS TYPE I response: code={code} text={text:?}");
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
            poisoned: false,
            control_fill_buf: fill_buf,
        })
    }

    /// Returns an error if this client has been poisoned by a prior control-channel desync.
    ///
    /// See the struct-level doc comment for the invariant this enforces. Called first by every
    /// public method on this client.
    fn check_poisoned(&self) -> Result<(), BambuError> {
        if self.poisoned {
            return Err(BambuError::ProtocolViolation(
                "FTPS client is poisoned after a previous control-channel desync — this \
                 instance must be discarded; reconnect with a new BambuFtpsClient::connect() \
                 call instead of reusing it"
                    .into(),
            ));
        }
        Ok(())
    }

    /// Fail-closed TLS-1.2 guard, shared by the control-channel check in `connect()` and the
    /// per-data-channel re-check in `list_directory`/`upload_file`/`download_file` (defense in
    /// depth — see `review/ftps.md` Phase 3: session resumption is expected to carry the
    /// control channel's negotiated version onto each data channel, but this isn't verified by
    /// this code, so the guard is re-run per connection rather than assumed to hold
    /// transitively).
    fn require_tls_1_2_if_enforced(
        tls_connector: &Tls,
        stream: &Tls::Stream,
        model: BambuModel,
    ) -> Result<(), BambuError> {
        if model.quirks().enforce_ftps_tls_1_2()
            && tls_connector.negotiated_version(stream) != Some(TlsVersion::Tls12)
        {
            return Err(BambuError::ProtocolViolation(
                "This model requires TLS 1.2 for FTPS but either a different version was \
                 negotiated or the negotiated version could not be determined \
                 — configure the TlsConnector with force_tls_1_2 enabled"
                    .into(),
            ));
        }
        Ok(())
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
        self.check_poisoned()?;
        validate_ftp_path(remote_path)?;

        let port = self.negotiate_passive_port().await?;
        let raw_data_socket = self.data_factory.dial(&self.ip, port).await?;

        let list_cmd = format!("LIST {}", remote_path);
        write_command(&mut self.control_stream, &list_cmd).await?;

        let mut ctrl_buf = Vec::new();
        let (code, _) = read_response(
            &mut self.control_stream,
            &mut ctrl_buf,
            &mut self.control_fill_buf,
        )
        .await?;
        if code != FTP_TRANSFER_OPENING && code != FTP_TRANSFER_STARTING {
            return Err(BambuError::ProtocolViolation(
                "LIST transfer initialization failed".into(),
            ));
        }

        // From here on, the server has committed to sending a final reply once the data
        // transfer concludes. Any error before that reply is read off the control channel
        // leaves it desynced for the next command — poison the client on every such path
        // (Phase 2) so a caller gets an immediate, clear error instead of a later command
        // silently misreading this stale reply.
        let mut listing_payload = Vec::new();
        if !self.model.quirks().uses_plaintext_ftps_data_channel() {
            let mut secure_data_socket =
                match self.tls_connector.connect(&self.ip, raw_data_socket).await {
                    Ok(s) => s,
                    Err(e) => {
                        self.poisoned = true;
                        return Err(e.into());
                    }
                };
            if let Err(e) = Self::require_tls_1_2_if_enforced(
                &self.tls_connector,
                &secure_data_socket,
                self.model,
            ) {
                self.poisoned = true;
                return Err(e);
            }
            if let Err(e) = read_to_eof(&mut secure_data_socket, &mut listing_payload).await {
                self.poisoned = true;
                return Err(e);
            }
            drop(secure_data_socket);
        } else {
            let mut plain_data_socket = raw_data_socket;
            if let Err(e) = read_to_eof(&mut plain_data_socket, &mut listing_payload).await {
                self.poisoned = true;
                return Err(e);
            }
            drop(plain_data_socket);
        }

        let (code, _) = match read_response(
            &mut self.control_stream,
            &mut ctrl_buf,
            &mut self.control_fill_buf,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                self.poisoned = true;
                return Err(e);
            }
        };
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
        self.check_poisoned()?;
        validate_ftp_path(remote_path)?;

        let size_cmd = format!("SIZE {}", remote_path);
        write_command(&mut self.control_stream, &size_cmd).await?;

        let mut buf = Vec::new();
        let (code, text) = read_response(
            &mut self.control_stream,
            &mut buf,
            &mut self.control_fill_buf,
        )
        .await?;
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
        self.check_poisoned()?;
        validate_ftp_path(remote_path)?;

        let dele_cmd = format!("DELE {}", remote_path);
        write_command(&mut self.control_stream, &dele_cmd).await?;

        let mut buf = Vec::new();
        let (code, _) = read_response(
            &mut self.control_stream,
            &mut buf,
            &mut self.control_fill_buf,
        )
        .await?;

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
        self.check_poisoned()?;
        validate_ftp_path(remote_path)?;

        let port = self.negotiate_passive_port().await?;
        let raw_data_socket = self.data_factory.dial(&self.ip, port).await?;

        let stor_cmd = format!("STOR {}", remote_path);
        write_command(&mut self.control_stream, &stor_cmd).await?;

        let mut ctrl_buf = Vec::new();
        let (code, _) = read_response(
            &mut self.control_stream,
            &mut ctrl_buf,
            &mut self.control_fill_buf,
        )
        .await?;
        if code != FTP_TRANSFER_OPENING && code != FTP_TRANSFER_STARTING {
            return Err(BambuError::ProtocolViolation(
                "STOR upload negotiation rejected".into(),
            ));
        }

        // From here on, the server has committed to sending a final reply once the data
        // transfer concludes. Any error before that reply is read off the control channel
        // leaves it desynced for the next command — poison the client on every such path
        // (Phase 2) so a caller gets an immediate, clear error instead of a later command
        // silently misreading this stale reply.
        if !self.model.quirks().uses_plaintext_ftps_data_channel() {
            let mut secure_data_socket =
                match self.tls_connector.connect(&self.ip, raw_data_socket).await {
                    Ok(s) => s,
                    Err(e) => {
                        self.poisoned = true;
                        return Err(e.into());
                    }
                };
            if let Err(e) = Self::require_tls_1_2_if_enforced(
                &self.tls_connector,
                &secure_data_socket,
                self.model,
            ) {
                self.poisoned = true;
                return Err(e);
            }

            let mut offset = 0;
            while offset < data.len() {
                let chunk_size = core::cmp::min(FTPS_UPLOAD_CHUNK_SIZE, data.len() - offset);
                if let Err(_e) = secure_data_socket
                    .write_all(&data[offset..offset + chunk_size])
                    .await
                {
                    self.poisoned = true;
                    return Err(BambuError::NetworkError(SocketError::ConnectionAborted));
                }
                offset += chunk_size;
            }
            if let Err(_e) = secure_data_socket.flush().await {
                self.poisoned = true;
                return Err(BambuError::NetworkError(SocketError::ConnectionAborted));
            }

            drop(secure_data_socket);
        } else {
            let mut plain_data_socket = raw_data_socket;
            let mut offset = 0;
            while offset < data.len() {
                let chunk_size = core::cmp::min(FTPS_UPLOAD_CHUNK_SIZE, data.len() - offset);
                if let Err(_e) = plain_data_socket
                    .write_all(&data[offset..offset + chunk_size])
                    .await
                {
                    self.poisoned = true;
                    return Err(BambuError::NetworkError(SocketError::ConnectionAborted));
                }
                offset += chunk_size;
            }
            if let Err(_e) = plain_data_socket.flush().await {
                self.poisoned = true;
                return Err(BambuError::NetworkError(SocketError::ConnectionAborted));
            }
            drop(plain_data_socket);
        }

        let res = read_response(
            &mut self.control_stream,
            &mut ctrl_buf,
            &mut self.control_fill_buf,
        )
        .await;
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
            Err(e) => {
                self.poisoned = true;
                Err(e)
            }
        }
    }

    /// Downloads the contents of a remote file from MicroSD storage via the RETR command.
    ///
    /// Negotiates a passive data channel, retrieves the binary payload, and returns the raw bytes.
    pub async fn download_file(&mut self, remote_path: &str) -> Result<Vec<u8>, BambuError> {
        self.check_poisoned()?;
        validate_ftp_path(remote_path)?;

        let port = self.negotiate_passive_port().await?;
        let raw_data_socket = self.data_factory.dial(&self.ip, port).await?;

        let retr_cmd = format!("RETR {}", remote_path);
        write_command(&mut self.control_stream, &retr_cmd).await?;

        let mut ctrl_buf = Vec::new();
        let (code, _) = read_response(
            &mut self.control_stream,
            &mut ctrl_buf,
            &mut self.control_fill_buf,
        )
        .await?;
        if code != FTP_TRANSFER_OPENING && code != FTP_TRANSFER_STARTING {
            return Err(BambuError::ProtocolViolation(
                "RETR transfer initialization failed".into(),
            ));
        }

        // From here on, the server has committed to sending a final reply once the data
        // transfer concludes. Any error before that reply is read off the control channel
        // leaves it desynced for the next command — poison the client on every such path
        // (Phase 2) so a caller gets an immediate, clear error instead of a later command
        // silently misreading this stale reply.
        let mut file_payload = Vec::new();
        if !self.model.quirks().uses_plaintext_ftps_data_channel() {
            let mut secure_data_socket =
                match self.tls_connector.connect(&self.ip, raw_data_socket).await {
                    Ok(s) => s,
                    Err(e) => {
                        self.poisoned = true;
                        return Err(e.into());
                    }
                };
            if let Err(e) = Self::require_tls_1_2_if_enforced(
                &self.tls_connector,
                &secure_data_socket,
                self.model,
            ) {
                self.poisoned = true;
                return Err(e);
            }
            if let Err(e) = read_to_eof(&mut secure_data_socket, &mut file_payload).await {
                self.poisoned = true;
                return Err(e);
            }
            drop(secure_data_socket);
        } else {
            let mut plain_data_socket = raw_data_socket;
            if let Err(e) = read_to_eof(&mut plain_data_socket, &mut file_payload).await {
                self.poisoned = true;
                return Err(e);
            }
            drop(plain_data_socket);
        }

        let (code, _) = match read_response(
            &mut self.control_stream,
            &mut ctrl_buf,
            &mut self.control_fill_buf,
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                self.poisoned = true;
                return Err(e);
            }
        };
        if code != FTP_TRANSFER_COMPLETE {
            return Err(BambuError::ProtocolViolation(
                "RETR transfer confirmation aborted".into(),
            ));
        }

        Ok(file_payload)
    }

    /// Creates a directory on the printer's MicroSD storage.
    pub async fn create_directory(&mut self, path: &str) -> Result<(), BambuError> {
        self.check_poisoned()?;
        validate_ftp_path(path)?;

        let mkd_cmd = format!("MKD {}", path);
        write_command(&mut self.control_stream, &mkd_cmd).await?;

        let mut buf = Vec::new();
        let (code, _) = read_response(
            &mut self.control_stream,
            &mut buf,
            &mut self.control_fill_buf,
        )
        .await?;
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
        self.check_poisoned()?;
        validate_ftp_path(path)?;

        let rmd_cmd = format!("RMD {}", path);
        write_command(&mut self.control_stream, &rmd_cmd).await?;

        let mut buf = Vec::new();
        let (code, _) = read_response(
            &mut self.control_stream,
            &mut buf,
            &mut self.control_fill_buf,
        )
        .await?;
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
        self.check_poisoned()?;
        validate_ftp_path(from)?;
        validate_ftp_path(to)?;

        let rnfr_cmd = format!("RNFR {}", from);
        write_command(&mut self.control_stream, &rnfr_cmd).await?;

        let mut buf = Vec::new();
        let (code, _) = read_response(
            &mut self.control_stream,
            &mut buf,
            &mut self.control_fill_buf,
        )
        .await?;
        if code != FTP_RENAME_PENDING {
            return Err(BambuError::ProtocolViolation(
                "RNFR rename source path rejected".into(),
            ));
        }

        let rnto_cmd = format!("RNTO {}", to);
        write_command(&mut self.control_stream, &rnto_cmd).await?;

        let (code, _) = read_response(
            &mut self.control_stream,
            &mut buf,
            &mut self.control_fill_buf,
        )
        .await?;
        if code != FTP_FILE_ACTION_OK {
            return Err(BambuError::ProtocolViolation(
                "RNTO rename destination path rejected".into(),
            ));
        }
        Ok(())
    }

    /// Issues a raw `STAT` command and returns the unparsed response code and body text.
    ///
    /// Diagnostic-only escape hatch — bypasses `get_available_space`'s `AVBL`-first, `STAT`-fallback
    /// parsing entirely so real firmware `STAT` output can be captured verbatim (e.g. via
    /// `bambino-cli files ... stat-raw`) for review, since `STAT`'s field layout isn't uniformly
    /// documented across firmware versions. Not intended for production capacity queries — use
    /// `get_available_space` for that.
    pub async fn debug_raw_stat(&mut self) -> Result<(u16, String), BambuError> {
        self.check_poisoned()?;

        write_command(&mut self.control_stream, "STAT").await?;
        let mut buf = Vec::new();
        read_response(
            &mut self.control_stream,
            &mut buf,
            &mut self.control_fill_buf,
        )
        .await
    }

    /// Queries the available capacity of the MicroSD card, in bytes.
    pub async fn get_available_space(&mut self) -> Result<u64, BambuError> {
        self.check_poisoned()?;

        write_command(&mut self.control_stream, "AVBL").await?;

        let mut buf = Vec::new();
        let (code, text) = read_response(
            &mut self.control_stream,
            &mut buf,
            &mut self.control_fill_buf,
        )
        .await?;

        if code == FTP_SIZE_OK {
            text.parse::<u64>().map_err(|_| {
                BambuError::ProtocolViolation("Malformed AVBL numeric response".into())
            })
        } else {
            Err(BambuError::ProtocolViolation(
                "Hardware capacity queries rejected".into(),
            ))
        }
    }

    /// Issues `PASV` over control channel and extracts passive connection port details.
    async fn negotiate_passive_port(&mut self) -> Result<u16, BambuError> {
        write_command(&mut self.control_stream, "PASV").await?;

        let mut buf = Vec::new();
        let (code, text) = read_response(
            &mut self.control_stream,
            &mut buf,
            &mut self.control_fill_buf,
        )
        .await?;
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
    /// connection is being torn down regardless. Non-consuming (`&mut self`, not `self`) by
    /// design (review/ftps.md Phase 7): `PrinterClient::storage()` only exposes
    /// `&mut BambuFtpsClient`, and direct-module consumers may want to disconnect and
    /// reconnect the same variable via a fresh `connect()` call without re-declaring it.
    ///
    /// Always poisons the client on the way out (extends the Phase 2 mechanism — see the
    /// struct doc comment) so every subsequent method call on this instance fails cleanly with
    /// the same "must reconnect" error, instead of a caller mistaking a disconnected client for
    /// a live one. Idempotent: calling this more than once is a no-op after the first call.
    pub async fn disconnect(&mut self) {
        if self.check_poisoned().is_err() {
            return;
        }
        let _ = write_command(&mut self.control_stream, "QUIT").await;
        let mut buf = Vec::new();
        let _ = read_response(
            &mut self.control_stream,
            &mut buf,
            &mut self.control_fill_buf,
        )
        .await;
        self.poisoned = true;
    }
}
