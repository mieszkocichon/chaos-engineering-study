//! Topology / fault-profile / matrix loading, port of `lib/topology-loader.js`.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};

/// Ports the browser `fetch` spec refuses to connect to (undici enforces it in Node).
const FETCH_BLOCKED_PORTS: [u16; 4] = [4045, 3659, 5060, 5061];

#[derive(Clone, Debug, Serialize)]
pub struct Downstream {
    pub id: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Neighbor {
    pub id: String,
    pub gossip_url: String,
    pub metrics_url: String,
    pub control_url: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
    pub id: String,
    pub port: u16,
    pub role: String,
    pub downstreams: Vec<Downstream>,
    pub neighbors: Vec<Neighbor>,
    /// number (constant ms) or `{distribution, ...}` – kept as raw JSON
    pub processing_delay: Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct EntryPoint {
    pub id: String,
    pub url: String,
    pub port: u16,
}

#[derive(Clone, Debug)]
pub struct Topology {
    pub name: Value,
    pub description: Value,
    pub services: Vec<Service>,
    pub edges: Value,
    pub entry_point: Option<EntryPoint>,
    pub port_map: Vec<(String, u16)>,
}

impl Topology {
    pub fn service(&self, id: &str) -> Option<&Service> {
        self.services.iter().find(|s| s.id == id)
    }
}

/// Config root: `CONFIG_DIR` env or `/app/config` (the image layout).
pub fn config_dir() -> PathBuf {
    std::env::var_os("CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/app/config"))
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// `loadTopology(name, { basePort, hostResolver })`; host = service id (Docker DNS) unless overridden.
pub fn load_topology(dir: &Path, name: &str, base_port: u16, host: &dyn Fn(&str) -> String) -> Result<Topology, String> {
    let t = read_json(&dir.join("topologies").join(format!("{name}.json")))?;
    let empty = vec![];
    let svcs = t["services"].as_array().unwrap_or(&empty);
    let edges = t["edges"].as_array().unwrap_or(&empty);

    let mut port_map: Vec<(String, u16)> = Vec::new();
    let mut next = base_port;
    for s in svcs {
        while FETCH_BLOCKED_PORTS.contains(&next) {
            next += 1;
        }
        port_map.push((s["id"].as_str().unwrap_or_default().to_string(), next));
        next += 1;
    }
    let ports: HashMap<&str, u16> = port_map.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let port = |id: &str| ports.get(id).map(|p| p.to_string()).unwrap_or_else(|| "undefined".into());

    let default_delay = t
        .get("defaults")
        .and_then(|d| d.get("processingDelayMs"))
        .filter(|v| truthy(v))
        .cloned()
        .unwrap_or_else(|| json!({ "distribution": "normal", "mean": 50, "std": 10 }));

    let mut services = Vec::new();
    for s in svcs {
        let id = s["id"].as_str().unwrap_or_default();
        let downstreams = edges
            .iter()
            .filter(|e| e["from"].as_str() == Some(id) && e["kind"].as_str() != Some("gossip"))
            .map(|e| {
                let to = e["to"].as_str().unwrap_or_default();
                Downstream { id: to.into(), url: format!("http://{}:{}/data", host(to), port(to)) }
            })
            .collect();
        // insertion-ordered unique set of neighbor ids (edges in both directions)
        let mut seen = BTreeSet::new();
        let mut neighbor_ids = Vec::new();
        for e in edges {
            for (a, b) in [("from", "to"), ("to", "from")] {
                if e[a].as_str() == Some(id) {
                    let other = e[b].as_str().unwrap_or_default().to_string();
                    if seen.insert(other.clone()) {
                        neighbor_ids.push(other);
                    }
                }
            }
        }
        let neighbors = neighbor_ids
            .iter()
            .map(|n| {
                let base = format!("http://{}:{}", host(n), port(n));
                Neighbor {
                    id: n.clone(),
                    gossip_url: format!("{base}/gossip"),
                    metrics_url: format!("{base}/metrics"),
                    control_url: format!("{base}/control"),
                }
            })
            .collect();
        services.push(Service {
            id: id.into(),
            port: ports[id],
            role: s.get("role").filter(|r| truthy(r)).and_then(Value::as_str).unwrap_or("service").into(),
            downstreams,
            neighbors,
            processing_delay: s.get("processingDelay").filter(|v| truthy(v)).cloned().unwrap_or_else(|| default_delay.clone()),
        });
    }

    let entry_point = services.iter().find(|s| s.role == "entry").map(|s| EntryPoint {
        id: s.id.clone(),
        url: format!("http://{}:{}/data", host(&s.id), s.port),
        port: s.port,
    });
    Ok(Topology {
        name: t["name"].clone(),
        description: t["description"].clone(),
        services,
        edges: t["edges"].clone(),
        entry_point,
        port_map,
    })
}

pub fn load_fault_profile(dir: &Path, name: &str) -> Result<Value, String> {
    read_json(&dir.join("faults").join(format!("{name}.json")))
}

pub fn load_experiment_matrix(dir: &Path, name: &str) -> Result<Value, String> {
    read_json(&dir.join("experiments").join(format!("{name}.json")))
}

/// JS truthiness of a JSON value.
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
