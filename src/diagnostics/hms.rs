//! # HMS Diagnostic Telemetry Parsing & Unpacking Engine
//!
//! Provides mathematical decoders to unpack physical printer hardware fault codes,
//! warning levels, and operational alerts from telemetry status streams [REF-DIAG-HMS].
//!
//! This module parses:
//! 1. The 32-bit `print_error` register into short-code formats.
//! 2. The `hms` array containing active telemetry blocks (`attr` and `code`) into
//!    both 16-character Wiki slugs and 8-character local short-codes.
//!
//! ## Technical Specifications
//! * **Fault Isolation**: Filters out non-error statuses and user action confirmation
//!   echoes (such as user-initiated cancellation events) to isolate genuine hardware
//!   failures from routine system state updates. The threshold differs per path: the
//!   low 16-bit word (< `0x4000`) gates the `print_error` register, while `hms[]`
//!   entries compare the full 32-bit `code` — a low-word-only check there would
//!   misclassify nearly every real fault.

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::String;

pub(crate) const HMS_FAULT_THRESHOLD: u32 = 0x4000;
/// `(high word, low word)` of the two user-cancellation echoes, `0300_400C` and `0500_400E`.
const HMS_CANCEL_ECHOES: [(u32, u32); 2] = [(0x0300, 0x400C), (0x0500, 0x400E)];

/// The `MMMM_CCCC` short code for a high and low word.
fn short_code(hi: u32, lo: u32) -> String {
    format!("{hi:04X}_{lo:04X}")
}

/// Whether `hi`/`lo` name a user-cancellation echo rather than a fault.
///
/// Raised as a confirmation when a user aborts a print; must not be flagged as an error.
fn is_cancel_echo(hi: u32, lo: u32) -> bool {
    HMS_CANCEL_ECHOES.contains(&(hi, lo))
}

/// The source module id, in the top byte of `attr` or of the `print_error` register.
fn module_id(word: u32) -> u8 {
    (word >> 24) as u8
}

/// Numerical classification of the severity level of an HMS diagnostic alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HmsSeverity {
    /// Severe operational failure requiring immediate print execution halt.
    Fatal = 1,
    /// High-priority alert requiring user intervention before execution resumes.
    Serious = 2,
    /// Non-blocking warning indicating minor runtime or environment issues.
    Warning = 3,
    /// Routine information prompt or system state confirmation event.
    Info = 4,
    /// Fallback classification for unrecognized alert bounds.
    Unknown,
}

impl HmsSeverity {
    /// Extracts the severity level from the high 16 bits of the 32-bit `code` value.
    ///
    /// Bit representation: `(code >> 16) & 0xFFFF` [REF-DIAG-HMS]. Confirmed against
    /// BambuStudio's `parse_hms_info` (`DevHMS.cpp:7-25`, identical in OrcaSlicer) and
    /// pybambu's `get_HMS_severity`, both of which derive severity from `code >> 16`.
    pub fn from_code(code: u32) -> Self {
        match (code >> 16) & 0xFFFF {
            1 => HmsSeverity::Fatal,
            2 => HmsSeverity::Serious,
            3 => HmsSeverity::Warning,
            4 => HmsSeverity::Info,
            _ => HmsSeverity::Unknown,
        }
    }
}

/// Fully decoded representation of an active diagnostic entry from the `hms` telemetry array.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DecodedHmsAlert {
    /// The standard 16-character wiki troubleshooting key (`MMMM_MMMM_CCCC_CCCC`).
    pub wiki_key: String,
    /// The local 8-character short-code format displayed on the physical LCD panel (`MMMM_CCCC`).
    pub short_code: String,
    /// Decoded physical severity rating of the active system alert.
    pub severity: HmsSeverity,
    /// Unique identifier of the source hardware module executing under failure.
    pub module_id: u8,
    /// Flags whether this alert represents a genuine hardware fault rather than a progress or state step.
    pub is_genuine_fault: bool,
}

/// Decodes an active entry from the `hms` telemetry array [REF-DIAG-HMS].
///
/// Unpacks the 32-bit `attr` and `code` parameters to reconstruct standard Wiki-slug
/// tracking variables, extract severity ratings, isolate module indexes, and filter
/// transient state updates.
pub fn decode_hms_alert(attr: u32, code: u32) -> DecodedHmsAlert {
    let attr_high = attr >> 16;
    let attr_low = attr & 0xFFFF;
    let code_high = code >> 16;
    let code_low = code & 0xFFFF;

    // Compare the full 32-bit code (not just its low 16 bits) against the fault
    // threshold — confirmed against BambuStudio's bundled `resources/hms/hms_en_093.json`
    // fault catalog (4591/4592 genuine hms[] faults have code_low < 0x4000, so a code_low-only
    // check misclassifies nearly every real fault as a non-fault status step).
    let is_status_step = code < HMS_FAULT_THRESHOLD;

    DecodedHmsAlert {
        // 16-character underscore-delimited format used on support channels.
        wiki_key: format!("{attr_high:04X}_{attr_low:04X}_{code_high:04X}_{code_low:04X}"),
        // Local LCD format: high word of attr with low word of code.
        short_code: short_code(attr_high, code_low),
        severity: HmsSeverity::from_code(code),
        module_id: module_id(attr),
        is_genuine_fault: !is_status_step && !is_cancel_echo(attr_high, code_low),
    }
}

/// Fully decoded representation of the primary system `print_error` register.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DecodedPrintError {
    /// The raw `print_error` register value this was decoded from — what the error-dialog
    /// commands (`PrinterClient::ignore_error_and_resume` and friends) take.
    pub code: u32,
    /// The local 8-character short-code format displayed on the physical LCD panel (`MMMM_CCCC`).
    pub short_code: String,
    /// Unpacked system module code where the primary print execution halted.
    pub module_id: u8,
    /// Flags whether this error register holds a genuine hardware failure block.
    pub is_genuine_fault: bool,
}

/// Normalizes the 32-bit decimal `print_error` register into its active diagnostic short-code.
///
/// Under the over-the-wire telemetry channel, the `print_error` status is passed as a packed
/// decimal integer. Reconstructing this to LCD standards requires hex-string conversion
/// and formatting with an underscore separator [REF-DIAG-HMS].
pub fn decode_print_error(print_error: u32) -> Option<DecodedPrintError> {
    if print_error == 0 {
        return None;
    }

    let hi = print_error >> 16;
    let lo = print_error & 0xFFFF;
    // Only the low word gates the register, unlike `hms[]` entries — see the module docs.
    let is_status_step = lo < HMS_FAULT_THRESHOLD;

    Some(DecodedPrintError {
        code: print_error,
        short_code: short_code(hi, lo),
        module_id: module_id(print_error),
        is_genuine_fault: !is_status_step && !is_cancel_echo(hi, lo),
    })
}

impl From<&DecodedPrintError> for u32 {
    fn from(error: &DecodedPrintError) -> u32 {
        error.code
    }
}

impl From<DecodedPrintError> for u32 {
    fn from(error: DecodedPrintError) -> u32 {
        error.code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hms_alert_decoding() {
        // Mock attr and code: represents typical module failure
        // attr = 50331904 (0x03000100) -> attr_high: 0x0300, module: 0x03
        // code = 65543 (0x00010007)    -> code_low: 0x0007, code_high: 0x0001, severity: Fatal (0x0001)
        let attr: u32 = 50331904;
        let code: u32 = 65543;

        let decoded = decode_hms_alert(attr, code);

        assert_eq!(decoded.wiki_key, "0300_0100_0001_0007");
        assert_eq!(decoded.short_code, "0300_0007");
        assert_eq!(decoded.module_id, 0x03);
        assert_eq!(decoded.severity, HmsSeverity::Fatal);

        // is_status_step compares the full code (65543), which is >= 0x4000,
        // so this is a genuine fault even though its low word (0x0007) alone is < 0x4000.
        assert!(decoded.is_genuine_fault);
    }

    #[test]
    fn test_genuine_hardware_fault_detection() {
        // Simulated structural error code where code_low is >= 0x4000
        // attr = 0x05000100 -> module: 0x05, attr_high: 0x0500
        // code = 0x0001400C -> code_low: 0x400C
        let attr: u32 = 0x05000100;
        let code: u32 = 0x0001400C;

        let decoded = decode_hms_alert(attr, code);
        assert_eq!(decoded.short_code, "0500_400C");
        assert!(decoded.is_genuine_fault);
    }

    #[test]
    fn test_user_cancellation_echo_exclusion() {
        // Simulated cancellation echo code: "0500_400E"
        // Even though code_low (0x400E) is >= 0x4000, it is mapped as a status confirmation
        let attr: u32 = 0x05000100;
        let code: u32 = 0x0001400E;

        let decoded = decode_hms_alert(attr, code);
        assert_eq!(decoded.short_code, "0500_400E");
        assert!(!decoded.is_genuine_fault);
    }

    #[test]
    fn test_print_error_register_decoding() {
        // print_error = 83902476 decimal -> 0x0500400C
        let print_error: u32 = 83902476;
        let decoded = decode_print_error(print_error).unwrap();

        assert_eq!(decoded.short_code, "0500_400C");
        assert_eq!(decoded.module_id, 0x05);
        assert!(decoded.is_genuine_fault);
    }

    #[test]
    fn test_print_error_zero_value() {
        assert!(decode_print_error(0).is_none());
    }

    #[test]
    fn test_all_severity_levels() {
        for (raw, expected) in [
            (1u32, HmsSeverity::Fatal),
            (2, HmsSeverity::Serious),
            (3, HmsSeverity::Warning),
            (4, HmsSeverity::Info),
            (0, HmsSeverity::Unknown),
            (5, HmsSeverity::Unknown),
            (0x0F, HmsSeverity::Unknown),
        ] {
            let code = raw << 16;
            assert_eq!(HmsSeverity::from_code(code), expected, "raw severity {raw}");
        }
    }

    #[test]
    fn test_cancel_echo_a_exclusion() {
        // Cancel echo A: short_code "0300_400C"
        // attr_high = 0x0300, code_low = 0x400C
        let attr: u32 = 0x03000200;
        let code: u32 = 0x0001400C;

        let decoded = decode_hms_alert(attr, code);
        assert_eq!(decoded.short_code, "0300_400C");
        assert!(!decoded.is_genuine_fault);
    }

    #[test]
    fn test_print_error_cancel_echo_a() {
        // print_error = 0x0300400C -> short_code "0300_400C"
        let decoded = decode_print_error(0x0300400C).unwrap();
        assert_eq!(decoded.short_code, "0300_400C");
        assert!(!decoded.is_genuine_fault);
    }

    #[test]
    fn test_print_error_cancel_echo_b() {
        // print_error = 0x0500400E -> short_code "0500_400E"
        let decoded = decode_print_error(0x0500400E).unwrap();
        assert_eq!(decoded.short_code, "0500_400E");
        assert!(!decoded.is_genuine_fault);
    }

    #[test]
    fn test_print_error_status_step_not_genuine() {
        // code_low = 0x0007 < 0x4000 -> status step
        let decoded = decode_print_error(0x03000007).unwrap();
        assert_eq!(decoded.short_code, "0300_0007");
        assert!(!decoded.is_genuine_fault);
    }

    #[test]
    fn test_real_x2d_hms_entry() {
        // From pybambu MOCK-X2D.json: attr=83887616 code=131184
        let decoded = decode_hms_alert(83887616, 131184);
        assert_eq!(decoded.wiki_key, "0500_0600_0002_0070");
        assert_eq!(decoded.short_code, "0500_0070");
        assert_eq!(decoded.module_id, 0x05);
        assert_eq!(decoded.severity, HmsSeverity::Serious);
        assert!(decoded.is_genuine_fault);
    }

    #[test]
    fn test_real_misc_hms_entry() {
        // From pybambu MOCK-MISC.json: attr=201327360 code=196615
        let decoded = decode_hms_alert(201327360, 196615);
        assert_eq!(decoded.wiki_key, "0C00_0300_0003_0007");
        assert_eq!(decoded.short_code, "0C00_0007");
        assert_eq!(decoded.module_id, 0x0C);
        assert_eq!(decoded.severity, HmsSeverity::Warning);
        assert!(decoded.is_genuine_fault);
    }
}
