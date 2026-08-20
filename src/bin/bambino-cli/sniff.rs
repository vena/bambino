#![cfg(feature = "cli")]
//! # Request-Topic Capture Harness
//!
//! Subscribes to `device/<serial>/request` — the topic clients publish *commands to* — and prints
//! every message another client sends the printer. The intended use is capturing what BambuStudio
//! puts on the wire, so this crate's own payloads can be diffed against the reference
//! implementation without reading a closed-source library or forging TLS.
//!
//! This works because the printer's broker is an ordinary MQTT broker and the access code is
//! already known: the capture connects as a second legitimate client. No interception, no proxy,
//! no certificate trickery. **Whether the broker's ACL permits a second subscriber on `request`
//! is a firmware question with no protocol guarantee** — see [`SubscriptionTopic::Request`]. A
//! refused SUBACK is a real answer ("this printer won't let you"), not a bug in this harness, and
//! is reported as such rather than as an opaque failure.
//!
//! Everything printed is passed through [`crate::redact::redact_secrets`] first. A `request`-topic
//! capture is exactly where credentials and serials show up — `subtask_name`, module lists, an
//! access code echoed in a `system` command — and the natural place to save a capture is a file in
//! this repo, which root `CLAUDE.md` forbids for both.

use std::io::Write;
use std::time::Duration;

use bambino::io::tokio::{TokioRawStreamFactory, build_unsafe_client_config};
use bambino::io::{RawStreamFactory, TlsConnector};
use bambino::mqtt::{MqttClient, SubscriptionTopic};
use bambino::{Error, PrinterIdentity};

use crate::connection::validate_params;
use crate::error::CliError;
use crate::redact::redact_secrets;

/// MQTT-over-TLS port on every Bambu printer.
const MQTT_PORT: u16 = 8883;


/// Subscribes to the request topic and prints each captured message until the deadline elapses.
///
/// `seconds` of `0` means run until interrupted. Output is one pretty-printed JSON object per
/// message on stdout, so it can be piped or teed; progress and diagnostics go to stderr.
pub async fn run(
    ip: &str,
    serial: &str,
    access_code: &str,
    seconds: u64,
    output: Option<&str>,
) -> Result<(), CliError> {
    validate_params(ip, serial, access_code)?;

    let identity = PrinterIdentity::new(ip, serial, access_code);
    let config = build_unsafe_client_config();
    let tls = bambino::io::tokio::TokioTlsConnector::new(tokio_rustls::TlsConnector::from(config));

    eprintln!("Connecting to {ip}:{MQTT_PORT} and subscribing to device/<serial>/request ...");

    let raw = TokioRawStreamFactory
        .dial(ip, MQTT_PORT)
        .await
        .map_err(|e| CliError::Library(Error::Network(e)))?;
    let stream = tls
        .connect(serial, raw)
        .await
        .map_err(|e| CliError::Library(Error::Network(e)))?;

    // A rejected SUBACK is the expected outcome if the broker's ACL forbids a second subscriber
    // on `request`. Translating it here keeps the operator from reading a generic protocol error
    // and concluding the harness is broken, when in fact the question has been answered.
    let mut client = match MqttClient::connect_subscribed(
        stream,
        &identity,
        SubscriptionTopic::Request,
    )
    .await
    {
        Ok(c) => c,
        Err(Error::ProtocolViolation(msg)) if msg.contains("Subscription rejected") => {
            return Err(CliError::Other(format!(
                "The printer's broker refused a subscription to device/<serial>/request ({msg}).\n\
                 That is a firmware ACL decision, not a harness failure — this printer will not \
                 let a second client observe the request topic.\n\
                 Fall back to a proxy that sits between BambuStudio and the printer if a capture \
                 is still needed."
            )));
        }
        Err(e) => return Err(e.into()),
    };

    eprintln!(
        "Subscribed. Capturing{}. Send a print from BambuStudio now; Ctrl-C to stop.",
        if seconds == 0 {
            String::from(" until interrupted")
        } else {
            format!(" for {seconds}s")
        }
    );

    let deadline = (seconds > 0).then(|| Duration::from_secs(seconds));
    let mut captured = 0usize;
    let mut sink = match output {
        Some(path) => Some(std::fs::File::create(path)?),
        None => None,
    };

    // The whole capture is bounded as one future rather than each read being individually
    // timed out. `poll_telemetry` is not documented as cancel-safe, and the request topic is
    // silent most of the time, so per-read timeouts would drop a partially-read frame on every
    // quiet slice — exactly when BambuStudio's burst is most likely to be mid-flight. Bounding
    // the outer loop means the only cancellation happens once, at teardown.
    let capture = async {
        loop {
            match client.poll_telemetry().await {
                Ok(message) => {
                    let Ok(payload) =
                        serde_json::from_slice::<serde_json::Value>(&message.payload)
                    else {
                        eprintln!("(non-JSON payload on {}, skipped)", message.topic);
                        continue;
                    };
                    let redacted = redact_secrets(payload);
                    let rendered = match serde_json::to_string_pretty(&redacted) {
                        Ok(r) => r,
                        Err(e) => {
                            eprintln!("failed to render captured JSON: {e}");
                            continue;
                        }
                    };

                    captured += 1;
                    println!("{rendered}");
                    if let Some(file) = sink.as_mut() {
                        let line = serde_json::to_string(&redacted).unwrap_or_default();
                        if let Err(e) = writeln!(file, "{line}") {
                            eprintln!("failed to append to output file: {e}");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Capture ended: {e}");
                    return;
                }
            }
        }
    };

    match deadline {
        Some(limit) => {
            let _ = tokio::time::timeout(limit, capture).await;
        }
        None => capture.await,
    }

    eprintln!("\nCaptured {captured} message(s) on the request topic.");
    if captured == 0 {
        eprintln!(
            "Nothing arrived. Either no other client published during the window, or the broker \
             accepted the subscription without actually delivering this topic — try again while \
             BambuStudio is mid-send before concluding the topic is unreadable."
        );
    }
    if let Some(path) = output {
        eprintln!("NDJSON written to {path} (secrets redacted).");
    }
    Ok(())
}
