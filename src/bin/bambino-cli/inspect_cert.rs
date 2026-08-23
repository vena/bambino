#![cfg(feature = "cli")]

//! Diagnostic-only command that captures the raw TLS certificate chain a
//! printer presents so it can be inspected for a Subject Alternative Name
//! (SAN) — every cert sample in `certs/` is a CA/intermediate cert, never an
//! actual per-printer leaf cert, so this is the only way to confirm what a
//! real printer sends. See `.claude/rules/tls-identity-sni.md` for the
//! resulting identity-verification invariant. Not part of the library's
//! connection path; does not touch any `PrinterClient` code.
//!
//! Runs through the library's own [`TokioTlsConnector`] and reads the chain back with
//! [`TlsConnector::peer_chain_der`], rather than driving `tokio_rustls` with a bespoke
//! capturing verifier as it used to. That makes this command an exercise of the exact code
//! path a consumer pinning a certificate would use, instead of a parallel implementation that
//! could drift from it — and it costs nothing, because `build_unsafe_client_config` already
//! builds the identical config (ring provider, `DEFAULT_VERSIONS`, `NoCertificateVerification`
//! advertising the same twelve signature schemes) that the local verifier was duplicating.
//!
//! The **whole** chain is captured, not just the leaf, because whether a printer sends its
//! issuing CA decides what pinning a consumer can build on top of `peer_chain_der`: if the
//! issuer is present it can be captured once and passed back through `with_certs(..)` for
//! genuine stack-enforced verification on every later connection; if only the leaf is sent, a
//! consumer is stuck comparing a leaf fingerprint with verification disabled.
//!
//! **Answered on hardware (P1S, issue #142): the printer sends two certificates** — the
//! `CN=<serial>` leaf (no extensions at all, i.e. X.509 v1, which is why
//! `CnFallbackServerVerifier` exists) followed by the self-signed `CN=BBL CA` root itself,
//! `CA:TRUE`. The captured root is byte-identical to the known BBL CA root. So the good case
//! holds: an anchor *can* be captured at first contact. Re-run this against any new model
//! rather than assuming it generalizes.

use std::path::{Path, PathBuf};

use bambino::io::tokio::{TokioRawStreamFactory, TokioTlsConnector, build_unsafe_client_config};
use bambino::io::{RawStreamFactory, TlsConnector};

use crate::error::CliError;

/// Builds the on-disk path for chain position `index`, counting the leaf as 0.
///
/// The leaf keeps `output` verbatim so an existing invocation writes exactly the file it always
/// did; each additional certificate gets `.chain<N>` spliced in before the extension
/// (`printer_leaf_cert.der` → `printer_leaf_cert.chain1.der`), which keeps the `.der` suffix
/// intact so `openssl -inform DER` and shell globs still work on the whole set.
fn chain_member_path(output: &str, index: usize) -> PathBuf {
    if index == 0 {
        return PathBuf::from(output);
    }

    let path = Path::new(output);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let name = match path.extension() {
        Some(ext) => format!("{stem}.chain{index}.{}", ext.to_string_lossy()),
        None => format!("{stem}.chain{index}"),
    };

    path.with_file_name(name)
}

/// Connects to `ip:port`, completes a TLS handshake sending `serial` as the SNI value
/// (matching what the library's own connection path sends), and writes every certificate the
/// printer presented as raw DER — the leaf to `output`, any further chain members to the
/// `.chain<N>` paths `chain_member_path` derives. No FTPS/MQTT protocol traffic is exchanged
/// beyond the handshake itself — the connection is dropped as soon as the chain is read off it.
pub async fn run(ip: &str, serial: &str, port: u16, output: &str) -> Result<(), CliError> {
    let addr = format!("{ip}:{port}");

    let raw_stream = TokioRawStreamFactory.dial(ip, port).await.map_err(|e| {
        CliError::Network(format!(
            "TCP connect to {addr} failed: {}",
            bambino::Error::from(e)
        ))
    })?;

    let connector = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(
        build_unsafe_client_config(),
    ));

    let tls_stream = connector.connect(serial, raw_stream).await.map_err(|e| {
        CliError::Network(format!(
            "TLS handshake with {addr} (SNI={serial}) failed: {}",
            bambino::Error::from(e)
        ))
    })?;

    // Must be read before the stream is dropped: the chain is owned by the live session.
    let chain = connector.peer_chain_der(&tls_stream).ok_or_else(|| {
        CliError::Other(
            "handshake succeeded but the connector reported no peer chain — on tokio this \
             should not happen, since rustls retains the peer certificates"
                .into(),
        )
    })?;
    drop(tls_stream);

    println!(
        "Printer presented {} certificate(s) in its handshake chain.",
        chain.len()
    );

    for (index, der) in chain.iter().enumerate() {
        let path = chain_member_path(output, index);
        std::fs::write(&path, der)?;

        let role = if index == 0 { "leaf" } else { "issuer/CA" };
        let display = path.display();
        println!("  [{index}] {role}, {} bytes → {display}", der.len());
        println!("      openssl x509 -in {display} -inform DER -noout -text -ext subjectAltName");
    }

    if chain.len() == 1 {
        println!(
            "Only the leaf was sent — a consumer pinning this printer cannot capture an anchor \
             from the handshake and must compare the leaf itself. Note this is NOT what a P1S \
             does (it sends the BBL CA root too), so it is worth double-checking (see issue \
             #142)."
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::chain_member_path;

    #[test]
    fn leaf_keeps_the_output_path_verbatim() {
        assert_eq!(
            chain_member_path("printer_leaf_cert.der", 0).to_string_lossy(),
            "printer_leaf_cert.der"
        );
    }

    #[test]
    fn chain_members_splice_the_index_before_the_extension() {
        assert_eq!(
            chain_member_path("printer_leaf_cert.der", 1).to_string_lossy(),
            "printer_leaf_cert.chain1.der"
        );
        assert_eq!(
            chain_member_path("/tmp/out/cert.der", 2).to_string_lossy(),
            "/tmp/out/cert.chain2.der"
        );
    }

    #[test]
    fn extensionless_output_appends_the_index() {
        assert_eq!(
            chain_member_path("cert", 1).to_string_lossy(),
            "cert.chain1"
        );
    }

    /// A dotfile is all stem and no extension to `Path`, so the suffix must not be mistaken for
    /// one and duplicated — `.cert` has to stay a single leading dot.
    #[test]
    fn dotfile_output_is_not_treated_as_an_extension() {
        assert_eq!(
            chain_member_path(".cert", 1).to_string_lossy(),
            ".cert.chain1"
        );
    }
}
