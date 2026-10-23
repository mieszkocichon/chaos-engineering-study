use std::collections::BTreeMap;
use std::path::Path;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::experiment::cell::Cell;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkloadConfig {
    pub rps: Option<u32>,
    pub duration_sec: Option<u32>,
    pub warmup_sec: Option<u32>,
    pub health_timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesConfig {
    pub gateway_cpus: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CellAction {
    pub at_sec: u64,
    pub kind: String,
    pub target: String,
    #[serde(default)]
    pub metadata: Option<SpofTestMeta>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SpofTestMeta {
    pub killed_service: String,
    pub kill_at_sec: u64,
    #[serde(default)]
    pub permanent: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VariantConfig {
    pub name: String,
    pub strategy: String,
    pub static_params: Option<Value>,
    pub swim_fanout: Option<u32>,
    pub spof_kill_at_sec: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExtraCellConfig {
    pub topology: Option<String>,
    pub strategy: String,
    pub base: Option<String>,
    pub fault: Option<String>,
    pub partition: Option<String>,
    #[serde(deserialize_with = "deserialize_seeds", default)]
    pub seeds: Option<Vec<i64>>,
    pub static_params: Option<Value>,
    pub swim_fanout: Option<u32>,
    pub spof_kill_at_sec: Option<u32>,
    pub gateway_cpus: Option<String>,
    pub workload: Option<WorkloadConfig>,
    pub env: Option<BTreeMap<String, String>>,
    pub network: Option<String>,
    pub actions: Option<Vec<CellAction>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Experiment {
    pub name: Option<String>,
    pub description: Option<String>,
    pub topology: Option<String>,
    pub topologies: Option<Vec<String>>,
    pub strategy: Option<String>,
    pub strategies: Option<Vec<String>>,
    pub fault_profile: Option<String>,
    #[serde(alias = "faults")]
    pub fault_profiles: Option<Vec<String>>,
    #[serde(alias = "bases")]
    pub fault_bases: Option<Vec<String>>,
    #[serde(deserialize_with = "deserialize_seeds", default)]
    pub seeds: Option<Vec<i64>>,
    pub repetitions: Option<u32>,
    pub seed_base: Option<i64>,
    pub workload: Option<WorkloadConfig>,
    pub resources: Option<ResourcesConfig>,
    pub variants: Option<Vec<VariantConfig>>,
    pub extra_cells: Option<Vec<ExtraCellConfig>>,
    #[serde(alias = "impl")]
    pub impl_type: Option<String>,
    pub results_dir: Option<String>,
    pub workers: Option<usize>,
    pub project_prefix: Option<String>,
    pub spof_kill_at_sec: Option<u32>,
    pub static_params: Option<Value>,
    pub swim_fanout: Option<u32>,
    pub backend: Option<String>,
    pub scale: Option<u32>,
    pub env: Option<BTreeMap<String, String>>,
    pub network: Option<String>,
    pub actions: Option<Vec<CellAction>>,
    pub health_timeout_ms: Option<u64>,
}

/// `"1000, 2000"` -> seeds; any entry that is not an integer is an error (never silently dropped).
pub fn parse_seeds(list: &str) -> Result<Vec<i64>, String> {
    list.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<i64>().map_err(|_| format!("seed is not an integer: '{p}'")))
        .collect()
}

fn deserialize_seeds<'de, D>(deserializer: D) -> Result<Option<Vec<i64>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    match Option::<Value>::deserialize(deserializer)? {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => parse_seeds(&s).map(Some).map_err(D::Error::custom),
        Some(Value::Number(n)) => n.as_i64().map(|v| Some(vec![v])).ok_or_else(|| D::Error::custom("seed is not an integer")),
        Some(Value::Array(arr)) => arr
            .iter()
            .map(|v| match v {
                Value::String(s) => parse_seeds(s).map_err(D::Error::custom).and_then(|mut x| {
                    if x.len() == 1 { Ok(x.remove(0)) } else { Err(D::Error::custom(format!("bad seed '{s}'"))) }
                }),
                other => other.as_i64().ok_or_else(|| D::Error::custom(format!("seed is not an integer: {other}"))),
            })
            .collect::<Result<_, _>>()
            .map(Some),
        Some(other) => Err(D::Error::custom(format!("seeds must be a list or a comma-separated string, got {other}"))),
    }
}

/// Strategy parameters as the env value: a string as is, an object as compact JSON
/// (`{"retryCount":3,...}`, the form the old scripts passed in `STATIC_PARAMS`).
pub fn param_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

impl Experiment {
    pub fn backend(&self) -> &str {
        self.backend.as_deref().unwrap_or("compose")
    }

    pub fn scale(&self) -> Option<u32> {
        self.scale
    }

    pub fn health_timeout_ms(&self) -> u64 {
        self.health_timeout_ms
            .or_else(|| self.workload.as_ref().and_then(|w| w.health_timeout_ms))
            .unwrap_or(300_000)
    }

    pub fn seeds(&self) -> Vec<i64> {
        if let Some(s) = &self.seeds {
            if !s.is_empty() {
                return s.clone();
            }
        }
        if let (Some(rep), Some(base)) = (self.repetitions, self.seed_base) {
            return (1..=rep).map(|i| (i as i64) * base).collect();
        }
        vec![]
    }

    pub fn topologies(&self) -> Vec<String> {
        if let Some(topos) = &self.topologies {
            if !topos.is_empty() {
                return topos.clone();
            }
        }
        if let Some(topo) = &self.topology {
            if !topo.is_empty() {
                return vec![topo.clone()];
            }
        }
        vec![]
    }

    pub fn strategies(&self) -> Vec<String> {
        if let Some(s) = &self.strategies {
            if !s.is_empty() {
                return s.clone();
            }
        }
        if let Some(s) = &self.strategy {
            if !s.is_empty() {
                return vec![s.clone()];
            }
        }
        vec![]
    }

    pub fn fault_profiles(&self) -> Vec<String> {
        if let Some(f) = &self.fault_profiles {
            if !f.is_empty() {
                return f.clone();
            }
        }
        if let Some(f) = &self.fault_profile {
            if !f.is_empty() {
                return vec![f.clone()];
            }
        }
        vec![]
    }

    pub fn fault_bases(&self) -> Vec<String> {
        if let Some(b) = &self.fault_bases {
            if !b.is_empty() {
                return b.clone();
            }
        }
        vec!["no-wan".to_string()]
    }

    pub fn results_dir(&self) -> &str {
        self.results_dir.as_deref().unwrap_or("./results")
    }

    pub fn workers(&self) -> usize {
        self.workers.unwrap_or(1).max(1)
    }

    pub fn project_prefix(&self) -> &str {
        self.project_prefix.as_deref().unwrap_or("w")
    }

    pub fn impl_type(&self) -> &str {
        self.impl_type.as_deref().unwrap_or("rust")
    }

    pub fn workload(&self) -> WorkloadConfig {
        self.workload.clone().unwrap_or_default()
    }

    pub fn total_experiments(&self, cells: &[Cell]) -> usize {
        if self.backend() == "swarm" {
            cells.len()
        } else {
            cells.len() * self.seeds().len()
        }
    }

    pub fn from_file(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("{}: {e}", path.display()))
    }
}
