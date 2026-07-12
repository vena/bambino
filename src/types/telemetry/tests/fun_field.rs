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
