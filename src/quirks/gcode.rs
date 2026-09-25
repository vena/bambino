//! Word-level G-code scanning behind [`ModelQuirks::validate_gcode`](super::ModelQuirks::validate_gcode).
//!
//! Bambu's firmware G-code parser is undocumented, so every ambiguity here resolves toward
//! *rejecting*: statements split on a bare `\r` as well as `\n`, a `(…)` comment resumes
//! executable text after its `)`, the command number tolerates a space and leading zeros
//! (`G 28`, `G028`), and a temperature argument that isn't a plain decimal (`S3e2`, `S0x1F`,
//! an empty `S`) is refused rather than guessed at. The only text skipped is text provably not
//! executable: comments and the operand of a leading `M117` display message. `no_std` rules
//! out `regex`, hence the manual byte scan.

#[cfg(not(feature = "std"))]
use alloc::{format, string::String};

use crate::error::Error;

/// Per-model temperature ceilings a statement's heater arguments are checked against.
pub(crate) struct TempLimits {
    pub(crate) nozzle_max: u16,
    pub(crate) bed_max: u16,
    /// `None` when the model has no active chamber heater — any `M141`/`M191` is then refused.
    pub(crate) chamber_max: Option<u16>,
}

/// Returns `Err` for the first statement in `gcode` that is unsafe under the given limits.
///
/// `reject_partial_homing` enables the bed-on-Z axis-constrained `G28` check; `temps`, when
/// `Some`, enables the heater-argument checks.
pub(crate) fn validate(
    gcode: &str,
    reject_partial_homing: bool,
    temps: Option<&TempLimits>,
) -> Result<(), Error> {
    for line in gcode.split(['\n', '\r']) {
        scan_statement(&executable(line), reject_partial_homing, temps)?;
    }
    Ok(())
}

/// Returns the line with every comment removed.
///
/// `;` ends the executable text. `(` opens a comment that ends at the next `)`, after which
/// scanning resumes; an unclosed `(` runs to the end of the line.
fn executable(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(pos) = rest.find([';', '(']) {
        out.push_str(&rest[..pos]);
        if rest.as_bytes()[pos] == b';' {
            return out;
        }
        match rest[pos..].find(')') {
            Some(close) => {
                // Keep the words on either side of the comment apart.
                out.push(' ');
                rest = &rest[pos + close + 1..];
            }
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// One address word: a letter and the numeric text after it (`G28`, `S200`, a bare `Z`).
struct Word<'a> {
    /// ASCII-uppercased address letter.
    letter: u8,
    /// Numeric characters after the letter (optional spaces skipped); empty for a bare letter.
    value: &'a str,
    /// The value is immediately followed by `e`/`E`/`x`/`X`, which a `strtod`-style parser
    /// could read as an exponent or hex prefix — the number's meaning is ambiguous.
    ambiguous: bool,
}

/// Iterates the address words of one comment-free statement.
struct Words<'a> {
    code: &'a str,
    pos: usize,
}

impl<'a> Iterator for Words<'a> {
    type Item = Word<'a>;

    fn next(&mut self) -> Option<Word<'a>> {
        let bytes = self.code.as_bytes();
        while self.pos < bytes.len() && !bytes[self.pos].is_ascii_alphabetic() {
            self.pos += 1;
        }
        if self.pos >= bytes.len() {
            return None;
        }
        let letter = bytes[self.pos].to_ascii_uppercase();
        self.pos += 1;
        while self.pos < bytes.len() && matches!(bytes[self.pos], b' ' | b'\t') {
            self.pos += 1;
        }
        let start = self.pos;
        while self.pos < bytes.len() && matches!(bytes[self.pos], b'0'..=b'9' | b'+' | b'-' | b'.')
        {
            self.pos += 1;
        }
        let value = &self.code[start..self.pos];
        let ambiguous = !value.is_empty()
            && self.pos < bytes.len()
            && matches!(bytes[self.pos], b'e' | b'E' | b'x' | b'X');
        Some(Word {
            letter,
            value,
            ambiguous,
        })
    }
}

/// Returns the integer command number of a `G`/`M` word (`"028"` → 28, `"28.1"` → 28).
fn command_number(value: &str) -> Option<u32> {
    value.split('.').next()?.parse().ok()
}

fn scan_statement(
    code: &str,
    reject_partial_homing: bool,
    temps: Option<&TempLimits>,
) -> Result<(), Error> {
    let mut command: Option<(u8, u32)> = None;
    let mut first_command = true;
    let mut saw_g28 = false;

    for word in (Words { code, pos: 0 }) {
        if matches!(word.letter, b'G' | b'M') {
            // A number-less `G`/`M` is a marker, not a new command: BambuStudio emits
            // `M104 M S<temp>` for layer-change temperatures, so the `S` still belongs to M104.
            if word.value.is_empty() {
                first_command = false;
                continue;
            }
            command = command_number(word.value).map(|n| (word.letter, n));
            // `M117` consumes the rest of its statement as LCD text — but only as the
            // statement's first command; anywhere else the text after it is still scanned.
            if first_command && command == Some((b'M', 117)) {
                return Ok(());
            }
            first_command = false;
            match command {
                Some((b'G', 28)) => saw_g28 = true,
                Some((b'M', n @ (141 | 191))) if temps.is_some_and(|t| t.chamber_max.is_none()) => {
                    return Err(Error::ModelMismatch(
                        format!("M{n}: active chamber heater not available on this model").into(),
                    ));
                }
                _ => {}
            }
            continue;
        }

        // Sticky across later commands on the same statement: an axis letter anywhere after a
        // G28 is treated as constraining it.
        if reject_partial_homing && saw_g28 && matches!(word.letter, b'X' | b'Y' | b'Z') {
            return Err(Error::ModelMismatch(
                "partial-axis homing unsafe on bed-on-Z model".into(),
            ));
        }

        let (Some(temps), Some((b'M', n))) = (temps, command) else {
            continue;
        };
        let (max, label) = match (n, word.letter) {
            (104 | 109, b'S' | b'R' | b'B') => (temps.nozzle_max, "nozzle"),
            (140 | 190, b'S' | b'R') => (temps.bed_max, "bed"),
            (141 | 191, b'S' | b'R') => match temps.chamber_max {
                Some(max) => (max, "chamber"),
                None => continue,
            },
            _ => continue,
        };
        check_temp(n, &word, max, label)?;
    }
    Ok(())
}

fn check_temp(command: u32, word: &Word<'_>, max: u16, label: &str) -> Result<(), Error> {
    let letter = char::from(word.letter);
    let parsed = if word.ambiguous {
        None
    } else {
        word.value
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
    };
    let Some(value) = parsed else {
        return Err(Error::InvalidArgument(
            format!(
                "M{command} {letter}{}: {label} temperature is not a plain decimal number",
                word.value
            )
            .into(),
        ));
    };
    if value > f32::from(max) {
        return Err(Error::ModelMismatch(
            format!(
                "M{command} {letter}{}: exceeds this model's {label} maximum of {max}°C",
                word.value
            )
            .into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: TempLimits = TempLimits {
        nozzle_max: 300,
        bed_max: 80,
        chamber_max: None,
    };

    const HEATED: TempLimits = TempLimits {
        nozzle_max: 350,
        bed_max: 120,
        chamber_max: Some(65),
    };

    fn homing(gcode: &str) -> bool {
        validate(gcode, true, None).is_err()
    }

    #[test]
    fn test_homing_spellings_the_old_prefix_match_missed() {
        // #354: leading zeros, a space inside the command, text after a closed `(…)`, and a
        // bare `\r` statement break.
        assert!(homing("G028 Z"));
        assert!(homing("G 28 Z"));
        assert!(homing("G28 (note) Z"));
        assert!(homing("G28 ; c\rG28 Z"));
        assert!(homing("M117 hi\rG28 Z"));
    }

    #[test]
    fn test_homing_non_executable_text_stays_safe() {
        assert!(!homing("G28"));
        assert!(!homing("G28 ; home XYZ"));
        assert!(!homing("G28 (home XYZ)"));
        assert!(!homing("; G28 Z"));
        assert!(!homing("M117 G28 Z"));
        assert!(!homing("G280 Z"));
        assert!(!homing("G1 Z10"));
        // M117 only swallows the rest of the statement when it is the first command.
        assert!(homing("G28 M117 Z"));
        assert!(homing("N10 G28 Z*55"));
    }

    #[test]
    fn test_temperature_over_limit_rejected() {
        assert!(validate("M140 S200", false, Some(&LIMITS)).is_err());
        assert!(validate("M190 R81", false, Some(&LIMITS)).is_err());
        assert!(validate("M104 S500", false, Some(&LIMITS)).is_err());
        assert!(validate("M109 T1 S301", false, Some(&LIMITS)).is_err());
        assert!(validate("M104 B400", false, Some(&LIMITS)).is_err());
        assert!(validate("M141 S66", false, Some(&HEATED)).is_err());
        assert!(validate("G91\nM104S999", false, Some(&LIMITS)).is_err());
    }

    #[test]
    fn test_temperature_within_limit_accepted() {
        assert!(validate("M140 S80", false, Some(&LIMITS)).is_ok());
        assert!(validate("M104 T0 S220\nM109 S220", false, Some(&LIMITS)).is_ok());
        assert!(validate("M141 S65", false, Some(&HEATED)).is_ok());
        assert!(validate("M140 S100 ; ceiling is 120", false, Some(&HEATED)).is_ok());
        assert!(validate("M106 P1 S255", false, Some(&LIMITS)).is_ok());
        assert!(validate("M1400 S999", false, Some(&LIMITS)).is_ok());
    }

    #[test]
    fn test_bare_command_marker_keeps_the_temperature_check() {
        assert!(validate("M104 M S999", false, Some(&LIMITS)).is_err());
        assert!(validate("M140 G S200", false, Some(&LIMITS)).is_err());
        assert!(validate("M104 M S220", false, Some(&LIMITS)).is_ok());
        // A leading bare marker still counts as the first command, so M117 can't swallow the rest.
        assert!(homing("M M117 G28 Z"));
    }

    #[test]
    fn test_ambiguous_temperature_rejected() {
        assert!(matches!(
            validate("M104 S3e2", false, Some(&LIMITS)),
            Err(Error::InvalidArgument(_))
        ));
        assert!(validate("M140 S0x1F", false, Some(&LIMITS)).is_err());
        assert!(validate("M140 Sinf", false, Some(&LIMITS)).is_err());
    }

    #[test]
    fn test_chamber_command_rejected_without_heater() {
        assert!(validate("M141 S0", false, Some(&LIMITS)).is_err());
        assert!(validate("M191 S40", false, Some(&LIMITS)).is_err());
    }

    #[test]
    fn test_temperature_checks_off_without_limits() {
        assert!(validate("M140 S200\nM141 S99", true, None).is_ok());
    }
}
