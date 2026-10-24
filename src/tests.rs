use std::collections::HashMap;
use zbus::Variant;

use crate::{CliAction, extract_uri_from_map, parse_cli_args};

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
        Variant::new(vec![Variant::new("foo"), Variant::new("file:///tmp/s3.png")]),
    );
    assert_eq!(
        extract_uri_from_map(&m).as_deref(),
        Some("file:///tmp/s3.png")
    );
}

#[test]
fn test_no_arguments_runs() {
    assert_eq!(parse_cli_args(&[]), Ok(CliAction::Run));
}

#[test]
fn test_help_argument() {
    for flag in ["-h", "--help"] {
        let args = vec![flag.to_string()];
        assert_eq!(parse_cli_args(&args), Ok(CliAction::Help));
    }
}

#[test]
fn test_version_argument() {
    for flag in ["-V", "--version"] {
        let args = vec![flag.to_string()];
        assert_eq!(parse_cli_args(&args), Ok(CliAction::Version));
    }
}

#[test]
fn test_unknown_argument_errors() {
    let args = vec!["--nope".to_string()];
    assert!(parse_cli_args(&args).is_err());
}

#[test]
fn test_multiple_arguments_error() {
    let args = vec!["-h".to_string(), "extra".to_string()];
    assert!(parse_cli_args(&args).is_err());
}
