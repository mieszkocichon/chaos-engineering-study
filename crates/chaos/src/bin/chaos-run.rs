//! `chaos-run <experiment.json> [flags]` – executes an experiment defined in a JSON file.

use std::path::PathBuf;

use chaos::experiment::schema::parse_seeds;
use chaos::experiment::{self, RunOptions};

const USAGE: &str = "usage: chaos-run <experiment.json> [options]

Options:
  --dry-run               Print planned cells and experiment count without running Docker
  --only <pattern>        Only cells whose label or topology contains the pattern
  --resume                Skip cells whose result files already exist in the results dir
  --workers <n>           Parallel workers (default: file, env WORKERS)
  --impl <rust|js>        Implementation (default: file, env IMPL)
  --results-dir <path>    Results directory (default: file, env RESULTS_DIR)
  --topology <name>       Topology override
  --seeds <s1,s2,...>     Seeds override (env SEEDS)
  --project-prefix <p>    Compose project prefix (default: file, env PROJECT_PREFIX)
  --gateway-cpus <n>      Gateway CPU limit (default: file, env GATEWAY_CPUS)
  --config-dir <path>     Config directory (default: env CONFIG_DIR, ./config, when-gossip-goes-silent/config)
  -h, --help              Show this help";

fn die(msg: &str) -> ! {
    eprintln!("[chaos-run] ERROR: {msg}");
    std::process::exit(1)
}

fn usage_error(msg: &str) -> ! {
    eprintln!("{msg}\n\n{USAGE}");
    std::process::exit(2)
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut file: Option<PathBuf> = None;
    let mut config_dir: Option<PathBuf> = None;
    let mut o = RunOptions::default();
    let mut workers: Option<String> = None;
    let mut seeds: Option<String> = None;

    while let Some(arg) = args.next() {
        let mut value = |flag: &str| args.next().unwrap_or_else(|| usage_error(&format!("{flag} needs a value")));
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            "--dry-run" => o.dry_run = true,
            "--resume" => o.resume = true,
            "--only" => o.only = Some(value("--only")),
            "--workers" => workers = Some(value("--workers")),
            "--impl" => o.impl_type = Some(value("--impl")),
            "--results-dir" => o.results_dir = Some(value("--results-dir")),
            "--topology" => o.topology = Some(value("--topology")),
            "--seeds" => seeds = Some(value("--seeds")),
            "--project-prefix" => o.project_prefix = Some(value("--project-prefix")),
            "--gateway-cpus" => o.gateway_cpus = Some(value("--gateway-cpus")),
            "--config-dir" => config_dir = Some(value("--config-dir").into()),
            flag if flag.starts_with('-') => usage_error(&format!("unknown option: {flag}")),
            path if file.is_none() => file = Some(path.into()),
            extra => usage_error(&format!("unexpected extra argument: {extra}")),
        }
    }
    let Some(file) = file else { usage_error("missing experiment file") };

    // flag > environment (the old scripts' variables) > experiment file
    o.impl_type = o.impl_type.or_else(|| env("IMPL"));
    o.results_dir = o.results_dir.or_else(|| env("RESULTS_DIR"));
    o.project_prefix = o.project_prefix.or_else(|| env("PROJECT_PREFIX"));
    o.gateway_cpus = o.gateway_cpus.or_else(|| env("GATEWAY_CPUS"));
    if let Some(w) = workers.or_else(|| env("WORKERS")) {
        o.workers = Some(w.parse().unwrap_or_else(|_| die(&format!("workers is not a number: '{w}'"))));
    }
    if let Some(s) = seeds.or_else(|| env("SEEDS")) {
        o.seeds = Some(parse_seeds(&s).unwrap_or_else(|e| die(&e)));
    }

    let config_dir = config_dir
        .or_else(|| env("CONFIG_DIR").map(PathBuf::from))
        .or_else(|| {
            ["config", "when-gossip-goes-silent/config", "/app/config"]
                .into_iter()
                .map(PathBuf::from)
                .find(|p| p.join("topologies").exists())
        })
        .unwrap_or_else(|| PathBuf::from("config"));

    let exp = experiment::Experiment::from_file(&file).unwrap_or_else(|e| die(&e));
    if let Err(e) = experiment::run_experiment(exp, &config_dir, &o) {
        die(&e);
    }
}
