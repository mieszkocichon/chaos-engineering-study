//! `chaos-run` without Docker: the migrated experiment files must give the same cells and the same
//! compose files as the removed `run-matrix-*.sh` scripts (the `Old*` data below is transcribed
//! from those scripts), plus matrix expansion, validation, `--resume`, variant tagging and locking.

use std::path::{Path, PathBuf};

use chaos::experiment::{self, schema::parse_seeds, Cell, Experiment, RunOptions};
use chaos::generate::{self, ComposeOpts};

fn config_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../when-gossip-goes-silent/config"))
}

fn load(name: &str) -> Experiment {
    let exp = Experiment::from_file(&config_dir().join("experiments").join(name)).unwrap_or_else(|e| panic!("{e}"));
    let errors = experiment::validate_experiment(&exp, &config_dir());
    assert!(errors.is_empty(), "{name}: {errors:?}");
    exp
}

fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("chaos-run-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A cell as the old scripts passed it to `gen compose` (positional arguments + env).
#[derive(Default, Clone)]
struct Old {
    strategy: &'static str,
    base: &'static str,
    fault: &'static str,
    partition: &'static str,
    variant: Option<&'static str>,
    static_params: Option<&'static str>,
    swim_fanout: Option<&'static str>,
    spof_kill_at_sec: Option<&'static str>,
}

fn old(strategy: &'static str, base: &'static str, fault: &'static str) -> Old {
    Old { strategy, base, fault, ..Default::default() }
}

const SEEDS_20: &str = "1000,2000,3000,4000,5000,6000,7000,8000,9000,10000,11000,12000,13000,14000,15000,16000,17000,18000,19000,20000";

/// What the old script generated for `o`: `gen compose <topology> <fault> <strategy> <seeds> <base> <partition> <project>`.
fn old_compose(topology: &str, o: &Old, seeds: &str, project: &str, results_dir: &str, gateway_sed: Option<&str>) -> String {
    let topo: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(config_dir().join("topologies").join(format!("{topology}.json"))).unwrap()).unwrap();
    let opts = ComposeOpts {
        topology,
        fault: o.fault,
        strategy: o.strategy,
        seeds,
        fault_base: o.base,
        fault_partition: o.partition,
        project,
        image: generate::IMAGE.into(),
        results_dir_host: results_dir.into(),
        results_dir_container: "/results".into(),
        static_params: o.static_params.map(String::from),
        swim_fanout: o.swim_fanout.map(String::from),
        spof_kill_at_sec: o.spof_kill_at_sec.map(String::from),
        gateway_cpus: None,
        rps: None,
        duration_sec: None,
        warmup_sec: None,
        config_dir_host: None,
        extra_env: None,
        network_driver: None,
    };
    let text = generate::compose(&topo, &opts);
    // old GATEWAY_CPUS handling: sed -i "s/cpus: '0.25'/cpus: '<n>'/"
    match gateway_sed {
        Some(n) => text.replace("cpus: '0.25'", &format!("cpus: '{n}'")),
        None => text,
    }
}

/// The experiment file must expand to exactly `expected` (order included) and generate the same compose files.
fn assert_matches_old(file: &str, topology: &str, seeds: &str, results_dir: &str, gateway_cpus: Option<&str>, expected: &[Old]) {
    let exp = load(file);
    assert_eq!(exp.seeds(), parse_seeds(seeds).unwrap(), "{file}: seeds");
    assert_eq!(exp.results_dir(), results_dir, "{file}: results dir");
    let cells = experiment::expand_cells(&exp);
    assert_eq!(cells.len(), expected.len(), "{file}: cell count");
    for (c, o) in cells.iter().zip(expected) {
        let what = format!("{file}: {}", c.label());
        assert_eq!(c.topology, topology, "{what}");
        assert_eq!(
            (c.strategy.as_str(), c.base.as_str(), c.fault.as_str(), c.partition.as_str()),
            (o.strategy, o.base, o.fault, o.partition),
            "{what}"
        );
        assert_eq!(c.variant.as_deref(), o.variant, "{what}: variant");
        let topo: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(config_dir().join("topologies").join(format!("{topology}.json"))).unwrap()).unwrap();
        let new = generate::compose(&topo, &c.compose_opts(seeds, "p1", generate::IMAGE.into(), results_dir.into(), "./config".into()));
        assert_eq!(new, old_compose(topology, o, seeds, "p1", results_dir, gateway_cpus), "{what}: compose differs");
    }
}

#[test]
fn matrix_compose_is_run_matrix_compose() {
    let mut expected = vec![];
    for s in ["none", "static", "static-conservative", "central", "mas-agent", "backpressure"] {
        for b in ["no-wan", "wan-50ms"] {
            for f in ["baseline-flaky", "burst-failure", "slow-degradation", "cascade-crash"] {
                expected.push(old(s, b, f));
            }
        }
    }
    for s in ["central", "mas-agent", "backpressure"] {
        expected.push(Old { partition: "network-partition", ..old(s, "no-wan", "baseline-flaky") });
    }
    assert_eq!(expected.len(), 51);
    assert_matches_old("matrix-compose.json", "ring-50", SEEDS_20, "./results", None, &expected);
}

#[test]
fn matrix_inbound_is_run_matrix_inbound() {
    let expected = [old("mas-agent", "no-wan", "baseline-flaky"), old("mas-agent-inbound", "no-wan", "baseline-flaky")];
    assert_matches_old("matrix-inbound.json", "ring-50", SEEDS_20, "./results-inbound", None, &expected);
}

#[test]
fn matrix_sensitivity_is_run_matrix_sensitivity() {
    let v = |label, params| Old { variant: Some(label), static_params: Some(params), ..old("static", "no-wan", "baseline-flaky") };
    let expected = [
        v("r4j-default", r#"{"retryCount":3,"retryBackoff":2,"cbThreshold":5,"cbTimeoutMs":10000,"requestTimeoutMs":2000,"maxConcurrent":25}"#),
        v("aggressive", r#"{"retryCount":5,"retryBackoff":1.5,"cbThreshold":3,"cbTimeoutMs":5000,"requestTimeoutMs":3000,"maxConcurrent":80}"#),
        v("conservative", r#"{"retryCount":1,"retryBackoff":3,"cbThreshold":10,"cbTimeoutMs":20000,"requestTimeoutMs":3000,"maxConcurrent":20}"#),
        v("matched", r#"{"retryCount":3,"retryBackoff":2,"cbThreshold":5,"cbTimeoutMs":10000,"requestTimeoutMs":3000,"maxConcurrent":50}"#),
    ];
    assert_matches_old("matrix-sensitivity.json", "ring-50", SEEDS_20, "./results-sensitivity", None, &expected);
}

#[test]
fn matrix_spof_is_run_matrix_spof() {
    let v = |label, strategy, kill| Old { variant: Some(label), spof_kill_at_sec: Some(kill), ..old(strategy, "no-wan", "baseline-flaky") };
    let expected = [v("central-baseline", "central", "0"), v("central-spof30", "central", "30"), v("mas-agent-ref", "mas-agent", "0")];
    assert_matches_old("matrix-spof.json", "ring-50", SEEDS_20, "./results-spof", None, &expected);
}

#[test]
fn matrix_swim_is_run_matrix_swim() {
    // the old script exported SWIM_FANOUT=3 for all three strategies
    let s = |strategy| Old { swim_fanout: Some("3"), ..old(strategy, "no-wan", "baseline-flaky") };
    let expected = [s("mas-agent"), s("mas-agent-swim"), s("backpressure")];
    assert_matches_old("matrix-swim.json", "mesh-30", SEEDS_20, "./results-swim", None, &expected);
}

#[test]
fn matrix_swarm_matches_run_matrix_swarm_deployments() {
    let exp = load("matrix-swarm.json");
    assert_eq!(exp.backend(), "swarm");
    assert_eq!(exp.scale(), Some(20));
    assert_eq!(exp.topologies(), vec!["scale-20"]);
    assert_eq!(exp.strategies(), vec!["none", "static", "central", "mas-agent", "backpressure"]);
    assert_eq!(exp.fault_profiles(), vec!["baseline-flaky-scale", "burst-failure-scale", "slow-degradation-scale", "cascade-crash-scale"]);
    assert_eq!(exp.seeds(), vec![1000, 2000, 3000, 4000, 5000]);
    let w = exp.workload();
    assert_eq!(w.rps, Some(50));
    assert_eq!(w.duration_sec, Some(60));
    assert_eq!(w.warmup_sec, Some(10));
    assert_eq!(exp.health_timeout_ms(), 540000);

    let cells = experiment::expand_cells(&exp);
    // 5 strategies * 4 faults * 5 seeds = 100 deployments
    assert_eq!(cells.len(), 100, "swarm must expand to 100 individual deployments (1 seed per deployment)");

    let mut expected_triplets = vec![];
    for s in ["none", "static", "central", "mas-agent", "backpressure"] {
        for f in ["baseline-flaky-scale", "burst-failure-scale", "slow-degradation-scale", "cascade-crash-scale"] {
            for seed in [1000, 2000, 3000, 4000, 5000] {
                expected_triplets.push((s, f, seed));
            }
        }
    }

    for (c, (s, f, seed)) in cells.iter().zip(expected_triplets.iter()) {
        assert_eq!(c.strategy, *s);
        assert_eq!(c.fault, *f);
        assert_eq!(c.seeds, vec![*seed]);
        assert_eq!(c.scale, Some(20));
        assert_eq!(c.rps, Some(50));
        assert_eq!(c.warmup_sec, Some(10));
        assert_eq!(c.duration_sec, Some(60));
        assert_eq!(c.health_timeout_ms, Some(540000));

        let (_, yaml) = generate::swarm_with_opts(&generate::SwarmOpts {
            scale: 20,
            fault: f,
            strategy: s,
            seed: &seed.to_string(),
            seeds: Some(&seed.to_string()),
            registry: None,
            rps: c.rps,
            duration_sec: c.duration_sec,
            warmup_sec: c.warmup_sec,
            health_timeout_ms: c.health_timeout_ms,
            config_dir_host: Some("/host/config".into()),
            results_dir_host: Some("./results".into()),
            image: Some(generate::IMAGE.into()),
            extra_env: None,
            ..Default::default()
        }).unwrap();

        assert!(yaml.contains("chaos-overlay"), "must use chaos-overlay network");
        assert!(yaml.contains("driver: overlay"), "must use driver: overlay");
        assert!(yaml.contains("RPS: \"50\""), "must use RPS 50");
        assert!(yaml.contains("WARMUP_SEC: \"10\""), "must use warmup 10");
        assert!(yaml.contains("DURATION_SEC: \"60\""), "must use duration 60");
        assert!(yaml.contains("HEALTH_TIMEOUT_MS: \"540000\""), "must use health timeout 540000");
        assert!(yaml.contains(&format!("SEEDS: \"{seed}\"")), "must specify seed {seed} for coordinator");
    }
}

#[test]
fn spof_test_matches_run_spof_test_cells_and_actions() {
    let exp = load("spof-test.json");
    assert_eq!(exp.backend(), "compose");
    assert_eq!(exp.topologies(), vec!["ring-5"]);
    assert_eq!(exp.results_dir(), "./results");

    let cells = experiment::expand_cells(&exp);
    // the old script's second phase (stop the coordinator) is gone: it cannot produce a result file
    assert_eq!(cells.len(), 5);

    let strats_phase1 = ["none", "static", "central", "mas-agent", "backpressure"];
    for (i, &s) in strats_phase1.iter().enumerate() {
        let c = &cells[i];
        assert_eq!(c.strategy, s);
        assert_eq!(c.fault, "baseline-flaky");
        assert_eq!(c.seeds, vec![1000]);
        assert_eq!(c.rps, Some(20));
        assert_eq!(c.duration_sec, Some(90));
        assert_eq!(c.warmup_sec, Some(15));
        assert_eq!(c.env.get("SIMULATE_CLOCK_SKEW").map(|s| s.as_str()), Some("true"));
        assert_eq!(c.env.get("SIMULATE_IO_CONTENTION").map(|s| s.as_str()), Some("true"));
        assert_eq!(c.actions.len(), 1);
        let a = &c.actions[0];
        assert_eq!(a.at_sec, 30);
        assert_eq!(a.kind, "stop-service");
        assert_eq!(a.target, "svc-03");
        let meta = a.metadata.as_ref().expect("metadata must be present");
        assert_eq!(meta.killed_service, "svc-03");
        assert_eq!(meta.kill_at_sec, 30);
        assert!(meta.permanent);
        assert_eq!(meta.phase, None);

        let topo: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(config_dir().join("topologies/ring-5.json")).unwrap()).unwrap();
        let compose = generate::compose(&topo, &c.compose_opts("1000", "p1", generate::IMAGE.into(), "./results".into(), "./config".into()));
        assert!(compose.contains("SIMULATE_CLOCK_SKEW: \"true\""));
        assert!(compose.contains("SIMULATE_IO_CONTENTION: \"true\""));
        assert!(compose.contains("DURATION_SEC: \"90\""));
        assert!(compose.contains("driver: bridge"));
        assert!(!compose.contains("driver: overlay"));
    }
}

fn subset() -> Vec<Old> {
    let mut v = vec![];
    for s in ["none", "mas-agent", "backpressure"] {
        for f in ["baseline-flaky", "slow-degradation"] {
            v.push(old(s, "no-wan", f));
        }
    }
    v
}

#[test]
fn equivalence_control_and_unthrottled_are_the_scripts_subset() {
    let seeds_30 = (1..=30).map(|i| (i * 1000).to_string()).collect::<Vec<_>>().join(",");
    assert_matches_old("equivalence.json", "ring-50", &seeds_30, "./results-rs", None, &subset());
    assert_matches_old("control-js.json", "ring-50", "1000,2000,3000,4000,5000", "./results-js-local", None, &subset());
    assert_eq!(load("control-js.json").impl_type(), "js");
    // old: GATEWAY_CPUS=4 -> sed on the generated file
    assert_matches_old("unthrottled-cpu4.json", "ring-50", "1000,2000,3000,4000,5000", "./results-rs-cpu4", Some("4"), &subset());
}

#[test]
fn variants_of_one_strategy_have_distinct_result_names() {
    for file in ["matrix-sensitivity.json", "matrix-spof.json"] {
        let cells = experiment::expand_cells(&load(file));
        let mut keys: Vec<_> = cells.iter().map(|c| (c.file_prefix(), c.variant.clone())).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), cells.len(), "{file}: cells would overwrite each other's results");
        assert!(cells.iter().all(|c| c.variant.is_some()), "{file}");
    }
}

#[test]
fn full_matrix_json_still_parses() {
    let exp = Experiment::from_file(&config_dir().join("experiments/full-matrix.json")).unwrap();
    let cells = experiment::expand_cells(&exp);
    assert_eq!(cells.len(), 3 * 4 * 4);
    assert_eq!(exp.seeds(), vec![1000, 2000, 3000, 4000, 5000]);
    // fanout and diamond have no topology files yet
    let errors = experiment::validate_experiment(&exp, &config_dir());
    assert_eq!(errors.len(), 2, "{errors:?}");
}

#[test]
fn validation_rejects_bad_experiments() {
    let exp = Experiment {
        topologies: Some(vec!["nope".into()]),
        strategies: Some(vec!["bogus-strategy".into()]),
        fault_profiles: Some(vec!["bogus-fault".into()]),
        extra_cells: Some(vec![serde_json::from_str(r#"{"strategy":"central","partition":"bogus-partition"}"#).unwrap()]),
        ..Default::default()
    };
    let errors = experiment::validate_experiment(&exp, &config_dir()).join("\n");
    for needle in ["topology file not found", "unknown strategy: 'bogus-strategy'", "unknown fault profile: 'bogus-fault'", "unknown fault partition: 'bogus-partition'", "no seeds"] {
        assert!(errors.contains(needle), "missing '{needle}' in:\n{errors}");
    }
}

#[test]
fn bad_seeds_and_unknown_keys_are_errors() {
    assert!(parse_seeds("1000,abc").is_err());
    assert_eq!(parse_seeds(" 1000, 2000 ").unwrap(), vec![1000, 2000]);
    for bad in [r#"{"seeds":[1000,"x"]}"#, r#"{"seeds":"1000,x"}"#, r#"{"seeds":true}"#, r#"{"strategyes":["none"]}"#] {
        assert!(serde_json::from_str::<Experiment>(bad).is_err(), "{bad} should be rejected");
    }
}

#[test]
fn extra_cell_params_use_the_same_format_as_variants() {
    let exp: Experiment = serde_json::from_str(
        r#"{"topology":"ring-5","faultProfiles":["baseline-flaky"],"seeds":[1],
            "extraCells":[{"strategy":"static","staticParams":"{\"retryCount\":3}"},
                          {"strategy":"static","staticParams":{"retryCount":3}}]}"#,
    )
    .unwrap();
    let cells = experiment::expand_cells(&exp);
    assert_eq!(cells[0].static_params.as_deref(), Some(r#"{"retryCount":3}"#));
    assert_eq!(cells[0].static_params, cells[1].static_params);
}

fn cell(strategy: &str, variant: Option<&str>) -> Cell {
    let mut c = experiment::expand_cells(&Experiment {
        topology: Some("ring-50".into()),
        strategy: Some(strategy.into()),
        fault_profile: Some("baseline-flaky".into()),
        seeds: Some(vec![1000]),
        ..Default::default()
    })
    .remove(0);
    c.variant = variant.map(String::from);
    c
}

#[test]
fn resume_is_variant_aware_and_variant_results_are_tagged() {
    let dir = tmp("resume");
    let plain = cell("static", None);
    let a = cell("static", Some("a"));
    let b = cell("static", Some("b"));
    let seeds = [1000, 2000];
    assert!(!experiment::is_cell_complete(&plain, &seeds, &dir));

    for seed in seeds {
        std::fs::write(dir.join(format!("ring-50_static_no-wan_baseline-flaky_{seed}_1776500000.json")), "{}").unwrap();
    }
    assert!(experiment::is_cell_complete(&plain, &seeds, &dir));
    assert!(!experiment::is_cell_complete(&a, &seeds, &dir), "untagged files are not variant a's");

    experiment::tag_variant_results(&a, &dir).unwrap();
    assert!(experiment::is_cell_complete(&a, &seeds, &dir));
    assert!(!experiment::is_cell_complete(&b, &seeds, &dir), "variant b must still run");
    assert!(!experiment::is_cell_complete(&plain, &seeds, &dir), "tagged files do not satisfy the untagged cell");
    assert!(dir.join("ring-50_static_no-wan_baseline-flaky_1000_1776500000__a__.json").exists());

    // a second variant tags only its own new files
    std::fs::write(dir.join("ring-50_static_no-wan_baseline-flaky_1000_1776500009.json"), "{}").unwrap();
    experiment::tag_variant_results(&b, &dir).unwrap();
    assert!(dir.join("ring-50_static_no-wan_baseline-flaky_1000_1776500009__b__.json").exists());
    assert!(dir.join("ring-50_static_no-wan_baseline-flaky_1000_1776500000__a__.json").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lock_is_per_project_prefix_and_released_on_drop() {
    let dir = tmp("lock");
    let first = experiment::LockGuard::acquire(&dir, "w").expect("first lock");
    assert!(experiment::LockGuard::acquire(&dir, "w").is_err(), "same prefix must be refused");
    assert!(experiment::LockGuard::acquire(&dir, "sens").is_ok(), "another prefix is independent");
    drop(first);
    assert!(experiment::LockGuard::acquire(&dir, "w").is_ok(), "released with the guard");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dry_run_creates_nothing() {
    let dir = std::env::temp_dir().join(format!("chaos-run-dry-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let opts = RunOptions { dry_run: true, results_dir: Some(dir.to_string_lossy().into()), ..Default::default() };
    experiment::run_experiment(load("matrix-inbound.json"), &config_dir(), &opts).unwrap();
    assert!(!Path::new(&dir).exists(), "--dry-run must not touch the filesystem");
}

#[test]
fn compose_options_replace_the_old_seds() {
    let topo: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(config_dir().join("topologies/ring-5.json")).unwrap()).unwrap();
    let mut c = cell("mas-agent", None);
    c.gateway_cpus = Some("4".into());
    c.rps = Some(100);
    c.duration_sec = Some(120);
    c.warmup_sec = Some(30);
    let compose = generate::compose(&topo, &c.compose_opts("1000", "t1", generate::IMAGE.into(), "./r".into(), "/host/config".into()));
    for needle in ["cpus: '4'", "RPS: \"100\"", "DURATION_SEC: \"120\"", "WARMUP_SEC: \"30\"", "- /host/config:/app/config:ro", "- ./r:/results"] {
        assert!(compose.contains(needle), "missing {needle}");
    }
}

#[derive(Clone, Default)]
struct MockBackend {
    calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    fail_action: std::sync::Arc<std::sync::Mutex<bool>>,
    exited_code: std::sync::Arc<std::sync::Mutex<Option<i32>>>,
}

impl chaos::experiment::backend::ClusterBackend for MockBackend {
    fn name(&self) -> &'static str { "mock" }
    fn prepare(&self, ctx: &chaos::experiment::backend::PrepareContext) -> Result<chaos::experiment::backend::BackendPlan, String> {
        self.calls.lock().unwrap().push(format!("prepare:{}", ctx.project));
        Ok(chaos::experiment::backend::BackendPlan {
            file_path: PathBuf::from(format!("mock-{}.yml", ctx.project)),
            project_or_stack: ctx.project.to_string(),
            network_driver: "bridge".into(),
        })
    }
    fn start(&self, plan: &chaos::experiment::backend::BackendPlan) -> Result<(), String> {
        self.calls.lock().unwrap().push(format!("start:{}", plan.project_or_stack));
        Ok(())
    }
    fn check_status(&self, plan: &chaos::experiment::backend::BackendPlan, _started: std::time::Instant) -> Result<chaos::experiment::backend::RunStatus, String> {
        self.calls.lock().unwrap().push(format!("check_status:{}", plan.project_or_stack));
        let code = self.exited_code.lock().unwrap().unwrap_or(0);
        Ok(chaos::experiment::backend::RunStatus::Exited(code))
    }
    fn stop_service(&self, plan: &chaos::experiment::backend::BackendPlan, target: &str) -> Result<(), String> {
        self.calls.lock().unwrap().push(format!("stop_service:{}:{}", plan.project_or_stack, target));
        if *self.fail_action.lock().unwrap() {
            return Err(format!("mock stop {target} failed"));
        }
        Ok(())
    }
    fn logs_tail(&self, plan: &chaos::experiment::backend::BackendPlan, lines: usize) -> String {
        format!("mock logs tail {lines} for {}", plan.project_or_stack)
    }
    fn teardown(&self, plan: &chaos::experiment::backend::BackendPlan) -> Result<(), String> {
        self.calls.lock().unwrap().push(format!("teardown:{}", plan.project_or_stack));
        Ok(())
    }
}

#[test]
fn runner_with_mock_backend_executes_actions_and_records_spof_metadata() {
    let dir = tmp("mock-runner-success");
    let mock = MockBackend::default();

    let mut c = cell("static", None);
    c.seeds = vec![1000];
    c.actions = vec![chaos::experiment::schema::CellAction {
        at_sec: 0,
        kind: "stop-service".into(),
        target: "svc-03".into(),
        metadata: Some(chaos::experiment::schema::SpofTestMeta {
            killed_service: "svc-03".into(),
            kill_at_sec: 30,
            permanent: true,
            phase: None,
            note: Some("test note".into()),
        }),
    }];

    // Pre-create result file so is_cell_complete passes
    let res_file = dir.join("ring-50_static_no-wan_baseline-flaky_1000_1776500000.json");
    std::fs::write(&res_file, r#"{"config":{"seed":1000}}"#).unwrap();

    let runner = chaos::experiment::runner::CellRunner {
        project: "p1".into(),
        image: "test-img".into(),
        config_dir: config_dir(),
        config_dir_host: "/host/config".into(),
        results_path: dir.clone(),
        results_dir_host: "/host/results".into(),
        seeds: vec![1000],
        seeds_str: "1000".into(),
    };

    let res = runner.run_with_backend(&c, &mock);
    assert!(res.is_ok(), "{res:?}");

    let calls = mock.calls.lock().unwrap().clone();
    assert_eq!(calls, vec![
        "prepare:p1",
        "start:p1",
        "stop_service:p1:svc-03",
        "check_status:p1",
        "teardown:p1"
    ]);

    // Check that spofTest metadata was appended
    let content = std::fs::read_to_string(&res_file).unwrap();
    let json: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(json["spofTest"]["killedService"], "svc-03");
    assert_eq!(json["spofTest"]["killAtSec"], 30);
    assert_eq!(json["spofTest"]["permanent"], true);
    assert_eq!(json["spofTest"]["note"], "test note");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn runner_with_mock_backend_reports_action_failure_and_cleans_up() {
    let dir = tmp("mock-runner-fail-action");
    let mock = MockBackend::default();
    *mock.fail_action.lock().unwrap() = true;

    let mut c = cell("static", None);
    c.seeds = vec![1000];
    c.actions = vec![chaos::experiment::schema::CellAction {
        at_sec: 0,
        kind: "stop-service".into(),
        target: "svc-03".into(),
        metadata: None,
    }];

    let runner = chaos::experiment::runner::CellRunner {
        project: "p2".into(),
        image: "test-img".into(),
        config_dir: config_dir(),
        config_dir_host: "/host/config".into(),
        results_path: dir.clone(),
        results_dir_host: "/host/results".into(),
        seeds: vec![1000],
        seeds_str: "1000".into(),
    };

    let res = runner.run_with_backend(&c, &mock);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(err.contains("action 'stop-service' on 'svc-03' failed"), "{err}");

    // Teardown MUST be called even on action failure
    let calls = mock.calls.lock().unwrap().clone();
    assert!(calls.contains(&"teardown:p2".to_string()), "teardown must be called");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn runner_with_mock_backend_reports_coordinator_exit_error_and_cleans_up() {
    let dir = tmp("mock-runner-coord-error");
    let mock = MockBackend::default();
    *mock.exited_code.lock().unwrap() = Some(137);

    let mut c = cell("static", None);
    c.seeds = vec![1000];

    let runner = chaos::experiment::runner::CellRunner {
        project: "p3".into(),
        image: "test-img".into(),
        config_dir: config_dir(),
        config_dir_host: "/host/config".into(),
        results_path: dir.clone(),
        results_dir_host: "/host/results".into(),
        seeds: vec![1000],
        seeds_str: "1000".into(),
    };

    let res = runner.run_with_backend(&c, &mock);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(err.contains("coordinator exited with code 137"), "{err}");

    // Teardown MUST be called
    let calls = mock.calls.lock().unwrap().clone();
    assert!(calls.contains(&"teardown:p3".to_string()), "teardown must be called");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn action_that_never_fires_fails_the_cell_and_adds_no_metadata() {
    let dir = tmp("mock-runner-late-action");
    let mock = MockBackend::default(); // exits at once with 0
    let mut c = cell("static", None);
    c.seeds = vec![1000];
    c.actions = vec![chaos::experiment::schema::CellAction { at_sec: 3600, kind: "stop-service".into(), target: "svc-03".into(), metadata: None }];
    let res_file = dir.join("ring-50_static_no-wan_baseline-flaky_1000_1776500000.json");
    std::fs::write(&res_file, r#"{"config":{"seed":1000}}"#).unwrap();

    let runner = chaos::experiment::runner::CellRunner {
        project: "p4".into(),
        image: "test-img".into(),
        config_dir: config_dir(),
        config_dir_host: "/host/config".into(),
        results_path: dir.clone(),
        results_dir_host: "/host/results".into(),
        seeds: vec![1000],
        seeds_str: "1000".into(),
    };
    let err = runner.run_with_backend(&c, &mock).unwrap_err();
    assert!(err.contains("was not executed"), "{err}");
    assert!(!mock.calls.lock().unwrap().iter().any(|c| c.starts_with("stop_service")), "{:?}", mock.calls);
    assert!(!std::fs::read_to_string(&res_file).unwrap().contains("spofTest"), "no spofTest for a kill that never happened");
    assert!(mock.calls.lock().unwrap().contains(&"teardown:p4".to_string()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stopping_the_coordinator_is_rejected_by_validation() {
    let exp: Experiment = serde_json::from_str(
        r#"{"topology":"ring-5","strategies":["central"],"faultProfiles":["baseline-flaky"],"seeds":[1],
            "actions":[{"kind":"stop-service","target":"coordinator","atSec":3}]}"#,
    )
    .unwrap();
    let errors = experiment::validate_experiment(&exp, &config_dir()).join("
");
    assert!(errors.contains("coordinator"), "{errors}");
}

#[test]
fn swarm_stack_has_no_depends_on_for_stack_deploy() {
    let base = generate::SwarmOpts { scale: 5, fault: "baseline-flaky", strategy: "none", seed: "1", ..Default::default() };
    let (_, js_compatible) = generate::swarm_with_opts(&base).unwrap();
    assert!(js_compatible.contains("depends_on:"), "default output stays identical to the JS generator");
    let (_, deployable) = generate::swarm_with_opts(&generate::SwarmOpts { stack_deploy: true, ..base }).unwrap();
    assert!(!deployable.contains("depends_on:"), "docker stack deploy rejects the map form");
}

#[test]
fn swarm_scale_must_have_its_topology_file() {
    let check = |json: &str| -> String {
        let exp: Experiment = serde_json::from_str(json).unwrap();
        experiment::validate_experiment(&exp, &config_dir()).join("
")
    };
    let base = r#""backend":"swarm","strategies":["none"],"faultProfiles":["baseline-flaky"],"seeds":[1]"#;
    assert_eq!(check(&format!("{{{base},\"scale\":20}}")), "", "scale-20.json exists");
    assert!(check(&format!("{{{base},\"scale\":7}}")).contains("scale-7.json"), "coordinator would exit 1: no topology file");
    assert!(check(&format!("{{{base},\"scale\":20,\"topology\":\"scale-50\"}}")).contains("must be 'scale-20'"));
}

#[test]
fn faults_that_target_no_service_of_the_topology_are_reported() {
    // the ring profile targets svc-01/03/05, scale-20 has svc-a..d and svc-1..: nothing would be injected
    let ring_profile_on_scale: Experiment = serde_json::from_str(
        r#"{"backend":"swarm","scale":20,"strategies":["none"],"faultProfiles":["baseline-flaky"],"seeds":[1]}"#,
    )
    .unwrap();
    let w = experiment::fault_target_warnings(&experiment::expand_cells(&ring_profile_on_scale), &config_dir());
    assert!(w.iter().any(|m| m.contains("baseline-flaky") && m.contains("scale-20")), "{w:?}");
    // every shipped experiment targets existing services, Swarm included (it uses the *-scale profiles)
    for file in ["matrix-compose.json", "matrix-inbound.json", "matrix-sensitivity.json", "matrix-spof.json", "matrix-swim.json", "matrix-swarm.json", "spof-test.json", "equivalence.json"] {
        let w = experiment::fault_target_warnings(&experiment::expand_cells(&load(file)), &config_dir());
        assert!(w.is_empty(), "{file}: {w:?}");
    }
}

#[test]
fn scale_profiles_target_only_existing_services_and_keep_the_ring_kinds() {
    let read = |p: String| -> serde_json::Value { serde_json::from_str(&std::fs::read_to_string(config_dir().join(p)).unwrap()).unwrap() };
    for topo in ["scale-20", "scale-50"] {
        let ids: Vec<String> = read(format!("topologies/{topo}.json"))["services"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap().to_string()).collect();
        for kind in ["baseline-flaky", "burst-failure", "slow-degradation", "cascade-crash"] {
            let (ring, scale) = (read(format!("faults/{kind}.json")), read(format!("faults/{kind}-scale.json")));
            let f = scale["faults"].as_array().unwrap();
            assert_eq!(f.len(), ring["faults"].as_array().unwrap().len(), "{kind}: same number of targets as the ring profile");
            for x in f {
                assert_eq!(x["type"], kind, "{kind}-scale must inject the same fault kind");
                assert!(ids.contains(&x["targetService"].as_str().unwrap().to_string()), "{kind}-scale targets {} - not in {topo}", x["targetService"]);
            }
        }
    }
}
