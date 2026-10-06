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
use crate::ftps::parser::{
    CurrentDateTime, FtpFile, FtpTimestamp, parse_mdtm_timestamp, parse_unix_listing,
};
use crate::identity::PrinterIdentity;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector, TlsVersion};
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
/// including the single-reply metadata/filesystem commands); every public method checks the
/// flag first and returns [`Error::ProtocolViolation`] immediately if set. A poisoned client
/// must be replaced: [`disconnect`](Self::disconnect) returns its parts for a fresh
/// [`FtpsClient::connect`], and `PrinterClient::ftps()` does that by itself.
///
/// **`FtpsTimer`** bounds every read against a per-call wall-clock deadline (see
/// `FTPS_READ_TIMEOUT_SECS`/`FTPS_TRANSFER_CONFIRM_TIMEOUT_SECS` in `protocol.rs`) — owned
/// independently of whatever `Timer` a `PrinterClient` that hands out this client is using,
/// since `PrinterClient::ftps()` hands out direct `&mut FtpsClient` access rather than
/// mediating every method call the way it does for MQTT/camera (no call site to thread
/// `&self.timer` through). Defaults to `DummyTimer` (unbounded, matching this crate's existing
/// `DummyTimer` convention) for direct (non-`PrinterClient`) callers that don't supply one.
pub struct FtpsClient<RawIO, Tls, Factory, FtpsTimer = crate::client::DummyTimer>
where
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: RawStreamFactory<RawIO>,
    FtpsTimer: TimerProvider,
{
    /// The secure control channel. [`disconnect()`](FtpsClient::disconnect) consumes the client,
    /// so it closes and then drops this session (MbedTLS frees ~48 KB of record buffers in
    /// `Drop`, not in `close()`, GitHub issue #293) and nothing can use it afterwards.
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
    /// Re-applied to every data channel, not just the control channel — see [`TlsVersionCheck`].
    tls_version_check: TlsVersionCheck,
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

/// Sends `close_notify` on `stream`, bounded by `FTPS_WRITE_TIMEOUT_SECS` under a real clock.
///
/// Every other network await in this client is bounded; an unbounded close could block
/// forever on a printer that stopped draining its receive buffer, and on the data channel
/// that happens before the `226` read, so the transfer call would never return (#323). A
/// failure or timeout is logged and the stream is dropped by the caller either way.
async fn close_bounded<RawIO: AsyncIo, Tls: TlsConnector<RawIO>, T: TimerProvider>(
    tls_connector: &Tls,
    timer: &T,
    stream: &mut Tls::Stream,
    channel: &str,
) {
    let result = if timer.has_real_clock() {
        let close_fut = tls_connector.close(stream);
        let sleep_fut = timer.sleep(core::time::Duration::from_secs(FTPS_WRITE_TIMEOUT_SECS));
        match crate::io::race(close_fut, sleep_fut).await {
            crate::io::Raced::Left(r) => r,
            crate::io::Raced::Right(r) => Err(crate::io::deadline_error(r)),
        }
    } else {
        tls_connector.close(stream).await
    };
    if let Err(e) = result {
        log::debug!("FTPS {channel} TLS close failed: {e:?}");
    }
}

/// Whether [`FtpsClient`] checks the negotiated TLS version on models that need TLS 1.2 for FTPS.
///
/// On P2S and X2D ([`ModelQuirks::requires_ftps_tls_1_2`](crate::quirks::ModelQuirks::requires_ftps_tls_1_2))
/// the client fails closed unless exactly TLS 1.2 was negotiated, on the control channel and on
/// every data channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TlsVersionCheck {
    /// Fail closed unless TLS 1.2 was negotiated, on models that require it. The default.
    #[default]
    Enforce,
    /// Skip the check and log a warning.
    ///
    /// For a backend that cannot cap the peer at TLS 1.2 (`esp-idf`, `embassy`) talking to a
    /// printer that offers 1.3. Safe despite failing open: `upload_file`'s and
    /// `download_file`'s `SIZE` rechecks catch a truncated transfer regardless; `list_directory`
    /// has only its line-framing check (see `src/ftps/CLAUDE.md`).
    Bypass,
}

/// Returns the reply text when `reply`'s code is one of `ok`, else [`Error::FtpReply`] carrying the code and text.
///
/// The reply was read in full either way, so the client is not poisoned.
fn expect_reply(verb: &'static str, reply: (u16, String), ok: &[u16]) -> Result<String, Error> {
    let (code, text) = reply;
    if ok.contains(&code) {
        Ok(text)
    } else {
        Err(Error::FtpReply {
            command: verb,
            code,
            text: text.into(),
        })
    }
}

/// Like [`expect_reply`] for `DELE`/`RMD`, but also accepts `550` as "already absent" and logs its text.
///
/// `550` is ambiguous in FTP (absent, denied, in use, non-empty directory); which of these the
/// printer's server sends for each case is unverified, so the text is logged for diagnosis
/// (GitHub issue #392).
fn expect_reply_or_absent(verb: &'static str, reply: (u16, String), ok: u16) -> Result<(), Error> {
    if reply.0 == FTP_FILE_NOT_FOUND {
        log::debug!(
            "FTPS {verb} got 550, treated as already absent: {:?}",
            reply.1
        );
        return Ok(());
    }
    expect_reply(verb, reply, &[ok]).map(drop)
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

/// Sends `cmd`, reads the reply, and returns [`Error::FtpReply`] (labelled `verb`) unless its code is `expected_code`.
///
/// The caller maps a rejected `PASS` to [`Error::AccessDenied`] instead.
async fn send_and_expect<IO: AsyncIo, T: TimerProvider>(
    ctx: &mut LoginCtx<'_, IO, T>,
    cmd: &str,
    verb: &'static str,
    expected_code: u16,
) -> Result<(), Error> {
    write_command(ctx.stream, cmd, ctx.timer, ctx.deadline_ms).await?;
    let reply = read_response(
        ctx.stream,
        ctx.buf,
        ctx.fill_buf,
        ctx.timer,
        ctx.deadline_ms,
    )
    .await?;
    log::debug!("FTPS {verb} response: code={} text={:?}", reply.0, reply.1);
    expect_reply(verb, reply, &[expected_code]).map(drop)
}

impl<RawIO, Tls, Factory, FtpsTimer> FtpsClient<RawIO, Tls, Factory, FtpsTimer>
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
    ///
    /// `raw_control` must already be dialed to the printer's [`FTPS_PORT`](crate::ftps::FTPS_PORT).
    /// `tls_version_check` is normally [`TlsVersionCheck::Enforce`]; see that type before
    /// choosing `Bypass`.
    pub async fn connect(
        raw_control: RawIO,
        tls_connector: Tls,
        data_factory: Factory,
        identity: PrinterIdentity,
        timer: FtpsTimer,
        tls_version_check: TlsVersionCheck,
    ) -> Result<Self, Error> {
        let (control_stream, fill_buf) = Self::connect_control_stream(
            raw_control,
            &tls_connector,
            &identity,
            &timer,
            tls_version_check,
        )
        .await?;
        Ok(Self::from_control_stream(
            control_stream,
            tls_connector,
            data_factory,
            &identity,
            timer,
            tls_version_check,
            fill_buf,
        ))
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
        tls_version_check: TlsVersionCheck,
    ) -> Result<(Tls::Stream, Vec<u8>), Error> {
        let serial = identity.serial.as_str();
        let access_code = identity.access_code.as_str();
        let mut control_stream = tls_connector.connect(serial, raw_control).await?;

        Self::require_tls_1_2_if_enforced(
            tls_connector,
            &control_stream,
            identity.model,
            tls_version_check,
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

        let greeting = read_response(
            ctx.stream,
            ctx.buf,
            ctx.fill_buf,
            ctx.timer,
            ctx.deadline_ms,
        )
        .await?;
        expect_reply("greeting", greeting, &[FTP_GREETING])?;

        let user_cmd = format!("USER {}", crate::identity::LAN_USERNAME);
        send_and_expect(&mut ctx, &user_cmd, "USER", FTP_PASSWORD_NEEDED).await?;

        let pass_cmd = format!("PASS {}", access_code);
        send_and_expect(&mut ctx, &pass_cmd, "PASS", FTP_LOGIN_OK)
            .await
            .map_err(|e| match e {
                Error::FtpReply { .. } => Error::AccessDenied,
                other => other,
            })?;

        send_and_expect(&mut ctx, FTP_CMD_PBSZ, "PBSZ", FTP_COMMAND_OK).await?;

        // Handle model-specific TLS Protection constraints [REF-FTPS-CONN]
        if !identity.model.quirks().uses_plaintext_ftps_data_channel() {
            send_and_expect(&mut ctx, FTP_CMD_PROT_PRIVATE, "PROT", FTP_COMMAND_OK).await?;
        }

        send_and_expect(&mut ctx, FTP_CMD_TYPE_BINARY, "TYPE", FTP_COMMAND_OK).await?;

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
        tls_version_check: TlsVersionCheck,
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
            tls_version_check,
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
                 instance must be discarded; reconnect with a new FtpsClient::connect() \
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
    async fn read_response_poisoning(
        &mut self,
        deadline_ms: Option<u64>,
    ) -> Result<(u16, String), Error> {
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
        check: TlsVersionCheck,
    ) -> Result<(), Error> {
        if check == TlsVersionCheck::Bypass {
            log::warn!(
                "FTPS TLS 1.2 enforcement bypassed by caller configuration \
                 (TlsVersionCheck::Bypass) — see src/ftps/CLAUDE.md"
            );
            return Ok(());
        }
        if model.quirks().requires_ftps_tls_1_2()
            && tls_connector.negotiated_version(stream) != Some(TlsVersion::Tls12)
        {
            return Err(Error::ProtocolViolation(
                "This model requires TLS 1.2 for FTPS but either a different version was \
                 negotiated or the handshake had not completed — on tokio, build the \
                 TlsConnector with TlsVersions::Tls12Only; on esp-idf/embassy, which \
                 cannot cap the maximum version, use with_ftps_allow_unverified_tls_1_2(true)"
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
            self.tls_version_check,
        ) {
            self.poisoned = true;
            return Err(e);
        }
        Ok(DataChannel::Secure(secure))
    }

    /// Shuts the data channel's TLS session down and releases it, at the end of a completed
    /// transfer.
    ///
    /// The counterpart to `open_data_channel`: a data channel is per-transfer, so its memory
    /// already comes back when this consumes it, but without
    /// [`TlsConnector::close`] the printer would see every
    /// transfer end in a truncated TLS stream rather than a `close_notify` (GitHub issue #293).
    /// The `Plain` variant has nothing to close — those models run the data channel
    /// unencrypted (`uses_plaintext_ftps_data_channel`).
    ///
    /// Only the success paths route through here. A transfer that fails mid-stream has already
    /// poisoned the client and drops the channel where it errors — there is no orderly shutdown
    /// to send on a connection that just broke, and adding one would mean awaiting a peer that
    /// may be the reason the transfer failed.
    async fn close_data_channel(&self, channel: DataChannel<RawIO, Tls::Stream>) {
        if let DataChannel::Secure(mut secure) = channel {
            close_bounded(
                &self.tls_connector,
                &self.timer,
                &mut secure,
                "data-channel",
            )
            .await;
        }
    }

    /// Sends one control-channel command and reads its reply, validating `arg` as a path first.
    ///
    /// Every command goes through here, so no path can reach a command line without
    /// `validate_ftp_path` (CR/LF injection, `src/ftps/CLAUDE.md`). Write and read failures
    /// poison the client; a reply with an unexpected code does not (`.claude/rules/ftps-poisoning.md`)
    /// — that is the caller's [`expect_reply`] check.
    async fn command(
        &mut self,
        verb: &'static str,
        arg: Option<&str>,
    ) -> Result<(u16, String), Error> {
        self.check_poisoned()?;
        let line = match arg {
            Some(arg) => {
                validate_ftp_path(arg)?;
                format!("{verb} {arg}")
            }
            None => String::from(verb),
        };
        self.write_command_poisoning(&line).await?;
        let deadline_ms = self.read_deadline_ms(FTPS_READ_TIMEOUT_SECS);
        self.read_response_poisoning(deadline_ms).await
    }

    /// Opens a data transfer: `PASV`, dial, `verb path`, expect `150`/`125`, then wrap the data channel.
    ///
    /// The order on the wire is fixed and shared by `list_directory`, `upload_file` and
    /// `download_file` (`.claude/rules/wire-framing-hardware-verification.md`: changing it needs
    /// hardware verification).
    async fn open_transfer(
        &mut self,
        verb: &'static str,
        path: &str,
    ) -> Result<DataChannel<RawIO, Tls::Stream>, Error> {
        self.check_poisoned()?;
        // Validated before PASV as well as inside `command`, so a bad path never opens a data
        // connection.
        validate_ftp_path(path)?;

        let port = self.negotiate_passive_port().await?;
        let raw_data_socket = self.data_factory.dial(&self.ip, port).await?;
        let reply = self.command(verb, Some(path)).await?;
        expect_reply(verb, reply, &[FTP_TRANSFER_OPENING, FTP_TRANSFER_STARTING])?;

        // From here on, the server has committed to sending a final reply once the data
        // transfer concludes. Any error before that reply is read off the control channel
        // leaves it desynced for the next command — poison the client on every such path
        // (`.claude/rules/ftps-poisoning.md`) so a caller gets an immediate, clear error
        // instead of a later command silently misreading this stale reply.
        self.open_data_channel(raw_data_socket).await
    }

    /// Reads the data channel to EOF, poisoning the client on failure (the final reply is still owed).
    async fn read_transfer(
        &mut self,
        channel: &mut DataChannel<RawIO, Tls::Stream>,
    ) -> Result<Vec<u8>, Error> {
        let mut payload = Vec::new();
        if let Err(e) = read_to_eof(
            channel,
            &mut payload,
            &self.timer,
            FTPS_READ_TIMEOUT_SECS * 1000,
        )
        .await
        {
            self.poisoned = true;
            return Err(e);
        }
        Ok(payload)
    }

    /// Closes the data channel and reads the transfer's final reply, accepting `226` or `426`.
    ///
    /// `426` is accepted because of the documented P2S/X2D close race [REF-FTPS-CONN]; each
    /// caller pairs it with its own integrity check (a `SIZE` recheck, or `list_directory`'s
    /// line framing). Returns the code so the caller can tell the two apart.
    async fn finish_transfer(
        &mut self,
        verb: &'static str,
        channel: DataChannel<RawIO, Tls::Stream>,
    ) -> Result<u16, Error> {
        self.close_data_channel(channel).await;
        let deadline_ms = self.read_deadline_ms(FTPS_TRANSFER_CONFIRM_TIMEOUT_SECS);
        let reply = self.read_response_poisoning(deadline_ms).await?;
        let code = reply.0;
        expect_reply(verb, reply, &[FTP_TRANSFER_COMPLETE, FTP_TRANSFER_ABORTED])?;
        Ok(code)
    }

    /// Queries the storage server for raw directory listings and parses their structures.
    ///
    /// `now` must carry the **printer's** wall-clock time, not the host's. A `LIST` line omits
    /// the year for recently-modified files, and the printer's clock is the reference vsFTPd used
    /// when deciding to omit it. Bambu printers in LAN mode routinely never sync time, so the two
    /// clocks can be years apart; see [`CurrentDateTime`] for how to recover the printer's and what
    /// passing host time instead costs. Entries whose year came from `now` are flagged with
    /// [`FtpFile::year_is_inferred`].
    pub async fn list_directory(
        &mut self,
        remote_path: &str,
        now: impl Into<CurrentDateTime>,
    ) -> Result<Vec<FtpFile>, Error> {
        let mut data_channel = self.open_transfer("LIST", remote_path).await?;
        let listing_payload = self.read_transfer(&mut data_channel).await?;
        let code = self.finish_transfer("LIST", data_channel).await?;

        // `upload_file`/`download_file` each pair their 426 tolerance with an independent SIZE
        // recheck — the compensating integrity check `src/ftps/CLAUDE.md` cites to justify the
        // fail-open `TlsVersionCheck::Bypass` opt-out. A listing has no SIZE to compare
        // against, so a 426 here was tolerated with nothing backing it: a data channel closing
        // early yields a listing truncated mid-line, `parse_unix_listing` drops the truncated
        // tail as just another malformed line, and the caller silently gets a short file list.
        // Line-framing is the one integrity signal a listing does carry — a complete transfer
        // ends on a line terminator. A truncation landing exactly on a line boundary still
        // passes; this narrows the window rather than closing it, which is why 426 is not
        // tolerated here as freely as on the byte-exact-verifiable transfer paths.
        if code == FTP_TRANSFER_ABORTED
            && (listing_payload.is_empty() || !listing_payload.ends_with(b"\n"))
        {
            // Not poisoned: the final reply was read, so the control channel is in sync —
            // per `.claude/rules/ftps-poisoning.md`, only a transport-level failure desyncs it.
            // A 426 with zero bytes is not the close race (vsFTPd reports a genuinely empty
            // directory with `226` + zero bytes) — the data channel died before delivering
            // anything, and accepting it would silently map that failure to "empty directory".
            return Err(Error::ProtocolViolation(
                "LIST aborted (426) with an empty or truncated payload".into(),
            ));
        }

        let now = now.into();
        // The common case: the whole listing is valid UTF-8 and is parsed in place.
        if let Ok(listing) = core::str::from_utf8(&listing_payload) {
            return Ok(parse_unix_listing(listing, now));
        }
        // One undecodable filename (e.g. a Latin-1 name on a FAT microSD) must skip just that
        // entry, not fail the whole directory — the same per-line leniency as the parser's
        // drop-malformed-lines contract. Filtered here rather than loosening the parser to
        // lossy `&str` conversion, which would surface U+FFFD mojibake as a real name.
        let mut listing = String::with_capacity(listing_payload.len());
        for line in listing_payload.split(|&b| b == b'\n') {
            if let Ok(line) = core::str::from_utf8(line) {
                listing.push_str(line);
                listing.push('\n');
            }
        }
        Ok(parse_unix_listing(&listing, now))
    }

    /// Queries the exact size of a file stored on the printer's MicroSD card.
    pub async fn get_file_size(&mut self, remote_path: &str) -> Result<u64, Error> {
        let reply = self.command("SIZE", Some(remote_path)).await?;
        let text = expect_reply("SIZE", reply, &[FTP_SIZE_OK])?;
        text.parse::<u64>()
            .map_err(|_| Error::ProtocolViolation("Invalid file size parameter returned".into()))
    }

    /// Queries a file's absolute modification time via `MDTM`, to one-second resolution.
    ///
    /// This is the only path to a file timestamp that doesn't go through a reference clock: the
    /// `213 YYYYMMDDHHMMSS` reply carries an explicit four-digit year, where a `LIST` line omits
    /// the year entirely for recently-modified files (see [`CurrentDateTime`]). It's still the
    /// printer's own notion of when the file was written; an unsynced printer answers
    /// confidently and wrongly rather than ambiguously.
    ///
    /// Confirmed working on a P1S (`reference/02_ftps.md` §2.2); unverified on other models, which
    /// is why the return is an `Option`. These firmware builds are trimmed (the same P1S answers
    /// `502` to `STAT` even though stock vsFTPd implements it), so `Ok(None)` is returned on
    /// `500`/`502` and callers should treat it as "fall back to the `LIST` heuristic", not as an
    /// error. A missing file still surfaces as `Err`, not `None`: unsupported and absent are
    /// different answers.
    ///
    /// This is a per-file query, not a listing strategy: it costs a round trip each, and a
    /// well-used printer's card holds thousands of files. Resolving a directory this way is not
    /// worth offering: use `list_directory` against a [`CurrentDateTime`] reference for that, and
    /// reach for `MDTM` when one file's timestamp needs to be exact.
    pub async fn modification_time(
        &mut self,
        remote_path: &str,
    ) -> Result<Option<FtpTimestamp>, Error> {
        let (code, text) = self.command("MDTM", Some(remote_path)).await?;

        // An unsupported-command reply is an ordinary, fully-read control-channel response. The
        // channel is still in sync, so this reports "unsupported" without poisoning the client.
        if code == FTP_SYNTAX_ERROR || code == FTP_NOT_IMPLEMENTED {
            log::debug!("MDTM not implemented by this firmware (reply {code})");
            return Ok(None);
        }
        let text = expect_reply("MDTM", (code, text), &[FTP_SIZE_OK])?;
        parse_mdtm_timestamp(&text)
            .map(Some)
            .ok_or_else(|| Error::ProtocolViolation("Malformed MDTM timestamp returned".into()))
    }

    /// Removes a targeted file from non-volatile storage.
    ///
    /// A `550` reply is treated as "already absent" and returns `Ok`, with its text logged. FTP
    /// also uses `550` for "permission denied" and "file in use", which this cannot yet tell
    /// apart from absence (GitHub issue #392).
    pub async fn delete_file(&mut self, remote_path: &str) -> Result<(), Error> {
        let reply = self.command("DELE", Some(remote_path)).await?;
        expect_reply_or_absent("DELE", reply, FTP_FILE_ACTION_OK)
    }

    /// Uploads a binary payload directly to MicroSD card storage.
    ///
    /// **Flush and Close Race Mitigation:**
    /// 1. Send a TLS `close_notify` on the passive data channel once the payload is written,
    ///    bounded by `FTPS_WRITE_TIMEOUT_SECS`. The close does not wait for the printer's own
    ///    `close_notify` (vsFTPd doesn't send one), and a failure or timeout is logged and ignored
    ///    rather than failing the transfer. Plaintext data channels are simply dropped.
    /// 2. Wait up to 300 seconds for the `226` transfer confirmation to print. Issuing downstream
    ///    print commands prior to this confirmation halts the printer due to microSD write latency exceptions [REF-FTPS-FLUSH].
    /// 3. Unconditionally verify the uploaded size via the `SIZE` command on both a `226` and a
    ///    transient `426` reply — this guards against silent SD card write truncation on every
    ///    model, not only the P2S/X2D TLS 1.3 close race [REF-FTPS-CONN]. A size mismatch is
    ///    [`Error::DiskWriteFailure`]; a final reply other than `226`/`426` is [`Error::FtpReply`].
    ///
    /// A payload larger than [`MAX_TRANSFER_BYTES`](crate::ftps::MAX_TRANSFER_BYTES) is refused
    /// with [`Error::InvalidArgument`] before anything is sent: `download_file` would refuse to
    /// read it back.
    pub async fn upload_file(&mut self, remote_path: &str, data: &[u8]) -> Result<(), Error> {
        if data.len() > FTPS_MAX_TRANSFER_BYTES {
            return Err(Error::InvalidArgument(
                format!(
                    "upload of {} bytes exceeds the {FTPS_MAX_TRANSFER_BYTES}-byte transfer limit",
                    data.len()
                )
                .into(),
            ));
        }
        let mut data_channel = self.open_transfer("STOR", remote_path).await?;

        // One `write_all` per chunk, each with its own fresh write deadline.
        for chunk in data.chunks(FTPS_UPLOAD_CHUNK_SIZE) {
            let deadline_ms = ftps_deadline_ms(&self.timer, FTPS_WRITE_TIMEOUT_SECS);
            if let Err(e) =
                write_bounded(data_channel.write_all(chunk), &self.timer, deadline_ms).await
            {
                self.poisoned = true;
                return Err(e);
            }
        }
        let deadline_ms = ftps_deadline_ms(&self.timer, FTPS_WRITE_TIMEOUT_SECS);
        if let Err(e) = write_bounded(data_channel.flush(), &self.timer, deadline_ms).await {
            self.poisoned = true;
            return Err(e);
        }

        self.finish_transfer("STOR", data_channel).await?;
        let remote_size = self.get_file_size(remote_path).await?;
        if remote_size == data.len() as u64 {
            Ok(())
        } else {
            Err(Error::DiskWriteFailure)
        }
    }

    /// Downloads the contents of a remote file from MicroSD storage via the RETR command.
    ///
    /// Negotiates a passive data channel, retrieves the binary payload, and returns the raw
    /// bytes. Unconditionally verifies the downloaded length against the `SIZE` command after
    /// transfer completes — a clean `226` reply alone doesn't prove the data channel didn't
    /// close early [REF-FTPS-CONN].
    pub async fn download_file(&mut self, remote_path: &str) -> Result<Vec<u8>, Error> {
        let mut data_channel = self.open_transfer("RETR", remote_path).await?;
        let file_payload = self.read_transfer(&mut data_channel).await?;
        // `426` (the documented P2S/X2D close race) is accepted alongside `226` and both go
        // through the SIZE recheck below, rather than discarding a fully received payload.
        self.finish_transfer("RETR", data_channel).await?;

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
        let reply = self.command("MKD", Some(path)).await?;
        expect_reply("MKD", reply, &[FTP_PATHNAME_CREATED]).map(drop)
    }

    /// Removes a directory from the printer's MicroSD storage.
    ///
    /// Returns success for both `250` (deleted) and `550` (treated as already absent, text
    /// logged), matching `delete_file`. On common servers `550` also answers `RMD` of a non-empty
    /// directory, which this cannot yet tell apart (GitHub issue #392).
    pub async fn remove_directory(&mut self, path: &str) -> Result<(), Error> {
        let reply = self.command("RMD", Some(path)).await?;
        expect_reply_or_absent("RMD", reply, FTP_FILE_ACTION_OK)
    }

    /// Renames a file or directory on the printer's MicroSD storage.
    ///
    /// Executes the standard FTP two-step rename sequence: `RNFR` (rename from)
    /// followed by `RNTO` (rename to).
    pub async fn rename_file(&mut self, from: &str, to: &str) -> Result<(), Error> {
        // Both paths validated before anything is sent, so an invalid `to` can't strand a
        // pending RNFR.
        validate_ftp_path(to)?;
        let reply = self.command("RNFR", Some(from)).await?;
        expect_reply("RNFR", reply, &[FTP_RENAME_PENDING])?;
        let reply = self.command("RNTO", Some(to)).await?;
        expect_reply("RNTO", reply, &[FTP_FILE_ACTION_OK]).map(drop)
    }

    /// Queries the available capacity of the MicroSD card, in bytes.
    pub async fn get_available_space(&mut self) -> Result<u64, Error> {
        let reply = self.command("AVBL", None).await?;
        let text = expect_reply("AVBL", reply, &[FTP_SIZE_OK])?;
        text.parse::<u64>()
            .map_err(|_| Error::ProtocolViolation("Malformed AVBL numeric response".into()))
    }

    /// Issues `PASV` over control channel and extracts passive connection port details.
    async fn negotiate_passive_port(&mut self) -> Result<u16, Error> {
        let reply = self.command("PASV", None).await?;
        let text = expect_reply("PASV", reply, &[FTP_PASSIVE_MODE])?;
        parse_pasv_port(&text)
    }

    /// Returns true once a control-channel desync is possible; every further call on this client fails.
    ///
    /// `PrinterClient::ftps()` checks this and redials instead of handing a poisoned client back.
    #[must_use]
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    /// Sends a QUIT command, terminates the FTP session, and returns the connector, factory and timer for a reconnect.
    ///
    /// Best-effort: errors during QUIT are silently ignored since the connection is being torn
    /// down regardless. Consuming, so a disconnected client can't be used by mistake; pass the
    /// returned parts to [`connect()`](Self::connect) to start a new session.
    ///
    /// After `QUIT` the TLS session is shut down properly and then dropped, in that order:
    /// [`TlsConnector::close`] sends `close_notify` so the peer
    /// sees an orderly teardown rather than a truncated stream, and dropping the stream is what
    /// actually returns its memory — MbedTLS frees a session's record buffers in `Drop`, not in
    /// `close()`. On an ESP32-C6 that is ~48 KB recovered here instead of whenever the client
    /// itself goes out of scope (GitHub issue #293).
    ///
    /// A poisoned client skips `QUIT` and the close — its stream may be desynced or dead — and
    /// only drops the control session (#320).
    pub async fn disconnect(self) -> (Tls, Factory, FtpsTimer) {
        let Self {
            mut control_stream,
            tls_connector,
            data_factory,
            timer,
            poisoned,
            mut control_fill_buf,
            ..
        } = self;
        if !poisoned {
            let write_deadline_ms = ftps_deadline_ms(&timer, FTPS_WRITE_TIMEOUT_SECS);
            let _ = write_command(&mut control_stream, "QUIT", &timer, write_deadline_ms).await;
            let deadline_ms = ftps_deadline_ms(&timer, FTPS_READ_TIMEOUT_SECS);
            let mut buf = Vec::new();
            let _ = read_response(
                &mut control_stream,
                &mut buf,
                &mut control_fill_buf,
                &timer,
                deadline_ms,
            )
            .await;
            close_bounded(&tls_connector, &timer, &mut control_stream, "control").await;
        }
        drop(control_stream);
        (tls_connector, data_factory, timer)
    }
}
