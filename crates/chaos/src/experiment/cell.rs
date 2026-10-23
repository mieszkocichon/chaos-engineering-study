use std::collections::BTreeMap;
use std::path::Path;

use crate::experiment::schema::{param_str, CellAction, Experiment};
use crate::generate::ComposeOpts;

/// One cluster lifecycle: all seeds of one (topology, strategy, base, fault, partition) combination,
/// or a single-seed deployment in Swarm mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub topology: String,
    pub strategy: String,
    pub base: String,
    pub fault: String,
    pub partition: String,
    /// Variant label; result files of the cell get `__<label>__` appended (same as the old scripts),
    /// because variants share strategy/fault and would otherwise overwrite each other.
    pub variant: Option<String>,
    pub static_params: Option<String>,
    pub swim_fanout: Option<String>,
    pub spof_kill_at_sec: Option<String>,
    pub gateway_cpus: Option<String>,
    pub rps: Option<u32>,
    pub duration_sec: Option<u32>,
    pub warmup_sec: Option<u32>,
    pub backend: String,
    pub scale: Option<u32>,
    pub seeds: Vec<i64>,
    pub env: BTreeMap<String, String>,
    pub network: Option<String>,
    pub actions: Vec<CellAction>,
    pub health_timeout_ms: Option<u64>,
}

impl Cell {
    pub fn label(&self) -> String {
        let mut l = format!(
            "Strategy={} | Fault={} | Base={} | Partition={}",
            self.strategy,
            self.fault,
            self.base,
            if self.partition.is_empty() { "none" } else { &self.partition }
        );
        if let Some(v) = &self.variant {
            l.push_str(&format!(" | Variant={v}"));
        }
        if self.backend == "swarm" {
            if let Some(scale) = self.scale {
                l.push_str(&format!(" | Scale={scale}"));
            }
            if let Some(seed) = self.seeds.first() {
                l.push_str(&format!(" | Seed={seed}"));
            }
        }
        for a in &self.actions {
            l.push_str(&format!(" | Action={} on {} at {}s", a.kind, a.target, a.at_sec));
        }
        l
    }

    /// Prefix of this cell's result files (`<topology>_<strategy>_<base>_<fault>[+partition]_`).
    pub fn file_prefix(&self) -> String {
        let part = if self.partition.is_empty() { String::new() } else { format!("+{}", self.partition) };
        format!("{}_{}_{}_{}{part}_", self.topology, self.strategy, self.base, self.fault)
    }

    /// The single place where a cell becomes generator input (used by the runner and by the tests).
    pub fn compose_opts<'a>(
        &'a self,
        seeds: &'a str,
        project: &'a str,
        image: String,
        results_dir_host: String,
        config_dir_host: String,
    ) -> ComposeOpts<'a> {
        ComposeOpts {
            topology: &self.topology,
            fault: &self.fault,
            strategy: &self.strategy,
            seeds,
            fault_base: &self.base,
            fault_partition: &self.partition,
            project,
            image,
            results_dir_host,
            results_dir_container: "/results".into(),
            static_params: self.static_params.clone(),
            swim_fanout: self.swim_fanout.clone(),
            spof_kill_at_sec: self.spof_kill_at_sec.clone(),
            gateway_cpus: self.gateway_cpus.clone(),
            rps: self.rps,
            duration_sec: self.duration_sec,
            warmup_sec: self.warmup_sec,
            config_dir_host: Some(config_dir_host),
            extra_env: if self.env.is_empty() { None } else { Some(self.env.clone()) },
            network_driver: self.network.clone(),
        }
    }
}

/// Matrix = cartesian product (strategies x bases x faults) + variants + `extraCells`, per topology.
pub fn expand_cells(exp: &Experiment) -> Vec<Cell> {
    let bases = exp.fault_bases();
    let faults = exp.fault_profiles();
    let w = exp.workload();
    let backend = exp.backend().to_string();
    let exp_seeds = exp.seeds();

    let global = |topology: &str, strategy: &str, base: &str, fault: &str, cell_seeds: Vec<i64>| Cell {
        topology: topology.into(),
        strategy: strategy.into(),
        base: base.into(),
        fault: fault.into(),
        partition: String::new(),
        variant: None,
        static_params: exp.static_params.as_ref().map(param_str),
        swim_fanout: exp.swim_fanout.map(|f| f.to_string()),
        spof_kill_at_sec: exp.spof_kill_at_sec.map(|s| s.to_string()),
        gateway_cpus: exp.resources.as_ref().and_then(|r| r.gateway_cpus.clone()),
        rps: w.rps,
        duration_sec: w.duration_sec,
        warmup_sec: w.warmup_sec,
        backend: backend.clone(),
        scale: exp.scale,
        seeds: cell_seeds,
        env: exp.env.clone().unwrap_or_default(),
        network: exp.network.clone(),
        actions: exp.actions.clone().unwrap_or_default(),
        health_timeout_ms: exp.health_timeout_ms.or(w.health_timeout_ms),
    };

    let mut cells = Vec::new();

    if backend == "swarm" {
        // Swarm: 1 seed per deployment (komórka × seed = jedno wdrożenie)
        let scale = exp.scale.unwrap_or(20);
        let default_topo = format!("scale-{scale}");
        let topos = if exp.topologies().is_empty() {
            vec![default_topo]
        } else {
            exp.topologies()
        };
        for topo in &topos {
            for strat in &exp.strategies() {
                for fault in &faults {
                    for &seed in &exp_seeds {
                        let mut c = global(topo, strat, "no-wan", fault, vec![seed]);
                        c.scale = Some(scale);
                        if c.rps.is_none() { c.rps = Some(50); }
                        if c.duration_sec.is_none() { c.duration_sec = Some(60); }
                        if c.warmup_sec.is_none() { c.warmup_sec = Some(10); }
                        if c.health_timeout_ms.is_none() { c.health_timeout_ms = Some(540_000); }
                        cells.push(c);
                    }
                }
            }
        }
        return cells;
    }

    for topo in &exp.topologies() {
        for strat in &exp.strategies() {
            for base in &bases {
                for fault in &faults {
                    cells.push(global(topo, strat, base, fault, exp_seeds.clone()));
                }
            }
        }
        for v in exp.variants.iter().flatten() {
            for base in &bases {
                for fault in &faults {
                    let mut c = global(topo, &v.strategy, base, fault, exp_seeds.clone());
                    c.variant = Some(v.name.clone());
                    c.static_params = v.static_params.as_ref().map(param_str).or(c.static_params);
                    c.swim_fanout = v.swim_fanout.map(|f| f.to_string()).or(c.swim_fanout);
                    c.spof_kill_at_sec = v.spof_kill_at_sec.map(|s| s.to_string()).or(c.spof_kill_at_sec);
                    cells.push(c);
                }
            }
        }
        for e in exp.extra_cells.iter().flatten() {
            let e_seeds = e.seeds.clone().unwrap_or_else(|| exp_seeds.clone());
            let mut c = global(
                e.topology.as_deref().unwrap_or(topo),
                &e.strategy,
                e.base.as_deref().unwrap_or("no-wan"),
                e.fault.as_deref().unwrap_or("baseline-flaky"),
                e_seeds,
            );
            c.partition = e.partition.clone().unwrap_or_default();
            c.static_params = e.static_params.as_ref().map(param_str).or(c.static_params);
            c.swim_fanout = e.swim_fanout.map(|f| f.to_string()).or(c.swim_fanout);
            c.spof_kill_at_sec = e.spof_kill_at_sec.map(|s| s.to_string()).or(c.spof_kill_at_sec);
            c.gateway_cpus = e.gateway_cpus.clone().or(c.gateway_cpus);
            if let Some(ew) = &e.workload {
                if ew.rps.is_some() { c.rps = ew.rps; }
                if ew.duration_sec.is_some() { c.duration_sec = ew.duration_sec; }
                if ew.warmup_sec.is_some() { c.warmup_sec = ew.warmup_sec; }
                if ew.health_timeout_ms.is_some() { c.health_timeout_ms = ew.health_timeout_ms; }
            }
            if let Some(ee) = &e.env {
                c.env.extend(ee.clone());
            }
            if e.network.is_some() {
                c.network = e.network.clone();
            }
            if let Some(ea) = &e.actions {
                c.actions = ea.clone();
            }
            cells.push(c);
        }
    }
    cells
}

/// Result files of `cell` written by the coordinator: `<prefix><seed>_<timestamp>[__<variant>__].json`.
fn result_files<'a>(cell: &Cell, names: &'a [String]) -> impl Iterator<Item = &'a String> {
    let prefix = cell.file_prefix();
    let suffix = cell.variant.as_ref().map(|v| format!("__{v}__.json"));
    names.iter().filter(move |f| {
        f.starts_with(&prefix)
            && match &suffix {
                Some(s) => f.ends_with(s.as_str()),
                None => f.ends_with(".json") && !f.ends_with("__.json"),
            }
    })
}

fn file_names(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect())
        .unwrap_or_default()
}

/// True when `results_dir` holds a result file for every seed of the cell (for `--resume` and the post-run check).
pub fn is_cell_complete(cell: &Cell, seeds: &[i64], results_dir: &Path) -> bool {
    let names = file_names(results_dir);
    let check_seeds = if !cell.seeds.is_empty() { &cell.seeds } else { seeds };
    !check_seeds.is_empty()
        && check_seeds.iter().all(|seed| {
            let p = format!("{}{seed}_", cell.file_prefix());
            result_files(cell, &names).any(|f| f.starts_with(&p))
        })
}

/// Old scripts' post-processing of variants: tag result files that are not yet tagged with `__<label>__`.
pub fn tag_variant_results(cell: &Cell, results_dir: &Path) -> Result<(), String> {
    let Some(label) = &cell.variant else { return Ok(()) };
    let prefix = cell.file_prefix();
    for f in file_names(results_dir) {
        if f.starts_with(&prefix) && f.ends_with(".json") && !f.contains("__") {
            let to = format!("{}__{label}__.json", f.trim_end_matches(".json"));
            std::fs::rename(results_dir.join(&f), results_dir.join(&to)).map_err(|e| format!("{f}: {e}"))?;
        }
    }
    Ok(())
}
