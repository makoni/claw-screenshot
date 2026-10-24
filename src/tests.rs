use std::collections::HashMap;
use std::time::Duration;
use zbus::Variant;

use crate::{
    CliArgs, ScreenshotTarget, extract_uri_from_map, parse_cli_args, parse_delay,
    parse_timeout_secs,
};

#[test]
fn test_uri_direct() {
    let mut m: HashMap<String, Variant> = HashMap::new();
    m.insert("uri".to_string(), Variant::new("file:///tmp/s1.png"));
    assert_eq!(
        extract_uri_from_map(&m).as_deref(),
        Some("file:///tmp/s1.png")
    );
}

#[test]
fn test_results_array_with_string() {
    let mut m: HashMap<String, Variant> = HashMap::new();
    // set results to an array containing the file URI
    m.insert(
        "results".to_string(),
        Variant::new(vec![Variant::new("file:///tmp/s2.png")]),
    );
    assert_eq!(
        extract_uri_from_map(&m).as_deref(),
        Some("file:///tmp/s2.png")
    );
}

#[test]
fn test_nested_array() {
    let mut m: HashMap<String, Variant> = HashMap::new();
    m.insert(
        "results".to_string(),
        Variant::new(vec![
            Variant::new("foo"),
            Variant::new("file:///tmp/s3.png"),
        ]),
    );
    assert_eq!(
        extract_uri_from_map(&m).as_deref(),
        Some("file:///tmp/s3.png")
    );
}

#[test]
fn test_no_uri_returns_none() {
    let mut m: HashMap<String, Variant> = HashMap::new();
    m.insert("other".to_string(), Variant::new("no screenshot here"));
    assert_eq!(extract_uri_from_map(&m), None);
}

#[test]
fn test_file_uri_in_middle_of_string_ignored() {
    let mut m: HashMap<String, Variant> = HashMap::new();
    m.insert(
        "note".to_string(),
        Variant::new("prefix file:///tmp/x.png suffix"),
    );
    assert_eq!(extract_uri_from_map(&m), None);
}

#[test]
fn test_non_file_uri_key_returned_verbatim() {
    // The spec key is trusted as-is, even for non-file schemes.
    let mut m: HashMap<String, Variant> = HashMap::new();
    m.insert("uri".to_string(), Variant::new("http://example.com/x.png"));
    assert_eq!(
        extract_uri_from_map(&m).as_deref(),
        Some("http://example.com/x.png")
    );
}

#[test]
fn test_wrapped_variant() {
    // A VARIANT holding the URI (signature v) must be unwrapped.
    let mut m: HashMap<String, Variant> = HashMap::new();
    m.insert(
        "other".to_string(),
        Variant::new(Variant::new("file:///tmp/s4.png")),
    );
    assert_eq!(
        extract_uri_from_map(&m).as_deref(),
        Some("file:///tmp/s4.png")
    );
}

#[test]
fn test_parse_timeout_secs() {
    assert_eq!(parse_timeout_secs("30"), Some(Duration::from_secs(30)));
    assert_eq!(parse_timeout_secs("0"), None);
    assert_eq!(parse_timeout_secs("-5"), None);
    assert_eq!(parse_timeout_secs("abc"), None);
    assert_eq!(parse_timeout_secs(""), None);
}

#[test]
fn test_no_arguments_runs() {
    assert_eq!(parse_cli_args(&[]), Ok(CliArgs::default()));
}

#[test]
fn test_help_argument() {
    for flag in ["-h", "--help"] {
        let args = vec![flag.to_string()];
        assert!(parse_cli_args(&args).unwrap().help);
    }
}

#[test]
fn test_version_argument() {
    for flag in ["-V", "--version"] {
        let args = vec![flag.to_string()];
        assert!(parse_cli_args(&args).unwrap().version);
    }
}

#[test]
fn test_window_argument() {
    let args = vec!["--window".to_string()];
    assert_eq!(
        parse_cli_args(&args).unwrap().target,
        Some(ScreenshotTarget::Window)
    );
}

#[test]
fn test_area_argument() {
    let args = vec!["--area".to_string()];
    assert_eq!(
        parse_cli_args(&args).unwrap().target,
        Some(ScreenshotTarget::Area)
    );
}

#[test]
fn test_active_window_argument() {
    let args = vec!["--active-window".to_string()];
    assert_eq!(
        parse_cli_args(&args).unwrap().target,
        Some(ScreenshotTarget::ActiveWindow)
    );
}

#[test]
fn test_target_bits() {
    assert_eq!(ScreenshotTarget::Window.bit(), 2);
    assert_eq!(ScreenshotTarget::Area.bit(), 4);
    assert_eq!(ScreenshotTarget::ActiveWindow.bit(), 8);
}

#[test]
fn test_two_targets_rejected() {
    let args = vec!["--window".to_string(), "--area".to_string()];
    assert!(parse_cli_args(&args).is_err());
}

#[test]
fn test_unknown_argument_errors() {
    let args = vec!["--nope".to_string()];
    assert!(parse_cli_args(&args).is_err());
}

#[test]
fn test_delay_flag_value() {
    let args = vec!["--delay".to_string(), "5".to_string()];
    assert_eq!(parse_cli_args(&args).unwrap().delay, Some(5));
}

#[test]
fn test_delay_equals_form() {
    let args = vec!["--delay=7".to_string()];
    assert_eq!(parse_cli_args(&args).unwrap().delay, Some(7));
}

#[test]
fn test_delay_zero_is_valid() {
    let args = vec!["--delay".to_string(), "0".to_string()];
    assert_eq!(parse_cli_args(&args).unwrap().delay, Some(0));
}

#[test]
fn test_delay_max_boundary() {
    let ok = vec!["--delay".to_string(), "3600".to_string()];
    assert_eq!(parse_cli_args(&ok).unwrap().delay, Some(3600));
    let too_big = vec!["--delay".to_string(), "3601".to_string()];
    assert!(parse_cli_args(&too_big).is_err());
}

#[test]
fn test_delay_missing_value_errors() {
    let args = vec!["--delay".to_string()];
    assert!(parse_cli_args(&args).is_err());
}

#[test]
fn test_delay_invalid_value_errors() {
    for bad in ["abc", "-3", "1.5", ""] {
        let args = vec!["--delay".to_string(), bad.to_string()];
        assert!(
            parse_cli_args(&args).is_err(),
            "expected {bad:?} to be rejected"
        );
    }
}

#[test]
fn test_delay_combines_with_target() {
    let args = vec![
        "--window".to_string(),
        "--delay".to_string(),
        "90".to_string(),
    ];
    let parsed = parse_cli_args(&args).unwrap();
    assert_eq!(parsed.target, Some(ScreenshotTarget::Window));
    assert_eq!(parsed.delay, Some(90));
}

#[test]
fn test_delay_last_wins() {
    let args = vec![
        "--delay".to_string(),
        "3".to_string(),
        "--delay".to_string(),
        "5".to_string(),
    ];
    assert_eq!(parse_cli_args(&args).unwrap().delay, Some(5));
}

#[test]
fn test_parse_delay_bounds() {
    assert_eq!(parse_delay("0"), Ok(0));
    assert_eq!(parse_delay("3600"), Ok(3600));
    assert!(parse_delay("3601").is_err());
    assert!(parse_delay("abc").is_err());
    assert!(parse_delay("-1").is_err());
}
