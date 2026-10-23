//! Resilience-pattern demos (circuit breaker, retry storm, saga) – Rust port of `demos/`.
//! One multi-call binary: `demo <service|client|scenario>`; see crates/demos/README.md.

pub mod clients;
pub mod scenarios;
pub mod services;

use serde_json::Value;

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64
}

/// `new Date().toISOString()`
pub fn iso_now() -> String {
    chaos::coordinator::iso8601(now_ms())
}

pub fn env_or(k: &str, d: &str) -> String {
    std::env::var(k).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| d.to_string())
}

/// `getArg(name, default)` of the JS clients: value after `--name`, if any.
pub fn arg(args: &[String], name: &str, default: &str) -> String {
    args.iter()
        .position(|a| *a == format!("--{name}"))
        .and_then(|i| args.get(i + 1))
        .filter(|v| !v.is_empty())
        .cloned()
        .unwrap_or_else(|| default.to_string())
}

/// `${v}` of a JSON value in a template literal.
pub fn js_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => metastable::js::js_number(n.as_f64().unwrap()),
        Value::Null => "undefined".into(),
        other => other.to_string(),
    }
}

pub fn truthy(v: &Value) -> bool {
    chaos::topology::truthy(v)
}
