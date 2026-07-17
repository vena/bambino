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
| [`CurrentDateTime`](#currentdatetime) | struct | Bundles the calendar-time components `parse_unix_listing`'s year-rollover heuristic needs. |
| [`FtpFile`](#ftpfile) | struct | Standardized representation of an entry retrieved from physical printer storage. |
| [`parse_unix_listing`](#parse-unix-listing) | fn | Parses a line-separated UNIX directory listing payload returned by `LIST`. |

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

Bundles the calendar-time components `parse_unix_listing`'s year-rollover heuristic needs.

#### Fields

- **`year`**: `i32`

  Current calendar year.

- **`month`**: `u8`

  Current month (1-12).

- **`day`**: `u8`

  Current day of month (1-31).

- **`hour`**: `u8`

  Current hour (0-23).

- **`minute`**: `u8`

  Current minute (0-59).

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

  Reconstructed modification year, calculated using current time markers.

- **`month`**: `u8`

  Numeric calendar month (1 to 12).

- **`day`**: `u8`

  Numeric day of the month (1 to 31).

- **`hour`**: `u8`

  Clock hour (0 to 23). Default is 0 if listing only provides a calendar year.

- **`minute`**: `u8`

  Clock minute (0 to 59). Default is 0 if listing only provides a calendar year.

- **`year_is_inferred`**: `bool`

  `true` when `year` was inferred from the host's current date (the wire's HH:MM-recent-
  file format, ambiguous by design — see this function's doc comment), `false` when the
  wire reported an explicit `YYYY` directly. `year`'s rollover math always lands
  in `{current_year, current_year - 1}` for an inferred entry by construction, so it can
  never itself look implausible even when the printer's own clock (the source of the
  month/day/HH:MM this was inferred from) is wrong — this flag is the only honest signal
  available without an independent probe like `bambino-cli`'s `files clock-check`.

#### Trait Implementations

##### `impl Clone for FtpFile`

- <span id="ftpfile-clone"></span>`fn clone(&self) -> FtpFile` — [`FtpFile`](#ftpfile)

##### `impl Debug for FtpFile`

- <span id="ftpfile-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for FtpFile`

##### `impl PartialEq for FtpFile`

- <span id="ftpfile-partialeq-eq"></span>`fn eq(&self, other: &FtpFile) -> bool` — [`FtpFile`](#ftpfile)


---

## Functions

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
was updated within the last six months. In this scenario, we default to the host system's
`current_year`. If comparing the parsed datetime markers against our system context reveals
that the parsed datetime is in the future, the file belongs to last year's calendar cycle
(e.g., parsing a December modification date in January). In this event, we decrement the
calculated year by 1.

