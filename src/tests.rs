use std::collections::HashMap;
use zbus::zvariant::Value;

use crate::extract_uri_from_map;

#[test]
fn test_uri_direct() {
    let mut m: HashMap<String, Value> = HashMap::new();
    m.insert(uri.to_string(), Value::Str(file:///tmp/s1.png.into()));
    assert_eq!(extract_uri_from_map(&m).as_deref(), Some(file:///tmp/s1.png));
}

#[test]
fn test_results_dict_with_pair() {
    // construct a dict like @a{sv} with a pair where value is the string
    let mut m: HashMap<String, Value> = HashMap::new();
    use zbus::zvariant::{Dict, Value as V, Signature};
    let mut d: Dict = Dict::new(&Signature::from(s), &Signature::from(v));
    d.add(key, Value::Str(file:///tmp/s2.png)) .unwrap();
    m.insert(results.to_string(), V::Dict(d));
    assert_eq!(extract_uri_from_map(&m).as_deref(), Some(file:///tmp/s2.png));
}

#[test]
fn test_nested_array() {
    let mut m: HashMap<String, Value> = HashMap::new();
    use zbus::zvariant::{Array, Signature, Value as V};
    let mut a: Array = Array::new(&Signature::from(s));
    a.append(Value::Str(foo)) .unwrap();
    a.append(Value::Str(file:///tmp/s3.png)) .unwrap();
    m.insert(results.to_string(), V::Array(a));
    assert_eq!(extract_uri_from_map(&m).as_deref(), Some(file:///tmp/s3.png));
}
