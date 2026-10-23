//! T044: topology loader and fault profiles vs `lib/topology-loader.js`.

mod common;
use std::path::PathBuf;

use chaos::topology::*;
use common::*;
use serde_json::json;

fn config() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../when-gossip-goes-silent/config"))
}

#[test]
fn topologies_match_js() {
    let g = golden("topology.json");
    for t in g["topologies"].as_array().unwrap() {
        let name = t["name"].as_str().unwrap();
        let base = t["basePort"].as_u64().unwrap() as u16;
        let label = format!("{name}@{base}");
        let got = load_topology(&config(), name, base, &|id| id.to_string()).unwrap();
        assert_json_eq(&serde_json::to_value(&got.services).unwrap(), &t["services"], &format!("{label} services"));
        assert_json_eq(&serde_json::to_value(&got.entry_point).unwrap(), &t["entryPoint"], &format!("{label} entry"));
        let pm: serde_json::Map<_, _> = got.port_map.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
        assert_json_eq(&serde_json::Value::Object(pm), &t["portMap"], &format!("{label} portMap"));
        assert_eq!(got.edges, t["edges"], "{label} edges");
        assert_eq!(got.name, t["name"]);
    }
}

#[test]
fn fault_profiles_load_verbatim() {
    let g = golden("topology.json");
    for f in g["faults"].as_array().unwrap() {
        let p = load_fault_profile(&config(), f["name"].as_str().unwrap()).unwrap();
        assert_eq!(p, f["profile"]);
    }
}

#[test]
fn missing_file_names_the_path() {
    let err = load_topology(&config(), "nope", 4000, &|id| id.to_string()).unwrap_err();
    assert!(err.contains("nope.json"), "{err}");
}
