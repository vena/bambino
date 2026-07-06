**bambino > ftps > parser**

# Module: ftps::parser

## Contents

**Structs**

- [`FtpFile`](#ftpfile) - Standardized representation of an entry retrieved from physical printer storage.

**Functions**

- [`parse_unix_listing`](#parse_unix_listing) - Parses a line-separated UNIX directory listing payload returned by `LIST`.

---

## bambino::ftps::parser::FtpFile

*Struct*

Standardized representation of an entry retrieved from physical printer storage.

**Fields:**
- `name: String` - The parsed file or directory name, preserving single spaces between words.
- `is_dir: bool` - Identifies directory nodes versus standard data payloads.
- `size: u64` - Absolute size of the file, in bytes.
- `year: i32` - Reconstructed modification year, calculated using current time markers.
- `month: u8` - Numeric calendar month (1 to 12).
- `day: u8` - Numeric day of the month (1 to 31).
- `hour: u8` - Clock hour (0 to 23). Default is 0 if listing only provides a calendar year.
- `minute: u8` - Clock minute (0 to 59). Default is 0 if listing only provides a calendar year.

**Traits:** Eq

**Trait Implementations:**

- **Clone**
  - `fn clone(self: &Self) -> FtpFile`
- **PartialEq**
  - `fn eq(self: &Self, other: &FtpFile) -> bool`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`



## bambino::ftps::parser::parse_unix_listing

*Function*

Parses a line-separated UNIX directory listing payload returned by `LIST`.

**Whitespace-Insensitive Delimiting:**
Embedded systems typically insert arbitrary, variable-width spacing gaps to line up listings.
Rather than relying on rigid column indexes, this implementation tokenizes columns by splitting
on contiguous whitespace sequences, collecting the initial 8 protocol columns, and rebuilding
the rest as the filename.

**Temporal Rollover Mitigation:**
UNIX listing formats omit the modification year and provide a timestamp (HH:MM) if the file
was updated within the last six months. In this scenario, we default to the host system's
`current_year`. If comparing the parsed datetime markers against our system context reveals
that the parsed datetime is in the future, the file belongs to last year's calendar cycle
(e.g., parsing a December modification date in January). In this event, we decrement the
calculated year by 1.

```rust
fn parse_unix_listing(payload: &str, current_year: i32, current_month: u8, current_day: u8, current_hour: u8, current_minute: u8) -> Vec<FtpFile>
```



