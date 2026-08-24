*[bambino](../../index.md) / [ftps](../index.md) / [client](index.md)*

---

# Module `client`

# Implicit FTPS Client Implementation

Implements a secure, platform-agnostic, asynchronous FTPS client designed to execute
over our abstract `AsyncIo` boundaries. This client coordinates implicitly encrypted control channels
on Port 990, Passive port negotiation, TLS session wrapping (with A1-series plaintext bypass),
whitespace-insensitive UNIX listings parsing, and robust chunked uploads [REF-FTPS-CONN] [REF-FTPS-OPS].

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`FtpsClient`](#ftpsclient) | struct | Lightweight, high-reliability implicit FTPS client running on top of abstract I/O traits. |

## Types

### `FtpsClient<RawIO, Tls, Factory, FtpsTimer>`

```rust
struct FtpsClient<RawIO, Tls, Factory, FtpsTimer>
where
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: RawStreamFactory<RawIO>,
    FtpsTimer: TimerProvider {
    // [REDACTED: Private Fields]
}
```

Lightweight, high-reliability implicit FTPS client running on top of abstract I/O traits.

**Poisoning invariant:** the control channel is a single ordered stream — every command gets
exactly one reply, and a `write_command`/`read_response` failure anywhere leaves no way to
know whether the server's reply for that command is still coming. Reusing the client at that
point risks a later, unrelated command silently reading the stale reply instead of its own.
To make this safe, the client sets `poisoned = true` (originally only on the
`list_directory`/`upload_file`/`download_file` data-transfer window between the server's
`150`/`125` "opening data connection" reply and the matching final reply, since that's the
widest such window; now on every `write_command`/`read_response` failure in every method,
including the single-reply metadata/filesystem commands, and unconditionally in
`disconnect()`); every public method checks the flag first and returns
[`Error::ProtocolViolation`](../../error/index.md#error) immediately if set. A poisoned client must be discarded —
reconnect via a fresh [`FtpsClient::connect`](#ftpsclient) call instead of reusing the instance.

**`FtpsTimer`** bounds every read against a per-call wall-clock deadline (see
`FTPS_READ_TIMEOUT_SECS`/`FTPS_TRANSFER_CONFIRM_TIMEOUT_SECS` in `protocol.rs`) — owned
independently of whatever `Timer` a `PrinterClient` that hands out this client is using,
since `PrinterClient::storage()` hands out direct `&mut FtpsClient` access rather than
mediating every method call the way it does for MQTT/camera (no call site to thread
`&self.timer` through). Defaults to `DummyTimer` (unbounded, matching this crate's existing
`DummyTimer` convention) for direct (non-`PrinterClient`) callers that don't supply one.

#### Implementations

- <span id="ftpsclient-connect"></span>`async fn connect(raw_control: RawIO, tls_connector: Tls, data_factory: Factory, identity: PrinterIdentity, timer: FtpsTimer, allow_unverified_tls_1_2: bool) -> Result<Self, Error>` — [`PrinterIdentity`](../../identity/index.md#printeridentity), [`Error`](../../error/index.md#error)

  Establishes the secure control channel, performs login handshakes, and configures security properties.

  **Implicit Security Handshake:**
  Prior to issuing or evaluating any standard text commands, the raw connection socket must be
  wrapped in a secure TLS session immediately upon establishment. Explicit handshakes (such as `AUTH TLS`)
  are not utilized.

- <span id="ftpsclient-list-directory"></span>`async fn list_directory(&mut self, remote_path: &str, now: CurrentDateTime) -> Result<Vec<FtpFile>, Error>` — [`CurrentDateTime`](../parser/index.md#currentdatetime), [`FtpFile`](../parser/index.md#ftpfile), [`Error`](../../error/index.md#error)

  Queries the storage server for raw directory listings and parses their structures.

  `now` must carry the **printer's** wall-clock time, not the host's. A `LIST` line omits
  the year for recently-modified files, and the printer's clock is the reference vsFTPd used
  when deciding to omit it. Bambu printers in LAN mode routinely never sync time, so the two
  clocks can be years apart; see [`CurrentDateTime`](../parser/index.md#currentdatetime) for how to recover the printer's and what
  passing host time instead costs. Entries whose year came from `now` are flagged with
  [`FtpFile::year_is_inferred`](../parser/index.md#ftpfile).

- <span id="ftpsclient-get-file-size"></span>`async fn get_file_size(&mut self, remote_path: &str) -> Result<u64, Error>` — [`Error`](../../error/index.md#error)

  Queries the exact size of a file stored on the printer's MicroSD card.

- <span id="ftpsclient-modification-time"></span>`async fn modification_time(&mut self, remote_path: &str) -> Result<Option<FtpTimestamp>, Error>` — [`FtpTimestamp`](../parser/index.md#ftptimestamp), [`Error`](../../error/index.md#error)

  Queries a file's absolute modification time via `MDTM`, to one-second resolution.

  This is the only path to a file timestamp that doesn't go through a reference clock: the
  `213 YYYYMMDDHHMMSS` reply carries an explicit four-digit year, where a `LIST` line omits
  the year entirely for recently-modified files (see [`CurrentDateTime`](../parser/index.md#currentdatetime)). It's still the
  printer's own notion of when the file was written; an unsynced printer answers
  confidently and wrongly rather than ambiguously.

  Confirmed working on a P1S (`reference/02_ftps.md` §2.2); unverified on other models, which
  is why the return is an `Option`. These firmware builds are trimmed (the same P1S answers
  `502` to `STAT` even though stock vsFTPd implements it), so `Ok(None)` is returned on
  `500`/`502` and callers should treat it as "fall back to the `LIST` heuristic", not as an
  error. A missing file still surfaces as `Err`, not `None`: unsupported and absent are
  different answers.

  This is a per-file query, not a listing strategy: it costs a round trip each, and a
  well-used printer's card holds thousands of files. Resolving a directory this way is not
  worth offering: use `list_directory` against a [`CurrentDateTime`](../parser/index.md#currentdatetime) reference for that, and
  reach for `MDTM` when one file's timestamp needs to be exact.

- <span id="ftpsclient-delete-file"></span>`async fn delete_file(&mut self, remote_path: &str) -> Result<(), Error>` — [`Error`](../../error/index.md#error)

  Removes a targeted file from non-volatile storage.

- <span id="ftpsclient-upload-file"></span>`async fn upload_file(&mut self, remote_path: &str, data: &[u8]) -> Result<(), Error>` — [`Error`](../../error/index.md#error)

  Uploads a binary payload directly to MicroSD card storage.

  **Flush and Close Race Mitigation:**
  1. Immediately drop the passive data channel upon finishing transmissions. This prevents
     standard TLS graceful shutdown waits which would trigger indefinite hangs on physical vsFTPd.
  2. Wait up to 300 seconds for the `226` transfer confirmation to print. Issuing downstream
     print commands prior to this confirmation halts the printer due to microSD write latency exceptions [REF-FTPS-FLUSH].
  3. Unconditionally verify the uploaded size via the `SIZE` command on both a `226` and a
     transient `426` reply — this guards against silent SD card write truncation on every
     model, not only the P2S/X2D TLS 1.3 close race [REF-FTPS-CONN].

- <span id="ftpsclient-download-file"></span>`async fn download_file(&mut self, remote_path: &str) -> Result<Vec<u8>, Error>` — [`Error`](../../error/index.md#error)

  Downloads the contents of a remote file from MicroSD storage via the RETR command.

  Negotiates a passive data channel, retrieves the binary payload, and returns the raw
  bytes. Unconditionally verifies the downloaded length against the `SIZE` command after
  transfer completes — a clean `226` reply alone doesn't prove the data channel didn't
  close early [REF-FTPS-CONN].

- <span id="ftpsclient-create-directory"></span>`async fn create_directory(&mut self, path: &str) -> Result<(), Error>` — [`Error`](../../error/index.md#error)

  Creates a directory on the printer's MicroSD storage.

- <span id="ftpsclient-remove-directory"></span>`async fn remove_directory(&mut self, path: &str) -> Result<(), Error>` — [`Error`](../../error/index.md#error)

  Removes a directory from the printer's MicroSD storage.

  Returns success for both `250` (deleted) and `550` (already absent),
  matching the idempotent cleanup semantics of `delete_file`.

- <span id="ftpsclient-rename-file"></span>`async fn rename_file(&mut self, from: &str, to: &str) -> Result<(), Error>` — [`Error`](../../error/index.md#error)

  Renames a file or directory on the printer's MicroSD storage.

  Executes the standard FTP two-step rename sequence: `RNFR` (rename from)
  followed by `RNTO` (rename to).

- <span id="ftpsclient-get-available-space"></span>`async fn get_available_space(&mut self) -> Result<u64, Error>` — [`Error`](../../error/index.md#error)

  Queries the available capacity of the MicroSD card, in bytes.

- <span id="ftpsclient-disconnect"></span>`async fn disconnect(&mut self)`

  Sends a QUIT command and cleanly terminates the FTP session.

  Best-effort: errors during QUIT are silently ignored since the connection is being torn
  down regardless. Non-consuming (`&mut self`, not `self`) by design:
  `PrinterClient::storage()` only exposes `&mut FtpsClient`, and direct-module
  consumers may want to disconnect and reconnect the same variable via a fresh `connect()`
  call without re-declaring it.

  Always poisons the client on the way out (extends the poisoning mechanism — see the
  struct doc comment) so every subsequent method call on this instance fails cleanly with
  the same "must reconnect" error, instead of a caller mistaking a disconnected client for
  a live one. Idempotent: calling this more than once is a no-op after the first call.

#### Trait Implementations

