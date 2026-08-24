#![cfg(feature = "cli")]
//! # Secret Redaction for Captured Payloads
//!
//! Used by every harness that writes a printer's own words to a file or to stdout — today
//! `ack_probe` and `probe`. Root `CLAUDE.md` forbids an access code or serial number landing
//! in a file in this repository, and both are exactly what a capture picks up:
//! `get_access_code`'s whole reply is a credential, and module lists and job payloads carry
//! serials. Kept as its own module rather than folded back into `ack_probe` because the rule
//! applies to any future capture harness, and the cost of re-deriving it after a leak is not
//! symmetric with the cost of keeping it here.

/// Keys whose values are credentials or device identity, and must never reach a report or stdout.
pub const REDACTED_KEYS: &[&str] = &["access_code", "sn", "serial", "serial_number", "dev_sn"];

/// Recursively replaces the value of every [`REDACTED_KEYS`] entry with a placeholder.
///
/// Keys are matched exactly and case-insensitively, at any depth. The surrounding structure is
/// preserved so a capture still shows *that* the field was present and where — which is the part
/// a protocol question actually needs.
pub fn redact_secrets(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    if REDACTED_KEYS.iter().any(|r| r.eq_ignore_ascii_case(&k)) {
                        (k, serde_json::Value::String("<redacted>".to_string()))
                    } else {
                        (k, redact_secrets(v))
                    }
                })
                .collect(),
        ),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(redact_secrets).collect())
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::redact_secrets;

    #[test]
    fn test_redact_secrets_replaces_access_code_at_any_depth() {
        let input = serde_json::json!({
            "system": {
                "command": "get_access_code",
                "access_code": "12345678",
                "sequence_id": "42",
            }
        });
        let out = redact_secrets(input);
        assert_eq!(out["system"]["access_code"], "<redacted>");
        assert_eq!(out["system"]["command"], "get_access_code");
        assert_eq!(out["system"]["sequence_id"], "42");
    }

    #[test]
    fn test_redact_secrets_walks_arrays_and_is_case_insensitive() {
        let input = serde_json::json!({
            "module": [
                { "name": "ams/0", "SN": "0123456789ABCDE" },
                { "name": "mc", "sn": "0123456789ABCDE" },
            ]
        });
        let out = redact_secrets(input);
        assert_eq!(out["module"][0]["SN"], "<redacted>");
        assert_eq!(out["module"][1]["sn"], "<redacted>");
        assert_eq!(out["module"][0]["name"], "ams/0");
    }

    #[test]
    fn test_redact_secrets_leaves_unrelated_scalars_alone() {
        let input = serde_json::json!({ "result": "success", "nested": [1, true, null] });
        let out = redact_secrets(input.clone());
        assert_eq!(out, input);
    }
}
