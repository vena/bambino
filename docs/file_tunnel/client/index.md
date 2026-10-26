*[bambino](../../index.md) / [file_tunnel](../index.md) / [client](index.md)*

---

# Module `client`

A connected port-6000 file tunnel session [REF-FILESVC].

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`FileTunnel`](#filetunnel) | struct | A [`FileTunnelClient`](#filetunnelclient) paired with the timer that bounds it, as [`PrinterClient::file_tunnel`](../../client/index.md#printerclient) hands it out. |
| [`FileTunnelClient`](#filetunnelclient) | struct | A session with a printer's port-6000 file tunnel. |

## Types

### `FileTunnel<'a, S: AsyncIo, T: TimerProvider>`

```rust
struct FileTunnel<'a, S: AsyncIo, T: TimerProvider> {
    // [REDACTED: Private Fields]
}
```

A [`FileTunnelClient`](#filetunnelclient) paired with the timer that bounds it, as [`PrinterClient::file_tunnel`](../../client/index.md#printerclient) hands it out.

The same operations as the client, without the timer argument. Borrowed from the
`PrinterClient` for the length of one use; get a fresh one per operation.

#### Implementations

- <span id="filetunnel-media-ability"></span>`async fn media_ability(&mut self) -> Result<Vec<String>, Error>` — [`Error`](../../error/index.md#error)

  See [`FileTunnelClient::media_ability`](#filetunnelclient).

- <span id="filetunnel-list-files"></span>`async fn list_files(&mut self, kind: FileKind, storage: Option<&str>) -> Result<Vec<TunnelFile>, Error>` — [`FileKind`](../index.md#filekind), [`TunnelFile`](../index.md#tunnelfile), [`Error`](../../error/index.md#error)

  See [`FileTunnelClient::list_files`](#filetunnelclient).

- <span id="filetunnel-download"></span>`async fn download(&mut self, file: &TunnelFile, sink: impl FnMut(&[u8]) -> Result<(), Error>) -> Result<u64, Error>` — [`TunnelFile`](../index.md#tunnelfile), [`Error`](../../error/index.md#error)

  See [`FileTunnelClient::download`](#filetunnelclient).

- <span id="filetunnel-read-running-job-member"></span>`async fn read_running_job_member(&mut self, member: &str, max_bytes: usize) -> Result<Vec<u8>, Error>` — [`Error`](../../error/index.md#error)

  See [`FileTunnelClient::read_running_job_member`](#filetunnelclient).

- <span id="filetunnel-read-running-job-member-into"></span>`async fn read_running_job_member_into(&mut self, member: &str, out: &mut [u8]) -> Result<usize, Error>` — [`Error`](../../error/index.md#error)

  See [`FileTunnelClient::read_running_job_member_into`](#filetunnelclient).

- <span id="filetunnel-read-file-member"></span>`async fn read_file_member(&mut self, file: &TunnelFile, member: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, Error>` — [`TunnelFile`](../index.md#tunnelfile), [`Error`](../../error/index.md#error)

  See [`FileTunnelClient::read_file_member`](#filetunnelclient).

- <span id="filetunnel-delete-file"></span>`async fn delete_file(&mut self, file: &TunnelFile) -> Result<(), Error>` — [`TunnelFile`](../index.md#tunnelfile), [`Error`](../../error/index.md#error)

  See [`FileTunnelClient::delete_file`](#filetunnelclient).

- <span id="filetunnel-upload"></span>`async fn upload(&mut self, storage: &str, name: &str, source: impl UploadSource) -> Result<(), Error>` — [`UploadSource`](../../upload/index.md#uploadsource), [`Error`](../../error/index.md#error)

  See [`upload`](../../upload/index.md).

- <span id="filetunnel-is-poisoned"></span>`fn is_poisoned(&self) -> bool`

  See [`FileTunnelClient::is_poisoned`](#filetunnelclient).

#### Trait Implementations

### `FileTunnelClient<S: AsyncIo>`

```rust
struct FileTunnelClient<S: AsyncIo> {
    // [REDACTED: Private Fields]
}
```

A session with a printer's port-6000 file tunnel.

**Poisoning.** Requests run one at a time, and each reply is matched by `sequence`. A
transport failure or a frame that can't be parsed can leave frames on the wire that a later
request would misread. The client then sets `poisoned`; every public method returns
[`Error::ProtocolViolation`](../../error/index.md#error) after that, and the only way back is a new connection. A stream
the client stops itself (a sink error, a size limit) is cancelled with `TASK_CANCEL` instead,
and poisons the session only if the cancel gets no answer. A failure `result` the printer
sends in full (`FILE_NO_EXIST`) is [`Error::TunnelReply`](../../error/index.md#error) and leaves the session usable.

**Timer.** Every method takes the timer that bounds its reads and writes, as the camera
stream's `read_next_frame_with_timer` does, so the client holds only its TLS stream `S`.
Under a timer with no real clock (the crate's `DummyTimer`) nothing is bounded, including the
handshake, so a printer that never answers the login (every P1/A1) hangs
[`connect`](#filetunnelclient) until it closes the socket after 60 s.
[`PrinterClient::file_tunnel`](../../client/index.md#printerclient) supplies its own
timer through [`FileTunnel`](#filetunnel).

**Frame size.** Each frame's payload is allocated in full before it is parsed, so a corrupt
header is refused above [`DEFAULT_MAX_FRAME_PAYLOAD`](../index.md#default-max-frame-payload)
(4 MiB). That is far more than an embedded heap holds; lower it with
[`with_max_frame_payload`](#filetunnelclient). The largest frames reported are
20 KiB download fragments; a `LIST_INFO` reply grows with the number of files.

#### Implementations

- <span id="filetunnelclient-connect"></span>`async fn connect<RawIO, Tls, T>(raw: RawIO, tls_connector: &Tls, identity: &PrinterIdentity, timer: &T) -> Result<Self, Error>` — [`PrinterIdentity`](../../identity/index.md#printeridentity), [`Error`](../../error/index.md#error)

  Wraps `raw` in TLS, logs in and runs the setup exchange.

  `raw` must already be dialed to the printer's [`FILE_TUNNEL_PORT`](../index.md#file-tunnel-port).
  TLS uses the printer's serial as its identity, like every other channel; keep
  `tls_connector` to [`disconnect`](#filetunnelclient) through. A printer that doesn't serve
  the tunnel either refuses the login ([`Error::ProtocolViolation`](../../error/index.md#error)) or, like every P1/A1,
  sends nothing, which the handshake timeout turns into
  [`SocketError::TimedOut`](../../io/index.md#socketerror) when `timer` has a real clock.

- <span id="filetunnelclient-with-max-frame-payload"></span>`fn with_max_frame_payload(self, bytes: usize) -> Self`

  Lowers (or raises) the largest frame payload accepted, in bytes; see the struct docs.

  A frame over the limit is a protocol violation and poisons the session.

- <span id="filetunnelclient-is-poisoned"></span>`fn is_poisoned(&self) -> bool`

  Whether an earlier failure left the session out of sync; see the struct docs.

- <span id="filetunnelclient-media-ability"></span>`async fn media_ability<T: TimerProvider>(&mut self, timer: &T) -> Result<Vec<String>, Error>` — [`Error`](../../error/index.md#error)

  Lists the storages the printer has (`REQUEST_MEDIA_ABILITY`), such as `["emmc","udisk"]`.

  Probe this instead of guessing labels for [`list_files`](#filetunnelclient).

- <span id="filetunnelclient-list-files"></span>`async fn list_files<T: TimerProvider>(&mut self, timer: &T, kind: FileKind, storage: Option<&str>) -> Result<Vec<TunnelFile>, Error>` — [`FileKind`](../index.md#filekind), [`TunnelFile`](../index.md#tunnelfile), [`Error`](../../error/index.md#error)

  Lists files of one kind on one storage (`LIST_INFO`).

  `storage` is a label from [`media_ability`](#filetunnelclient), or `None` for the
  printer's default, which Studio's file browser treats as external storage.

- <span id="filetunnelclient-download"></span>`async fn download<T: TimerProvider>(&mut self, timer: &T, file: &TunnelFile, sink: impl FnMut(&[u8]) -> Result<(), Error>) -> Result<u64, Error>` — [`TunnelFile`](../index.md#tunnelfile), [`Error`](../../error/index.md#error)

  Downloads a listed file, passing each fragment to `sink` in order, and returns its size.

  The transfer is checked as Studio checks it: the byte count must reach the reply's `total`,
  and the MD5 of the bytes must match the final frame's `file_md5`. A mismatch is reported
  after `sink` has seen every byte, so a caller writing to storage should treat its output as
  provisional until this returns `Ok`. An error from `sink` stops the transfer: the client
  cancels the rest with `TASK_CANCEL`, and poisons the session only if that fails.

- <span id="filetunnelclient-read-running-job-member"></span>`async fn read_running_job_member<T: TimerProvider>(&mut self, timer: &T, member: &str, max_bytes: usize) -> Result<Vec<u8>, Error>` — [`Error`](../../error/index.md#error)

  Reads a member of the running job's `.3mf` by its path inside the archive, with no file path (`mem:/16`, `get_project_file`).

  This is the read BambuStudio's skip-objects dialog makes for `Metadata/pick_<plate>.png`,
  `Metadata/model_settings.config` and `Metadata/slice_info.config`
  (`PartSkipDialog::DownloadPartsFile`). A member larger than `max_bytes` is refused before
  any of it is buffered. A member the printer doesn't have is refused with
  [`Error::TunnelReply`](../../error/index.md#error); Studio treats every refusal here as "no such file". The client
  cancels whatever the printer would still send, and poisons the session only if that fails.

- <span id="filetunnelclient-read-running-job-member-into"></span>`async fn read_running_job_member_into<T: TimerProvider>(&mut self, timer: &T, member: &str, out: &mut [u8]) -> Result<usize, Error>` — [`Error`](../../error/index.md#error)

  Like [`read_running_job_member`](#filetunnelclient), but fills `out` instead of allocating, and returns the member's length.

  A member longer than `out` is refused before any of it is written.

- <span id="filetunnelclient-read-file-member"></span>`async fn read_file_member<T: TimerProvider>(&mut self, timer: &T, file: &TunnelFile, member: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, Error>` — [`TunnelFile`](../index.md#tunnelfile), [`Error`](../../error/index.md#error)

  Reads one member of a listed file without downloading the file (`SUB_FILE`), or `None` when the file has no such member.

  `member` is a path inside the archive, such as `Metadata/plate_1.png`; for a video, the
  member `thumbnail` is its thumbnail. `file` must carry the `path` its listing gave:
  `SUB_FILE` addresses members as `<path>#<member>` (Studio's
  `PrinterFileSystem::UpdateFocusThumbnail2`). A member larger than `max_bytes` is refused,
  and the rest of the stream cancelled.

- <span id="filetunnelclient-delete-file"></span>`async fn delete_file<T: TimerProvider>(&mut self, timer: &T, file: &TunnelFile) -> Result<(), Error>` — [`TunnelFile`](../index.md#tunnelfile), [`Error`](../../error/index.md#error)

  Deletes a listed file (`FILE_DEL`).

  Addresses it by `path` when its listing gave one and by name otherwise, as Studio's
  `PrinterFileSystem::DeleteFilesContinue` does. Don't delete before an upload of the same
  name: an upload overwrites, and an extra request first can race the upload's reply.

- <span id="filetunnelclient-upload"></span>`async fn upload<T: TimerProvider>(&mut self, timer: &T, storage: &str, name: &str, source: impl UploadSource) -> Result<(), Error>` — [`UploadSource`](../../upload/index.md#uploadsource), [`Error`](../../error/index.md#error)

  Uploads `source` to `storage` under `name` (`FILE_UPLOAD`), overwriting a file of that name.

  `storage` is a label from [`media_ability`](#filetunnelclient), such as `emmc`; there is
  no default. To print the file afterwards from internal storage, start the job with
  [`PrintJobConfig::from_internal_storage`](../../mqtt/index.md).

  The flow follows Studio's `PrinterFileSystem::RequestUploadFile` and `UploadFileTask`: an
  init request, whose reply sets the chunk size, then every chunk on the init's `sequence`
  with the whole file's MD5 on the last, then one final reply. The chunks go out back to
  back without reading between them: open-bamboo-networking found that a P2S answers a
  per-chunk read with `-9203`.

  **Not yet run against a printer.** The pipelining rests on open-bamboo-networking's P2S
  captures alone. Which print URL finds a file uploaded to a storage other than `emmc`
  (`udisk`) is also unchecked.

- <span id="filetunnelclient-disconnect"></span>`async fn disconnect<RawIO, Tls, T>(self, tls_connector: &Tls, timer: &T)`

  Closes the TLS session through `tls_connector` (the one that opened it), then drops it.

  A poisoned session is dropped without a close: there is no orderly shutdown to negotiate
  with a peer in an unknown state (`.claude/rules/tls-session-teardown.md`).

#### Trait Implementations

