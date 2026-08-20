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

use embedded_io_async::{Error as _, Write};

use crate::error::Error;
use crate::ftps::parser::{CurrentDateTime, FtpFile, parse_unix_listing};
use crate::io::{AsyncIo, RawStreamFactory, SocketError, TimerProvider, TlsConnector, TlsVersion};
use crate::identity::PrinterIdentity;
use crate::models::PrinterModel;

use super::protocol::*;

/// Unifies a data-channel socket that may or may not be TLS-wrapped behind one concrete type, so `list_directory`/`upload_file`/`download_file` can share a single transfer code path instead of duplicating it once per branch.
/// `RawIO` and `Tls::Stream` are different concrete types (one wrapped in TLS, one not), so
/// returning "either" from `open_data_channel` requires this enum wrapper rather than plain `impl
/// AsyncIo`.
///
/// `CLAUDE.md` calls out this exact shape of branch duplication as the root cause of the
/// `write_command` regression (commit `6385019`) — a fix applied to one branch and missed in
/// its sibling silently reintroduces that failure class, and mocks can't distinguish
/// branch-level duplication bugs from correct code. Both variants are always reachable
/// (selected by `model.quirks().uses_plaintext_ftps_data_channel()`), so neither is dead code.
enum DataChannel<RawIO, TlsStream> {
    Plain(RawIO),
    Secure(TlsStream),
}

impl<RawIO: AsyncIo, TlsStream: AsyncIo> embedded_io_async::ErrorType
    for DataChannel<RawIO, TlsStream>
{
    type Error = embedded_io_async::ErrorKind;
}

impl<RawIO: AsyncIo, TlsStream: AsyncIo> embedded_io_async::Read for DataChannel<RawIO, TlsStream> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        match self {
            DataChannel::Plain(io) => io.read(buf).await.map_err(|e| e.kind()),
            DataChannel::Secure(io) => io.read(buf).await.map_err(|e| e.kind()),
        }
    }
}

impl<RawIO: AsyncIo, TlsStream: AsyncIo> embedded_io_async::Write
    for DataChannel<RawIO, TlsStream>
{
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        match self {
            DataChannel::Plain(io) => io.write(buf).await.map_err(|e| e.kind()),
            DataChannel::Secure(io) => io.write(buf).await.map_err(|e| e.kind()),
        }
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        match self {
            DataChannel::Plain(io) => io.flush().await.map_err(|e| e.kind()),
            DataChannel::Secure(io) => io.flush().await.map_err(|e| e.kind()),
        }
    }
}

/// Lightweight, high-reliability implicit FTPS client running on top of abstract I/O traits.
///
/// **Poisoning invariant:** the control channel is a single ordered stream — every command gets
/// exactly one reply, and a `write_command`/`read_response` failure anywhere leaves no way to
/// know whether the server's reply for that command is still coming. Reusing the client at that
/// point risks a later, unrelated command silently reading the stale reply instead of its own.
/// To make this safe, the client sets `poisoned = true` (originally only on the
/// `list_directory`/`upload_file`/`download_file` data-transfer window between the server's
/// `150`/`125` "opening data connection" reply and the matching final reply, since that's the
/// widest such window; now on every `write_command`/`read_response` failure in every method,
/// including the single-reply metadata/filesystem commands, and unconditionally in
/// `disconnect()`); every public method checks the flag first and returns
/// [`Error::ProtocolViolation`] immediately if set. A poisoned client must be discarded —
/// reconnect via a fresh [`BambuFtpsClient::connect`] call instead of reusing the instance.
///
/// **`FtpsTimer`** bounds every read against a per-call wall-clock deadline (see
/// `FTPS_READ_TIMEOUT_SECS`/`FTPS_TRANSFER_CONFIRM_TIMEOUT_SECS` in `protocol.rs`) — owned
/// independently of whatever `Timer` a `PrinterClient` that hands out this client is using,
/// since `PrinterClient::storage()` hands out direct `&mut BambuFtpsClient` access rather than
/// mediating every method call the way it does for MQTT/camera (no call site to thread
/// `&self.timer` through). Defaults to `DummyTimer` (unbounded, matching this crate's existing
/// `DummyTimer` convention) for direct (non-`PrinterClient`) callers that don't supply one.
pub struct BambuFtpsClient<RawIO, Tls, Factory, FtpsTimer = crate::client::DummyTimer>
where
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: RawStreamFactory<RawIO>,
    FtpsTimer: TimerProvider,
{
    control_stream: Tls::Stream,
    tls_connector: Tls,
    data_factory: Factory,
    model: PrinterModel,
    ip: String,
    /// The printer's serial number — carried separately from `ip` because it, not the IP, is
    /// what the printer's TLS server expects as SNI/identity (see
    /// `.claude/rules/tls-identity-sni.md`); used for the data-channel TLS connect in
    /// `open_data_channel`.
    serial: String,
    timer: FtpsTimer,
    /// Set once a control-channel desync is possible (see struct doc comment).
    /// Checked by every public method; once `true` the client must be discarded and reconnected.
    poisoned: bool,
    /// Bypasses `require_tls_1_2_if_enforced`'s rejection when set — safe despite being
    /// fail-open (see `src/ftps/CLAUDE.md`): `upload_file`'s and `download_file`'s symmetric
    /// `SIZE` rechecks already catch a truncated/corrupted transfer regardless of this flag.
    /// Only meaningful today for the `embassy` feature talking to P2S/X2D, where no available
    /// TLS backend can honestly satisfy the exact-version check.
    allow_unverified_tls_1_2: bool,
    /// `read_line_raw`'s leftover-byte carry buffer, threaded through every `read_response` call made against `control_stream` for the life of this client — not reset per method call.
    /// This must live at least as long as `control_stream` itself: FTP servers may write two logically
    /// separate replies to one command (e.g. `150` immediately followed by `226`) without waiting for
    /// the client to finish reading the first, so a single socket read can contain bytes belonging to a
    /// reply a *later* method call is expecting. Scoping this buffer to a single `read_response` call
    /// instead (an earlier version of this fix did) silently dropped those bytes and desynced the next
    /// read — confirmed via `tests/ftps_test.rs::test_ftps_download_file` failing with a spurious
    /// `ConnectionReset` when scoped too narrowly.
    control_fill_buf: Vec<u8>,
}

/// Bundles the args a login-step command shares across calls, so each call site only spells
/// out what varies: the command, its log label, the expected reply code, and the rejection.
struct LoginCtx<'a, IO, T> {
    stream: &'a mut IO,
    buf: &'a mut Vec<u8>,
    fill_buf: &'a mut Vec<u8>,
    timer: &'a T,
    deadline_ms: Option<u64>,
}

/// Sends `cmd`, reads the reply, and maps a non-matching code via `on_reject()`. `on_reject` is
/// a closure, not a fixed `ProtocolViolation` message, because a rejected PASS must return
/// `Error::AccessDenied` instead.
async fn send_and_expect<IO: AsyncIo, T: TimerProvider, F: FnOnce() -> Error>(
    ctx: &mut LoginCtx<'_, IO, T>,
    cmd: &str,
    log_label: &str,
    expected_code: u16,
    on_reject: F,
) -> Result<(), Error> {
    write_command(ctx.stream, cmd, ctx.timer, ctx.deadline_ms).await?;
    let (code, text) = read_response(ctx.stream, ctx.buf, ctx.fill_buf, ctx.timer, ctx.deadline_ms).await?;
    log::debug!("FTPS {log_label} response: code={code} text={text:?}");
    if code != expected_code {
        return Err(on_reject());
    }
    Ok(())
}

impl<RawIO, Tls, Factory, FtpsTimer> BambuFtpsClient<RawIO, Tls, Factory, FtpsTimer>
where
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: RawStreamFactory<RawIO>,
    FtpsTimer: TimerProvider,
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
        identity: PrinterIdentity,
        timer: FtpsTimer,
        allow_unverified_tls_1_2: bool,
    ) -> Result<Self, Error> {
        let (control_stream, fill_buf) = Self::connect_control_stream(
            raw_control,
            &tls_connector,
            &identity,
            &timer,
            allow_unverified_tls_1_2,
        )
        .await?;
        Ok(Self {
            control_stream,
            tls_connector,
            data_factory,
            model: identity.model,
            ip: identity.ip,
            serial: identity.serial,
            timer,
            poisoned: false,
            allow_unverified_tls_1_2,
            control_fill_buf: fill_buf,
        })
    }

    /// Performs the TLS-wrap + login handshake using only borrowed `tls_connector`/`timer`,
    /// returning the resulting stream and carry-buffer state instead of a fully-assembled
    /// `Self`.
    ///
    /// Split out of `connect()` so `PrinterClient::ensure_ftps()` (`src/client/connect.rs`)
    /// can run the handshake against `self.ftps_config`'s borrowed contents without
    /// consuming them first — a failed attempt (including a `connect_timeout_secs`
    /// timeout on a slow LAN) then leaves the config untouched for a retry, instead of
    /// permanently discarding it via a premature `.take()`. `connect()` above stays the
    /// normal owned-argument entry point for direct (non-`PrinterClient`) callers and is
    /// implemented in terms of this helper.
    pub(crate) async fn connect_control_stream(
        raw_control: RawIO,
        tls_connector: &Tls,
        identity: &PrinterIdentity,
        timer: &FtpsTimer,
        allow_unverified_tls_1_2: bool,
    ) -> Result<(Tls::Stream, Vec<u8>), Error> {
        let serial = identity.serial.as_str();
        let access_code = identity.access_code.as_str();
        let mut control_stream = tls_connector.connect(serial, raw_control).await?;

        Self::require_tls_1_2_if_enforced(
            tls_connector,
            &control_stream,
            identity.model,
            allow_unverified_tls_1_2,
        )?;

        let mut buf = Vec::new();
        // Persists across every read_response call in this login sequence, and is carried
        // forward into `Self` below — see `control_fill_buf`'s doc comment on the struct.
        let mut fill_buf = Vec::new();
        let deadline_ms = ftps_deadline_ms(timer, FTPS_READ_TIMEOUT_SECS);

        let mut ctx = LoginCtx {
            stream: &mut control_stream,
            buf: &mut buf,
            fill_buf: &mut fill_buf,
            timer,
            deadline_ms,
        };

        let (code, _) = read_response(
            ctx.stream,
            ctx.buf,
            ctx.fill_buf,
            ctx.timer,
            ctx.deadline_ms,
        )
        .await?;
        if code != FTP_GREETING {
            return Err(Error::ProtocolViolation(
                "Unexpected greeting from FTP server".into(),
            ));
        }

        send_and_expect(
            &mut ctx,
            "USER bblp",
            "USER",
            FTP_PASSWORD_NEEDED,
            || Error::ProtocolViolation("USER authentication phase rejected".into()),
        )
        .await?;

        let pass_cmd = format!("PASS {}", access_code);
        send_and_expect(
            &mut ctx,
            &pass_cmd,
            "PASS",
            FTP_LOGIN_OK,
            || Error::AccessDenied,
        )
        .await?;

        send_and_expect(
            &mut ctx,
            "PBSZ 0",
            "PBSZ",
            FTP_COMMAND_OK,
            || Error::ProtocolViolation("PBSZ protection sizing configuration failed".into()),
        )
        .await?;

        // Handle model-specific TLS Protection constraints [REF-FTPS-CONN]
        if !identity.model.quirks().uses_plaintext_ftps_data_channel() {
            send_and_expect(
                &mut ctx,
                "PROT P",
                "PROT P",
                FTP_COMMAND_OK,
                || Error::ProtocolViolation("Failed to enable TLS data channel protection".into()),
            )
            .await?;
        }

        // Set binary transfer mode — RFC 959 defaults to ASCII which corrupts binary payloads.
        send_and_expect(
            &mut ctx,
            "TYPE I",
            "TYPE I",
            FTP_COMMAND_OK,
            || Error::ProtocolViolation("TYPE I binary mode configuration failed".into()),
        )
        .await?;

        Ok((control_stream, fill_buf))
    }

    /// Assembles a `Self` from an already-established control stream plus the config that
    /// produced it — the second half of the `connect_control_stream()` split.
    /// `PrinterClient::ensure_ftps()` calls this only after `connect_control_stream()` has
    /// already succeeded, once it's safe to actually consume `self.ftps_config` via `.take()`.
    pub(crate) fn from_control_stream(
        control_stream: Tls::Stream,
        tls_connector: Tls,
        data_factory: Factory,
        identity: &PrinterIdentity,
        timer: FtpsTimer,
        allow_unverified_tls_1_2: bool,
        control_fill_buf: Vec<u8>,
    ) -> Self {
        Self {
            control_stream,
            tls_connector,
            data_factory,
            model: identity.model,
            ip: identity.ip.clone(),
            serial: identity.serial.clone(),
            timer,
            poisoned: false,
            allow_unverified_tls_1_2,
            control_fill_buf,
        }
    }

    /// Returns an error if this client has been poisoned by a prior control-channel desync.
    ///
    /// See the struct-level doc comment for the invariant this enforces. Called first by every
    /// public method on this client.
    fn check_poisoned(&self) -> Result<(), Error> {
        if self.poisoned {
            return Err(Error::ProtocolViolation(
                "FTPS client is poisoned after a previous control-channel desync — this \
                 instance must be discarded; reconnect with a new BambuFtpsClient::connect() \
                 call instead of reusing it"
                    .into(),
            ));
        }
        Ok(())
    }

    /// Computes a fresh absolute deadline `budget_secs` in the future against `self.timer`, or `None` under `DummyTimer` (unbounded) — see `ftps_deadline_ms`'s doc comment.
    /// Call this fresh immediately before each `read_response`/`read_to_eof` call rather than reusing a
    /// value computed earlier, so every call gets its own full budget.
    fn read_deadline_ms(&self, budget_secs: u64) -> Option<u64> {
        ftps_deadline_ms(&self.timer, budget_secs)
    }

    /// Writes `cmd` to the control channel, poisoning the client (see struct doc comment) and
    /// propagating the error on failure. Shared by every method's write-then-read-response
    /// pattern — was duplicated verbatim across 11 call sites.
    async fn write_command_poisoning(&mut self, cmd: &str) -> Result<(), Error> {
        // Fresh write deadline per command, same shape as the per-call read deadline: a wedged
        // printer must not be able to block the control channel indefinitely before this
        // method gets the chance to poison the client.
        let deadline_ms = ftps_deadline_ms(&self.timer, FTPS_WRITE_TIMEOUT_SECS);
        if let Err(e) = write_command(&mut self.control_stream, cmd, &self.timer, deadline_ms).await
        {
            self.poisoned = true;
            return Err(e);
        }
        Ok(())
    }

    /// Reads one control-channel response, poisoning the client (see struct doc comment) and
    /// propagating the error on failure. Shared sibling of `write_command_poisoning`
    /// — was duplicated verbatim across 13 call sites. Owns its own scratch `line_buf`, safe
    /// since only `control_fill_buf` needs to persist across calls (see `read_response`'s doc
    /// comment).
    async fn read_response_poisoning(&mut self, deadline_ms: Option<u64>) -> Result<(u16, String), Error> {
        let mut buf = Vec::new();
        match read_response(
            &mut self.control_stream,
            &mut buf,
            &mut self.control_fill_buf,
            &self.timer,
            deadline_ms,
        )
        .await
        {
            Ok(v) => Ok(v),
            Err(e) => {
                self.poisoned = true;
                Err(e)
            }
        }
    }

    /// Fail-closed TLS-1.2 guard, shared by the control-channel check in `connect()` and the
    /// per-data-channel re-check in `list_directory`/`upload_file`/`download_file` (defense in
    /// depth: session resumption is expected to carry the control channel's negotiated version
    /// onto each data channel, but this isn't verified by this code, so the guard is re-run per
    /// connection rather than assumed to hold transitively).
    fn require_tls_1_2_if_enforced(
        tls_connector: &Tls,
        stream: &Tls::Stream,
        model: PrinterModel,
        allow_unverified: bool,
    ) -> Result<(), Error> {
        if allow_unverified {
            log::warn!(
                "FTPS TLS 1.2 enforcement bypassed by caller configuration \
                 (allow_unverified_tls_1_2) — see src/ftps/CLAUDE.md"
            );
            return Ok(());
        }
        if model.quirks().enforces_ftps_tls_1_2()
            && tls_connector.negotiated_version(stream) != Some(TlsVersion::Tls12)
        {
            return Err(Error::ProtocolViolation(
                "This model requires TLS 1.2 for FTPS but either a different version was \
                 negotiated or the negotiated version could not be determined \
                 — configure the TlsConnector with force_tls_1_2 enabled"
                    .into(),
            ));
        }
        Ok(())
    }

    /// Wraps a raw data-channel socket in TLS (or not, per `model.quirks().uses_plaintext_ftps_data_channel()`), re-checking TLS-1.2 enforcement on the resulting stream, and returns it behind the unified `DataChannel` type.
    /// Poisons the client on either failure path — see the struct doc comment's poisoning invariant —
    /// so `list_directory`/`upload_file`/`download_file` can all share this one path instead of
    /// duplicating it.
    async fn open_data_channel(
        &mut self,
        raw_data_socket: RawIO,
    ) -> Result<DataChannel<RawIO, Tls::Stream>, Error> {
        if self.model.quirks().uses_plaintext_ftps_data_channel() {
            return Ok(DataChannel::Plain(raw_data_socket));
        }
        let secure = match self
            .tls_connector
            .connect(&self.serial, raw_data_socket)
            .await
        {
            Ok(s) => s,
            Err(e) => {
                self.poisoned = true;
                return Err(e.into());
            }
        };
        if let Err(e) = Self::require_tls_1_2_if_enforced(
            &self.tls_connector,
            &secure,
            self.model,
            self.allow_unverified_tls_1_2,
        ) {
            self.poisoned = true;
            return Err(e);
        }
        Ok(DataChannel::Secure(secure))
    }

    /// Queries the storage server for raw directory listings and parses their structures.
    pub async fn list_directory(
        &mut self,
        remote_path: &str,
        now: CurrentDateTime,
    ) -> Result<Vec<FtpFile>, Error> {
        self.check_poisoned()?;
        validate_ftp_path(remote_path)?;

        let port = self.negotiate_passive_port().await?;
        let raw_data_socket = self.data_factory.dial(&self.ip, port).await?;

        let list_cmd = format!("LIST {}", remote_path);
        // Poison on the initial write/read too, matching every other control-channel
        // operation in this file (per .claude/rules/ftps-poisoning.md) — an unpoisoned failure
        // here leaves the control channel in the same desynced state the poisoning mechanism
        // exists to prevent.
        self.write_command_poisoning(&list_cmd).await?;

        let deadline_ms = self.read_deadline_ms(FTPS_READ_TIMEOUT_SECS);
        let (code, _) = self.read_response_poisoning(deadline_ms).await?;
        if code != FTP_TRANSFER_OPENING && code != FTP_TRANSFER_STARTING {
            return Err(Error::ProtocolViolation(
                "LIST transfer initialization failed".into(),
            ));
        }

        // From here on, the server has committed to sending a final reply once the data
        // transfer concludes. Any error before that reply is read off the control channel
        // leaves it desynced for the next command — poison the client on every such path
        // (Phase 2) so a caller gets an immediate, clear error instead of a later command
        // silently misreading this stale reply.
        let mut listing_payload = Vec::new();
        let mut data_channel = self.open_data_channel(raw_data_socket).await?;
        if let Err(e) = read_to_eof(
            &mut data_channel,
            &mut listing_payload,
            &self.timer,
            FTPS_READ_TIMEOUT_SECS * 1000,
        )
        .await
        {
            self.poisoned = true;
            return Err(e);
        }
        drop(data_channel);

        let deadline_ms = self.read_deadline_ms(FTPS_TRANSFER_CONFIRM_TIMEOUT_SECS);
        let (code, _) = self.read_response_poisoning(deadline_ms).await?;
        // Sibling gap to upload_file/download_file's handling — the same
        // P2S/X2D TLS 1.3 close race [REF-FTPS-CONN] can arrive after read_to_eof has already
        // drained the listing to EOF, so 426 must be accepted alongside 226 here too.
        if code != FTP_TRANSFER_COMPLETE && code != FTP_TRANSFER_ABORTED {
            return Err(Error::ProtocolViolation(
                "LIST transfer confirmation aborted".into(),
            ));
        }

        // `upload_file`/`download_file` each pair their 426 tolerance with an independent SIZE
        // recheck — the compensating integrity check `src/ftps/CLAUDE.md` cites to justify the
        // fail-open `allow_unverified_tls_1_2` opt-out. A listing has no SIZE to compare
        // against, so a 426 here was tolerated with nothing backing it: a data channel closing
        // early yields a listing truncated mid-line, `parse_unix_listing` drops the truncated
        // tail as just another malformed line, and the caller silently gets a short file list.
        // Line-framing is the one integrity signal a listing does carry — a complete transfer
        // ends on a line terminator. A truncation landing exactly on a line boundary still
        // passes; this narrows the window rather than closing it, which is why 426 is not
        // tolerated here as freely as on the byte-exact-verifiable transfer paths.
        if code == FTP_TRANSFER_ABORTED
            && !listing_payload.is_empty()
            && !listing_payload.ends_with(b"\n")
        {
            // Not poisoned: the final reply was read, so the control channel is in sync —
            // per `.claude/rules/ftps-poisoning.md`, only a transport-level failure desyncs it.
            return Err(Error::ProtocolViolation(
                "LIST aborted (426) with a truncated final entry".into(),
            ));
        }

        let payload_str = core::str::from_utf8(&listing_payload).map_err(|_| {
            Error::ProtocolViolation("Non-UTF8 directory listings response".into())
        })?;

        Ok(parse_unix_listing(payload_str, now))
    }

    /// Queries the exact size of a file stored on the printer's MicroSD card.
    pub async fn get_file_size(&mut self, remote_path: &str) -> Result<u64, Error> {
        self.check_poisoned()?;
        validate_ftp_path(remote_path)?;

        let size_cmd = format!("SIZE {}", remote_path);
        self.write_command_poisoning(&size_cmd).await?;

        let deadline_ms = self.read_deadline_ms(FTPS_READ_TIMEOUT_SECS);
        let (code, text) = self.read_response_poisoning(deadline_ms).await?;
        if code != FTP_SIZE_OK {
            return Err(Error::ProtocolViolation(
                "SIZE query rejected by storage server".into(),
            ));
        }

        text.parse::<u64>().map_err(|_| {
            Error::ProtocolViolation("Invalid file size parameter returned".into())
        })
    }

    /// Removes a targeted file from non-volatile storage.
    pub async fn delete_file(&mut self, remote_path: &str) -> Result<(), Error> {
        self.check_poisoned()?;
        validate_ftp_path(remote_path)?;

        let dele_cmd = format!("DELE {}", remote_path);
        self.write_command_poisoning(&dele_cmd).await?;

        let deadline_ms = self.read_deadline_ms(FTPS_READ_TIMEOUT_SECS);
        let (code, _) = self.read_response_poisoning(deadline_ms).await?;

        if code == FTP_FILE_ACTION_OK || code == FTP_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(Error::ProtocolViolation(
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
    /// 3. Unconditionally verify the uploaded size via the `SIZE` command on both a `226` and a
    ///    transient `426` reply — this guards against silent SD card write truncation on every
    ///    model, not only the P2S/X2D TLS 1.3 close race [REF-FTPS-CONN].
    pub async fn upload_file(&mut self, remote_path: &str, data: &[u8]) -> Result<(), Error> {
        self.check_poisoned()?;
        validate_ftp_path(remote_path)?;

        let port = self.negotiate_passive_port().await?;
        let raw_data_socket = self.data_factory.dial(&self.ip, port).await?;

        let stor_cmd = format!("STOR {}", remote_path);
        // Poison on the initial write/read too — see the matching comment in
        // list_directory() above.
        self.write_command_poisoning(&stor_cmd).await?;

        let mut ctrl_buf = Vec::new();
        let deadline_ms = self.read_deadline_ms(FTPS_READ_TIMEOUT_SECS);
        let (code, _) = self.read_response_poisoning(deadline_ms).await?;
        if code != FTP_TRANSFER_OPENING && code != FTP_TRANSFER_STARTING {
            return Err(Error::ProtocolViolation(
                "STOR upload negotiation rejected".into(),
            ));
        }

        // From here on, the server has committed to sending a final reply once the data
        // transfer concludes. Any error before that reply is read off the control channel
        // leaves it desynced for the next command — poison the client on every such path
        // (Phase 2) so a caller gets an immediate, clear error instead of a later command
        // silently misreading this stale reply.
        let mut data_channel = self.open_data_channel(raw_data_socket).await?;

        for chunk in data.chunks(FTPS_UPLOAD_CHUNK_SIZE) {
            let write_result = if self.timer.has_real_clock() {
                let write_fut = data_channel.write_all(chunk);
                let sleep_fut = self
                    .timer
                    .sleep(core::time::Duration::from_secs(FTPS_WRITE_TIMEOUT_SECS));
                match crate::io::race(write_fut, sleep_fut).await {
                    crate::io::Raced::Left(r) => r,
                    crate::io::Raced::Right(_) => {
                        self.poisoned = true;
                        return Err(Error::Network(SocketError::TimedOut));
                    }
                }
            } else {
                data_channel.write_all(chunk).await
            };
            if let Err(_e) = write_result {
                self.poisoned = true;
                return Err(Error::Network(SocketError::ConnectionAborted));
            }
        }
        let flush_result = if self.timer.has_real_clock() {
            let flush_fut = data_channel.flush();
            let sleep_fut = self
                .timer
                .sleep(core::time::Duration::from_secs(FTPS_WRITE_TIMEOUT_SECS));
            match crate::io::race(flush_fut, sleep_fut).await {
                crate::io::Raced::Left(r) => r,
                crate::io::Raced::Right(_) => {
                    self.poisoned = true;
                    return Err(Error::Network(SocketError::TimedOut));
                }
            }
        } else {
            data_channel.flush().await
        };
        if let Err(_e) = flush_result {
            self.poisoned = true;
            return Err(Error::Network(SocketError::ConnectionAborted));
        }
        drop(data_channel);

        let deadline_ms = self.read_deadline_ms(FTPS_TRANSFER_CONFIRM_TIMEOUT_SECS);
        let res = read_response(
            &mut self.control_stream,
            &mut ctrl_buf,
            &mut self.control_fill_buf,
            &self.timer,
            deadline_ms,
        )
        .await;
        match res {
            Ok((FTP_TRANSFER_COMPLETE, _)) | Ok((FTP_TRANSFER_ABORTED, _)) => {
                let remote_size = self.get_file_size(remote_path).await?;
                if remote_size == data.len() as u64 {
                    Ok(())
                } else {
                    Err(Error::DiskWriteFailure)
                }
            }
            Ok((_, _)) => Err(Error::DiskWriteFailure),
            Err(e) => {
                self.poisoned = true;
                Err(e)
            }
        }
    }

    /// Downloads the contents of a remote file from MicroSD storage via the RETR command.
    ///
    /// Negotiates a passive data channel, retrieves the binary payload, and returns the raw
    /// bytes. Unconditionally verifies the downloaded length against the `SIZE` command after
    /// transfer completes — a clean `226` reply alone doesn't prove the data channel didn't
    /// close early [REF-FTPS-CONN].
    pub async fn download_file(&mut self, remote_path: &str) -> Result<Vec<u8>, Error> {
        self.check_poisoned()?;
        validate_ftp_path(remote_path)?;

        let port = self.negotiate_passive_port().await?;
        let raw_data_socket = self.data_factory.dial(&self.ip, port).await?;

        let retr_cmd = format!("RETR {}", remote_path);
        // Poison on the initial write/read too — see the matching comment in
        // list_directory() above.
        self.write_command_poisoning(&retr_cmd).await?;

        let deadline_ms = self.read_deadline_ms(FTPS_READ_TIMEOUT_SECS);
        let (code, _) = self.read_response_poisoning(deadline_ms).await?;
        if code != FTP_TRANSFER_OPENING && code != FTP_TRANSFER_STARTING {
            return Err(Error::ProtocolViolation(
                "RETR transfer initialization failed".into(),
            ));
        }

        // From here on, the server has committed to sending a final reply once the data
        // transfer concludes. Any error before that reply is read off the control channel
        // leaves it desynced for the next command — poison the client on every such path
        // (Phase 2) so a caller gets an immediate, clear error instead of a later command
        // silently misreading this stale reply.
        let mut file_payload = Vec::new();
        let mut data_channel = self.open_data_channel(raw_data_socket).await?;
        if let Err(e) = read_to_eof(
            &mut data_channel,
            &mut file_payload,
            &self.timer,
            FTPS_READ_TIMEOUT_SECS * 1000,
        )
        .await
        {
            self.poisoned = true;
            return Err(e);
        }
        drop(data_channel);

        let deadline_ms = self.read_deadline_ms(FTPS_TRANSFER_CONFIRM_TIMEOUT_SECS);
        let (code, _) = self.read_response_poisoning(deadline_ms).await?;
        // Also attempt the SIZE recheck on 426 (transient close, e.g. the documented
        // P2S/X2D TLS 1.3 close race [REF-FTPS-CONN]), matching upload_file's symmetric
        // handling — previously this branch treated 426 as an unconditional hard failure,
        // discarding an already-fully-received payload on exactly the race this recheck
        // exists to catch.
        if code != FTP_TRANSFER_COMPLETE && code != FTP_TRANSFER_ABORTED {
            return Err(Error::ProtocolViolation(
                "RETR transfer confirmation aborted".into(),
            ));
        }

        // Unconditionally verify the downloaded size via SIZE, mirroring upload_file's
        // symmetric recheck — a clean 226 alone doesn't prove the data channel didn't close
        // early (same failure class documented for P2S/X2D, or any other early-close
        // condition). The control channel was read cleanly here, so this is a plain error,
        // not a poisoning path.
        let remote_size = self.get_file_size(remote_path).await?;
        if remote_size != file_payload.len() as u64 {
            return Err(Error::ProtocolViolation(
                "Downloaded file size does not match remote SIZE (possible truncated transfer)"
                    .into(),
            ));
        }

        Ok(file_payload)
    }

    /// Creates a directory on the printer's MicroSD storage.
    pub async fn create_directory(&mut self, path: &str) -> Result<(), Error> {
        self.check_poisoned()?;
        validate_ftp_path(path)?;

        let mkd_cmd = format!("MKD {}", path);
        self.write_command_poisoning(&mkd_cmd).await?;

        let deadline_ms = self.read_deadline_ms(FTPS_READ_TIMEOUT_SECS);
        let (code, _) = self.read_response_poisoning(deadline_ms).await?;
        if code != FTP_PATHNAME_CREATED {
            return Err(Error::ProtocolViolation(
                "MKD directory creation failed".into(),
            ));
        }
        Ok(())
    }

    /// Removes a directory from the printer's MicroSD storage.
    ///
    /// Returns success for both `250` (deleted) and `550` (already absent),
    /// matching the idempotent cleanup semantics of `delete_file`.
    pub async fn remove_directory(&mut self, path: &str) -> Result<(), Error> {
        self.check_poisoned()?;
        validate_ftp_path(path)?;

        let rmd_cmd = format!("RMD {}", path);
        self.write_command_poisoning(&rmd_cmd).await?;

        let deadline_ms = self.read_deadline_ms(FTPS_READ_TIMEOUT_SECS);
        let (code, _) = self.read_response_poisoning(deadline_ms).await?;
        if code == FTP_FILE_ACTION_OK || code == FTP_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(Error::ProtocolViolation(
                "RMD directory removal request failed".into(),
            ))
        }
    }

    /// Renames a file or directory on the printer's MicroSD storage.
    ///
    /// Executes the standard FTP two-step rename sequence: `RNFR` (rename from)
    /// followed by `RNTO` (rename to).
    pub async fn rename_file(&mut self, from: &str, to: &str) -> Result<(), Error> {
        self.check_poisoned()?;
        validate_ftp_path(from)?;
        validate_ftp_path(to)?;

        let rnfr_cmd = format!("RNFR {}", from);
        self.write_command_poisoning(&rnfr_cmd).await?;

        let deadline_ms = self.read_deadline_ms(FTPS_READ_TIMEOUT_SECS);
        let (code, _) = self.read_response_poisoning(deadline_ms).await?;
        if code != FTP_RENAME_PENDING {
            return Err(Error::ProtocolViolation(
                "RNFR rename source path rejected".into(),
            ));
        }

        let rnto_cmd = format!("RNTO {}", to);
        self.write_command_poisoning(&rnto_cmd).await?;

        let deadline_ms = self.read_deadline_ms(FTPS_READ_TIMEOUT_SECS);
        let (code, _) = self.read_response_poisoning(deadline_ms).await?;
        if code != FTP_FILE_ACTION_OK {
            return Err(Error::ProtocolViolation(
                "RNTO rename destination path rejected".into(),
            ));
        }
        Ok(())
    }

    /// Queries the available capacity of the MicroSD card, in bytes.
    pub async fn get_available_space(&mut self) -> Result<u64, Error> {
        self.check_poisoned()?;

        self.write_command_poisoning("AVBL").await?;

        let deadline_ms = self.read_deadline_ms(FTPS_READ_TIMEOUT_SECS);
        let (code, text) = self.read_response_poisoning(deadline_ms).await?;

        if code == FTP_SIZE_OK {
            text.parse::<u64>().map_err(|_| {
                Error::ProtocolViolation("Malformed AVBL numeric response".into())
            })
        } else {
            Err(Error::ProtocolViolation(
                "Hardware capacity queries rejected".into(),
            ))
        }
    }

    /// Issues `PASV` over control channel and extracts passive connection port details.
    async fn negotiate_passive_port(&mut self) -> Result<u16, Error> {
        self.write_command_poisoning("PASV").await?;

        let deadline_ms = self.read_deadline_ms(FTPS_READ_TIMEOUT_SECS);
        let (code, text) = self.read_response_poisoning(deadline_ms).await?;
        if code != FTP_PASSIVE_MODE {
            return Err(Error::ProtocolViolation(
                "PASV port negotiation rejected".into(),
            ));
        }

        parse_pasv_port(&text)
    }

    /// Sends a QUIT command and cleanly terminates the FTP session.
    ///
    /// Best-effort: errors during QUIT are silently ignored since the connection is being torn
    /// down regardless. Non-consuming (`&mut self`, not `self`) by design:
    /// `PrinterClient::storage()` only exposes `&mut BambuFtpsClient`, and direct-module
    /// consumers may want to disconnect and reconnect the same variable via a fresh `connect()`
    /// call without re-declaring it.
    ///
    /// Always poisons the client on the way out (extends the poisoning mechanism — see the
    /// struct doc comment) so every subsequent method call on this instance fails cleanly with
    /// the same "must reconnect" error, instead of a caller mistaking a disconnected client for
    /// a live one. Idempotent: calling this more than once is a no-op after the first call.
    pub async fn disconnect(&mut self) {
        if self.check_poisoned().is_err() {
            return;
        }
        let write_deadline_ms = ftps_deadline_ms(&self.timer, FTPS_WRITE_TIMEOUT_SECS);
        let _ = write_command(
            &mut self.control_stream,
            "QUIT",
            &self.timer,
            write_deadline_ms,
        )
        .await;
        let mut buf = Vec::new();
        let deadline_ms = self.read_deadline_ms(FTPS_READ_TIMEOUT_SECS);
        let _ = read_response(
            &mut self.control_stream,
            &mut buf,
            &mut self.control_fill_buf,
            &self.timer,
            deadline_ms,
        )
        .await;
        self.poisoned = true;
    }
}
