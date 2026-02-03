use std::collections::HashMap;
use zbus::zvariant::Value;

use crate::extract_uri_from_map;

#[test]
fn test_uri_direct() {
    let mut m: HashMap<String, Value> = HashMap::new();
    m.insert("uri".to_string(), Value::new("file:///tmp/s1.png"));
    assert_eq!(extract_uri_from_map(&m).as_deref(), Some("file:///tmp/s1.png"));
}

#[test]
fn test_results_array_with_string() {
    let mut m: HashMap<String, Value> = HashMap::new();
    // set results to an array containing the file URI
    m.insert("results".to_string(), Value::new(vec![Value::new("file:///tmp/s2.png")]));
    assert_eq!(extract_uri_from_map(&m).as_deref(), Some("file:///tmp/s2.png"));
}

#[test]
fn test_nested_array() {
    let mut m: HashMap<String, Value> = HashMap::new();
    m.insert("results".to_string(), Value::new(vec![Value::new("foo"), Value::new("file:///tmp/s3.png")]));
    assert_eq!(extract_uri_from_map(&m).as_deref(), Some("file:///tmp/s3.png"));
}
