*[bambino](../../index.md) / [ftps](../index.md) / [parser](index.md)*

---

# Module `parser`

# UNIX Directory Listing Parsing Engine for FTPS

Decodes raw UNIX-style directory listings emitted by the printer's onboard vsFTPd server
over passive data channels. Employs whitespace-insensitive tokenization to handle
variable-width column padding and embeds robust temporal rollover heuristics.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`CurrentDateTime`](#currentdatetime) | struct | The printer's own wall-clock time, used as the reference for `parse_unix_listing`'s year-rollover heuristic. |
| [`FtpFile`](#ftpfile) | struct | Standardized representation of an entry retrieved from physical printer storage. |
| [`FtpTimestamp`](#ftptimestamp) | struct | An absolute file modification time as reported by the `MDTM` command, to one-second resolution. |
| [`parse_mdtm_timestamp`](#parse-mdtm-timestamp) | fn | Parses an `MDTM` reply body (`YYYYMMDDHHMMSS`) into an [`FtpTimestamp`](#ftptimestamp). |
| [`parse_unix_listing`](#parse-unix-listing) | fn | Parses a line-separated UNIX directory listing payload returned by `LIST`. |
| [`FTP_MAX_LISTING_ENTRIES`](#ftp-max-listing-entries) | const | Maximum number of entries [`parse_unix_listing`](#parse-unix-listing) will return from one `LIST` payload. |

## Types

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
[`FtpsClient::modification_time`](../index.md) for its mtime:
the printer stamps the file from its own clock as it writes, and `MDTM` reports that stamp with
an explicit four-digit year, so the reply is the printer's current time outright. `bambino-cli`'s
`files clock-check` does exactly this.

Where the firmware doesn't implement `MDTM`, the same probe read back through `LIST` gives only
a partial answer: month/day/HH:MM come off the printer's clock no matter what reference is
passed here, but the year is whatever that reference supplied. An unsynced printer typically
reads months behind real time. Host time is a fine fallback when the printer's clock is
unknown, and [`FtpFile::year_is_inferred`](#ftpfile) marks every entry whose year came from this
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

- <span id="currentdatetime-clone"></span>`fn clone(&self) -> CurrentDateTime` — [`CurrentDateTime`](#currentdatetime)

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
  otherwise inferred against the [`CurrentDateTime`](#currentdatetime) reference (see `year_is_inferred`).

- **`month`**: `u8`

  Numeric calendar month (1 to 12).

- **`day`**: `u8`

  Numeric day of the month (1 to 31).

- **`hour`**: `u8`

  Clock hour (0 to 23). Default is 0 if listing only provides a calendar year.

- **`minute`**: `u8`

  Clock minute (0 to 59). Default is 0 if listing only provides a calendar year.

- **`year_is_inferred`**: `bool`

  `true` when `year` was inferred from the [`CurrentDateTime`](#currentdatetime) reference (the wire's
  HH:MM-recent-file format, ambiguous by design; see `parse_unix_listing`'s doc comment),
  `false` when the wire reported an explicit `YYYY` directly. `year`'s rollover math always
  lands in `{reference_year, reference_year - 1}` for an inferred entry by construction, so it
  can never itself look implausible even when the reference is wrong. This flag is the only
  honest signal available without an independent probe like `bambino-cli`'s
  `files clock-check`.

#### Trait Implementations

##### `impl Clone for FtpFile`

- <span id="ftpfile-clone"></span>`fn clone(&self) -> FtpFile` — [`FtpFile`](#ftpfile)

##### `impl Debug for FtpFile`

- <span id="ftpfile-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for FtpFile`

##### `impl PartialEq for FtpFile`

- <span id="ftpfile-partialeq-eq"></span>`fn eq(&self, other: &FtpFile) -> bool` — [`FtpFile`](#ftpfile)

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

See [`FtpsClient::modification_time`](../index.md), which returns
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

- <span id="ftptimestamp-clone"></span>`fn clone(&self) -> FtpTimestamp` — [`FtpTimestamp`](#ftptimestamp)

##### `impl Copy for FtpTimestamp`

##### `impl Debug for FtpTimestamp`

- <span id="ftptimestamp-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for FtpTimestamp`

##### `impl PartialEq for FtpTimestamp`

- <span id="ftptimestamp-partialeq-eq"></span>`fn eq(&self, other: &FtpTimestamp) -> bool` — [`FtpTimestamp`](#ftptimestamp)


---

## Functions

### `parse_mdtm_timestamp`

```rust
fn parse_mdtm_timestamp(body: &str) -> Option<FtpTimestamp>
```

**Types:** [`FtpTimestamp`](#ftptimestamp)

Parses an `MDTM` reply body (`YYYYMMDDHHMMSS`) into an [`FtpTimestamp`](#ftptimestamp).

Returns `None` when the body isn't exactly 14 ASCII digits or the fields fall outside a valid
calendar date. The reply comes from the printer and is untrusted, same as a `LIST` line. Any
fractional-second suffix RFC 3659 permits (`.sss`) is ignored.

### `parse_unix_listing`

```rust
fn parse_unix_listing(payload: &str, now: CurrentDateTime) -> Vec<FtpFile>
```

**Types:** [`CurrentDateTime`](#currentdatetime), [`FtpFile`](#ftpfile)

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
must carry the *printer's* clock and not the host's (see [`CurrentDateTime`](#currentdatetime)). If comparing the
parsed datetime markers against that reference reveals
that the parsed datetime is in the future, the file belongs to last year's calendar cycle
(e.g., parsing a December modification date in January). In this event, we decrement the
calculated year by 1.


---

## Constants

### `FTP_MAX_LISTING_ENTRIES`
```rust
const FTP_MAX_LISTING_ENTRIES: usize = 65_536usize;
```

Maximum number of entries [`parse_unix_listing`](#parse-unix-listing) will return from one `LIST` payload.

The raw payload is already bounded by `FTPS_MAX_TRANSFER_BYTES` (512 MiB), but that cap is one
level upstream of the allocation that matters here: a listing of millions of ~20-byte lines
fits inside it and still expands into tens of millions of `FtpFile`s, each carrying its own
heap `String` — a larger and far more fragmented footprint than the bytes it came from, and on
no_std/Embassy the resulting exhaustion is the uncatchable `alloc_error_handler` abort, not a
`Result`. Mirrors `FTP_MAX_RESPONSE_LINES`' role on the control channel. Set well above any
plausible real printer directory (the microSD holds thousands of files, not millions).

