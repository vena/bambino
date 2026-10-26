*[bambino](../index.md) / [file_tunnel](index.md)*

---

# Module `file_tunnel`

# Port-6000 File Tunnel

Client for the TLS file service newer printers run on port 6000 [REF-FILESVC]
(`reference/08_file_service.md`). It reaches internal (eMMC) storage, which FTPS on port 990
does not, and reads members of the running job's `.3mf` without a path.

Which printers serve it: those whose telemetry sets `fun` bit 31, which BambuStudio reads as
`is_support_brtc` ("support tcp and upload protocol") and uses to choose this service over
FTPS for uploads. P1 and A1 firmware sends no `fun` and serves only the chamber-image stream
on this port; a P1S ignores this tunnel's login and closes the connection after 60 s.

[`FileTunnelClient`](client/index.md#filetunnelclient) speaks the protocol over an already-dialed raw stream, like
[`FtpsClient`](../ftps/index.md). The two services address files differently (a card
path against a storage label plus the `path` a listing returns), so neither wraps the other.

## Contents

- [Modules](#modules)
  - [`client`](client/index.md)
- [Types](#types)
  - [`TunnelFile`](#tunnelfile)
  - [`FileKind`](#filekind)
- [Functions](#functions)
  - [`result_name`](#result-name)
- [Constants](#constants)
  - [`DEFAULT_MAX_FRAME_PAYLOAD`](#default-max-frame-payload)
  - [`FILE_TUNNEL_PORT`](#file-tunnel-port)

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`client`](client/index.md) | mod | A connected port-6000 file tunnel session [REF-FILESVC]. |
| [`TunnelFile`](#tunnelfile) | struct | One file from a [`FileTunnelClient::list_files`](client/index.md#filetunnelclient) listing. |
| [`FileKind`](#filekind) | enum | Which kind of file a listing asks for: `LIST_INFO`'s `type`, and the kind `PrinterClient::list_files` lists over either service. |
| [`result_name`](#result-name) | fn | Names a file tunnel `result` code as BambuStudio's `PrinterFileSystem.h` does, or `None` for a code it doesn't define. |
| [`DEFAULT_MAX_FRAME_PAYLOAD`](#default-max-frame-payload) | const | Default largest frame payload [`FileTunnelClient`](client/index.md#filetunnelclient) accepts, in bytes (4 MiB). |
| [`FILE_TUNNEL_PORT`](#file-tunnel-port) | const | Port the file tunnel listens on, shared with the chamber-image stream of P1/A1 printers. |

## Modules

- [`client`](client/index.md) — A connected port-6000 file tunnel session [REF-FILESVC].


---

## Types

### `FileTunnel<'a, S: AsyncIo, T: TimerProvider>`

```rust
struct FileTunnel<'a, S: AsyncIo, T: TimerProvider> {
    // [REDACTED: Private Fields]
}
```

A [`FileTunnelClient`](client/index.md#filetunnelclient) paired with the timer that bounds it, as [`PrinterClient::file_tunnel`](../client/index.md#printerclient) hands it out.

The same operations as the client, without the timer argument. Borrowed from the
`PrinterClient` for the length of one use; get a fresh one per operation.

#### Implementations

- <span id="filetunnel-media-ability"></span>`async fn media_ability(&mut self) -> Result<Vec<String>, Error>` — [`Error`](../error/index.md#error)

  See [`FileTunnelClient::media_ability`](client/index.md#filetunnelclient).

- <span id="filetunnel-list-files"></span>`async fn list_files(&mut self, kind: FileKind, storage: Option<&str>) -> Result<Vec<TunnelFile>, Error>` — [`FileKind`](#filekind), [`TunnelFile`](#tunnelfile), [`Error`](../error/index.md#error)

  See [`FileTunnelClient::list_files`](client/index.md#filetunnelclient).

- <span id="filetunnel-download"></span>`async fn download(&mut self, file: &TunnelFile, sink: impl FnMut(&[u8]) -> Result<(), Error>) -> Result<u64, Error>` — [`TunnelFile`](#tunnelfile), [`Error`](../error/index.md#error)

  See [`FileTunnelClient::download`](client/index.md#filetunnelclient).

- <span id="filetunnel-read-running-job-member"></span>`async fn read_running_job_member(&mut self, member: &str, max_bytes: usize) -> Result<Vec<u8>, Error>` — [`Error`](../error/index.md#error)

  See [`FileTunnelClient::read_running_job_member`](client/index.md#filetunnelclient).

- <span id="filetunnel-read-running-job-member-into"></span>`async fn read_running_job_member_into(&mut self, member: &str, out: &mut [u8]) -> Result<usize, Error>` — [`Error`](../error/index.md#error)

  See [`FileTunnelClient::read_running_job_member_into`](client/index.md#filetunnelclient).

- <span id="filetunnel-read-file-member"></span>`async fn read_file_member(&mut self, file: &TunnelFile, member: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, Error>` — [`TunnelFile`](#tunnelfile), [`Error`](../error/index.md#error)

  See [`FileTunnelClient::read_file_member`](client/index.md#filetunnelclient).

- <span id="filetunnel-delete-file"></span>`async fn delete_file(&mut self, file: &TunnelFile) -> Result<(), Error>` — [`TunnelFile`](#tunnelfile), [`Error`](../error/index.md#error)

  See [`FileTunnelClient::delete_file`](client/index.md#filetunnelclient).

- <span id="filetunnel-upload"></span>`async fn upload(&mut self, storage: &str, name: &str, source: impl UploadSource) -> Result<(), Error>` — [`UploadSource`](../upload/index.md#uploadsource), [`Error`](../error/index.md#error)

  See [`upload`](../upload/index.md).

- <span id="filetunnel-is-poisoned"></span>`fn is_poisoned(&self) -> bool`

  See [`FileTunnelClient::is_poisoned`](client/index.md#filetunnelclient).

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
[`Error::ProtocolViolation`](../error/index.md#error) after that, and the only way back is a new connection. A stream
the client stops itself (a sink error, a size limit) is cancelled with `TASK_CANCEL` instead,
and poisons the session only if the cancel gets no answer. A failure `result` the printer
sends in full (`FILE_NO_EXIST`) is [`Error::TunnelReply`](../error/index.md#error) and leaves the session usable.

**Timer.** Every method takes the timer that bounds its reads and writes, as the camera
stream's `read_next_frame_with_timer` does, so the client holds only its TLS stream `S`.
Under a timer with no real clock (the crate's `DummyTimer`) nothing is bounded, including the
handshake, so a printer that never answers the login (every P1/A1) hangs
[`connect`](client/index.md#filetunnelclient) until it closes the socket after 60 s.
[`PrinterClient::file_tunnel`](../client/index.md#printerclient) supplies its own
timer through [`FileTunnel`](client/index.md#filetunnel).

**Frame size.** Each frame's payload is allocated in full before it is parsed, so a corrupt
header is refused above [`DEFAULT_MAX_FRAME_PAYLOAD`](#default-max-frame-payload)
(4 MiB). That is far more than an embedded heap holds; lower it with
[`with_max_frame_payload`](client/index.md#filetunnelclient). The largest frames reported are
20 KiB download fragments; a `LIST_INFO` reply grows with the number of files.

#### Implementations

- <span id="filetunnelclient-connect"></span>`async fn connect<RawIO, Tls, T>(raw: RawIO, tls_connector: &Tls, identity: &PrinterIdentity, timer: &T) -> Result<Self, Error>` — [`PrinterIdentity`](../identity/index.md#printeridentity), [`Error`](../error/index.md#error)

  Wraps `raw` in TLS, logs in and runs the setup exchange.

  `raw` must already be dialed to the printer's [`FILE_TUNNEL_PORT`](#file-tunnel-port).
  TLS uses the printer's serial as its identity, like every other channel; keep
  `tls_connector` to [`disconnect`](client/index.md#filetunnelclient) through. A printer that doesn't serve
  the tunnel either refuses the login ([`Error::ProtocolViolation`](../error/index.md#error)) or, like every P1/A1,
  sends nothing, which the handshake timeout turns into
  [`SocketError::TimedOut`](../io/index.md#socketerror) when `timer` has a real clock.

- <span id="filetunnelclient-with-max-frame-payload"></span>`fn with_max_frame_payload(self, bytes: usize) -> Self`

  Lowers (or raises) the largest frame payload accepted, in bytes; see the struct docs.

  A frame over the limit is a protocol violation and poisons the session.

- <span id="filetunnelclient-is-poisoned"></span>`fn is_poisoned(&self) -> bool`

  Whether an earlier failure left the session out of sync; see the struct docs.

- <span id="filetunnelclient-media-ability"></span>`async fn media_ability<T: TimerProvider>(&mut self, timer: &T) -> Result<Vec<String>, Error>` — [`Error`](../error/index.md#error)

  Lists the storages the printer has (`REQUEST_MEDIA_ABILITY`), such as `["emmc","udisk"]`.

  Probe this instead of guessing labels for [`list_files`](client/index.md#filetunnelclient).

- <span id="filetunnelclient-list-files"></span>`async fn list_files<T: TimerProvider>(&mut self, timer: &T, kind: FileKind, storage: Option<&str>) -> Result<Vec<TunnelFile>, Error>` — [`FileKind`](#filekind), [`TunnelFile`](#tunnelfile), [`Error`](../error/index.md#error)

  Lists files of one kind on one storage (`LIST_INFO`).

  `storage` is a label from [`media_ability`](client/index.md#filetunnelclient), or `None` for the
  printer's default, which Studio's file browser treats as external storage.

- <span id="filetunnelclient-download"></span>`async fn download<T: TimerProvider>(&mut self, timer: &T, file: &TunnelFile, sink: impl FnMut(&[u8]) -> Result<(), Error>) -> Result<u64, Error>` — [`TunnelFile`](#tunnelfile), [`Error`](../error/index.md#error)

  Downloads a listed file, passing each fragment to `sink` in order, and returns its size.

  The transfer is checked as Studio checks it: the byte count must reach the reply's `total`,
  and the MD5 of the bytes must match the final frame's `file_md5`. A mismatch is reported
  after `sink` has seen every byte, so a caller writing to storage should treat its output as
  provisional until this returns `Ok`. An error from `sink` stops the transfer: the client
  cancels the rest with `TASK_CANCEL`, and poisons the session only if that fails.

- <span id="filetunnelclient-read-running-job-member"></span>`async fn read_running_job_member<T: TimerProvider>(&mut self, timer: &T, member: &str, max_bytes: usize) -> Result<Vec<u8>, Error>` — [`Error`](../error/index.md#error)

  Reads a member of the running job's `.3mf` by its path inside the archive, with no file path (`mem:/16`, `get_project_file`).

  This is the read BambuStudio's skip-objects dialog makes for `Metadata/pick_<plate>.png`,
  `Metadata/model_settings.config` and `Metadata/slice_info.config`
  (`PartSkipDialog::DownloadPartsFile`). A member larger than `max_bytes` is refused before
  any of it is buffered. A member the printer doesn't have is refused with
  [`Error::TunnelReply`](../error/index.md#error); Studio treats every refusal here as "no such file". The client
  cancels whatever the printer would still send, and poisons the session only if that fails.

- <span id="filetunnelclient-read-running-job-member-into"></span>`async fn read_running_job_member_into<T: TimerProvider>(&mut self, timer: &T, member: &str, out: &mut [u8]) -> Result<usize, Error>` — [`Error`](../error/index.md#error)

  Like [`read_running_job_member`](client/index.md#filetunnelclient), but fills `out` instead of allocating, and returns the member's length.

  A member longer than `out` is refused before any of it is written.

- <span id="filetunnelclient-read-file-member"></span>`async fn read_file_member<T: TimerProvider>(&mut self, timer: &T, file: &TunnelFile, member: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, Error>` — [`TunnelFile`](#tunnelfile), [`Error`](../error/index.md#error)

  Reads one member of a listed file without downloading the file (`SUB_FILE`), or `None` when the file has no such member.

  `member` is a path inside the archive, such as `Metadata/plate_1.png`; for a video, the
  member `thumbnail` is its thumbnail. `file` must carry the `path` its listing gave:
  `SUB_FILE` addresses members as `<path>#<member>` (Studio's
  `PrinterFileSystem::UpdateFocusThumbnail2`). A member larger than `max_bytes` is refused,
  and the rest of the stream cancelled.

- <span id="filetunnelclient-delete-file"></span>`async fn delete_file<T: TimerProvider>(&mut self, timer: &T, file: &TunnelFile) -> Result<(), Error>` — [`TunnelFile`](#tunnelfile), [`Error`](../error/index.md#error)

  Deletes a listed file (`FILE_DEL`).

  Addresses it by `path` when its listing gave one and by name otherwise, as Studio's
  `PrinterFileSystem::DeleteFilesContinue` does. Don't delete before an upload of the same
  name: an upload overwrites, and an extra request first can race the upload's reply.

- <span id="filetunnelclient-upload"></span>`async fn upload<T: TimerProvider>(&mut self, timer: &T, storage: &str, name: &str, source: impl UploadSource) -> Result<(), Error>` — [`UploadSource`](../upload/index.md#uploadsource), [`Error`](../error/index.md#error)

  Uploads `source` to `storage` under `name` (`FILE_UPLOAD`), overwriting a file of that name.

  `storage` is a label from [`media_ability`](client/index.md#filetunnelclient), such as `emmc`; there is
  no default. To print the file afterwards from internal storage, start the job with
  [`PrintJobConfig::from_internal_storage`](../mqtt/index.md).

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

### `TunnelFile`

```rust
struct TunnelFile {
    pub name: String,
    pub path: Option<String>,
    pub time: u64,
    pub size: u64,
    pub storage: Option<String>,
}
```

One file from a [`FileTunnelClient::list_files`](client/index.md#filetunnelclient) listing.

Pass it back to [`FileTunnelClient::download`](client/index.md#filetunnelclient) unchanged: it addresses the file by `path` when
the printer gave one and by `name` otherwise, as Studio does.

#### Fields

- **`name`**: `String`

  The file name.

- **`path`**: `Option<String>`

  The printer's path for the file, or `None` when the listing gave none.

- **`time`**: `u64`

  Modification time, Unix seconds; `0` when the listing gave none.

- **`size`**: `u64`

  Size in bytes.

- **`storage`**: `Option<String>`

  The storage label the listing was made against, or `None` for the printer's default.

#### Trait Implementations

##### `impl Clone for TunnelFile`

- <span id="tunnelfile-clone"></span>`fn clone(&self) -> TunnelFile` — [`TunnelFile`](#tunnelfile)

##### `impl Debug for TunnelFile`

- <span id="tunnelfile-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for TunnelFile`

##### `impl PartialEq for TunnelFile`

- <span id="tunnelfile-partialeq-eq"></span>`fn eq(&self, other: &TunnelFile) -> bool` — [`TunnelFile`](#tunnelfile)

### `FileKind`

```rust
enum FileKind {
    Timelapse,
    Video,
    Model,
}
```

Which kind of file a listing asks for: `LIST_INFO`'s `type`, and the kind `PrinterClient::list_files` lists over either service.

Re-exported as `bambino::client::FileKind`.

#### Variants

- **`Timelapse`**

  Timelapse videos.

- **`Video`**

  Recorded videos.

- **`Model`**

  Print files (`.3mf`).

#### Trait Implementations

##### `impl Clone for FileKind`

- <span id="filekind-clone"></span>`fn clone(&self) -> FileKind` — [`FileKind`](#filekind)

##### `impl Copy for FileKind`

##### `impl Debug for FileKind`

- <span id="filekind-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for FileKind`

##### `impl Hash for FileKind`

- <span id="filekind-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for FileKind`

- <span id="filekind-partialeq-eq"></span>`fn eq(&self, other: &FileKind) -> bool` — [`FileKind`](#filekind)


---

## Functions

### `result_name`

```rust
fn result_name(result: i64) -> Option<&'static str>
```

Names a file tunnel `result` code as BambuStudio's `PrinterFileSystem.h` does, or `None` for a code it doesn't define.

Codes 3–6 are also Studio's own client-side failures, so seeing one from a printer is unusual.


---

## Constants

### `DEFAULT_MAX_FRAME_PAYLOAD`
```rust
const DEFAULT_MAX_FRAME_PAYLOAD: usize = 4_194_304usize;
```

Default largest frame payload [`FileTunnelClient`](client/index.md#filetunnelclient) accepts, in bytes (4 MiB).

A corrupt or hostile header could otherwise make the reader allocate up to 4 GiB, which on a
`no_std` target aborts through `alloc_error_handler` rather than returning an error. 4 MiB
leaves room for a long `LIST_INFO` reply on a host, but is far more than an embedded heap
holds: lower it there with [`FileTunnelClient::with_max_frame_payload`](client/index.md#filetunnelclient) or
`PrinterClient::with_file_tunnel_max_frame_size`.

### `FILE_TUNNEL_PORT`
```rust
const FILE_TUNNEL_PORT: u16 = 6_000u16;
```

Port the file tunnel listens on, shared with the chamber-image stream of P1/A1 printers.

