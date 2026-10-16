//! A connected port-6000 file tunnel session [REF-FILESVC].

#[cfg(not(feature = "std"))]
use alloc::{format, string::String, vec, vec::Vec};

use serde::Serialize;
use serde::de::{DeserializeOwned, IgnoredAny};

use crate::error::Error;
use crate::ftps::protocol::{ftps_deadline_ms, write_bounded};
use crate::identity::PrinterIdentity;
use crate::io::{AsyncIo, TimerProvider, TlsConnector, read_chunk, with_deadline};

use super::protocol::{
    self as p, DownloadReply, DownloadReq, Envelope, FileDelReq, FrameHeader, ListInfoReply,
    ListInfoReq, MediaAbilityReply, MediaAbilityReq, MemDownloadStatus, ProjectFileParam,
    SubFileReply, SubFileReq, TaskCancelReq, UploadChunkReq, UploadInitReply, UploadInitReq, cmd,
    result,
};
use super::{FileKind, TunnelFile};

/// A reply body the client doesn't read, for [`FileTunnelClient::single_reply`]'s `Default` bound.
#[derive(Default)]
struct IgnoredAnyDefault;

impl<'de> serde::Deserialize<'de> for IgnoredAnyDefault {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        IgnoredAny::deserialize(d).map(|_| Self)
    }
}

/// Wall-clock budget for each handshake reply: the login ack and the setup reply.
///
/// Studio gives the whole tunnel connect 8 s before falling back to FTPS
/// (`SendToPrinterDialog::GetConnection`). A printer that doesn't serve the tunnel sends nothing at
/// all, so this timeout is what detects it; the P1S would otherwise hold the connection 60 s.
pub(crate) const FILE_TUNNEL_HANDSHAKE_TIMEOUT_SECS: u64 = 8;
/// Wall-clock budget for each frame read after the handshake, matching the other channels' 30 s.
pub(crate) const FILE_TUNNEL_READ_TIMEOUT_SECS: u64 = 30;
/// Wall-clock budget for writing one frame, and for the TLS close at disconnect.
pub(crate) const FILE_TUNNEL_WRITE_TIMEOUT_SECS: u64 = 30;
/// Frames skipped while waiting for one reply (notifies, stale replies) before the session is declared desynced.
const MAX_SKIPPED_FRAMES: usize = 64;
/// Frames skipped while waiting for a `TASK_CANCEL` reply: the abandoned stream's in-flight fragments come first.
const CANCEL_MAX_SKIPPED_FRAMES: usize = 256;
/// `mem:/` index of a member of the running job's `.3mf` (Studio's `PrinterFileSystem::GetPickImage`).
pub(crate) const MEM_PROJECT_FILE_INDEX: u32 = 16;
/// The command name a running-job member read reports in its errors.
const MEM_DOWNLOAD_COMMAND: &str = "FILE_DOWNLOAD mem:/16";
/// Largest frame accepted during the handshake: the login ack is 4 bytes and the setup reply a short JSON object.
const HANDSHAKE_MAX_FRAME_PAYLOAD: usize = 1024;

/// A session with a printer's port-6000 file tunnel.
///
/// **Poisoning.** Requests run one at a time, and each reply is matched by `sequence`. A
/// transport failure, a frame that can't be parsed, or a stream abandoned before its last frame
/// leaves frames on the wire that a later request would misread. The client then sets
/// `poisoned`; every public method returns [`Error::ProtocolViolation`] after that, and the only
/// way back is a new connection. A failure `result` the printer sends in full (`FILE_NO_EXIST`)
/// is [`Error::TunnelReply`] and leaves the session usable.
///
/// **Timer.** Every method takes the timer that bounds its reads and writes, as the camera
/// stream's `read_next_frame_with_timer` does, so the client holds only its TLS stream `S`.
/// Under a timer with no real clock (the crate's `DummyTimer`) nothing is bounded, including the
/// handshake, so a printer that never answers the login (every P1/A1) hangs
/// [`connect`](Self::connect) until it closes the socket after 60 s.
/// [`PrinterClient::file_tunnel`](crate::client::PrinterClient::file_tunnel) supplies its own
/// timer through [`FileTunnel`].
///
/// **Frame size.** Each frame's payload is allocated in full before it is parsed, so a corrupt
/// header is refused above [`DEFAULT_MAX_FRAME_PAYLOAD`](super::DEFAULT_MAX_FRAME_PAYLOAD)
/// (4 MiB). That is far more than an embedded heap holds; lower it with
/// [`with_max_frame_payload`](Self::with_max_frame_payload). The largest frames reported are
/// 20 KiB download fragments; a `LIST_INFO` reply grows with the number of files.
pub struct FileTunnelClient<S: AsyncIo> {
    stream: S,
    /// Largest frame payload accepted after the handshake.
    max_frame_payload: usize,
    /// The header `seq` of the next frame this client sends.
    frame_seq: u32,
    /// The JSON `sequence` of the next request; `0` belongs to the setup frame.
    next_sequence: u32,
    poisoned: bool,
}

/// Writes one frame: the header, then the payload, as two writes, the way both reference clients send them.
async fn write_frame<IO: AsyncIo, T: TimerProvider>(
    stream: &mut IO,
    timer: &T,
    frame_seq: &mut u32,
    magic: u32,
    payload: &[u8],
) -> Result<(), Error> {
    let payload_len = u32::try_from(payload.len())
        .map_err(|_| Error::InvalidArgument("file tunnel frame over 4 GiB".into()))?;
    let header = FrameHeader {
        payload_len,
        magic,
        seq: *frame_seq,
    }
    .encode();
    *frame_seq = frame_seq.wrapping_add(1);
    let deadline_ms = ftps_deadline_ms(timer, FILE_TUNNEL_WRITE_TIMEOUT_SECS);
    write_bounded(stream.write_all(&header), timer, deadline_ms).await?;
    if !payload.is_empty() {
        write_bounded(stream.write_all(payload), timer, deadline_ms).await?;
    }
    write_bounded(stream.flush(), timer, deadline_ms).await
}

/// Fills `buf` completely before `deadline_ms`; a peer close mid-buffer is `ConnectionReset`.
async fn read_exact_bounded<IO: AsyncIo, T: TimerProvider>(
    stream: &mut IO,
    buf: &mut [u8],
    timer: &T,
    deadline_ms: Option<u64>,
) -> Result<(), Error> {
    let mut filled = 0;
    while filled < buf.len() {
        filled += read_chunk(stream, &mut buf[filled..], timer, deadline_ms).await?;
    }
    Ok(())
}

/// Reads one whole frame, header and payload, within `budget_secs`, refusing a payload over `max_payload`.
async fn read_frame<IO: AsyncIo, T: TimerProvider>(
    stream: &mut IO,
    timer: &T,
    budget_secs: u64,
    max_payload: usize,
) -> Result<(FrameHeader, Vec<u8>), Error> {
    let deadline_ms = ftps_deadline_ms(timer, budget_secs);
    let mut header = [0u8; p::FRAME_HEADER_LEN];
    read_exact_bounded(stream, &mut header, timer, deadline_ms).await?;
    let header = FrameHeader::decode(&header);
    let len = header.payload_len as usize;
    if len > max_payload {
        return Err(Error::ProtocolViolation(
            format!("file tunnel frame declares {len} bytes, over the {max_payload} byte limit")
                .into(),
        ));
    }
    let mut payload = vec![0u8; len];
    read_exact_bounded(stream, &mut payload, timer, deadline_ms).await?;
    Ok((header, payload))
}

fn rejected() -> Error {
    Error::ProtocolViolation(
        "file tunnel refused the session (reject frame 0x0003013F); the printer does not serve \
         the file tunnel to this client"
            .into(),
    )
}

/// Logs in and runs the setup exchange on a fresh TLS stream [REF-FILESVC] §8.2.
async fn handshake<IO: AsyncIo, T: TimerProvider>(
    stream: &mut IO,
    access_code: &str,
    timer: &T,
    frame_seq: &mut u32,
) -> Result<(), Error> {
    let login = p::build_login_payload(access_code)?;
    write_frame(stream, timer, frame_seq, p::MAGIC_LOGIN, &login).await?;
    let (ack, _) = read_frame(
        stream,
        timer,
        FILE_TUNNEL_HANDSHAKE_TIMEOUT_SECS,
        HANDSHAKE_MAX_FRAME_PAYLOAD,
    )
    .await?;
    match ack.magic {
        p::MAGIC_LOGIN_ACK => {}
        p::MAGIC_REJECT => return Err(rejected()),
        // ha-bambulab accepts any frame here; only open-bamboo-networking checks the magic.
        other => log::debug!("file tunnel login answered with magic {other:#010x}"),
    }

    let pid = p::setup_pid(*frame_seq);
    let setup = p::build_setup_json(&pid)?;
    write_frame(stream, timer, frame_seq, p::MAGIC_CTRL, &setup).await?;
    let (header, payload) = read_frame(
        stream,
        timer,
        FILE_TUNNEL_HANDSHAKE_TIMEOUT_SECS,
        HANDSHAKE_MAX_FRAME_PAYLOAD,
    )
    .await?;
    if header.magic == p::MAGIC_REJECT {
        return Err(rejected());
    }
    let (json, _) = p::split_payload(&payload);
    let reply: Envelope<IgnoredAny> = p::parse_envelope(json, "setup reply")?;
    match reply.result {
        Some(result::SUCCESS) => Ok(()),
        Some(code) => Err(Error::TunnelReply {
            command: "setup",
            result: code,
        }),
        None => Err(Error::ProtocolViolation(
            "file tunnel setup reply carries no result".into(),
        )),
    }
}

/// Returns `Ok` for `SUCCESS`, and `TunnelReply` for any failure code.
///
/// `CONTINUE` is handled by the caller, since only a streamed reply may send it.
fn require_success(command: &'static str, code: Option<i64>) -> Result<(), Error> {
    match code {
        Some(result::SUCCESS) => Ok(()),
        Some(code) => Err(Error::TunnelReply {
            command,
            result: code,
        }),
        None => Err(Error::ProtocolViolation(
            format!("file tunnel {command} reply carries no result").into(),
        )),
    }
}

impl<S: AsyncIo> FileTunnelClient<S> {
    /// Wraps `raw` in TLS, logs in and runs the setup exchange.
    ///
    /// `raw` must already be dialed to the printer's [`FILE_TUNNEL_PORT`](super::FILE_TUNNEL_PORT).
    /// TLS uses the printer's serial as its identity, like every other channel; keep
    /// `tls_connector` to [`disconnect`](Self::disconnect) through. A printer that doesn't serve
    /// the tunnel either refuses the login ([`Error::ProtocolViolation`]) or, like every P1/A1,
    /// sends nothing, which the handshake timeout turns into
    /// [`SocketError::TimedOut`](crate::io::SocketError::TimedOut) when `timer` has a real clock.
    pub async fn connect<RawIO, Tls, T>(
        raw: RawIO,
        tls_connector: &Tls,
        identity: &PrinterIdentity,
        timer: &T,
    ) -> Result<Self, Error>
    where
        RawIO: AsyncIo,
        Tls: TlsConnector<RawIO, Stream = S>,
        T: TimerProvider,
    {
        identity.validate()?;
        let mut stream = tls_connector.connect(&identity.serial, raw).await?;
        // Both reference clients seed the frame counter randomly; a clock reading serves.
        let mut frame_seq = timer.unix_millis().unwrap_or_else(|| timer.now_millis()) as u32;
        handshake(&mut stream, &identity.access_code, timer, &mut frame_seq).await?;
        Ok(Self {
            stream,
            max_frame_payload: super::DEFAULT_MAX_FRAME_PAYLOAD,
            frame_seq,
            next_sequence: 1,
            poisoned: false,
        })
    }

    /// Lowers (or raises) the largest frame payload accepted, in bytes; see the struct docs.
    ///
    /// A frame over the limit is a protocol violation and poisons the session.
    #[must_use]
    pub fn with_max_frame_payload(mut self, bytes: usize) -> Self {
        self.max_frame_payload = bytes;
        self
    }

    /// Whether an earlier failure left the session out of sync; see the struct docs.
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    fn check_poisoned(&self) -> Result<(), Error> {
        if self.poisoned {
            return Err(Error::ProtocolViolation(
                "file tunnel session is out of sync after an earlier failure; reconnect".into(),
            ));
        }
        Ok(())
    }

    /// Records a failure that leaves the session out of sync, and passes it on.
    fn poison<R>(&mut self, err: Error) -> Result<R, Error> {
        self.poisoned = true;
        Err(err)
    }

    /// Sends one request and returns the `sequence` its reply will carry.
    async fn send_request<R: Serialize, T: TimerProvider>(
        &mut self,
        timer: &T,
        cmdtype: u32,
        req: &R,
        binary: &[u8],
    ) -> Result<u32, Error> {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1).max(1);
        let payload = p::encode_request(cmdtype, sequence, req, binary)?;
        if let Err(e) = write_frame(
            &mut self.stream,
            timer,
            &mut self.frame_seq,
            p::MAGIC_CTRL,
            &payload,
        )
        .await
        {
            return self.poison(e);
        }
        Ok(sequence)
    }

    /// Reads frames until one answers `cmdtype`/`sequence`, skipping notifies and stale replies.
    ///
    /// Matches on both, as ha-bambulab and open-bamboo-networking do; Studio matches on
    /// `sequence` alone. A reply that omits `cmdtype` is accepted on its `sequence`.
    async fn read_reply_frame<T: TimerProvider>(
        &mut self,
        timer: &T,
        cmdtype: u32,
        sequence: u32,
    ) -> Result<Vec<u8>, Error> {
        self.read_reply_frame_within(timer, cmdtype, sequence, MAX_SKIPPED_FRAMES)
            .await
    }

    /// [`read_reply_frame`](Self::read_reply_frame) with its own budget of skipped frames.
    async fn read_reply_frame_within<T: TimerProvider>(
        &mut self,
        timer: &T,
        cmdtype: u32,
        sequence: u32,
        max_skipped: usize,
    ) -> Result<Vec<u8>, Error> {
        for _ in 0..=max_skipped {
            let (header, payload) = match read_frame(
                &mut self.stream,
                timer,
                FILE_TUNNEL_READ_TIMEOUT_SECS,
                self.max_frame_payload,
            )
            .await
            {
                Ok(frame) => frame,
                Err(e) => return self.poison(e),
            };
            match header.magic {
                p::MAGIC_REJECT => return self.poison(rejected()),
                p::MAGIC_CTRL_REPLY => {}
                // Only open-bamboo-networking reports the reply magic; ha-bambulab ignores it.
                other => log::debug!("file tunnel reply frame with magic {other:#010x}"),
            }
            let (json, _) = p::split_payload(&payload);
            let head: Envelope<IgnoredAny> = match p::parse_envelope(json, "reply") {
                Ok(head) => head,
                Err(e) => return self.poison(e),
            };
            if head.is_notify() {
                log::debug!("file tunnel notify {:?} skipped", head.cmdtype);
                continue;
            }
            if head.sequence == Some(sequence) && head.cmdtype.is_none_or(|c| c == cmdtype) {
                return Ok(payload);
            }
            log::debug!(
                "file tunnel skipped a stale frame (cmdtype {:?}, sequence {:?}) waiting for {cmdtype}/{sequence}",
                head.cmdtype,
                head.sequence
            );
        }
        self.poison(Error::ProtocolViolation(
            format!("file tunnel sent {max_skipped} unrelated frames without answering").into(),
        ))
    }

    /// Stops an unfinished streamed reply with `TASK_CANCEL`, as Studio's `CancelRequest` does, so the session stays usable.
    ///
    /// Frames of the abandoned stream that arrive before the cancel's reply are skipped as stale.
    /// If the cancel can't be sent or answered, the session is poisoned instead.
    async fn abandon<T: TimerProvider>(&mut self, timer: &T, sequence: u32) {
        let tasks = [sequence];
        let cancelled = match self
            .send_request(
                timer,
                cmd::TASK_CANCEL,
                &TaskCancelReq { tasks: &tasks },
                &[],
            )
            .await
        {
            Ok(cancel_seq) => {
                match self
                    .read_reply_frame_within(
                        timer,
                        cmd::TASK_CANCEL,
                        cancel_seq,
                        CANCEL_MAX_SKIPPED_FRAMES,
                    )
                    .await
                {
                    Ok(payload) => {
                        let (json, _) = p::split_payload(&payload);
                        p::parse_envelope::<IgnoredAny>(json, "TASK_CANCEL")
                            .is_ok_and(|env| env.result == Some(result::SUCCESS))
                    }
                    Err(_) => false,
                }
            }
            Err(_) => false,
        };
        if !cancelled {
            log::debug!("file tunnel could not cancel sequence {sequence}; session poisoned");
            self.poisoned = true;
        }
    }

    /// Sends a single-reply request and parses its reply body.
    async fn single_reply<R: Serialize, B: DeserializeOwned + Default, T: TimerProvider>(
        &mut self,
        timer: &T,
        command: &'static str,
        cmdtype: u32,
        req: &R,
    ) -> Result<B, Error> {
        self.check_poisoned()?;
        let sequence = self.send_request(timer, cmdtype, req, &[]).await?;
        let payload = self.read_reply_frame(timer, cmdtype, sequence).await?;
        let (json, _) = p::split_payload(&payload);
        let env: Envelope<B> = p::parse_envelope(json, command)?;
        if env.result == Some(result::CONTINUE) {
            return self.poison(Error::ProtocolViolation(
                format!("file tunnel {command} answered CONTINUE to a single-reply request").into(),
            ));
        }
        require_success(command, env.result)?;
        Ok(env.reply.unwrap_or_default())
    }

    /// Lists the storages the printer has (`REQUEST_MEDIA_ABILITY`), such as `["emmc","udisk"]`.
    ///
    /// Probe this instead of guessing labels for [`list_files`](Self::list_files).
    pub async fn media_ability<T: TimerProvider>(
        &mut self,
        timer: &T,
    ) -> Result<Vec<String>, Error> {
        let reply: MediaAbilityReply = self
            .single_reply(
                timer,
                "REQUEST_MEDIA_ABILITY",
                cmd::REQUEST_MEDIA_ABILITY,
                &MediaAbilityReq {
                    peer: p::PEER_STUDIO,
                    api_version: p::API_VERSION,
                },
            )
            .await?;
        Ok(reply.storage)
    }

    /// Lists files of one kind on one storage (`LIST_INFO`).
    ///
    /// `storage` is a label from [`media_ability`](Self::media_ability), or `None` for the
    /// printer's default, which Studio's file browser treats as external storage.
    pub async fn list_files<T: TimerProvider>(
        &mut self,
        timer: &T,
        kind: FileKind,
        storage: Option<&str>,
    ) -> Result<Vec<TunnelFile>, Error> {
        let reply: ListInfoReply = self
            .single_reply(
                timer,
                "LIST_INFO",
                cmd::LIST_INFO,
                &ListInfoReq {
                    kind: kind.as_wire(),
                    storage,
                    api_version: p::API_VERSION,
                    notify: p::LIST_NOTIFY_DETAIL,
                },
            )
            .await?;
        // Studio rejects a video or model listing with a pathless entry: older firmware answers
        // every listing with timelapses (`PrinterFileSystem::ListAllFiles`).
        if kind != FileKind::Timelapse && reply.file_lists.iter().any(|f| f.path.is_empty()) {
            return Err(Error::ProtocolViolation(
                "file tunnel listing has entries without a path; this firmware lists only \
                 timelapses"
                    .into(),
            ));
        }
        Ok(reply
            .file_lists
            .into_iter()
            .map(|f| TunnelFile {
                name: f.name,
                path: (!f.path.is_empty()).then_some(f.path),
                time: f.time,
                size: f.size,
                storage: storage.map(String::from),
            })
            .collect())
    }

    /// Downloads a listed file, passing each fragment to `sink` in order, and returns its size.
    ///
    /// The transfer is checked as Studio checks it: the byte count must reach the reply's `total`,
    /// and the MD5 of the bytes must match the final frame's `file_md5`. A mismatch is reported
    /// after `sink` has seen every byte, so a caller writing to storage should treat its output as
    /// provisional until this returns `Ok`. An error from `sink` stops the transfer: the client
    /// cancels the rest with `TASK_CANCEL`, and poisons the session only if that fails.
    pub async fn download<T: TimerProvider>(
        &mut self,
        timer: &T,
        file: &TunnelFile,
        mut sink: impl FnMut(&[u8]) -> Result<(), Error>,
    ) -> Result<u64, Error> {
        self.check_poisoned()?;
        let req = DownloadReq {
            path: file.path.as_deref(),
            file: file.path.is_none().then_some(file.name.as_str()),
            offset: 0,
            mem_dl_param_size: None,
        };
        let sequence = self
            .send_request(timer, cmd::FILE_DOWNLOAD, &req, &[])
            .await?;
        self.receive_download(timer, "FILE_DOWNLOAD", sequence, None, &mut sink)
            .await
    }

    /// Reads a member of the running job's `.3mf` by its path inside the archive, with no file path (`mem:/16`, `get_project_file`).
    ///
    /// This is the read BambuStudio's skip-objects dialog makes for `Metadata/pick_<plate>.png`,
    /// `Metadata/model_settings.config` and `Metadata/slice_info.config`
    /// (`PartSkipDialog::DownloadPartsFile`). A member larger than `max_bytes` is refused before
    /// any of it is buffered. A member the printer doesn't have is refused with
    /// [`Error::TunnelReply`]; Studio treats every refusal here as "no such file". The client
    /// cancels whatever the printer would still send, and poisons the session only if that fails.
    pub async fn read_running_job_member<T: TimerProvider>(
        &mut self,
        timer: &T,
        member: &str,
        max_bytes: usize,
    ) -> Result<Vec<u8>, Error> {
        let sequence = self.request_running_job_member(timer, member).await?;
        let mut out = Vec::new();
        self.receive_download(
            timer,
            MEM_DOWNLOAD_COMMAND,
            sequence,
            Some(max_bytes as u64),
            &mut |chunk: &[u8]| {
                out.extend_from_slice(chunk);
                Ok(())
            },
        )
        .await?;
        Ok(out)
    }

    /// Like [`read_running_job_member`](Self::read_running_job_member), but fills `out` instead of allocating, and returns the member's length.
    ///
    /// A member longer than `out` is refused before any of it is written.
    pub async fn read_running_job_member_into<T: TimerProvider>(
        &mut self,
        timer: &T,
        member: &str,
        out: &mut [u8],
    ) -> Result<usize, Error> {
        let sequence = self.request_running_job_member(timer, member).await?;
        let mut filled = 0usize;
        self.receive_download(
            timer,
            MEM_DOWNLOAD_COMMAND,
            sequence,
            Some(out.len() as u64),
            &mut |chunk: &[u8]| {
                // `receive_download` keeps the running total within `out.len()`.
                let dest = out
                    .get_mut(filled..filled + chunk.len())
                    .ok_or_else(|| Error::ProtocolViolation("fragment past the buffer".into()))?;
                dest.copy_from_slice(chunk);
                filled += chunk.len();
                Ok(())
            },
        )
        .await?;
        Ok(filled)
    }

    /// Sends a `mem:/16` `get_project_file` request for `member` and returns its `sequence`.
    async fn request_running_job_member<T: TimerProvider>(
        &mut self,
        timer: &T,
        member: &str,
    ) -> Result<u32, Error> {
        self.check_poisoned()?;
        if member.is_empty() {
            return Err(Error::InvalidArgument(
                "running-job member path is empty".into(),
            ));
        }
        // Studio numbers its three reads 1, 2, 3; Bambu's own plugin repeats the request's
        // `sequence` (open-bamboo-networking capture). The printer accepts either.
        let param = crate::client::serialize(&ProjectFileParam {
            sequence_id: self.next_sequence,
            version: p::MEM_PARAM_VERSION,
            peer_host: p::PEER_STUDIO,
            command: p::MEM_COMMAND_GET_PROJECT_FILE,
            file_rel_path: member,
        })?;
        let path = format!("mem:/{MEM_PROJECT_FILE_INDEX}");
        let req = DownloadReq {
            path: Some(&path),
            file: None,
            offset: 0,
            mem_dl_param_size: Some(param.len()),
        };
        self.send_request(timer, cmd::FILE_DOWNLOAD, &req, &param)
            .await
    }

    /// Reads one member of a listed file without downloading the file (`SUB_FILE`), or `None` when the file has no such member.
    ///
    /// `member` is a path inside the archive, such as `Metadata/plate_1.png`; for a video, the
    /// member `thumbnail` is its thumbnail. `file` must carry the `path` its listing gave:
    /// `SUB_FILE` addresses members as `<path>#<member>` (Studio's
    /// `PrinterFileSystem::UpdateFocusThumbnail2`). A member larger than `max_bytes` is refused,
    /// and the rest of the stream cancelled.
    pub async fn read_file_member<T: TimerProvider>(
        &mut self,
        timer: &T,
        file: &TunnelFile,
        member: &str,
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, Error> {
        self.check_poisoned()?;
        let Some(path) = file.path.as_deref() else {
            return Err(Error::InvalidArgument(
                "SUB_FILE needs the file's path, which its listing didn't give".into(),
            ));
        };
        if member.is_empty() {
            return Err(Error::InvalidArgument("file member path is empty".into()));
        }
        let paths = [format!("{path}#{member}")];
        let sequence = self
            .send_request(
                timer,
                cmd::SUB_FILE,
                &SubFileReq {
                    paths: &paths,
                    storage: file.storage.as_deref(),
                },
                &[],
            )
            .await?;
        let mut out = Vec::new();
        loop {
            let payload = self
                .read_reply_frame(timer, cmd::SUB_FILE, sequence)
                .await?;
            let (json, binary) = p::split_payload(&payload);
            let env: Envelope<SubFileReply> = match p::parse_envelope(json, "SUB_FILE") {
                Ok(env) => env,
                Err(e) => return self.poison(e),
            };
            let more = env.result == Some(result::CONTINUE);
            if !more {
                require_success("SUB_FILE", env.result)?;
            }
            let body = env.reply.unwrap_or_default();
            // Studio matches each frame to its file by `path`; a frame for another member means
            // the stream isn't the one asked for.
            if !body.path.is_empty() && body.path != paths[0] {
                let err = Error::ProtocolViolation(
                    format!(
                        "file tunnel SUB_FILE answered for {:?}, not {:?}",
                        body.path, paths[0]
                    )
                    .into(),
                );
                return if more { self.poison(err) } else { Err(err) };
            }
            // A size of 0 is Studio's "no such member" (`FILE_SIZE_ERR` in its translator).
            if body.size == 0 {
                if more {
                    self.abandon(timer, sequence).await;
                }
                return Ok(None);
            }
            let Some(chunk) = usize::try_from(body.size)
                .ok()
                .and_then(|n| binary.get(..n))
            else {
                let err = Error::ProtocolViolation(
                    format!(
                        "file tunnel SUB_FILE fragment declares {} bytes but carries {}",
                        body.size,
                        binary.len()
                    )
                    .into(),
                );
                return if more { self.poison(err) } else { Err(err) };
            };
            if out.len() + chunk.len() > max_bytes {
                if more {
                    self.abandon(timer, sequence).await;
                }
                return Err(Error::ProtocolViolation(
                    format!("file tunnel SUB_FILE member exceeds the {max_bytes} byte limit")
                        .into(),
                ));
            }
            out.extend_from_slice(chunk);
            if body.more_fragments && more {
                continue;
            }
            // One member was asked for; a stream still marked CONTINUE after it is cancelled.
            if more {
                self.abandon(timer, sequence).await;
            }
            return Ok(Some(out));
        }
    }

    /// Deletes a listed file (`FILE_DEL`).
    ///
    /// Addresses it by `path` when its listing gave one and by name otherwise, as Studio's
    /// `PrinterFileSystem::DeleteFilesContinue` does. Don't delete before an upload of the same
    /// name: an upload overwrites, and an extra request first can race the upload's reply.
    pub async fn delete_file<T: TimerProvider>(
        &mut self,
        timer: &T,
        file: &TunnelFile,
    ) -> Result<(), Error> {
        let target = [file.path.as_deref().unwrap_or(file.name.as_str())];
        let req = if file.path.is_some() {
            FileDelReq {
                paths: Some(&target),
                delete: None,
            }
        } else {
            FileDelReq {
                paths: None,
                delete: Some(&target),
            }
        };
        let _: IgnoredAnyDefault = self
            .single_reply(timer, "FILE_DEL", cmd::FILE_DEL, &req)
            .await?;
        Ok(())
    }

    /// Uploads `data` to `storage` under `name` (`FILE_UPLOAD`), overwriting a file of that name.
    ///
    /// `storage` is a label from [`media_ability`](Self::media_ability), such as `emmc`; there is
    /// no default. To print the file afterwards from internal storage, start the job with
    /// [`PrintJobConfig::from_internal_storage`](crate::mqtt::PrintJobConfig::from_internal_storage).
    ///
    /// The flow follows Studio's `PrinterFileSystem::RequestUploadFile` and `UploadFileTask`: an
    /// init request, whose reply sets the chunk size, then every chunk on the init's `sequence`
    /// with the whole file's MD5 on the last, then one final reply. The chunks go out back to
    /// back without reading between them: open-bamboo-networking found that a P2S answers a
    /// per-chunk read with `-9203`.
    ///
    /// **Not yet run against a printer.** The pipelining rests on open-bamboo-networking's P2S
    /// captures alone.
    pub async fn upload<T: TimerProvider>(
        &mut self,
        timer: &T,
        storage: &str,
        name: &str,
        data: &[u8],
    ) -> Result<(), Error> {
        self.check_poisoned()?;
        if storage.is_empty() || name.is_empty() {
            return Err(Error::InvalidArgument(
                "upload needs a storage label and a file name".into(),
            ));
        }
        // Studio refuses an empty file (`UploadFileTask` reads nothing and fails).
        if data.is_empty() {
            return Err(Error::InvalidArgument("upload of an empty file".into()));
        }
        let total = data.len() as u64;
        let sequence = self
            .send_request(
                timer,
                cmd::FILE_UPLOAD,
                &UploadInitReq {
                    kind: p::UPLOAD_TYPE_MODEL,
                    storage,
                    path: name,
                    total,
                },
                &[],
            )
            .await?;
        let payload = self
            .read_reply_frame(timer, cmd::FILE_UPLOAD, sequence)
            .await?;
        let (json, _) = p::split_payload(&payload);
        let env: Envelope<UploadInitReply> = p::parse_envelope(json, "FILE_UPLOAD init")?;
        // Anything but CONTINUE or FILE_EXIST ends the request before a chunk is sent.
        match env.result {
            Some(result::CONTINUE | result::FILE_EXIST) => {}
            Some(result::SUCCESS) => {
                return Err(Error::ProtocolViolation(
                    "file tunnel FILE_UPLOAD init finished without asking for data".into(),
                ));
            }
            other => require_success("FILE_UPLOAD", other)?,
        }
        let init = env.reply.unwrap_or_default();
        let chunk_len = init
            .chunk_size
            .checked_mul(1024)
            .and_then(|n| usize::try_from(n).ok())
            .filter(|&n| n > 0 && n <= self.max_frame_payload)
            .ok_or_else(|| {
                Error::ProtocolViolation(
                    format!(
                        "file tunnel FILE_UPLOAD chunk_size {} KiB is zero or over the {} byte \
                         frame limit",
                        init.chunk_size, self.max_frame_payload
                    )
                    .into(),
                )
            })?;
        let start = usize::try_from(init.offset)
            .ok()
            .filter(|&o| o < data.len())
            .ok_or_else(|| {
                Error::ProtocolViolation(
                    format!(
                        "file tunnel FILE_UPLOAD resume offset {} is past the {total} byte file",
                        init.offset
                    )
                    .into(),
                )
            })?;
        // open-bamboo-networking: the MD5 of the entire file, on the last chunk only.
        let file_md5 = format!("{:x}", md5::compute(data));
        let chunks = data[start..].chunks(chunk_len);
        let last = chunks.len().saturating_sub(1);
        for (index, chunk) in chunks.enumerate() {
            let offset = start + index * chunk_len;
            let req = UploadChunkReq {
                frag_id: index as u32,
                offset: offset as u64,
                size: chunk.len(),
                file_md5: (index == last).then_some(file_md5.as_str()),
            };
            let frame = p::encode_request(cmd::FILE_UPLOAD, sequence, &req, chunk)?;
            if let Err(e) = write_frame(
                &mut self.stream,
                timer,
                &mut self.frame_seq,
                p::MAGIC_CTRL,
                &frame,
            )
            .await
            {
                return self.poison(e);
            }
        }
        // One final reply; skip any progress frames a firmware sends in between.
        loop {
            let payload = self
                .read_reply_frame(timer, cmd::FILE_UPLOAD, sequence)
                .await?;
            let (json, _) = p::split_payload(&payload);
            let env: Envelope<IgnoredAny> = match p::parse_envelope(json, "FILE_UPLOAD") {
                Ok(env) => env,
                Err(e) => return self.poison(e),
            };
            if env.result == Some(result::CONTINUE) {
                continue;
            }
            return require_success("FILE_UPLOAD", env.result);
        }
    }

    /// Receives a streamed `FILE_DOWNLOAD` reply, checking offsets, `total` and `file_md5`.
    async fn receive_download<T: TimerProvider>(
        &mut self,
        timer: &T,
        command: &'static str,
        sequence: u32,
        max_total: Option<u64>,
        sink: &mut impl FnMut(&[u8]) -> Result<(), Error>,
    ) -> Result<u64, Error> {
        let mut digest = md5::Context::new();
        let mut received: u64 = 0;
        loop {
            let payload = self
                .read_reply_frame(timer, cmd::FILE_DOWNLOAD, sequence)
                .await?;
            let (json, binary) = p::split_payload(&payload);
            let env: Envelope<DownloadReply> = match p::parse_envelope(json, command) {
                Ok(env) => env,
                Err(e) => return self.poison(e),
            };
            let code = env.result;
            let more = code == Some(result::CONTINUE);
            if !more {
                require_success(command, code)?;
            }
            let body = env.reply.unwrap_or_default();

            // The first frame of a `mem:/` read carries the printer's status blob, not file data
            // (`PrinterFileSystem::DownloadRamFile`).
            if let Some(len) = body.mem_dl_param_size {
                let status = binary
                    .get(..len)
                    .and_then(|b| serde_json::from_slice::<MemDownloadStatus>(b).ok());
                let Some(status) = status else {
                    return self.poison(Error::ProtocolViolation(
                        format!("file tunnel {command} status blob is missing or malformed").into(),
                    ));
                };
                if !status.accepted() {
                    // Studio's codes for the two refusals: ERROR_JSON for a missing or `1`
                    // result, FILE_SIZE_ERR for size 0. Frames may follow, so cancel the rest.
                    let code = if status.size == Some(0) {
                        result::FILE_SIZE_ERR
                    } else {
                        result::ERROR_JSON
                    };
                    if more {
                        self.abandon(timer, sequence).await;
                    }
                    return Err(Error::TunnelReply {
                        command,
                        result: code,
                    });
                }
                if more {
                    continue;
                }
                return Err(Error::ProtocolViolation(
                    format!("file tunnel {command} ended after its status blob").into(),
                ));
            }

            if let Some(max) = max_total
                && body.total > max
            {
                let err = Error::ProtocolViolation(
                    format!(
                        "file tunnel {command}: {} bytes exceeds the {max} byte limit",
                        body.total
                    )
                    .into(),
                );
                if more {
                    self.abandon(timer, sequence).await;
                }
                return Err(err);
            }
            if received.saturating_add(body.size) > body.total {
                let err = Error::ProtocolViolation(
                    format!(
                        "file tunnel {command} fragment of {} bytes at {received} runs past the \
                         {} byte total",
                        body.size, body.total
                    )
                    .into(),
                );
                return if more { self.poison(err) } else { Err(err) };
            }
            if body.offset != received {
                let err = Error::ProtocolViolation(
                    format!(
                        "file tunnel {command} fragment at offset {} after {received} bytes",
                        body.offset
                    )
                    .into(),
                );
                return if more { self.poison(err) } else { Err(err) };
            }
            let Some(chunk) = usize::try_from(body.size)
                .ok()
                .and_then(|n| binary.get(..n))
            else {
                let err = Error::ProtocolViolation(
                    format!(
                        "file tunnel {command} fragment declares {} bytes but carries {}",
                        body.size,
                        binary.len()
                    )
                    .into(),
                );
                return if more { self.poison(err) } else { Err(err) };
            };
            if let Err(e) = sink(chunk) {
                if more {
                    self.abandon(timer, sequence).await;
                }
                return Err(e);
            }
            digest.consume(chunk);
            received += body.size;
            if more {
                continue;
            }

            if received != body.total {
                return Err(Error::ProtocolViolation(
                    format!(
                        "file tunnel {command} ended at {received} of {} bytes",
                        body.total
                    )
                    .into(),
                ));
            }
            let Some(expected) = body.file_md5.as_deref() else {
                return Err(Error::ProtocolViolation(
                    format!("file tunnel {command} final frame carries no file_md5").into(),
                ));
            };
            if !p::md5_matches(&digest.finalize(), expected) {
                return Err(Error::ProtocolViolation(
                    format!("file tunnel {command} MD5 mismatch").into(),
                ));
            }
            return Ok(received);
        }
    }

    /// Closes the TLS session through `tls_connector` (the one that opened it), then drops it.
    ///
    /// A poisoned session is dropped without a close: there is no orderly shutdown to negotiate
    /// with a peer in an unknown state (`.claude/rules/tls-session-teardown.md`).
    pub async fn disconnect<RawIO, Tls, T>(mut self, tls_connector: &Tls, timer: &T)
    where
        RawIO: AsyncIo,
        Tls: TlsConnector<RawIO, Stream = S>,
        T: TimerProvider,
    {
        if !self.poisoned {
            let close = tls_connector.close(&mut self.stream);
            match with_deadline(
                close,
                timer,
                FILE_TUNNEL_WRITE_TIMEOUT_SECS.saturating_mul(1000),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(e)) | Err(e) => log::debug!("file tunnel TLS close failed: {e:?}"),
            }
        }
        drop(self.stream);
    }

    #[cfg(test)]
    pub(crate) fn stream(&self) -> &S {
        &self.stream
    }
}

/// A [`FileTunnelClient`] paired with the timer that bounds it, as [`PrinterClient::file_tunnel`](crate::client::PrinterClient::file_tunnel) hands it out.
///
/// The same operations as the client, without the timer argument. Borrowed from the
/// `PrinterClient` for the length of one use; get a fresh one per operation.
pub struct FileTunnel<'a, S: AsyncIo, T: TimerProvider> {
    pub(crate) client: &'a mut FileTunnelClient<S>,
    pub(crate) timer: &'a T,
}

impl<S: AsyncIo, T: TimerProvider> FileTunnel<'_, S, T> {
    /// See [`FileTunnelClient::media_ability`].
    pub async fn media_ability(&mut self) -> Result<Vec<String>, Error> {
        self.client.media_ability(self.timer).await
    }

    /// See [`FileTunnelClient::list_files`].
    pub async fn list_files(
        &mut self,
        kind: FileKind,
        storage: Option<&str>,
    ) -> Result<Vec<TunnelFile>, Error> {
        self.client.list_files(self.timer, kind, storage).await
    }

    /// See [`FileTunnelClient::download`].
    pub async fn download(
        &mut self,
        file: &TunnelFile,
        sink: impl FnMut(&[u8]) -> Result<(), Error>,
    ) -> Result<u64, Error> {
        self.client.download(self.timer, file, sink).await
    }

    /// See [`FileTunnelClient::read_running_job_member`].
    pub async fn read_running_job_member(
        &mut self,
        member: &str,
        max_bytes: usize,
    ) -> Result<Vec<u8>, Error> {
        self.client
            .read_running_job_member(self.timer, member, max_bytes)
            .await
    }

    /// See [`FileTunnelClient::read_running_job_member_into`].
    pub async fn read_running_job_member_into(
        &mut self,
        member: &str,
        out: &mut [u8],
    ) -> Result<usize, Error> {
        self.client
            .read_running_job_member_into(self.timer, member, out)
            .await
    }

    /// See [`FileTunnelClient::read_file_member`].
    pub async fn read_file_member(
        &mut self,
        file: &TunnelFile,
        member: &str,
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, Error> {
        self.client
            .read_file_member(self.timer, file, member, max_bytes)
            .await
    }

    /// See [`FileTunnelClient::delete_file`].
    pub async fn delete_file(&mut self, file: &TunnelFile) -> Result<(), Error> {
        self.client.delete_file(self.timer, file).await
    }

    /// See [`FileTunnelClient::upload`].
    pub async fn upload(&mut self, storage: &str, name: &str, data: &[u8]) -> Result<(), Error> {
        self.client.upload(self.timer, storage, name, data).await
    }

    /// See [`FileTunnelClient::is_poisoned`].
    pub fn is_poisoned(&self) -> bool {
        self.client.is_poisoned()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::DummyTimer;
    use crate::io::SocketError;
    use crate::test_support::MockIo;

    #[cfg(not(feature = "std"))]
    use alloc::vec::Vec;

    /// Hands the raw stream back as the "TLS" stream, so a test scripts the printer's bytes directly.
    struct PassthroughTls;

    impl TlsConnector<MockIo> for PassthroughTls {
        type Stream = MockIo;
        async fn connect(&self, _host: &str, raw: MockIo) -> Result<MockIo, SocketError> {
            Ok(raw)
        }
    }

    fn identity() -> PrinterIdentity {
        PrinterIdentity::new("192.168.1.50", "00M00A000000000", "12345678")
    }

    fn frame(magic: u32, payload: &[u8]) -> Vec<u8> {
        let mut out = FrameHeader {
            payload_len: payload.len() as u32,
            magic,
            seq: 0,
        }
        .encode()
        .to_vec();
        out.extend_from_slice(payload);
        out
    }

    fn reply(json: &str) -> Vec<u8> {
        frame(p::MAGIC_CTRL_REPLY, json.as_bytes())
    }

    fn reply_with(json: &str, binary: &[u8]) -> Vec<u8> {
        let mut payload = json.as_bytes().to_vec();
        payload.extend_from_slice(p::BINARY_SEPARATOR);
        payload.extend_from_slice(binary);
        frame(p::MAGIC_CTRL_REPLY, &payload)
    }

    fn handshake_frames() -> Vec<Vec<u8>> {
        vec![
            frame(p::MAGIC_LOGIN_ACK, &[0, 0, 0, 0]),
            reply(r#"{"mtype":12291,"sequence":0,"result":0,"reply":{}}"#),
        ]
    }

    async fn connected(after_handshake: &[Vec<u8>]) -> FileTunnelClient<MockIo> {
        let mut frames = handshake_frames();
        frames.extend_from_slice(after_handshake);
        let chunks: Vec<&[u8]> = frames.iter().map(Vec::as_slice).collect();
        FileTunnelClient::connect(
            MockIo::with_chunks(&chunks),
            &PassthroughTls,
            &identity(),
            &DummyTimer,
        )
        .await
        .unwrap()
    }

    /// Splits everything the client wrote into `(magic, payload)` frames.
    fn written_frames(io: &MockIo) -> Vec<(u32, Vec<u8>)> {
        let bytes = io.written();
        let mut out = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            let header: [u8; p::FRAME_HEADER_LEN] = bytes[at..at + 16].try_into().unwrap();
            let h = FrameHeader::decode(&header);
            let end = at + 16 + h.payload_len as usize;
            out.push((h.magic, bytes[at + 16..end].to_vec()));
            at = end;
        }
        out
    }

    #[tokio::test]
    async fn handshake_sends_login_then_setup() {
        let client = connected(&[]).await;
        let sent = written_frames(client.stream());
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].0, p::MAGIC_LOGIN);
        assert_eq!(sent[0].1, [&b"bblp\0\0\0\0"[..], b"12345678"].concat());
        assert_eq!(sent[1].0, p::MAGIC_CTRL);
        let setup = core::str::from_utf8(&sent[1].1).unwrap();
        assert!(setup.starts_with(r#"{"sequence":0,"mtype":12291,"req":{"t_av":1"#));
    }

    #[tokio::test]
    async fn header_and_payload_are_separate_writes() {
        let client = connected(&[]).await;
        // Login: header write, payload write. Setup: header write, payload write.
        assert_eq!(client.stream().writes.len(), 4);
        assert_eq!(client.stream().writes[0].len(), p::FRAME_HEADER_LEN);
    }

    #[tokio::test]
    async fn reject_frame_fails_the_connect() {
        let reject = frame(p::MAGIC_REJECT, &[0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0]);
        let result = FileTunnelClient::connect(
            MockIo::with_chunks(&[&reject]),
            &PassthroughTls,
            &identity(),
            &DummyTimer,
        )
        .await;
        assert!(matches!(result, Err(Error::ProtocolViolation(_))));
    }

    #[tokio::test]
    async fn silent_peer_that_closes_fails_the_connect() {
        let result =
            FileTunnelClient::connect(MockIo::empty(), &PassthroughTls, &identity(), &DummyTimer)
                .await;
        assert!(matches!(
            result,
            Err(Error::Network(SocketError::ConnectionReset))
        ));
    }

    #[tokio::test]
    async fn failing_setup_result_is_a_tunnel_reply() {
        let frames = [
            frame(p::MAGIC_LOGIN_ACK, &[0, 0, 0, 0]),
            reply(r#"{"mtype":12291,"sequence":0,"result":18,"reply":{}}"#),
        ];
        let chunks: Vec<&[u8]> = frames.iter().map(Vec::as_slice).collect();
        let result = FileTunnelClient::connect(
            MockIo::with_chunks(&chunks),
            &PassthroughTls,
            &identity(),
            &DummyTimer,
        )
        .await;
        assert!(matches!(
            result,
            Err(Error::TunnelReply {
                command: "setup",
                result: 18
            })
        ));
    }

    #[tokio::test]
    async fn media_ability_returns_storage_labels() {
        let mut client = connected(&[reply(
            r#"{"cmdtype":7,"mtype":12289,"reply":{"storage":["emmc","udisk"]},"result":0,"sequence":1}"#,
        )])
        .await;
        assert_eq!(
            client.media_ability(&DummyTimer).await.unwrap(),
            vec!["emmc", "udisk"]
        );
        let sent = written_frames(client.stream());
        assert_eq!(
            core::str::from_utf8(&sent[2].1).unwrap(),
            r#"{"mtype":12289,"cmdtype":7,"sequence":1,"req":{"peer":"studio","api_version":2}}"#
        );
    }

    #[tokio::test]
    async fn notify_and_stale_frames_are_skipped() {
        let mut client = connected(&[
            reply(r#"{"cmdtype":256,"sequence":0,"notify":{}}"#),
            reply(r#"{"cmdtype":3,"sequence":99,"result":0}"#),
            reply(r#"{"cmdtype":7,"reply":{"storage":["emmc"]},"result":0,"sequence":1}"#),
        ])
        .await;
        assert_eq!(
            client.media_ability(&DummyTimer).await.unwrap(),
            vec!["emmc"]
        );
    }

    #[tokio::test]
    async fn failure_result_keeps_the_session_usable() {
        let mut client = connected(&[
            reply(r#"{"cmdtype":1,"result":17,"sequence":1}"#),
            reply(r#"{"cmdtype":7,"reply":{"storage":[]},"result":0,"sequence":2}"#),
        ])
        .await;
        let err = client
            .list_files(&DummyTimer, FileKind::Model, Some("emmc"))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            Error::TunnelReply {
                command: "LIST_INFO",
                result: 17
            }
        ));
        assert!(!client.is_poisoned());
        assert!(client.media_ability(&DummyTimer).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn transport_failure_poisons_the_session() {
        let mut client = connected(&[]).await;
        assert!(client.media_ability(&DummyTimer).await.is_err());
        assert!(client.is_poisoned());
        assert!(matches!(
            client.media_ability(&DummyTimer).await,
            Err(Error::ProtocolViolation(_))
        ));
    }

    #[tokio::test]
    async fn list_files_carries_storage_and_optional_path() {
        let mut client = connected(&[reply(
            r#"{"cmdtype":1,"result":0,"sequence":1,"reply":{"file_lists":[{"name":"a.mp4","path":"/t/a.mp4","time":5,"size":9},{"name":"b.mp4","size":3}]}}"#,
        )])
        .await;
        let files = client
            .list_files(&DummyTimer, FileKind::Timelapse, Some("internal"))
            .await
            .unwrap();
        assert_eq!(files[0].path.as_deref(), Some("/t/a.mp4"));
        assert_eq!(files[0].storage.as_deref(), Some("internal"));
        assert_eq!(files[1].path, None);
    }

    #[tokio::test]
    async fn model_listing_without_paths_is_refused() {
        let mut client = connected(&[reply(
            r#"{"cmdtype":1,"result":0,"sequence":1,"reply":{"file_lists":[{"name":"x.mp4","size":3}]}}"#,
        )])
        .await;
        assert!(matches!(
            client.list_files(&DummyTimer, FileKind::Model, None).await,
            Err(Error::ProtocolViolation(_))
        ));
    }

    fn md5_hex(data: &[u8]) -> String {
        format!("{:x}", md5::compute(data))
    }

    #[tokio::test]
    async fn download_streams_fragments_and_checks_size_and_md5() {
        let data = b"hello world!";
        let final_json = format!(
            r#"{{"cmdtype":4,"result":0,"sequence":1,"reply":{{"offset":6,"total":12,"size":6,"file_md5":"{}"}}}}"#,
            md5_hex(data)
        );
        let mut client = connected(&[
            reply_with(
                r#"{"cmdtype":4,"result":1,"sequence":1,"reply":{"offset":0,"total":12,"size":6}}"#,
                &data[..6],
            ),
            reply_with(&final_json, &data[6..]),
        ])
        .await;
        let file = TunnelFile {
            name: "a.3mf".into(),
            path: Some("/userdata/model/a.3mf".into()),
            time: 0,
            size: 12,
            storage: None,
        };
        let mut out = Vec::new();
        let n = client
            .download(&DummyTimer, &file, |chunk| {
                out.extend_from_slice(chunk);
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(n, 12);
        assert_eq!(out, data);
        let sent = written_frames(client.stream());
        assert_eq!(
            core::str::from_utf8(&sent[2].1).unwrap(),
            r#"{"mtype":12289,"cmdtype":4,"sequence":1,"req":{"path":"/userdata/model/a.3mf","offset":0}}"#
        );
    }

    #[tokio::test]
    async fn download_addresses_a_pathless_file_by_name() {
        let mut client = connected(&[reply_with(
            &format!(
                r#"{{"cmdtype":4,"result":0,"sequence":1,"reply":{{"offset":0,"total":1,"size":1,"file_md5":"{}"}}}}"#,
                md5_hex(b"x")
            ),
            b"x",
        )])
        .await;
        let file = TunnelFile {
            name: "old.mp4".into(),
            path: None,
            time: 0,
            size: 1,
            storage: None,
        };
        client
            .download(&DummyTimer, &file, |_| Ok(()))
            .await
            .unwrap();
        let sent = written_frames(client.stream());
        assert!(
            core::str::from_utf8(&sent[2].1)
                .unwrap()
                .contains(r#""file":"old.mp4""#)
        );
    }

    #[tokio::test]
    async fn download_md5_mismatch_is_an_error() {
        let mut client = connected(&[reply_with(
            r#"{"cmdtype":4,"result":0,"sequence":1,"reply":{"offset":0,"total":1,"size":1,"file_md5":"00000000000000000000000000000000"}}"#,
            b"x",
        )])
        .await;
        let file = TunnelFile {
            name: "a".into(),
            path: Some("/a".into()),
            time: 0,
            size: 1,
            storage: None,
        };
        assert!(matches!(
            client.download(&DummyTimer, &file, |_| Ok(())).await,
            Err(Error::ProtocolViolation(_))
        ));
    }

    #[tokio::test]
    async fn running_job_member_skips_the_status_blob() {
        let png = b"\x89PNG....";
        let final_json = format!(
            r#"{{"cmdtype":4,"result":0,"sequence":1,"reply":{{"offset":0,"total":8,"size":8,"file_md5":"{}"}}}}"#,
            md5_hex(png)
        );
        let mut client = connected(&[
            reply_with(
                r#"{"cmdtype":4,"result":1,"sequence":1,"reply":{"mem_dl_param_size":12,"total":8,"size":0}}"#,
                br#"{"result":0}"#,
            ),
            reply_with(&final_json, png),
        ])
        .await;
        let out = client
            .read_running_job_member(&DummyTimer, "Metadata/pick_1.png", 1024)
            .await
            .unwrap();
        assert_eq!(out, png);

        let sent = written_frames(client.stream());
        let (json, blob) = p::split_payload(&sent[2].1);
        assert_eq!(
            core::str::from_utf8(json).unwrap(),
            format!(
                r#"{{"mtype":12289,"cmdtype":4,"sequence":1,"req":{{"path":"mem:/16","offset":0,"mem_dl_param_size":{}}}}}"#,
                blob.len()
            )
        );
        assert_eq!(
            core::str::from_utf8(blob).unwrap(),
            r#"{"sequence_id":1,"version":1,"peer_host":"studio","command":"get_project_file","file_rel_path":"Metadata/pick_1.png"}"#
        );
    }

    #[tokio::test]
    async fn refused_running_job_member_is_a_tunnel_reply() {
        let mut client = connected(&[reply_with(
            r#"{"cmdtype":4,"result":1,"sequence":1,"reply":{"mem_dl_param_size":12}}"#,
            br#"{"result":1}"#,
        )])
        .await;
        assert!(matches!(
            client
                .read_running_job_member(&DummyTimer, "Metadata/pick_9.png", 1024)
                .await,
            Err(Error::TunnelReply { result: 2, .. })
        ));
        assert!(client.is_poisoned());
    }

    fn listed(path: Option<&str>, storage: Option<&str>) -> TunnelFile {
        TunnelFile {
            name: "a.3mf".into(),
            path: path.map(Into::into),
            time: 0,
            size: 1,
            storage: storage.map(Into::into),
        }
    }

    #[tokio::test]
    async fn sink_error_cancels_the_stream_and_keeps_the_session() {
        let mut client = connected(&[
            reply_with(
                r#"{"cmdtype":4,"result":1,"sequence":1,"reply":{"offset":0,"total":8,"size":4}}"#,
                b"abcd",
            ),
            // A fragment already in flight when the cancel goes out, skipped as stale.
            reply_with(
                r#"{"cmdtype":4,"result":1,"sequence":1,"reply":{"offset":4,"total":8,"size":4}}"#,
                b"efgh",
            ),
            reply(r#"{"cmdtype":4096,"result":0,"sequence":2,"reply":{"tasks":[1]}}"#),
            reply(r#"{"cmdtype":7,"result":0,"sequence":3,"reply":{"storage":["emmc"]}}"#),
        ])
        .await;
        let err = client
            .download(&DummyTimer, &listed(Some("/a"), None), |_| {
                Err(Error::InvalidArgument("disk full".into()))
            })
            .await
            .unwrap_err();
        assert!(matches!(err, Error::InvalidArgument(_)));
        assert!(!client.is_poisoned());
        let sent = written_frames(client.stream());
        assert_eq!(
            core::str::from_utf8(&sent[3].1).unwrap(),
            r#"{"mtype":12289,"cmdtype":4096,"sequence":2,"req":{"tasks":[1]}}"#
        );
        assert_eq!(
            client.media_ability(&DummyTimer).await.unwrap(),
            vec!["emmc"]
        );
    }

    #[tokio::test]
    async fn file_member_is_read_by_path_and_storage() {
        let mut client = connected(&[
            reply_with(
                r#"{"cmdtype":2,"result":1,"sequence":1,"reply":{"path":"/m/a.3mf#Metadata/plate_1.png","size":3,"continue":true}}"#,
                b"abc",
            ),
            reply_with(
                r#"{"cmdtype":2,"result":0,"sequence":1,"reply":{"path":"/m/a.3mf#Metadata/plate_1.png","size":2}}"#,
                b"de",
            ),
        ])
        .await;
        let data = client
            .read_file_member(
                &DummyTimer,
                &listed(Some("/m/a.3mf"), Some("emmc")),
                "Metadata/plate_1.png",
                64,
            )
            .await
            .unwrap();
        assert_eq!(data.as_deref(), Some(&b"abcde"[..]));
        let sent = written_frames(client.stream());
        assert_eq!(
            core::str::from_utf8(&sent[2].1).unwrap(),
            r##"{"mtype":12289,"cmdtype":2,"sequence":1,"req":{"paths":["/m/a.3mf#Metadata/plate_1.png"],"storage":"emmc"}}"##
        );
    }

    #[tokio::test]
    async fn missing_file_member_is_none() {
        let mut client = connected(&[reply(
            r#"{"cmdtype":2,"result":0,"sequence":1,"reply":{"path":"/m/a.3mf#x","size":0}}"#,
        )])
        .await;
        let data = client
            .read_file_member(&DummyTimer, &listed(Some("/m/a.3mf"), None), "x", 64)
            .await
            .unwrap();
        assert_eq!(data, None);
    }

    #[tokio::test]
    async fn file_member_needs_a_listed_path() {
        let mut client = connected(&[]).await;
        assert!(matches!(
            client
                .read_file_member(&DummyTimer, &listed(None, None), "x", 64)
                .await,
            Err(Error::InvalidArgument(_))
        ));
    }

    #[tokio::test]
    async fn delete_uses_paths_or_bare_names() {
        let mut client = connected(&[
            reply(r#"{"cmdtype":3,"result":0,"sequence":1}"#),
            reply(r#"{"cmdtype":3,"result":0,"sequence":2}"#),
        ])
        .await;
        client
            .delete_file(&DummyTimer, &listed(Some("/m/a.3mf"), None))
            .await
            .unwrap();
        client
            .delete_file(&DummyTimer, &listed(None, None))
            .await
            .unwrap();
        let sent = written_frames(client.stream());
        assert_eq!(
            core::str::from_utf8(&sent[2].1).unwrap(),
            r#"{"mtype":12289,"cmdtype":3,"sequence":1,"req":{"paths":["/m/a.3mf"]}}"#
        );
        assert_eq!(
            core::str::from_utf8(&sent[3].1).unwrap(),
            r#"{"mtype":12289,"cmdtype":3,"sequence":2,"req":{"delete":["a.3mf"]}}"#
        );
    }

    #[tokio::test]
    async fn upload_pipelines_chunks_with_the_md5_on_the_last() {
        let data = vec![7u8; 2048 + 100];
        let mut client = connected(&[
            // `FILE_EXIST` continues and overwrites, as Studio does.
            reply(r#"{"cmdtype":5,"result":19,"sequence":1,"reply":{"chunk_size":1,"offset":0}}"#),
            reply(r#"{"cmdtype":5,"result":0,"sequence":1}"#),
        ])
        .await;
        client
            .upload(&DummyTimer, "emmc", "job.gcode.3mf", &data)
            .await
            .unwrap();
        let sent = written_frames(client.stream());
        assert_eq!(
            core::str::from_utf8(&sent[2].1).unwrap(),
            r#"{"mtype":12289,"cmdtype":5,"sequence":1,"req":{"type":"model","storage":"emmc","path":"job.gcode.3mf","total":2148}}"#
        );
        // Three 1 KiB-or-less chunks on the init's sequence, all written before any reply is read.
        let chunks: Vec<_> = sent[3..].iter().map(|(_, p)| p::split_payload(p)).collect();
        assert_eq!(chunks.len(), 3);
        let json = |i: usize| core::str::from_utf8(chunks[i].0).unwrap();
        assert_eq!(
            json(0),
            r#"{"mtype":12289,"cmdtype":5,"sequence":1,"req":{"frag_id":0,"offset":0,"size":1024}}"#
        );
        assert_eq!(
            json(1),
            r#"{"mtype":12289,"cmdtype":5,"sequence":1,"req":{"frag_id":1,"offset":1024,"size":1024}}"#
        );
        assert_eq!(
            json(2),
            format!(
                r#"{{"mtype":12289,"cmdtype":5,"sequence":1,"req":{{"frag_id":2,"offset":2048,"size":100,"file_md5":"{}"}}}}"#,
                md5_hex(&data)
            )
        );
        let rejoined: Vec<u8> = chunks
            .iter()
            .flat_map(|(_, bin)| bin.iter().copied())
            .collect();
        assert_eq!(rejoined, data);
    }

    #[tokio::test]
    async fn refused_upload_init_sends_no_chunks() {
        let mut client = connected(&[reply(r#"{"cmdtype":5,"result":20,"sequence":1}"#)]).await;
        let err = client
            .upload(&DummyTimer, "emmc", "big.3mf", b"data")
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            Error::TunnelReply {
                command: "FILE_UPLOAD",
                result: 20
            }
        ));
        assert_eq!(written_frames(client.stream()).len(), 3);
        assert!(!client.is_poisoned());
    }

    #[tokio::test]
    async fn fragments_past_the_declared_total_are_refused() {
        // A printer that keeps sending past `total` must not grow the caller's buffer.
        let mut client = connected(&[
            reply_with(
                r#"{"cmdtype":4,"result":1,"sequence":1,"reply":{"offset":0,"total":4,"size":4}}"#,
                b"abcd",
            ),
            reply_with(
                r#"{"cmdtype":4,"result":1,"sequence":1,"reply":{"offset":4,"total":4,"size":4}}"#,
                b"efgh",
            ),
        ])
        .await;
        let file = TunnelFile {
            name: "a".into(),
            path: Some("/a".into()),
            time: 0,
            size: 4,
            storage: None,
        };
        assert!(matches!(
            client.download(&DummyTimer, &file, |_| Ok(())).await,
            Err(Error::ProtocolViolation(_))
        ));
        assert!(client.is_poisoned());
    }

    #[tokio::test]
    async fn frame_over_the_payload_limit_is_refused() {
        let mut client = connected(&[reply(
            r#"{"cmdtype":7,"reply":{"storage":["emmc"]},"result":0,"sequence":1}"#,
        )])
        .await
        .with_max_frame_payload(16);
        assert!(matches!(
            client.media_ability(&DummyTimer).await,
            Err(Error::ProtocolViolation(_))
        ));
        assert!(client.is_poisoned());
    }

    #[tokio::test]
    async fn running_job_member_over_the_limit_is_refused() {
        let mut client = connected(&[
            reply_with(
                r#"{"cmdtype":4,"result":1,"sequence":1,"reply":{"mem_dl_param_size":12}}"#,
                br#"{"result":0}"#,
            ),
            reply_with(
                r#"{"cmdtype":4,"result":1,"sequence":1,"reply":{"offset":0,"total":5000,"size":4}}"#,
                b"abcd",
            ),
        ])
        .await;
        assert!(matches!(
            client
                .read_running_job_member(&DummyTimer, "Metadata/pick_1.png", 100)
                .await,
            Err(Error::ProtocolViolation(_))
        ));
    }
}
