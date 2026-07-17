//! # UNIX Directory Listing Parsing Engine for FTPS
//!
//! Decodes raw UNIX-style directory listings emitted by the printer's onboard vsFTPd server
//! over passive data channels. Employs whitespace-insensitive tokenization to handle
//! variable-width column padding and embeds robust temporal rollover heuristics.

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

/// Standardized representation of an entry retrieved from physical printer storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtpFile {
    /// The parsed file or directory name, exactly as reported by the raw `LIST` line
    /// (BUG-088) — recovered via `SplitWhitespace::remainder()` rather than re-tokenizing
    /// and rejoining with a single space, so internal runs of multiple consecutive spaces
    /// round-trip exactly and remain usable as-is in `delete_file`/`download_file`.
    pub name: String,
    /// Identifies directory nodes versus standard data payloads.
    pub is_dir: bool,
    /// Absolute size of the file, in bytes.
    pub size: u64,
    /// Reconstructed modification year, calculated using current time markers.
    pub year: i32,
    /// Numeric calendar month (1 to 12).
    pub month: u8,
    /// Numeric day of the month (1 to 31).
    pub day: u8,
    /// Clock hour (0 to 23). Default is 0 if listing only provides a calendar year.
    pub hour: u8,
    /// Clock minute (0 to 59). Default is 0 if listing only provides a calendar year.
    pub minute: u8,
    /// `true` when `year` was inferred from the host's current date (the wire's HH:MM-recent-
    /// file format, ambiguous by design — see this function's doc comment), `false` when the
    /// wire reported an explicit `YYYY` directly. BUG-042: `year`'s rollover math always lands
    /// in `{current_year, current_year - 1}` for an inferred entry by construction, so it can
    /// never itself look implausible even when the printer's own clock (the source of the
    /// month/day/HH:MM this was inferred from) is wrong — this flag is the only honest signal
    /// available without an independent probe like `bambino-cli`'s `files clock-check`.
    pub year_is_inferred: bool,
}

/// Converts a 3-letter month abbreviation into a calendar month index.
fn parse_month(month: &str) -> Option<u8> {
    match month {
        "Jan" | "jan" => Some(1),
        "Feb" | "feb" => Some(2),
        "Mar" | "mar" => Some(3),
        "Apr" | "apr" => Some(4),
        "May" | "may" => Some(5),
        "Jun" | "jun" => Some(6),
        "Jul" | "jul" => Some(7),
        "Aug" | "aug" => Some(8),
        "Sep" | "sep" => Some(9),
        "Oct" | "oct" => Some(10),
        "Nov" | "nov" => Some(11),
        "Dec" | "dec" => Some(12),
        _ => None,
    }
}

/// Returns the number of days in a given calendar month, leap-year-aware for February.
fn days_in_month(month: u8, year: i32) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let is_leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
            if is_leap { 29 } else { 28 }
        }
        _ => 31,
    }
}

/// Splits the next whitespace-delimited token off the front of `s`, returning
/// `(token, remainder)`. Unlike `str::split_whitespace()`, callers retain a real `&str`
/// slice into the original string at every step, so the untouched tail (e.g. everything
/// after the Nth column) can be sliced out verbatim — preserving any internal multi-space
/// runs — instead of losing that spacing by re-tokenizing and rejoining with `.join(" ")`
/// (BUG-088).
fn next_token(s: &str) -> Option<(&str, &str)> {
    let trimmed = s.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let end = trimmed
        .find(|c: char| c.is_whitespace())
        .unwrap_or(trimmed.len());
    Some((&trimmed[..end], &trimmed[end..]))
}

/// Bundles the 4 consecutive same-typed (`u8`) "current time" components `parse_unix_listing`
/// needs for its rollover heuristic — replaces 4 adjacent positional `u8` params that were a
/// transposition footgun with no compiler catch.
#[derive(Debug, Clone, Copy)]
pub struct CurrentDateTime {
    /// Current month (1-12).
    pub month: u8,
    /// Current day of month (1-31).
    pub day: u8,
    /// Current hour (0-23).
    pub hour: u8,
    /// Current minute (0-59).
    pub minute: u8,
}

/// Parses a line-separated UNIX directory listing payload returned by `LIST`.
///
/// **Whitespace-Insensitive Delimiting:**
/// Embedded systems typically insert arbitrary, variable-width spacing gaps to line up listings.
/// Rather than relying on rigid column indexes, this implementation tokenizes columns by splitting
/// on contiguous whitespace sequences, collecting the initial 8 protocol columns, and slicing
/// the untouched remainder verbatim as the filename (BUG-088 — preserves internal multi-space
/// runs exactly, rather than re-tokenizing and rejoining with a single space).
///
/// **Temporal Rollover Mitigation:**
/// UNIX listing formats omit the modification year and provide a timestamp (HH:MM) if the file
/// was updated within the last six months. In this scenario, we default to the host system's
/// `current_year`. If comparing the parsed datetime markers against our system context reveals
/// that the parsed datetime is in the future, the file belongs to last year's calendar cycle
/// (e.g., parsing a December modification date in January). In this event, we decrement the
/// calculated year by 1.
pub fn parse_unix_listing(payload: &str, current_year: i32, now: CurrentDateTime) -> Vec<FtpFile> {
    let current_month = now.month;
    let current_day = now.day;
    let current_hour = now.hour;
    let current_minute = now.minute;
    let mut files = Vec::new();

    for line in payload.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut rest = trimmed;

        // Standard UNIX listings contain exactly 9 base columns:
        // [0:Perms] [1:Links] [2:Owner] [3:Group] [4:Size] [5:Month] [6:Day] [7:TimeOrYear] [8+:Name]
        let perms = match next_token(rest) {
            Some((tok, r)) => {
                rest = r;
                tok
            }
            None => continue,
        };
        for _ in 0..3 {
            if let Some((_, r)) = next_token(rest) {
                rest = r;
            }
        }

        let size = match next_token(rest) {
            Some((tok, r)) => match tok.parse::<u64>() {
                Ok(sz) => {
                    rest = r;
                    sz
                }
                Err(_) => continue,
            },
            None => continue,
        };
        let month_str = match next_token(rest) {
            Some((tok, r)) => {
                rest = r;
                tok
            }
            None => continue,
        };
        let day_str = match next_token(rest) {
            Some((tok, r)) => {
                rest = r;
                tok
            }
            None => continue,
        };
        let time_or_year = match next_token(rest) {
            Some((tok, r)) => {
                rest = r;
                tok
            }
            None => continue,
        };

        // BUG-088: everything following the 8th whitespace-delimited block is the filename.
        // `rest` is a real slice into `trimmed` at this point (see `next_token`'s doc comment),
        // so it's sliced out verbatim rather than re-tokenized and rejoined with `.join(" ")`
        // — that used to collapse any run of multiple consecutive spaces in the real filename
        // down to one, confirmed on real hardware (a P1S) to desync the reported name from the
        // printer's actual on-disk name and make `delete_file`/`download_file` silently no-op
        // (masked by `delete_file`'s intentional idempotent "already gone" 550 handling) when
        // called with the reported name. `trim_start()` only strips the whitespace run
        // *before* the filename (the 8/9-column separator); the whole line was already
        // `.trim()`-med above, so there's no trailing whitespace to strip.
        let name = rest.trim_start();
        if name.is_empty() {
            continue;
        }
        let name = name.to_string();

        // Defense in depth: reject filenames containing the same command-injection-capable
        // control characters `validate_ftp_path` rejects on the way out. A caller might
        // round-trip a name returned here into another FTP command
        // (`delete_file`/`rename_file`/`download_file`) — this client can't control what
        // characters the printer's own filesystem (or a MITM'd `LIST` response) contains, so
        // skip any entry that couldn't be safely reused as a path argument.
        if super::protocol::validate_ftp_path(&name).is_err() {
            continue;
        }

        let is_dir = perms.starts_with('d');
        let month = match parse_month(month_str) {
            Some(m) => m,
            None => continue,
        };
        let day = match day_str.parse::<u8>().ok() {
            Some(d) if (1..=31).contains(&d) => d,
            _ => continue,
        };

        let mut hour = 0;
        let mut minute = 0;
        let mut year = current_year;
        let year_is_inferred = time_or_year.contains(':');

        if time_or_year.contains(':') {
            // Field contains HH:MM time layout. Parse temporal properties.
            let mut time_parts = time_or_year.split(':');
            let parsed_hour = time_parts
                .next()
                .and_then(|h| h.parse::<u8>().ok())
                .unwrap_or(0);
            let parsed_minute = time_parts
                .next()
                .and_then(|m| m.parse::<u8>().ok())
                .unwrap_or(0);

            if parsed_hour > 23 || parsed_minute > 59 {
                continue;
            }
            hour = parsed_hour;
            minute = parsed_minute;

            // Rollover calculation: If parsed datetime attributes exceed our current system markers,
            // we have crossed a calendar year boundary. Drop the year parameter accordingly.
            let parsed_dt = (month, day, hour, minute);
            let current_dt = (current_month, current_day, current_hour, current_minute);
            if parsed_dt > current_dt {
                year = current_year - 1;
            }
        } else {
            // Field contains direct YYYY calendar year representation.
            // BUG-149: sibling gap to BUG-061's day/hour/minute range checks — a parse failure
            // must reject the line, not silently default to current_year, and a parsed value
            // still needs a sanity range (printer filesystems don't predate 2000).
            match time_or_year.parse::<i32>() {
                Ok(y) if (2000..=9999).contains(&y) => year = y,
                _ => continue,
            }
        }

        // BUG-133: sibling gap to BUG-061's flat 1..=31 day range check — validate against the
        // actual month length (leap-year-aware, using the now-finalized `year`) so a
        // calendar-invalid line (e.g. "Feb 30") from the untrusted printer LIST output is
        // rejected instead of silently accepted.
        if day > days_in_month(month, year) {
            continue;
        }

        files.push(FtpFile {
            name,
            is_dir,
            size,
            year,
            month,
            day,
            hour,
            minute,
            year_is_inferred,
        });
    }

    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_unix_file_parsing() {
        let payload = "-rw-r--r--    1 1000     1000      1632221 Jun 17 12:14 video_2026-06-17.mp4\r\n\
                       drwxr-xr-x    2 1000     1000         4096 Jun 17  2025 cache\n";

        // Baseline: We evaluate these listings at Jun 17, 2026, 15:00
        let files = parse_unix_listing(payload, 2026, CurrentDateTime { month: 6, day: 17, hour: 15, minute: 0 });

        assert_eq!(files.len(), 2);

        // Verify standard data payload properties
        let file = &files[0];
        assert_eq!(file.name, "video_2026-06-17.mp4");
        assert!(!file.is_dir);
        assert_eq!(file.size, 1632221);
        assert_eq!(file.year, 2026);
        assert_eq!(file.month, 6);
        assert_eq!(file.day, 17);
        assert_eq!(file.hour, 12);
        assert_eq!(file.minute, 14);
        assert!(file.year_is_inferred, "HH:MM-format entry must be flagged as inferred");

        // Verify standard directory node properties
        let dir = &files[1];
        assert_eq!(dir.name, "cache");
        assert!(dir.is_dir);
        assert_eq!(dir.size, 4096);
        assert_eq!(dir.year, 2025);
        assert_eq!(dir.month, 6);
        assert_eq!(dir.day, 17);
        assert_eq!(dir.hour, 0);
        assert_eq!(dir.minute, 0);
        assert!(
            !dir.year_is_inferred,
            "explicit-YYYY-format entry must not be flagged as inferred"
        );
    }

    #[test]
    fn test_multiple_internal_spaces_preserved_exactly() {
        // BUG-088: internal multi-space runs in the filename must round-trip exactly —
        // confirmed on real P1S hardware that collapsing them (the old `.join(" ")`
        // behavior) desyncs the reported name from the printer's actual on-disk name,
        // silently breaking delete_file/download_file for that file.
        let payload =
            "-rwxrwxrwx   1 root     root           12 Jan  1  2030  weird_spacing   name.3mf";
        let files = parse_unix_listing(payload, 2026, CurrentDateTime { month: 6, day: 17, hour: 12, minute: 0 });

        assert_eq!(files.len(), 1);
        let f = &files[0];
        assert_eq!(f.name, "weird_spacing   name.3mf");
        assert_eq!(f.size, 12);
        assert_eq!(f.year, 2030);
    }

    #[test]
    fn test_empty_listing() {
        let files = parse_unix_listing("", 2026, CurrentDateTime { month: 6, day: 17, hour: 15, minute: 0 });
        assert!(files.is_empty());
    }

    #[test]
    fn test_whitespace_only_listing() {
        let files = parse_unix_listing("   \n  \n\r\n", 2026, CurrentDateTime { month: 6, day: 17, hour: 15, minute: 0 });
        assert!(files.is_empty());
    }

    #[test]
    fn test_malformed_lines_skipped() {
        let payload = "not a valid listing line\n\
                       -rw-r--r--    1 1000     1000      1024 Jun 17 12:00 valid.3mf\n\
                       truncated\n";
        let files = parse_unix_listing(payload, 2026, CurrentDateTime { month: 6, day: 17, hour: 15, minute: 0 });
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "valid.3mf");
    }

    #[test]
    fn test_garbage_timestamp_rejected() {
        // BUG-061: day/hour/minute weren't range-validated (unlike month), so a malformed
        // "Jun 99 88:70 file.gcode" entry would previously reach FtpFile verbatim.
        let payload = "-rw-r--r--    1 1000     1000      1024 Jun 99 12:00 bad_day.gcode\n\
                       -rw-r--r--    1 1000     1000      1024 Jun 17 88:00 bad_hour.gcode\n\
                       -rw-r--r--    1 1000     1000      1024 Jun 17 12:70 bad_minute.gcode\n\
                       -rw-r--r--    1 1000     1000      1024 Jun 17 12:00 valid.gcode\n";
        let files = parse_unix_listing(payload, 2026, CurrentDateTime { month: 6, day: 17, hour: 15, minute: 0 });
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "valid.gcode");
    }

    #[test]
    fn test_temporal_rollover_boundary_heuristics() {
        // Today is January 2nd, 2026 (01:15).
        // We parse a file modified on December 31st containing a timestamp of 23:59.
        // It has no year representation, so standard logic yields December 31st, 2026.
        // Because that datetime resides in our system's relative future, rollover math must
        // automatically correct this to December 31st, 2025.
        let payload = "-rw-r--r--    1 1000     1000          100 Dec 31 23:59 print_job.gcode";
        let files = parse_unix_listing(payload, 2026, CurrentDateTime { month: 1, day: 2, hour: 1, minute: 15 });

        assert_eq!(files.len(), 1);
        let file = &files[0];
        assert_eq!(file.year, 2025);
        assert_eq!(file.month, 12);
        assert_eq!(file.day, 31);
        assert_eq!(file.hour, 23);
        assert_eq!(file.minute, 59);
    }
}
