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
    /// The parsed file or directory name, preserving single spaces between words.
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

/// Parses a line-separated UNIX directory listing payload returned by `LIST`.
///
/// **Whitespace-Insensitive Delimiting:**
/// Embedded systems typically insert arbitrary, variable-width spacing gaps to line up listings.
/// Rather than relying on rigid column indexes, this implementation tokenizes columns by splitting
/// on contiguous whitespace sequences, collecting the initial 8 protocol columns, and rebuilding
/// the rest as the filename.
///
/// **Temporal Rollover Mitigation:**
/// UNIX listing formats omit the modification year and provide a timestamp (HH:MM) if the file
/// was updated within the last six months. In this scenario, we default to the host system's
/// `current_year`. If comparing the parsed datetime markers against our system context reveals
/// that the parsed datetime is in the future, the file belongs to last year's calendar cycle
/// (e.g., parsing a December modification date in January). In this event, we decrement the
/// calculated year by 1.
pub fn parse_unix_listing(
    payload: &str,
    current_year: i32,
    current_month: u8,
    current_day: u8,
    current_hour: u8,
    current_minute: u8,
) -> Vec<FtpFile> {
    let mut files = Vec::new();

    for line in payload.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let mut tokens = trimmed.split_whitespace();

        // Standard UNIX listings contain exactly 9 base columns:
        // [0:Perms] [1:Links] [2:Owner] [3:Group] [4:Size] [5:Month] [6:Day] [7:TimeOrYear] [8+:Name]
        let perms = match tokens.next() {
            Some(p) => p,
            None => continue,
        };
        let _links = tokens.next();
        let _owner = tokens.next();
        let _group = tokens.next();

        let size = match tokens.next().and_then(|s| s.parse::<u64>().ok()) {
            Some(sz) => sz,
            None => continue,
        };
        let month_str = match tokens.next() {
            Some(m) => m,
            None => continue,
        };
        let day_str = match tokens.next() {
            Some(d) => d,
            None => continue,
        };
        let time_or_year = match tokens.next() {
            Some(t) => t,
            None => continue,
        };

        // Standardized UNIX specifications mandate that everything following the 8th
        // whitespace-delimited block constitutes the filename. Rebuilding via spacing
        // joins ensures we safely support file names containing space characters.
        let name_tokens = tokens.collect::<Vec<&str>>();
        if name_tokens.is_empty() {
            continue;
        }
        let name = name_tokens.join(" ");

        let is_dir = perms.starts_with('d');
        let month = match parse_month(month_str) {
            Some(m) => m,
            None => continue,
        };
        let day = match day_str.parse::<u8>().ok() {
            Some(d) => d,
            None => continue,
        };

        let mut hour = 0;
        let mut minute = 0;
        let mut year = current_year;

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
            year = time_or_year.parse::<i32>().ok().unwrap_or(current_year);
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
        });
    }

    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_unix_file_parsing() {
        let payload =
            "-rw-r--r--    1 1000     1000      1632221 Jun 17 12:14 video_2026-06-17.mp4\r\n\
                       drwxr-xr-x    2 1000     1000         4096 Jun 17  2025 cache\n";

        // Baseline: We evaluate these listings at Jun 17, 2026, 15:00
        let files = parse_unix_listing(payload, 2026, 6, 17, 15, 0);

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
    }

    #[test]
    fn test_weird_spacing_handling() {
        let payload =
            "-rwxrwxrwx   1 root     root           12 Jan  1  2030  weird_spacing   name.3mf";
        let files = parse_unix_listing(payload, 2026, 6, 17, 12, 0);

        assert_eq!(files.len(), 1);
        let f = &files[0];
        assert_eq!(f.name, "weird_spacing name.3mf");
        assert_eq!(f.size, 12);
        assert_eq!(f.year, 2030);
    }

    #[test]
    fn test_temporal_rollover_boundary_heuristics() {
        // Today is January 2nd, 2026 (01:15).
        // We parse a file modified on December 31st containing a timestamp of 23:59.
        // It has no year representation, so standard logic yields December 31st, 2026.
        // Because that datetime resides in our system's relative future, rollover math must
        // automatically correct this to December 31st, 2025.
        let payload = "-rw-r--r--    1 1000     1000          100 Dec 31 23:59 print_job.gcode";
        let files = parse_unix_listing(payload, 2026, 1, 2, 1, 15);

        assert_eq!(files.len(), 1);
        let file = &files[0];
        assert_eq!(file.year, 2025);
        assert_eq!(file.month, 12);
        assert_eq!(file.day, 31);
        assert_eq!(file.hour, 23);
        assert_eq!(file.minute, 59);
    }
}
