*[bambino](../index.md) / [ftps](index.md)*

---

# Module `ftps`

# FTPS File Transfer Client

Implicit FTPS client for reading and writing files on the printer's SD card.

[`FtpsClient`](client/index.md#ftpsclient) handles the TLS control channel, passive-mode data connections,
and FTP command sequencing. It supports listing directories, uploading/downloading
files, checking free space, and basic file management (rename, delete, mkdir).
The [`parser`](../ams/parser/index.md) submodule handles UNIX-style directory listing output.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`client`](client/index.md) | mod | # Implicit FTPS Client Implementation |
| [`parser`](parser/index.md) | mod | # UNIX Directory Listing Parsing Engine for FTPS |

## Modules

- [`client`](client/index.md) — # Implicit FTPS Client Implementation
- [`parser`](parser/index.md) — # UNIX Directory Listing Parsing Engine for FTPS


---

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
[`Error::ProtocolViolation`](../error/index.md#error) immediately if set. A poisoned client must be discarded —
reconnect via a fresh [`FtpsClient::connect`](client/index.md#ftpsclient) call instead of reusing the instance.

**`FtpsTimer`** bounds every read against a per-call wall-clock deadline (see
`FTPS_READ_TIMEOUT_SECS`/`FTPS_TRANSFER_CONFIRM_TIMEOUT_SECS` in `protocol.rs`) — owned
independently of whatever `Timer` a `PrinterClient` that hands out this client is using,
since `PrinterClient::storage()` hands out direct `&mut FtpsClient` access rather than
mediating every method call the way it does for MQTT/camera (no call site to thread
`&self.timer` through). Defaults to `DummyTimer` (unbounded, matching this crate's existing
`DummyTimer` convention) for direct (non-`PrinterClient`) callers that don't supply one.

#### Implementations

- <span id="ftpsclient-connect"></span>`async fn connect(raw_control: RawIO, tls_connector: Tls, data_factory: Factory, identity: PrinterIdentity, timer: FtpsTimer, allow_unverified_tls_1_2: bool) -> Result<Self, Error>` — [`PrinterIdentity`](../identity/index.md#printeridentity), [`Error`](../error/index.md#error)

  Establishes the secure control channel, performs login handshakes, and configures security properties.

  **Implicit Security Handshake:**
  Prior to issuing or evaluating any standard text commands, the raw connection socket must be
  wrapped in a secure TLS session immediately upon establishment. Explicit handshakes (such as `AUTH TLS`)
  are not utilized.

- <span id="ftpsclient-list-directory"></span>`async fn list_directory(&mut self, remote_path: &str, now: CurrentDateTime) -> Result<Vec<FtpFile>, Error>` — [`CurrentDateTime`](parser/index.md#currentdatetime), [`FtpFile`](parser/index.md#ftpfile), [`Error`](../error/index.md#error)

  Queries the storage server for raw directory listings and parses their structures.

  `now` must carry the **printer's** wall-clock time, not the host's. A `LIST` line omits
  the year for recently-modified files, and the printer's clock is the reference vsFTPd used
  when deciding to omit it. Bambu printers in LAN mode routinely never sync time, so the two
  clocks can be years apart; see [`CurrentDateTime`](parser/index.md#currentdatetime) for how to recover the printer's and what
  passing host time instead costs. Entries whose year came from `now` are flagged with
  [`FtpFile::year_is_inferred`](parser/index.md#ftpfile).

- <span id="ftpsclient-get-file-size"></span>`async fn get_file_size(&mut self, remote_path: &str) -> Result<u64, Error>` — [`Error`](../error/index.md#error)

  Queries the exact size of a file stored on the printer's MicroSD card.

- <span id="ftpsclient-modification-time"></span>`async fn modification_time(&mut self, remote_path: &str) -> Result<Option<FtpTimestamp>, Error>` — [`FtpTimestamp`](parser/index.md#ftptimestamp), [`Error`](../error/index.md#error)

  Queries a file's absolute modification time via `MDTM`, to one-second resolution.

  This is the only path to a file timestamp that doesn't go through a reference clock: the
  `213 YYYYMMDDHHMMSS` reply carries an explicit four-digit year, where a `LIST` line omits
  the year entirely for recently-modified files (see [`CurrentDateTime`](parser/index.md#currentdatetime)). It's still the
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
  worth offering: use `list_directory` against a [`CurrentDateTime`](parser/index.md#currentdatetime) reference for that, and
  reach for `MDTM` when one file's timestamp needs to be exact.

- <span id="ftpsclient-delete-file"></span>`async fn delete_file(&mut self, remote_path: &str) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Removes a targeted file from non-volatile storage.

- <span id="ftpsclient-upload-file"></span>`async fn upload_file(&mut self, remote_path: &str, data: &[u8]) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Uploads a binary payload directly to MicroSD card storage.

  **Flush and Close Race Mitigation:**
  1. Immediately drop the passive data channel upon finishing transmissions. This prevents
     standard TLS graceful shutdown waits which would trigger indefinite hangs on physical vsFTPd.
  2. Wait up to 300 seconds for the `226` transfer confirmation to print. Issuing downstream
     print commands prior to this confirmation halts the printer due to microSD write latency exceptions [REF-FTPS-FLUSH].
  3. Unconditionally verify the uploaded size via the `SIZE` command on both a `226` and a
     transient `426` reply — this guards against silent SD card write truncation on every
     model, not only the P2S/X2D TLS 1.3 close race [REF-FTPS-CONN].

- <span id="ftpsclient-download-file"></span>`async fn download_file(&mut self, remote_path: &str) -> Result<Vec<u8>, Error>` — [`Error`](../error/index.md#error)

  Downloads the contents of a remote file from MicroSD storage via the RETR command.

  Negotiates a passive data channel, retrieves the binary payload, and returns the raw
  bytes. Unconditionally verifies the downloaded length against the `SIZE` command after
  transfer completes — a clean `226` reply alone doesn't prove the data channel didn't
  close early [REF-FTPS-CONN].

- <span id="ftpsclient-create-directory"></span>`async fn create_directory(&mut self, path: &str) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Creates a directory on the printer's MicroSD storage.

- <span id="ftpsclient-remove-directory"></span>`async fn remove_directory(&mut self, path: &str) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Removes a directory from the printer's MicroSD storage.

  Returns success for both `250` (deleted) and `550` (already absent),
  matching the idempotent cleanup semantics of `delete_file`.

- <span id="ftpsclient-rename-file"></span>`async fn rename_file(&mut self, from: &str, to: &str) -> Result<(), Error>` — [`Error`](../error/index.md#error)

  Renames a file or directory on the printer's MicroSD storage.

  Executes the standard FTP two-step rename sequence: `RNFR` (rename from)
  followed by `RNTO` (rename to).

- <span id="ftpsclient-get-available-space"></span>`async fn get_available_space(&mut self) -> Result<u64, Error>` — [`Error`](../error/index.md#error)

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

### `CurrentDateTime`

```rust
struct CurrentDateTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}
```

The printer's own wall-clock time, used as the reference for `parse_unix_listing`'s year-rollover heuristic.

This must be the **printer's** believed current time, not the host's. vsFTPd decides whether
to emit `HH:MM` or `YYYY` in a `LIST` line by comparing each file's mtime against the printer's
clock, so that clock is the only reference against which an omitted year can be recovered.
Bambu printers in LAN mode frequently never sync time; their clock restarts from a fixed base
on every boot, so it reads months stale. Passing host time to such a printer stamps every
year-omitted entry with the wrong year, and not even consistently wrong, because the rollover
comparison below then fires on an arbitrary subset of entries.

Recover it by uploading a throwaway file and asking
[`FtpsClient::modification_time`](#) for its mtime:
the printer stamps the file from its own clock as it writes, and `MDTM` reports that stamp with
an explicit four-digit year, so the reply is the printer's current time outright. `bambino-cli`'s
`files clock-check` does exactly this.

Where the firmware doesn't implement `MDTM`, the same probe read back through `LIST` gives only
a partial answer: month/day/HH:MM come off the printer's clock no matter what reference is
passed here, but the year is whatever that reference supplied. An unsynced printer typically
reads months behind real time. Host time is a fine fallback when the printer's clock is
unknown, and [`FtpFile::year_is_inferred`](parser/index.md#ftpfile) marks every entry whose year came from this
reference rather than from the wire.

Note the limit of what this buys: it is the printer's *current* time, not evidence of when any
given file was actually written. Every timestamp FTPS reports, whether reconstructed from
`LIST` or read outright via `MDTM`, is what the printer believed when it wrote the file, so a
clock that was wrong then is still wrong now. A better reference makes the reconstruction more
likely to be right, never authoritative.

#### Fields

- **`year`**: `i32`

  The printer's calendar year.

- **`month`**: `u8`

  The printer's month (1-12).

- **`day`**: `u8`

  The printer's day of month (1-31).

- **`hour`**: `u8`

  The printer's hour (0-23).

- **`minute`**: `u8`

  The printer's minute (0-59).

#### Trait Implementations

##### `impl Clone for CurrentDateTime`

- <span id="currentdatetime-clone"></span>`fn clone(&self) -> CurrentDateTime` — [`CurrentDateTime`](parser/index.md#currentdatetime)

##### `impl Copy for CurrentDateTime`

##### `impl Debug for CurrentDateTime`

- <span id="currentdatetime-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

### `FtpFile`

```rust
struct FtpFile {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub year_is_inferred: bool,
}
```

Standardized representation of an entry retrieved from physical printer storage.

#### Fields

- **`name`**: `String`

  The parsed file or directory name, exactly as reported by the raw `LIST` line
  — recovered via `SplitWhitespace::remainder()` rather than re-tokenizing
  and rejoining with a single space, so internal runs of multiple consecutive spaces
  round-trip exactly and remain usable as-is in `delete_file`/`download_file`.

- **`is_dir`**: `bool`

  Identifies directory nodes versus standard data payloads.

- **`size`**: `u64`

  Absolute size of the file, in bytes.

- **`year`**: `i32`

  Reconstructed modification year: taken verbatim from the wire when the listing carried one,
  otherwise inferred against the [`CurrentDateTime`](parser/index.md#currentdatetime) reference (see `year_is_inferred`).

- **`month`**: `u8`

  Numeric calendar month (1 to 12).

- **`day`**: `u8`

  Numeric day of the month (1 to 31).

- **`hour`**: `u8`

  Clock hour (0 to 23). Default is 0 if listing only provides a calendar year.

- **`minute`**: `u8`

  Clock minute (0 to 59). Default is 0 if listing only provides a calendar year.

- **`year_is_inferred`**: `bool`

  `true` when `year` was inferred from the [`CurrentDateTime`](parser/index.md#currentdatetime) reference (the wire's
  HH:MM-recent-file format, ambiguous by design; see `parse_unix_listing`'s doc comment),
  `false` when the wire reported an explicit `YYYY` directly. `year`'s rollover math always
  lands in `{reference_year, reference_year - 1}` for an inferred entry by construction, so it
  can never itself look implausible even when the reference is wrong. This flag is the only
  honest signal available without an independent probe like `bambino-cli`'s
  `files clock-check`.

#### Trait Implementations

##### `impl Clone for FtpFile`

- <span id="ftpfile-clone"></span>`fn clone(&self) -> FtpFile` — [`FtpFile`](parser/index.md#ftpfile)

##### `impl Debug for FtpFile`

- <span id="ftpfile-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for FtpFile`

##### `impl PartialEq for FtpFile`

- <span id="ftpfile-partialeq-eq"></span>`fn eq(&self, other: &FtpFile) -> bool` — [`FtpFile`](parser/index.md#ftpfile)

### `FtpTimestamp`

```rust
struct FtpTimestamp {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}
```

An absolute file modification time as reported by the `MDTM` command, to one-second resolution.

Unlike everything derived from a `LIST` line, this carries an explicit four-digit year straight
off the wire, with no reference clock and no inference. It is still the printer's *own* notion of
when the file was written, so an unsynced printer yields a confidently-wrong absolute timestamp
rather than an ambiguous one; `MDTM` removes the reconstruction, not the clock skew.

See [`FtpsClient::modification_time`](#), which returns
`None` when the printer's firmware doesn't implement the command.

#### Fields

- **`year`**: `i32`

  Four-digit calendar year, as reported.

- **`month`**: `u8`

  Calendar month (1-12).

- **`day`**: `u8`

  Day of month (1-31).

- **`hour`**: `u8`

  Hour (0-23).

- **`minute`**: `u8`

  Minute (0-59).

- **`second`**: `u8`

  Second (0-59).

#### Trait Implementations

##### `impl Clone for FtpTimestamp`

- <span id="ftptimestamp-clone"></span>`fn clone(&self) -> FtpTimestamp` — [`FtpTimestamp`](parser/index.md#ftptimestamp)

##### `impl Copy for FtpTimestamp`

##### `impl Debug for FtpTimestamp`

- <span id="ftptimestamp-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for FtpTimestamp`

##### `impl PartialEq for FtpTimestamp`

- <span id="ftptimestamp-partialeq-eq"></span>`fn eq(&self, other: &FtpTimestamp) -> bool` — [`FtpTimestamp`](parser/index.md#ftptimestamp)


---

## Functions

### `parse_mdtm_timestamp`

```rust
fn parse_mdtm_timestamp(body: &str) -> Option<FtpTimestamp>
```

**Types:** [`FtpTimestamp`](parser/index.md#ftptimestamp)

Parses an `MDTM` reply body (`YYYYMMDDHHMMSS`) into an [`FtpTimestamp`](parser/index.md#ftptimestamp).

Returns `None` when the body isn't exactly 14 ASCII digits or the fields fall outside a valid
calendar date. The reply comes from the printer and is untrusted, same as a `LIST` line. Any
fractional-second suffix RFC 3659 permits (`.sss`) is ignored.

### `parse_unix_listing`

```rust
fn parse_unix_listing(payload: &str, now: CurrentDateTime) -> Vec<FtpFile>
```

**Types:** [`CurrentDateTime`](parser/index.md#currentdatetime), [`FtpFile`](parser/index.md#ftpfile)

Parses a line-separated UNIX directory listing payload returned by `LIST`.

**Whitespace-Insensitive Delimiting:**
Embedded systems typically insert arbitrary, variable-width spacing gaps to line up listings.
Rather than relying on rigid column indexes, this implementation tokenizes columns by splitting
on contiguous whitespace sequences, collecting the initial 8 protocol columns, and slicing
the untouched remainder verbatim as the filename — preserves internal multi-space
runs exactly, rather than re-tokenizing and rejoining with a single space.

**Temporal Rollover Mitigation:**
UNIX listing formats omit the modification year and provide a timestamp (HH:MM) if the file
was updated within the last six months. In this scenario the year is taken from `now`, which
must carry the *printer's* clock and not the host's (see [`CurrentDateTime`](parser/index.md#currentdatetime)). If comparing the
parsed datetime markers against that reference reveals
that the parsed datetime is in the future, the file belongs to last year's calendar cycle
(e.g., parsing a December modification date in January). In this event, we decrement the
calculated year by 1.

