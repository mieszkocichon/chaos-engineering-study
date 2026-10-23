//! T067: `chaos-coord` end to end on localhost with a fake k6 (tests/fixtures/fake-k6.sh).

mod common;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::svc::*;
use serde_json::Value;

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("chaos-coord-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// fake k6 must be executable: copy it (with its fixture) next to the results.
fn fake_k6(dir: &Path) -> PathBuf {
    let src = fixtures_dir();
    std::fs::copy(src.join("k6-summary.json"), dir.join("k6-summary.json")).unwrap();
    let k6 = dir.join("fake-k6.sh");
    std::fs::copy(src.join("fake-k6.sh"), &k6).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&k6, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    k6
}

fn coord(base: u16, results: &Path, k6: &Path, env: &[(&str, &str)]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_chaos-coord"));
    c.env("TOPOLOGY", "line-3")
        .env("CONFIG_DIR", fixtures_dir())
        .env("HOST_OVERRIDE", "127.0.0.1")
        .env("BASE_PORT", base.to_string())
        .env("FAULT_BASE", "no-wan")
        .env("FAULT_PROFILE", "no-wan")
        .env("RESULTS_DIR", results)
        .env("K6_BIN", k6)
        .env("K6_SCRIPT", "/nonexistent/k6-script.js")
        .env("INTER_SEED_SETTLE_MS", "100");
    for (k, v) in env {
        c.env(k, v);
    }
    c.output().unwrap()
}

fn results(dir: &Path) -> Vec<(String, Value)> {
    let mut v: Vec<(String, Value)> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("line-3_"))
        .map(|p| (p.file_name().unwrap().to_string_lossy().to_string(), serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap()))
        .collect();
    v.sort_by_key(|(_, r)| r["config"]["seed"].as_i64());
    v
}

#[test]
fn cell_with_two_seeds_writes_two_result_files() {
    let base = free_base_port(3);
    let cfg = fixtures_dir();
    let _svcs = [
        spawn("svc-01", "line-3", &cfg, base, base + 1, "none", &[]),
        spawn("svc-02", "line-3", &cfg, base, base + 2, "none", &[]),
        spawn("gateway", "line-3", &cfg, base, base, "mas-agent", &[]),
    ];
    let dir = tmpdir("cell");
    let k6 = fake_k6(&dir);
    let out = coord(base, &dir, &k6, &[("STRATEGY", "mas-agent"), ("SEEDS", "1, 2")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("[coordinator] Soft reset — next seed=2"), "{stdout}");
    assert!(stdout.contains("[coordinator] seed=1 SR="), "{stdout}");

    let files = results(&dir);
    assert_eq!(files.len(), 2, "{files:?}");
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(cfg.join("k6-summary.json")).unwrap()).unwrap();
    for ((name, r), seed) in files.iter().zip([1, 2]) {
        assert!(name.starts_with(&format!("line-3_mas-agent_no-wan_no-wan_{seed}_")), "{name}");
        assert_eq!(r["config"]["seed"], seed);
        assert_eq!(r["config"]["spof"], Value::Null);
        assert_eq!(r["k6Metrics"], fixture["metrics"]);
        let keys: Vec<&String> = r.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["config", "experimentStartTime", "summary", "k6Metrics", "serviceMetrics", "timestamp"]);
        let sm = r["serviceMetrics"].as_object().unwrap();
        assert_eq!(sm.keys().collect::<Vec<_>>(), ["gateway", "svc-01", "svc-02"]);
        assert_eq!(sm["gateway"]["strategyStats"]["strategy"], "mas-agent");
        assert!(sm["svc-01"]["strategyStats"].is_null());
    }
    // the temp k6 summary file is removed after parsing
    assert!(!std::fs::read_dir(&dir).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with("_k6_summary_tmp_")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn health_timeout_names_the_missing_service() {
    let base = free_base_port(3);
    let cfg = fixtures_dir();
    let _up = [spawn("gateway", "line-3", &cfg, base, base, "none", &[]), spawn("svc-01", "line-3", &cfg, base, base + 1, "none", &[])];
    let dir = tmpdir("timeout");
    let k6 = fake_k6(&dir);
    let out = coord(base, &dir, &k6, &[("HEALTH_TIMEOUT_MS", "2000"), ("SEEDS", "1")]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Health check timeout. Not ready: svc-02"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn spof_kill_is_recorded_for_central() {
    let base = free_base_port(3);
    let cfg = fixtures_dir();
    let _svcs = [
        spawn("svc-01", "line-3", &cfg, base, base + 1, "none", &[]),
        spawn("svc-02", "line-3", &cfg, base, base + 2, "none", &[]),
        spawn("gateway", "line-3", &cfg, base, base, "central", &[]),
    ];
    let dir = tmpdir("spof");
    let k6 = fake_k6(&dir);
    let out = coord(
        base,
        &dir,
        &k6,
        &[("STRATEGY", "central"), ("SEEDS", "5"), ("WARMUP_SEC", "0"), ("SPOF_KILL_AT_SEC", "1"), ("FAKE_K6_SLEEP", "3")],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert!(stdout.contains("SPOF: Central controller killed at T+1s"), "{stdout}");
    let (_, r) = &results(&dir)[0];
    assert_eq!(r["config"]["spof"]["killAtSec"], 1);
    assert!(r["config"]["spof"]["killedAtMs"].as_u64().is_some());
    // the controller ran for ~1 s before the kill → it pushed parameters to the gateway
    let st = &r["serviceMetrics"]["gateway"]["strategyStats"];
    assert_eq!(st["strategy"], "central");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn empty_seeds_fail_fast() {
    let dir = tmpdir("seeds");
    let out = Command::new(env!("CARGO_BIN_EXE_chaos-coord")).env("SEEDS", "x,y").env("RESULTS_DIR", &dir).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("SEEDS env var is empty or invalid"));
}
