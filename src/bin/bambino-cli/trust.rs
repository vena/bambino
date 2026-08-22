#![cfg(feature = "cli")]

//! Optional CA trust anchors for every printer-facing subcommand, supplied by the global
//! `--with-certs <PATH>` flag.
//!
//! Without the flag the CLI keeps its historical behaviour: `NoCertificateVerification`, which
//! skips the chain walk *and* asserts the handshake signature unconditionally, so the peer never
//! proves possession of the presented cert's private key. With it, every TLS config the CLI
//! builds (MQTT, FTPS control + data, binary camera) goes through
//! `CnFallbackServerVerifier` instead, making `monitor`/`dump`/`probe`/`control`/`files`/`camera`
//! usable as end-to-end verification tests rather than only the purpose-built `verify-tls`
//! diagnostic.
//!
//! Anchors live in a process-global `OnceLock` for the same reason [`crate::VERBOSE`] is a global
//! atomic: reaching them from inside async tasks and submodules would otherwise mean threading an
//! extra parameter through every subcommand's `run()` signature.

use std::sync::OnceLock;

use bambino::io::tokio::{
    build_unsafe_client_config_with_options, build_verified_client_config_with_options,
};
use rustls_pki_types::{CertificateDer, pem::PemObject};
use tokio_rustls::rustls;

use crate::error::CliError;

/// File extensions treated as certificate files when `--with-certs` names a directory.
/// Anything else (READMEs, keys, notes) is skipped rather than failing the whole load.
const CERT_EXTENSIONS: &[&str] = &["pem", "cert", "crt", "cer", "der"];

static TRUSTED_ROOTS: OnceLock<Vec<CertificateDer<'static>>> = OnceLock::new();

/// Installs the trust anchors parsed from `--with-certs`. Called once from `main` before any
/// subcommand runs.
pub(crate) fn set_trusted_roots(certs: Vec<CertificateDer<'static>>) {
    let _ = TRUSTED_ROOTS.set(certs);
}

/// Returns the installed trust anchors, or `None` when `--with-certs` was not passed.
pub(crate) fn trusted_roots() -> Option<&'static [CertificateDer<'static>]> {
    TRUSTED_ROOTS.get().map(Vec::as_slice)
}

/// Builds the `ClientConfig` every CLI TLS call site should use: CA-verified when
/// `--with-certs` supplied anchors, otherwise the unverified default.
///
/// `force_tls_1_2` is passed through unchanged so the P2S/X2D FTPS quirk
/// (`enforces_ftps_tls_1_2`) applies identically on both paths.
pub(crate) fn build_cli_tls_config(
    force_tls_1_2: bool,
) -> Result<std::sync::Arc<rustls::ClientConfig>, CliError> {
    match trusted_roots() {
        Some(roots) => {
            build_verified_client_config_with_options(roots.to_vec(), None, force_tls_1_2)
                .map_err(|e| CliError::Other(format!("failed to build verified TLS config: {e}")))
        }
        None => Ok(build_unsafe_client_config_with_options(force_tls_1_2)),
    }
}

/// Loads trust anchors from `path`, which may be a single certificate file or a directory of
/// them (as the uncommitted `certs/` collection is).
///
/// Both PEM and raw DER are accepted, sniffed per file rather than by extension — the captured
/// BBL certs use a `.cert` suffix but are PEM inside. A PEM file may hold several certs; all of
/// them are taken, so a bundle file and a directory of single-cert files behave the same.
///
/// Every anchor is passed to `CnFallbackServerVerifier`, which accepts intermediates as anchors
/// directly (the chain walk checks each cert's issuer against the anchor set at every hop). So
/// pointing this at a directory holding both a root and its device-CA intermediates works
/// whether or not the printer presents the full chain in its handshake.
pub(crate) fn load_trust_anchors(path: &str) -> Result<Vec<CertificateDer<'static>>, CliError> {
    let meta = std::fs::metadata(path)
        .map_err(|e| CliError::InvalidArgs(format!("--with-certs {path}: {e}")))?;

    let files: Vec<std::path::PathBuf> = if meta.is_dir() {
        let mut found: Vec<_> = std::fs::read_dir(path)
            .map_err(|e| CliError::InvalidArgs(format!("--with-certs {path}: {e}")))?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| CERT_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
            })
            .collect();
        // Deterministic order so a chain-walk failure is reproducible run to run.
        found.sort();
        found
    } else {
        vec![std::path::PathBuf::from(path)]
    };

    // Origin filename is tracked per *anchor*, not per file: a PEM bundle contributes several
    // anchors from one path, so zipping the two lists afterwards would misattribute them.
    let mut anchors = Vec::new();
    let mut origins: Vec<String> = Vec::new();
    for file in &files {
        let name = file.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let bytes = std::fs::read(file)
            .map_err(|e| CliError::InvalidArgs(format!("--with-certs {}: {e}", file.display())))?;

        if bytes.windows(5).any(|w| w == b"-----") {
            for cert in CertificateDer::pem_slice_iter(&bytes) {
                let cert = cert.map_err(|e| {
                    CliError::InvalidArgs(format!("--with-certs {}: {e}", file.display()))
                })?;
                anchors.push(cert.into_owned());
                origins.push(name.clone());
            }
        } else {
            anchors.push(CertificateDer::from(bytes));
            origins.push(name);
        }
    }

    if anchors.is_empty() {
        return Err(CliError::InvalidArgs(format!(
            "--with-certs {path}: no certificates found (looked for {})",
            CERT_EXTENSIONS.join(", ")
        )));
    }

    println!(
        "TLS certificate verification ENABLED: {} trust anchor(s) from {path}",
        anchors.len()
    );

    // Which anchors were actually loaded is the other half of the audit question the verifier's
    // own "TLS chain anchored" log answers — printed only under -v so the plain run keeps its
    // one-line summary.
    if crate::is_verbose() {
        for (der, origin) in anchors.iter().zip(origins.iter()) {
            match x509_parser::parse_x509_certificate(der.as_ref()) {
                Ok((_, cert)) => println!("  trust anchor: {} [{origin}]", cert.subject()),
                Err(e) => println!("  trust anchor: <unparsable: {e}> [{origin}]"),
            }
        }
    }

    Ok(anchors)
}
