#![allow(dead_code)]
//! Shared helpers for golden-vector and integration tests.

pub mod svc;

use serde_json::Value;
use sha2::{Digest, Sha256};

pub fn golden(name: &str) -> Value {
    let path = format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))).unwrap()
}

pub fn hex(x: f64) -> String {
    format!("{:x}", x.to_bits())
}

pub fn sha(s: &str) -> String {
    Sha256::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// Compare a hashed golden sequence `{n, sha256, head}` against generated hex values.
pub fn check_seq(label: &str, g: &Value, got: &[String]) {
    let head: Vec<&str> = g["head"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    for (i, (a, b)) in got.iter().zip(&head).enumerate() {
        assert_eq!(a, b, "{label}: first mismatch at index {i}");
    }
    assert_eq!(got.len() as u64, g["n"].as_u64().unwrap(), "{label}: length");
    assert_eq!(sha(&got.join(",")), g["sha256"].as_str().unwrap(), "{label}: digest");
}

/// Numbers in golden JSON may be ints or floats; compare numerically.
pub fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

/// Assert JSON equality treating 1 and 1.0 as equal.
pub fn assert_json_eq(got: &Value, exp: &Value, label: &str) {
    fn norm(v: &Value) -> Value {
        match v {
            Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
            Value::Array(a) => Value::Array(a.iter().map(norm).collect()),
            Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), norm(v))).collect()),
            other => other.clone(),
        }
    }
    assert_eq!(norm(got), norm(exp), "{label}");
}
