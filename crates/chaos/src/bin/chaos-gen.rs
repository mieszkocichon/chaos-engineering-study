//! `chaos-gen <ring|ring-swarm|swarm|compose|k8s|mesh> [args…]` – same positional arguments, env vars and
//! output files as the JS generators (paths relative to the working directory).

use std::path::{Path, PathBuf};

use chaos::generate::{self, ComposeOpts};
use metastable::js::{js_parse_int, to_js_json_pretty};

fn arg(a: &[String], i: usize, d: &str) -> String {
    a.get(i).filter(|v| !v.is_empty()).cloned().unwrap_or_else(|| d.to_string())
}

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

fn int(s: &str) -> u32 {
    js_parse_int(s).filter(|v| *v >= 0.0).unwrap_or_else(|| die(&format!("not a number: {s}"))) as u32
}

fn die(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1)
}

fn config_dir() -> PathBuf {
    env("CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("config"))
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = argv.first() else {
        die("usage: chaos-gen <ring|ring-swarm|swarm|compose|k8s|mesh> [args…]");
    };
    let a = &argv[1..];
    let ok = |r: Result<(), String>| r.unwrap_or_else(|e| die(&e));
    match cmd.as_str() {
        "ring" => {
            println!("Generating configuration for 20 services in RING topology...");
            ok(generate::write(Path::new("docker-compose-scale.yml"), &generate::ring_scale()));
            println!("Done! File saved to: docker-compose-scale.yml");
        }
        "ring-swarm" => {
            // legacy repo-root generator: `parseInt(args[0]) || 20`, paths relative to the repo root
            let n = js_parse_int(&arg(a, 0, "")).filter(|v| *v != 0.0).unwrap_or(20.0) as u32;
            let (fault, strategy, seed) = (arg(a, 1, "cascade-crash"), arg(a, 2, "mas-agent"), arg(a, 3, "42"));
            println!("Generating SWARM configuration for {n} services in RING topology...");
            let (topo, compose) = generate::ring_swarm(n, &fault, &strategy, &seed);
            let topo_path = format!("when-gossip-goes-silent/config/topologies/ring-{n}.json");
            ok(generate::write(Path::new(&topo_path), &to_js_json_pretty(&topo)));
            println!("[generate] {topo_path}");
            ok(generate::write(Path::new("docker-compose-swarm.yml"), &compose));
            println!("Done! File saved to: docker-compose-swarm.yml");
        }
        "swarm" => {
            let (scale, fault, strategy, seed) = (int(&arg(a, 0, "20")), arg(a, 1, "baseline-flaky"), arg(a, 2, "none"), arg(a, 3, "42"));
            let (topo, compose) = generate::swarm(scale, &fault, &strategy, &seed, env("REGISTRY").as_deref()).unwrap_or_else(|e| die(&e));
            let name = topo["name"].as_str().unwrap();
            let topo_path = format!("config/topologies/{name}.json");
            ok(generate::write(Path::new(&topo_path), &to_js_json_pretty(&topo)));
            let n_svc = topo["services"].as_array().unwrap().len();
            let n_edges = topo["edges"].as_array().unwrap().len();
            println!("[generate] {topo_path}  ({n_svc} services, {n_edges} edges)");
            ok(generate::write(Path::new("docker-compose-swarm.yml"), &compose));
            println!("[generate] docker-compose-swarm.yml  (fault={fault} strategy={strategy})");
        }
        "compose" => {
            let topology = arg(a, 0, "ring-50");
            let topo_file = config_dir().join("topologies").join(format!("{topology}.json"));
            let topo = std::fs::read_to_string(&topo_file)
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_else(|| {
                    eprintln!("[generate] ERROR: cannot read topology file: {}", topo_file.display());
                    die("           Available topologies are in when-gossip-goes-silent/config/topologies/");
                });
            let (fault, strategy, seeds) = (arg(a, 1, "baseline-flaky"), arg(a, 2, "none"), arg(a, 3, "1000,2000,3000,4000,5000"));
            let (base, partition, project) = (arg(a, 4, "no-wan"), arg(a, 5, ""), arg(a, 6, "chaos"));
            let o = ComposeOpts {
                topology: &topology,
                fault: &fault,
                strategy: &strategy,
                seeds: &seeds,
                fault_base: &base,
                fault_partition: &partition,
                project: &project,
                image: env("IMAGE").unwrap_or_else(|| generate::IMAGE.into()),
                results_dir_host: env("RESULTS_DIR_HOST").unwrap_or_else(|| "./results".into()),
                results_dir_container: env("RESULTS_DIR_CONTAINER").unwrap_or_else(|| "/results".into()),
                static_params: env("STATIC_PARAMS"),
                swim_fanout: env("SWIM_FANOUT"),
                spof_kill_at_sec: env("SPOF_KILL_AT_SEC"),
                gateway_cpus: env("GATEWAY_CPUS"),
                rps: env("RPS").and_then(|s| s.parse().ok()),
                duration_sec: env("DURATION_SEC").and_then(|s| s.parse().ok()),
                warmup_sec: env("WARMUP_SEC").and_then(|s| s.parse().ok()),
                config_dir_host: None,
                extra_env: None,
                network_driver: None,
            };
            let host_dir = o.results_dir_host.strip_prefix("./").unwrap_or(&o.results_dir_host).to_string();
            std::fs::create_dir_all(&host_dir).unwrap_or_else(|e| die(&format!("{host_dir}: {e}")));
            let out = format!("docker-compose-local-{project}.yml");
            ok(generate::write(Path::new(&out), &generate::compose(&topo, &o)));
            println!(
                "[generate] {out}  (topology={topology} strategy={strategy} base={base} fault={fault} partition={} seeds={seeds})",
                if partition.is_empty() { "(none)" } else { &partition }
            );
        }
        "k8s" => {
            let (scale, fault, strategy) = (int(&arg(a, 0, "20")), arg(a, 1, "baseline-flaky"), arg(a, 2, "none"));
            let fault_json = std::fs::read_to_string(config_dir().join("faults").join(format!("{fault}.json"))).ok();
            if fault_json.is_none() {
                eprintln!("[generate] Warning: could not read config/faults/{fault}.json — using empty fault config");
            }
            let (topo, manifests) = generate::k8s(scale, &fault, &strategy, fault_json.as_deref()).unwrap_or_else(|e| die(&e));
            let name = topo["name"].as_str().unwrap();
            let topo_path = format!("config/topologies/{name}.json");
            ok(generate::write(Path::new(&topo_path), &to_js_json_pretty(&topo)));
            let n_svc = topo["services"].as_array().unwrap().len();
            let n_edges = topo["edges"].as_array().unwrap().len();
            println!("[generate] {topo_path}  ({n_svc} nodes, {n_edges} edges)");
            let path = format!("k8s/manifests-{fault}-{strategy}.yaml");
            ok(generate::write(Path::new(&path), &manifests));
            println!("[generate] {path}  (fault={fault} strategy={strategy})");
        }
        "mesh" => {
            let (n, k) = (int(&arg(a, 0, "50")), int(&arg(a, 1, "8")));
            let seed = js_parse_int(&arg(a, 2, "424242")).unwrap_or_else(|| die("bad seed"));
            let topo = generate::mesh(n, k, seed).unwrap_or_else(|e| die(&e));
            let out = config_dir().join("topologies").join(format!("mesh-{n}.json"));
            ok(generate::write(&out, &(to_js_json_pretty(&topo) + "\n")));
            let edges = topo["edges"].as_array().unwrap().len();
            println!("[generate-mesh] Wrote {} (N={n}, K={k}, seed={})", out.display(), metastable::js::js_number(seed));
            println!("[generate-mesh] services={} edges={edges} (downstream={n} gossip={})", n + 1, edges - n as usize);
        }
        other => die(&format!("unknown generator: {other} (ring|ring-swarm|swarm|compose|k8s|mesh)")),
    }
}
