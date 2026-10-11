//! # Printer Identity
//!
//! [`PrinterIdentity`] bundles the LAN address, serial number, and access code every
//! "connect to protocol X" entry point in this crate needs to dial and authenticate
//! against a specific printer, instead of passing them as three adjacent same-typed
//! `&str` parameters a caller could transpose without a compile error.
#[cfg(not(feature = "std"))]
use alloc::string::String;

use crate::error::Error;
use crate::models::{PrinterModel, resolve_model};

/// Length of every printer-issued LAN access code.
///
/// BambuStudio's connect-by-IP dialog (`InputIpAddressDialog::on_text` in
/// `src/slic3r/GUI/ReleaseNote.cpp`) enables its OK button only for exactly this many ASCII
/// letters or digits.
pub const ACCESS_CODE_LEN: usize = 8;

/// The fixed username every Bambu LAN protocol logs in with (MQTT, FTPS, the binary camera); the access code is the password.
pub(crate) const LAN_USERNAME: &str = "bblp";

/// Longest serial number accepted. Current Bambu serials are 15 characters.
pub const SERIAL_MAX_LEN: usize = 20;

/// Checks that `access_code` is exactly [`ACCESS_CODE_LEN`] ASCII letters or digits.
///
/// Printer-issued LAN access codes are 8 case-sensitive alphanumerics, so a rejection almost
/// always means a copy-paste mistake (whitespace, a trailing newline, a truncated code). A
/// wrong-length code would otherwise surface only at connect, as an MQTT refusal a powered-off
/// printer also produces. The alphanumeric rule is also what keeps the code safe to
/// interpolate into an RTSPS URL's userinfo.
pub fn validate_access_code(access_code: &str) -> Result<(), Error> {
    if access_code.len() != ACCESS_CODE_LEN
        || !access_code.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return Err(Error::InvalidArgument(
            "access code must be exactly 8 ASCII letters or digits".into(),
        ));
    }
    Ok(())
}

/// Checks that `serial` is 1 to [`SERIAL_MAX_LEN`] ASCII letters or digits.
///
/// The serial becomes an MQTT topic segment and the TLS SNI name, so anything else would
/// reach the wire malformed.
pub fn validate_serial(serial: &str) -> Result<(), Error> {
    if serial.is_empty()
        || serial.len() > SERIAL_MAX_LEN
        || !serial.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return Err(Error::InvalidArgument(
            "serial must be 1-20 ASCII letters or digits".into(),
        ));
    }
    Ok(())
}

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
    /// To use a specific model regardless of what the serial prefix implies, chain
    /// [`with_model`](Self::with_model); for a discovered printer, use
    /// [`SsdpDevice::into_identity`](crate::discovery::SsdpDevice::into_identity), which keeps the
    /// model discovery resolved from its headers.
    pub fn new(
        ip: impl Into<String>,
        serial: impl Into<String>,
        access_code: impl Into<String>,
    ) -> Self {
        let serial = serial.into();
        let model = resolve_model(&serial, None);
        Self {
            ip: ip.into(),
            serial,
            access_code: access_code.into(),
            model,
        }
    }

    /// Replaces the model `new()` derived from the serial.
    #[must_use]
    pub fn with_model(mut self, model: PrinterModel) -> Self {
        self.model = model;
        self
    }

    /// Like [`PrinterIdentity::new`], but rejects a malformed serial or access code up front.
    pub fn try_new(
        ip: impl Into<String>,
        serial: impl Into<String>,
        access_code: impl Into<String>,
    ) -> Result<Self, Error> {
        let identity = Self::new(ip, serial, access_code);
        identity.validate()?;
        Ok(identity)
    }

    /// Checks the serial and access code with [`validate_serial`] and [`validate_access_code`].
    ///
    /// `ip` is not checked: it may be a hostname, which only the dial can resolve.
    pub fn validate(&self) -> Result<(), Error> {
        validate_serial(&self.serial)?;
        validate_access_code(&self.access_code)
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

        assert!(
            !rendered.contains(secret),
            "access code leaked into Debug: {rendered}"
        );
        assert!(rendered.contains("<redacted>"));
        assert!(rendered.contains("192.168.1.50"));
        assert!(rendered.contains("00M00A000000000"));
    }

    #[test]
    fn access_code_must_be_exactly_eight_alphanumerics() {
        assert!(validate_access_code("aB3dE6g8").is_ok());
        for code in [
            "",
            "aB3dE6g",
            "aB3dE6g89",
            "aB3dE6g8\n",
            " aB3dE6g",
            "aB3d-6g8",
            "aB3dé6g",
        ] {
            assert!(
                matches!(validate_access_code(code), Err(Error::InvalidArgument(_))),
                "accepted {code:?}"
            );
        }
    }
}
