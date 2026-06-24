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
//! * **Fault Isolation**: Filters out non-error statuses (low 16-bit word < `0x4000`)
//!   and user action confirmation echoes (such as user-initiated cancellation events)
//!   to isolate genuine hardware failures from routine system state updates.

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::String;

pub(crate) const HMS_FAULT_THRESHOLD: u32 = 0x4000;
pub(crate) const HMS_CANCEL_ECHO_A: &str = "0300_400C";
pub(crate) const HMS_CANCEL_ECHO_B: &str = "0500_400E";

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
    /// Extracts the severity level from the second byte of the 32-bit `attr` value.
    ///
    /// Bit representation: `(attr >> 8) & 0x0F` [REF-DIAG-HMS].
    pub fn from_attr(attr: u32) -> Self {
        match (attr >> 8) & 0x0F {
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
    let attr_high = (attr >> 16) & 0xFFFF;
    let attr_low = attr & 0xFFFF;
    let code_high = (code >> 16) & 0xFFFF;
    let code_low = code & 0xFFFF;

    // Build the 16-character underscore-delimited format used on support channels
    let wiki_key = format!(
        "{:04X}_{:04X}_{:04X}_{:04X}",
        attr_high, attr_low, code_high, code_low
    );

    // Build local 8-character LCD format: High word of attr combined with low word of code
    let short_code = format!("{:04X}_{:04X}", attr_high, code_low);

    // Module ID resides on the fourth byte of the attr parameter: (attr >> 24) & 0xFF
    let module_id = ((attr >> 24) & 0xFF) as u8;
    let severity = HmsSeverity::from_attr(attr);

    // Under physical printer firmware architectures, low-word code indexes below 0x4000
    // represent transient progress alerts (such as axis homing states) rather than errors.
    let is_status_step = code_low < HMS_FAULT_THRESHOLD;

    // Cancellation echoes (e.g., 0300_400C) are raised as system confirmations when
    // a user aborts a print. These must not be flagged as actual errors.
    let is_cancel_echo = short_code == HMS_CANCEL_ECHO_A || short_code == HMS_CANCEL_ECHO_B;

    let is_genuine_fault = !is_status_step && !is_cancel_echo;

    DecodedHmsAlert {
        wiki_key,
        short_code,
        severity,
        module_id,
        is_genuine_fault,
    }
}

/// Fully decoded representation of the primary system `print_error` register.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DecodedPrintError {
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

    // Convert decimal representation to formatted 8-character hex equivalent
    let hex_val = format!("{:08X}", print_error);
    if hex_val.len() != 8 {
        return None;
    }

    let short_code = format!("{}_{}", &hex_val[0..4], &hex_val[4..8]);

    // Unpack mathematically to prevent overflow hazards during string parsing [REF-DIAG-HMS]
    let module_id = ((print_error >> 24) & 0xFF) as u8;
    let code_low = (print_error & 0xFFFF) as u16;

    let is_status_step = (code_low as u32) < HMS_FAULT_THRESHOLD;

    // Filter out standard cancellation status echoes
    let is_cancel_echo = short_code == HMS_CANCEL_ECHO_A || short_code == HMS_CANCEL_ECHO_B;

    let is_genuine_fault = !is_status_step && !is_cancel_echo;

    Some(DecodedPrintError {
        short_code,
        module_id,
        is_genuine_fault,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hms_alert_decoding() {
        // Mock attr and code: represents typical module failure
        // attr = 50331904 (0x03000100) -> attr_high: 0x0300, severity: Fatal (0x01), module: 0x03
        // code = 65543 (0x00010007)    -> code_low: 0x0007, code_high: 0x0001
        let attr: u32 = 50331904;
        let code: u32 = 65543;

        let decoded = decode_hms_alert(attr, code);

        assert_eq!(decoded.wiki_key, "0300_0100_0001_0007");
        assert_eq!(decoded.short_code, "0300_0007");
        assert_eq!(decoded.module_id, 0x03);
        assert_eq!(decoded.severity, HmsSeverity::Fatal);

        // low word is 0x0007 which is < 0x4000 -> status step, not genuine fault
        assert!(!decoded.is_genuine_fault);
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
        // Register value of 0 means no active error condition
        assert!(decode_print_error(0).is_none());
    }
}
