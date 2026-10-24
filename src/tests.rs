use std::collections::HashMap;
use std::time::Duration;
use zbus::Variant;

use crate::{CliArgs, ScreenshotTarget, extract_uri_from_map, parse_cli_args, parse_timeout_secs};

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
