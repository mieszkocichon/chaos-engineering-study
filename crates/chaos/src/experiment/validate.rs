use std::path::Path;

use crate::strategy::STRATEGY_NAMES;
use crate::experiment::action::find_action_kind;
use crate::experiment::backend::find_backend;
use crate::experiment::schema::Experiment;

pub fn validate_experiment(exp: &Experiment, config_dir: &Path) -> Vec<String> {
    let mut errors = Vec::new();

    if find_backend(exp.backend()).is_none() {
        errors.push(format!("unknown backend: '{}'", exp.backend()));
    }
    // Swarm runs on the topology `scale-<N>` (the stack is generated from `scale`, the coordinator
    // reads config/topologies/scale-<N>.json): the file must exist and `topology` must agree.
    let mut topos = exp.topologies();
    if exp.backend() == "swarm" {
        let scale = exp.scale.unwrap_or(20);
        if scale < 5 {
            errors.push("scale must be >= 5".into());
        }
        let expected = format!("scale-{scale}");
        if topos.is_empty() {
            topos.push(expected);
        } else if topos.iter().any(|t| *t != expected) {
            errors.push(format!("swarm topology must be '{expected}' for scale {scale}, got {}", topos.join(", ")));
        }
    }

    if topos.is_empty() {
        errors.push("experiment has no topologies defined".into());
    }
    let extra_topos = exp.extra_cells.iter().flatten().filter_map(|e| e.topology.clone());
    for topo in topos.iter().cloned().chain(extra_topos) {
        let topo_path = config_dir.join("topologies").join(format!("{topo}.json"));
        if !topo_path.exists() {
            errors.push(format!("topology file not found: {}", topo_path.display()));
        }
    }

    let mut strats = exp.strategies();
    if let Some(variants) = &exp.variants {
        for v in variants {
            strats.push(v.strategy.clone());
        }
    }
    if let Some(extra) = &exp.extra_cells {
        for e in extra {
            strats.push(e.strategy.clone());
        }
    }

    if strats.is_empty() {
        errors.push("experiment has no strategies defined".into());
    }
    for s in &strats {
        if !STRATEGY_NAMES.contains(&s.as_str()) {
            errors.push(format!(
                "unknown strategy: '{s}' (supported: {})",
                STRATEGY_NAMES.join(", ")
            ));
        }
    }

    let faults = exp.fault_profiles();
    if faults.is_empty() && exp.extra_cells.is_none() {
        errors.push("experiment has no fault profiles defined".into());
    }
    let mut check_fault = |kind: &str, name: &str| {
        let on_disk = config_dir.join("faults").join(format!("{name}.json")).exists();
        if !on_disk && crate::faults::find_fault_kind(name).is_none() {
            errors.push(format!("unknown fault {kind}: '{name}'"));
        }
    };
    for f in &faults {
        check_fault("profile", f);
    }
    for b in exp.fault_bases() {
        if b != "no-wan" {
            check_fault("base", &b);
        }
    }
    for e in exp.extra_cells.iter().flatten() {
        if let Some(f) = &e.fault {
            check_fault("profile", f);
        }
        if let Some(b) = e.base.as_deref().filter(|b| *b != "no-wan") {
            check_fault("base", b);
        }
        if let Some(p) = e.partition.as_deref().filter(|p| !p.is_empty()) {
            check_fault("partition", p);
        }
    }

    if exp.seeds().is_empty() {
        errors.push("experiment has no seeds (specify 'seeds' list or 'seedBase' + 'repetitions')".into());
    }

    if let Some(w) = &exp.workload {
        if let Some(rps) = w.rps {
            if rps == 0 {
                errors.push("workload.rps must be > 0".into());
            }
        }
        if let Some(dur) = w.duration_sec {
            if dur == 0 {
                errors.push("workload.durationSec must be > 0".into());
            }
        }
    }

    if let Some(w) = exp.workers {
        if w == 0 {
            errors.push("workers must be >= 1".into());
        }
    }

    let mut check_actions = |actions: &[crate::experiment::schema::CellAction]| {
        for a in actions {
            if find_action_kind(&a.kind).is_none() {
                errors.push(format!("unknown action kind: '{}'", a.kind));
            }
            if a.target.is_empty() {
                errors.push("action target cannot be empty".into());
            }
            if a.target == "coordinator" {
                errors.push("action on 'coordinator': it writes the result files, a run without it has no result".into());
            }
        }
    };
    if let Some(actions) = &exp.actions {
        check_actions(actions);
    }
    for e in exp.extra_cells.iter().flatten() {
        if let Some(actions) = &e.actions {
            check_actions(actions);
        }
    }

    errors
}

/// Fault profiles name their targets (`targetService`); the coordinator silently skips a target that
/// is not in the topology. A profile whose targets are all missing injects nothing, so the cell
/// measures a healthy cluster - worth a warning before hours of runs (e.g. `svc-01` vs `scale-20`'s `svc-1`).
pub fn fault_target_warnings(cells: &[crate::experiment::cell::Cell], config_dir: &Path) -> Vec<String> {
    let read = |path: std::path::PathBuf| -> Option<serde_json::Value> {
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut out = vec![];
    for c in cells {
        let profiles = [&c.fault, &c.base, &c.partition];
        for profile in profiles.into_iter().filter(|p| !p.is_empty() && p.as_str() != "no-wan") {
            if !seen.insert((c.topology.clone(), profile.clone())) {
                continue;
            }
            let (Some(topo), Some(prof)) = (
                read(config_dir.join("topologies").join(format!("{}.json", c.topology))),
                read(config_dir.join("faults").join(format!("{profile}.json"))),
            ) else {
                continue;
            };
            let ids: Vec<&str> = topo["services"].as_array().into_iter().flatten().filter_map(|s| s["id"].as_str()).collect();
            let targets: Vec<&str> = prof["faults"].as_array().into_iter().flatten().filter_map(|f| f["targetService"].as_str()).collect();
            if !targets.is_empty() && !targets.iter().any(|t| ids.contains(t)) {
                out.push(format!(
                    "fault '{profile}' targets {} - none exist in topology '{}': no fault will be injected",
                    targets.join(", "),
                    c.topology
                ));
            }
        }
    }
    out
}
