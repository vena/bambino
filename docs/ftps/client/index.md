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
| [`BambuFtpsClient`](#bambuftpsclient) | struct | Lightweight, high-reliability implicit FTPS client running on top of abstract I/O traits. |

## Types

### `BambuFtpsClient<RawIO, Tls, Factory, FtpsTimer>`

```rust
struct BambuFtpsClient<RawIO, Tls, Factory, FtpsTimer>
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
To make this safe, the client sets `poisoned = true` (BUG-004: originally only on the
`list_directory`/`upload_file`/`download_file` data-transfer window between the server's
`150`/`125` "opening data connection" reply and the matching final reply, since that's the
widest such window; now on every `write_command`/`read_response` failure in every method,
including the single-reply metadata/filesystem commands, and unconditionally in
`disconnect()`); every public method checks the flag first and returns
[`Error::ProtocolViolation`] immediately if set. A poisoned client must be discarded —
reconnect via a fresh [`BambuFtpsClient::connect`] call instead of reusing the instance.

**`FtpsTimer`** bounds every read against a per-call wall-clock deadline (see
`FTPS_READ_TIMEOUT_SECS`/`FTPS_TRANSFER_CONFIRM_TIMEOUT_SECS` in `protocol.rs`) — owned
independently of whatever `Timer` a `PrinterClient` that hands out this client is using,
since `PrinterClient::storage()` hands out direct `&mut BambuFtpsClient` access rather than
mediating every method call the way it does for MQTT/camera (no call site to thread
`&self.timer` through). Defaults to `DummyTimer` (unbounded, matching this crate's existing
`DummyTimer` convention) for direct (non-`PrinterClient`) callers that don't supply one.

#### Implementations

- <span id="bambuftpsclient-connect"></span>`async fn connect(raw_control: RawIO, tls_connector: Tls, data_factory: Factory, model: PrinterModel, ip: &str, serial: &str, access_code: &str, timer: FtpsTimer, allow_unverified_tls_1_2: bool) -> Result<Self, Error>` — [`PrinterModel`](../../models/index.md#printermodel), [`Error`](../../error/index.md#error)

  Establishes the secure control channel, performs login handshakes, and configures security properties.

- <span id="bambuftpsclient-list-directory"></span>`async fn list_directory(&mut self, remote_path: &str, current_year: i32, current_month: u8, current_day: u8, current_hour: u8, current_minute: u8) -> Result<Vec<FtpFile>, Error>` — [`FtpFile`](../parser/index.md#ftpfile), [`Error`](../../error/index.md#error)

  Queries the storage server for raw directory listings and parses their structures.

- <span id="bambuftpsclient-get-file-size"></span>`async fn get_file_size(&mut self, remote_path: &str) -> Result<u64, Error>` — [`Error`](../../error/index.md#error)

  Queries the exact size of a file stored on the printer's MicroSD card.

- <span id="bambuftpsclient-delete-file"></span>`async fn delete_file(&mut self, remote_path: &str) -> Result<(), Error>` — [`Error`](../../error/index.md#error)

  Removes a targeted file from non-volatile storage.

- <span id="bambuftpsclient-upload-file"></span>`async fn upload_file(&mut self, remote_path: &str, data: &[u8]) -> Result<(), Error>` — [`Error`](../../error/index.md#error)

  Uploads a binary payload directly to MicroSD card storage.

- <span id="bambuftpsclient-download-file"></span>`async fn download_file(&mut self, remote_path: &str) -> Result<Vec<u8>, Error>` — [`Error`](../../error/index.md#error)

  Downloads the contents of a remote file from MicroSD storage via the RETR command.

- <span id="bambuftpsclient-create-directory"></span>`async fn create_directory(&mut self, path: &str) -> Result<(), Error>` — [`Error`](../../error/index.md#error)

  Creates a directory on the printer's MicroSD storage.

- <span id="bambuftpsclient-remove-directory"></span>`async fn remove_directory(&mut self, path: &str) -> Result<(), Error>` — [`Error`](../../error/index.md#error)

  Removes a directory from the printer's MicroSD storage.

- <span id="bambuftpsclient-rename-file"></span>`async fn rename_file(&mut self, from: &str, to: &str) -> Result<(), Error>` — [`Error`](../../error/index.md#error)

  Renames a file or directory on the printer's MicroSD storage.

- <span id="bambuftpsclient-get-available-space"></span>`async fn get_available_space(&mut self) -> Result<u64, Error>` — [`Error`](../../error/index.md#error)

  Queries the available capacity of the MicroSD card, in bytes.

- <span id="bambuftpsclient-disconnect"></span>`async fn disconnect(&mut self)`

  Sends a QUIT command and cleanly terminates the FTP session.

#### Trait Implementations

