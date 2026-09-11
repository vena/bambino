use super::*;

#[test]
fn test_fun_field_deserialization_top_level() {
    let json = r#"{ "fun": "3EC1AFFF9CFF" }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    assert_eq!(report.fun.as_deref(), Some("3EC1AFFF9CFF"));
}

#[test]
fn test_fun_field_deserialization_nested_in_print() {
    let json = r#"{ "print": { "fun": "1AFFF9CFF" } }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    assert_eq!(report.print.unwrap().fun.as_deref(), Some("1AFFF9CFF"));
}

#[test]
fn test_fun_top_level_only() {
    let json = r#"{ "fun": "3EC1AFFF9CFF" }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    assert_eq!(report.fun(), Some("3EC1AFFF9CFF"));
}

#[test]
fn test_fun_print_nested_only() {
    let json = r#"{ "print": { "fun": "3EC1AFFF9CFF" } }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    assert_eq!(report.fun(), Some("3EC1AFFF9CFF"));
}

#[test]
fn test_fun2_deserialization_and_location_fallback() {
    // BambuStudio reads only print.fun2, but this accessor mirrors fun's documented drift.
    let nested: TelemetryReport = serde_json::from_str(r#"{ "print": { "fun2": "20" } }"#).unwrap();
    assert_eq!(nested.fun2(), Some("20"));

    let top: TelemetryReport = serde_json::from_str(r#"{ "fun2": "20" }"#).unwrap();
    assert_eq!(top.fun2(), Some("20"));

    // Top level wins, same first-found-wins order as fun().
    let both: TelemetryReport =
        serde_json::from_str(r#"{ "fun2": "20", "print": { "fun2": "00" } }"#).unwrap();
    assert_eq!(both.fun2(), Some("20"));

    // Absent entirely is None, not a defaulted zero.
    let absent: TelemetryReport = serde_json::from_str(r#"{ "print": {} }"#).unwrap();
    assert_eq!(absent.fun2(), None);
    assert_eq!(absent.supports_remote_dry(), None);
}

#[test]
fn test_fun2_bit_reads_lsb_first_from_the_right() {
    // 0x20 = 0b0010_0000 -> bit 5 set, neighbours clear.
    assert_eq!(fun2_bit("20", 5), Some(true));
    assert_eq!(fun2_bit("20", 4), Some(false));
    assert_eq!(fun2_bit("20", 6), Some(false));
    assert_eq!(fun2_bit("00", 5), Some(false));

    // A bit index past the end of the string reads false, not None — BambuStudio's
    // get_flag_bits_no_border returns 0 there rather than failing.
    assert_eq!(fun2_bit("20", 99), Some(false));

    // A 0x prefix and stray non-hex characters are ignored, as upstream filters them.
    assert_eq!(fun2_bit("0x20", 5), Some(true));

    // No hex digits at all is the one None case: the printer told us nothing.
    assert_eq!(fun2_bit("", 5), None);
    assert_eq!(fun2_bit("zz", 5), None);
}

#[test]
fn test_fun2_bit_handles_strings_longer_than_u64() {
    // fun2 "may have infinite length" upstream. A 20-digit string overflows
    // u64::from_str_radix, which is exactly why this does not go through it — the `fun` parse
    // would return None here and report every capability as absent.
    let long_hex = "1234567890ABCDEF1234";
    assert!(u64::from_str_radix(long_hex, 16).is_err());

    // Low nibble is 4 = 0b0100, so bit 2 is set and bit 0 is clear.
    assert_eq!(fun2_bit(long_hex, 2), Some(true));
    assert_eq!(fun2_bit(long_hex, 0), Some(false));

    // Bit 76 lands in the leading "1", well past 64 bits.
    assert_eq!(fun2_bit(long_hex, 76), Some(true));
}

#[test]
fn test_supports_remote_dry_reads_bit_five() {
    let on: TelemetryReport = serde_json::from_str(r#"{ "print": { "fun2": "20" } }"#).unwrap();
    assert_eq!(on.supports_remote_dry(), Some(true));

    let off: TelemetryReport = serde_json::from_str(r#"{ "print": { "fun2": "1F" } }"#).unwrap();
    assert_eq!(off.supports_remote_dry(), Some(false));

    // A printer reporting bit 5 clear is a real "no", distinct from never reporting fun2.
    assert_ne!(off.supports_remote_dry(), None);
}

#[test]
fn test_fun_both_present_top_level_wins() {
    let json = r#"{
            "fun": "TOP_LEVEL",
            "print": { "fun": "NESTED" }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    assert_eq!(report.fun(), Some("TOP_LEVEL"));
}

#[test]
fn test_fun_neither_present() {
    let json = r#"{ "print": {} }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    assert!(report.fun().is_none());
}
