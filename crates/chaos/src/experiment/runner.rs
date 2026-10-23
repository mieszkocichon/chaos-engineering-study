use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::coordinator::iso8601;
use crate::experiment::action::find_action_kind;
use crate::experiment::backend::{find_backend, BackendPlan, ClusterBackend, PrepareContext, RunStatus};
use crate::experiment::cell::{expand_cells, is_cell_complete, tag_variant_results, Cell};
use crate::experiment::lock::LockGuard;
use crate::experiment::schema::{Experiment, SpofTestMeta};
use crate::experiment::validate::{fault_target_warnings, validate_experiment};
use crate::generate;
use crate::now_ms;

const WAIT_TIMEOUT: Duration = Duration::from_secs(5400); // 90 min per cell, like the old scripts

#[derive(Clone, Debug, Default)]
pub struct RunOptions {
    pub dry_run: bool,
    pub only: Option<String>,
    pub resume: bool,
    pub workers: Option<usize>,
    pub impl_type: Option<String>,
    pub results_dir: Option<String>,
    pub topology: Option<String>,
    pub seeds: Option<Vec<i64>>,
    pub project_prefix: Option<String>,
    pub gateway_cpus: Option<String>,
}

fn get_git_commit() -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

/// Path as `docker compose` on the host sees it (bind mounts). Inside `scripts/dev.sh` the
/// repo is `/app`; `HOST_APP_PATH` (or `docker inspect $HOSTNAME`) gives its host location.
pub fn resolve_host_path(path: &Path) -> String {
    let host_app = std::env::var("HOST_APP_PATH").ok().or_else(|| {
        let hostname = std::env::var("HOSTNAME").ok()?;
        let out = Command::new("docker")
            .args([
                "inspect",
                &hostname,
                "--format",
                "{{range .Mounts}}{{if eq .Destination \"/app\"}}{{.Source}}{{end}}{{end}}",
            ])
            .output()
            .ok()?;
        let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
        (!s.is_empty()).then_some(s)
    });

    if let Some(host_root) = host_app {
        let host_root = host_root.trim_end_matches('/').replace('\\', "/");
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().map(|c| c.join(path)).unwrap_or_else(|_| path.to_path_buf())
        };
        let abs_str = abs.to_string_lossy().replace('\\', "/");
        if let Some(rel) = abs_str.strip_prefix("/app/") {
            return format!("{host_root}/{rel}");
        } else if abs_str == "/app" {
            return host_root;
        }
    }
    let p_str = path.to_string_lossy().replace('\\', "/");
    if p_str.starts_with('/') || p_str.starts_with("./") || p_str.starts_with("../") || p_str.contains(':') {
        p_str
    } else {
        format!("./{p_str}")
    }
}

/// CLI overrides win over the experiment file.
pub fn apply_overrides(exp: &mut Experiment, opts: &RunOptions) {
    if let Some(topo) = &opts.topology {
        exp.topology = Some(topo.clone());
        exp.topologies = Some(vec![topo.clone()]);
    }
    if let Some(seeds) = &opts.seeds {
        exp.seeds = Some(seeds.clone());
    }
    if let Some(dir) = &opts.results_dir {
        exp.results_dir = Some(dir.clone());
    }
    if let Some(workers) = opts.workers {
        exp.workers = Some(workers);
    }
    if let Some(impl_type) = &opts.impl_type {
        exp.impl_type = Some(impl_type.clone());
    }
    if let Some(prefix) = &opts.project_prefix {
        exp.project_prefix = Some(prefix.clone());
    }
    if let Some(cpus) = &opts.gateway_cpus {
        exp.resources.get_or_insert_with(Default::default).gateway_cpus = Some(cpus.clone());
    }
}

/// Everything one worker needs to run cells.
#[derive(Clone)]
pub struct CellRunner {
    pub project: String,
    pub image: String,
    pub config_dir: PathBuf,
    pub config_dir_host: String,
    pub results_path: PathBuf,
    pub results_dir_host: String,
    pub seeds: Vec<i64>,
    pub seeds_str: String,
}

impl CellRunner {
    pub fn run_with_backend(&self, cell: &Cell, backend: &dyn ClusterBackend) -> Result<(), String> {
        let effective_seeds = if !cell.seeds.is_empty() { &cell.seeds } else { &self.seeds };
        let effective_seeds_str = effective_seeds.iter().map(ToString::to_string).collect::<Vec<_>>().join(",");

        let ctx = PrepareContext {
            cell,
            seeds_str: &effective_seeds_str,
            project: &self.project,
            image: &self.image,
            results_dir_host: &self.results_dir_host,
            results_path: &self.results_path,
            config_dir: &self.config_dir,
            config_dir_host: &self.config_dir_host,
        };

        let plan = backend.prepare(&ctx)?;
        let result = self.run_cluster(&plan, cell, backend, effective_seeds);
        let _ = backend.teardown(&plan);
        result
    }

    pub fn run(&self, cell: &Cell) -> Result<(), String> {
        let backend = find_backend(&cell.backend)
            .ok_or_else(|| format!("unknown backend: '{}'", cell.backend))?;
        self.run_with_backend(cell, backend)
    }

    fn run_cluster(
        &self,
        plan: &BackendPlan,
        cell: &Cell,
        backend: &dyn ClusterBackend,
        seeds: &[i64],
    ) -> Result<(), String> {
        backend.start(plan)?;

        let started = Instant::now();
        let mut executed_actions = vec![false; cell.actions.len()];

        loop {
            // Check and execute pending mid-run actions
            for (idx, action) in cell.actions.iter().enumerate() {
                if !executed_actions[idx] && started.elapsed().as_secs() >= action.at_sec {
                    let action_kind = find_action_kind(&action.kind)
                        .ok_or_else(|| format!("unknown action kind: '{}'", action.kind))?;
                    println!(
                        "[chaos-run] [{}] [t={}s] Executing action '{}' on '{}'",
                        self.project, action.at_sec, action.kind, action.target
                    );
                    action_kind
                        .execute(backend, plan, &action.target)
                        .map_err(|e| format!("action '{}' on '{}' failed at t={}s: {e}", action.kind, action.target, action.at_sec))?;
                    executed_actions[idx] = true;
                }
            }

            let status = backend.check_status(plan, started)?;
            match status {
                RunStatus::Exited(code) => {
                    if code != 0 {
                        let tail = backend.logs_tail(plan, 30);
                        return Err(format!("coordinator exited with code {code}:\n{tail}"));
                    }
                    break;
                }
                RunStatus::Disappeared => {
                    return Err(format!("coordinator disappeared (OOM?) after {}s", started.elapsed().as_secs()));
                }
                RunStatus::Running(_) => {}
            }

            if started.elapsed() > WAIT_TIMEOUT {
                return Err(format!("timeout after {}s", started.elapsed().as_secs()));
            }

            std::thread::sleep(Duration::from_millis(500));
        }

        // an action that never fired means the run did not test what the file says
        if let Some(idx) = executed_actions.iter().position(|done| !done) {
            let a = &cell.actions[idx];
            return Err(format!(
                "action '{}' on '{}' was not executed: the run ended after {}s, before atSec={}",
                a.kind,
                a.target,
                started.elapsed().as_secs(),
                a.at_sec
            ));
        }

        tag_variant_results(cell, &self.results_path)?;
        attach_action_metadata(cell, &self.results_path, seeds)?;

        if is_cell_complete(cell, seeds, &self.results_path) {
            Ok(())
        } else {
            Err(format!("coordinator finished but result files are missing in {}", self.results_path.display()))
        }
    }
}

/// Attach `spofTest` metadata directly into the result file of the matching cell and seed.
fn attach_action_metadata(cell: &Cell, results_path: &Path, seeds: &[i64]) -> Result<(), String> {
    for action in &cell.actions {
        let meta = match &action.metadata {
            Some(m) => m.clone(),
            None if action.kind == "stop-service" => SpofTestMeta {
                killed_service: action.target.clone(),
                kill_at_sec: action.at_sec,
                permanent: true,
                phase: None,
                note: None,
            },
            None => continue,
        };
        for &seed in seeds {
            let p = format!("{}{seed}_", cell.file_prefix());
            let entries = match std::fs::read_dir(results_path) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for e in entries.flatten() {
                let fname = e.file_name().to_string_lossy().to_string();
                if fname.starts_with(&p) && fname.ends_with(".json") {
                    let full_path = e.path();
                    let content = std::fs::read_to_string(&full_path).map_err(|e| format!("{}: {e}", full_path.display()))?;
                    if let Ok(mut json) = serde_json::from_str::<Value>(&content) {
                        if let Some(obj) = json.as_object_mut() {
                            obj.insert("spofTest".to_string(), serde_json::to_value(&meta).map_err(|e| e.to_string())?);
                            let updated = serde_json::to_string_pretty(&obj).map_err(|e| e.to_string())?;
                            std::fs::write(&full_path, updated).map_err(|e| format!("{}: {e}", full_path.display()))?;
                            println!("[spof] Attached spofTest metadata to {}", fname);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

pub fn run_experiment(mut exp: Experiment, config_dir: &Path, opts: &RunOptions) -> Result<(), String> {
    apply_overrides(&mut exp, opts);

    let errors = validate_experiment(&exp, config_dir);
    if !errors.is_empty() {
        return Err(format!("experiment validation failed:\n  - {}", errors.join("\n  - ")));
    }

    let seeds = exp.seeds();
    let seed_count = seeds.len();
    let seeds_str = seeds.iter().map(ToString::to_string).collect::<Vec<_>>().join(",");
    let results_path = PathBuf::from(exp.results_dir());

    let mut cells = expand_cells(&exp);
    if let Some(pat) = &opts.only {
        cells.retain(|c| c.label().contains(pat) || c.topology.contains(pat));
    }
    let initial_total = cells.len();
    if opts.resume {
        cells.retain(|c| !is_cell_complete(c, &seeds, &results_path));
    }
    let total_cells = cells.len();
    let total_exp_count = exp.total_experiments(&cells);

    println!("=== CHAOS EXPERIMENT RUNNER ===");
    if let Some(name) = &exp.name {
        println!("Experiment:   {name}");
    }
    println!("Backend:      {}", exp.backend());
    println!("Topology:     {}", exp.topologies().join(", "));
    println!("Seeds:        {seeds_str} ({seed_count} per cell)");
    println!("Cells:        {total_cells} (remaining from {initial_total})");
    println!("Deployments:  {total_exp_count}");
    println!("Results dir:  {}", exp.results_dir());
    println!("Workers:      {}", exp.workers());
    println!("Impl:         {}", exp.impl_type());
    println!("===============================");
    for w in fault_target_warnings(&cells, config_dir) {
        eprintln!("[chaos-run] WARNING: {w}");
    }

    if opts.dry_run {
        for (i, c) in cells.iter().enumerate() {
            println!("  [{}/{total_cells}] {}", i + 1, c.label());
        }
        return Ok(());
    }
    if cells.is_empty() {
        println!("[chaos-run] No cells to run (all cells already completed or filtered).");
        return Ok(());
    }

    std::fs::create_dir_all(&results_path).map_err(|e| format!("{}: {e}", results_path.display()))?;
    let lock_dir = config_dir.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let _lock = LockGuard::acquire(lock_dir, exp.project_prefix())?;

    let image = std::env::var("IMAGE").unwrap_or_else(|_| {
        if exp.impl_type() == "js" { "chaos-experiment:latest" } else { generate::IMAGE }.to_string()
    });
    write_manifest(&exp, &results_path, &image)?;

    let queue = Arc::new(Mutex::new(cells));
    let failures = Arc::new(Mutex::new(Vec::<String>::new()));
    let finished = Arc::new(AtomicUsize::new(0));
    let started = Instant::now();
    let handles: Vec<_> = (1..=exp.workers())
        .map(|worker_id| {
            let project = format!("{}{worker_id}", exp.project_prefix());
            let runner = CellRunner {
                project,
                image: image.clone(),
                config_dir: config_dir.to_path_buf(),
                config_dir_host: resolve_host_path(config_dir),
                results_path: results_path.clone(),
                results_dir_host: resolve_host_path(&results_path),
                seeds: seeds.clone(),
                seeds_str: seeds_str.clone(),
            };
            let (queue, failures, finished) = (queue.clone(), failures.clone(), finished.clone());
            std::thread::spawn(move || loop {
                let Some(cell) = ({
                    let mut q = queue.lock().unwrap();
                    if q.is_empty() { None } else { Some(q.remove(0)) }
                }) else {
                    break;
                };
                let outcome = runner.run(&cell);
                let done = finished.fetch_add(1, Ordering::SeqCst) + 1;
                match outcome {
                    Ok(()) => println!("[chaos-run] [W{worker_id}] [{done}/{total_cells}] OK: {}", cell.label()),
                    Err(e) => {
                        eprintln!("[chaos-run] [W{worker_id}] [{done}/{total_cells}] FAILED: {}\n  {e}", cell.label());
                        failures.lock().unwrap().push(cell.label());
                    }
                }
            })
        })
        .collect();
    for h in handles {
        h.join().map_err(|_| "worker thread panicked".to_string())?;
    }

    println!("\n=== RUN FINISHED in {}s | results: {} ===", started.elapsed().as_secs(), results_path.display());
    let failed = failures.lock().unwrap();
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("{} of {total_cells} cell(s) failed:\n  - {}", failed.len(), failed.join("\n  - ")))
    }
}

/// Reproducibility record next to the results (`<results>/manifests/`, so analyzers globbing
/// `<results>/*.json` never read it): effective experiment, commit, image, timestamp.
fn write_manifest(exp: &Experiment, results_path: &Path, image: &str) -> Result<(), String> {
    let dir = results_path.join("manifests");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let image_id = Command::new("docker")
        .args(["image", "inspect", "--format", "{{.Id}}", image])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    let manifest = json!({
        "experiment": exp,
        "gitCommit": get_git_commit(),
        "image": image,
        "imageId": image_id,
        "timestamp": iso8601(now_ms()),
    });
    let path = dir.join(format!("{}-{}.json", exp.name.as_deref().unwrap_or("experiment"), now_ms() as i64));
    std::fs::write(&path, serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{}: {e}", path.display()))
}
