//! # Printer Identity
//!
//! [`PrinterIdentity`] bundles the LAN address, serial number, and access code every
//! "connect to protocol X" entry point in this crate needs to dial and authenticate
//! against a specific printer, instead of passing them as three adjacent same-typed
//! `&str` parameters a caller could transpose without a compile error.
#[cfg(not(feature = "std"))]
use alloc::string::String;

use crate::models::{PrinterModel, resolve_model};

/// Address, serial number, and access code identifying one printer on the LAN.
///
/// [`Debug`] is implemented manually to redact `access_code`; see the impl below.
#[derive(Clone, PartialEq, Eq)]
pub struct PrinterIdentity {
    /// LAN IP address or hostname of the printer.
    pub ip: String,
    /// Printer's serial number, used for TLS SNI and MQTT topic scoping.
    pub serial: String,
    /// Printer's local network access code (found in its LAN-only settings screen).
    pub access_code: String,
    /// Printer model, used for quirks dispatch. Derivable from `serial` via
    /// [`resolve_model`]; see [`PrinterIdentity::new`] for the common case.
    pub model: PrinterModel,
}

impl PrinterIdentity {
    /// Builds an identity, deriving `model` from `serial` via [`resolve_model`].
    ///
    /// For callers who need a specific `model` regardless of what the serial
    /// prefix implies, construct the struct literal directly instead.
    pub fn new(ip: impl Into<String>, serial: impl Into<String>, access_code: impl Into<String>) -> Self {
        let serial = serial.into();
        let model = resolve_model(&serial, None);
        Self { ip: ip.into(), serial, access_code: access_code.into(), model }
    }
}

/// Redacts `access_code`, which is a network credential and must never reach a log line
/// through an incidental `{:?}` on the whole identity. Every other field prints verbatim.
impl core::fmt::Debug for PrinterIdentity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PrinterIdentity")
            .field("ip", &self.ip)
            .field("serial", &self.serial)
            .field("access_code", &"<redacted>")
            .field("model", &self.model)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(feature = "std"))]
    use alloc::format;

    #[test]
    fn debug_redacts_access_code_but_keeps_the_other_fields() {
        let secret = "s3cr3tcode";
        let identity = PrinterIdentity::new("192.168.1.50", "00M00A000000000", secret);
        let rendered = format!("{:?}", identity);

        assert!(!rendered.contains(secret), "access code leaked into Debug: {rendered}");
        assert!(rendered.contains("<redacted>"));
        assert!(rendered.contains("192.168.1.50"));
        assert!(rendered.contains("00M00A000000000"));
    }
}
