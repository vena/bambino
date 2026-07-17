#![cfg(feature = "cli")]

//! # CLI-Local Error Type
//!
//! Wraps failures local to the CLI binary itself (bad arguments, local file I/O, terminal
//! setup, stdin reads) separately from `bambino::Error`, whose variants describe wire-protocol
//! and transport failures against the printer. Reusing e.g. `Error::ProtocolViolation` for "you
//! typo'd a test name" or "couldn't write the output file" was misleading — a reader can no
//! longer tell "the printer's server sent malformed JSON" apart from "the local file doesn't
//! exist" (BUG-181).
//!
//! Named `CliError`, not bare `Error`, because CLI code routinely needs both this type and
//! `bambino::Error` in scope at once (e.g. the `Library` variant below, or any `?` converting
//! one into the other) — naming both `Error` would force aliasing at every such site.
//!
//! Sized for `Display` quality and `?`-ergonomics, not a full taxonomy: nothing downstream ever
//! matches on a specific variant, the only consumer is `main()`'s final `eprintln!` +
//! `process::exit(1)`.

use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum CliError {
    /// Local file/terminal I/O failure: reading a file to upload, writing an output file,
    /// initializing raw terminal mode, reading a confirmation from stdin.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Caller-supplied argument failed local validation (bad ip/serial/access_code, unknown
    /// test name, wrong model for the requested command) — never reaches the printer.
    #[error("{0}")]
    InvalidArgs(String),

    /// A real `bambino` library error (wire protocol, transport, TLS via `PrinterClient`)
    /// passed through unchanged.
    #[error(transparent)]
    Library(#[from] bambino::Error),

    /// Catch-all for formatted diagnostic failures that are none of the above — mainly
    /// `inspect-cert`/`verify-tls`'s raw TLS config/handshake errors (`rustls::Error`,
    /// `rustls_pki_types` PEM/SNI errors), which aren't `std::io::Error` and aren't
    /// `bambino::Error` since those tools deliberately bypass `PrinterClient`.
    #[error("{0}")]
    Other(String),
}
