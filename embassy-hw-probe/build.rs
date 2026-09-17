//! Loads the probe's target details from a local `.env` so no Wi-Fi password, printer IP,
//! serial, or access code has to be typed on a command line (where it would land in shell
//! history) or committed. `.env` is gitignored; see `.env.example` for the keys. Values
//! reach `main.rs` through `env!`/`option_env!`.
//!
//! Deliberately a near-copy of `esp32-hw-probe/build.rs` minus its
//! `embuild::espidf::sysenv::output()` call: this target has no ESP-IDF kconfig to re-emit.
//! Kept as a copy rather than a shared crate because the two probes are independent
//! packages by design (different target triple, different runtime, mutually exclusive
//! feature sets) and a shared build-support crate would be the one thing coupling them.

use std::io::Write;

/// Keys every probe needs, read with `env!`. All must be present or the build stops here
/// with a message naming the missing key, rather than at a bare `env!` "not found" error.
const REQUIRED_KEYS: [&str; 5] = [
    "PROBE_WIFI_SSID",
    "PROBE_WIFI_PASS",
    "PROBE_PRINTER_IP",
    "PROBE_SERIAL",
    // Required here, unlike on the esp-idf probe: this probe's MQTT and FTPS stages
    // authenticate, so a build without it could only ever reach the TLS handshake.
    "PROBE_ACCESS_CODE",
];

/// Keys forwarded when present and silently skipped when absent. Read them with
/// `option_env!`, not `env!` — `env!` on an absent key is a compile error, which would drag
/// the key back into being effectively required.
const OPTIONAL_KEYS: [&str; 0] = [];

fn main() {
    load_dotenv();
}

/// Parses `.env` (`KEY=VALUE` per line) and forwards each key to the compiler.
///
/// A value already set in the process environment wins, so a one-off
/// `PROBE_PRINTER_IP=... cargo build` can still override the file without editing it.
fn load_dotenv() {
    println!("cargo:rerun-if-changed=.env");
    for key in REQUIRED_KEYS.iter().chain(OPTIONAL_KEYS.iter()) {
        println!("cargo:rerun-if-env-changed={key}");
    }

    let mut values: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    if let Ok(contents) = std::fs::read_to_string(".env") {
        for line in contents.lines() {
            let line = line.trim_start();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Split on the *first* `=` only: a WPA password may legitimately contain one.
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            // Only \r is trimmed from the value (CRLF files); leading/trailing spaces are
            // kept because they can be real password characters. Wrap a value in single or
            // double quotes to make surrounding whitespace explicit.
            let value = value.trim_end_matches('\r');
            let value = match (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
                || (value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2)
            {
                true => &value[1..value.len() - 1],
                false => value,
            };
            values.insert(key.to_string(), value.to_string());
        }
    }

    // Resolve everything before emitting anything. Cargo dumps a build script's captured
    // stdout when the script *fails*, so emitting `cargo:rustc-env=` lines as we go would
    // print a Wi-Fi password to the terminal the moment some later key turned out to be
    // missing. Nothing is printed until every key is known to be present.
    let mut resolved = Vec::new();
    let mut missing = Vec::new();
    for key in REQUIRED_KEYS {
        // Process env beats the file. Already in the compiler's environment, so it needs
        // no `cargo:rustc-env` line of its own.
        if std::env::var_os(key).is_some() {
            continue;
        }
        match values.get(key) {
            Some(value) if !value.is_empty() => resolved.push((key, value)),
            _ => missing.push(key),
        }
    }

    // Optional keys never land in `missing`: absent means "this probe does not use it",
    // which `option_env!` reports as `None` at the use site rather than as a build failure.
    for key in OPTIONAL_KEYS {
        if std::env::var_os(key).is_some() {
            continue;
        }
        if let Some(value) = values.get(key).filter(|value| !value.is_empty()) {
            resolved.push((key, value));
        }
    }

    if !missing.is_empty() {
        let _ = writeln!(
            std::io::stderr(),
            "\nembassy-hw-probe: missing {} — set {} in embassy-hw-probe/.env (copy \
             .env.example) or pass them in the environment.\n",
            if missing.len() == 1 { "key" } else { "keys" },
            missing.join(", "),
        );
        std::process::exit(1);
    }

    for (key, value) in resolved {
        println!("cargo:rustc-env={key}={value}");
    }
}
