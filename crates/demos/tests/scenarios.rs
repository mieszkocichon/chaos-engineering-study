//! T085: demo scenarios end to end on localhost (random ports, ≤50 requests each).
//! Services and clients are the real `demo` binary; reports are read back from disk.

use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

struct Cluster {
    procs: Vec<Child>,
    gw: u16,
    results: PathBuf,
}

impl Drop for Cluster {
    fn drop(&mut self) {
        for p in &mut self.procs {
            let _ = p.kill();
            let _ = p.wait();
        }
        let _ = std::fs::remove_dir_all(&self.results);
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn wait_port(p: u16) {
    let end = Instant::now() + Duration::from_secs(15);
    while std::net::TcpStream::connect(("127.0.0.1", p)).is_err() {
        assert!(Instant::now() < end, "port {p} not listening");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn cluster(tag: &str, gateway: &str, a: &str, b: &str, extra: &[(&str, &str)]) -> Cluster {
    let (gw, pa, pb) = (free_port(), free_port(), free_port());
    let results = std::env::temp_dir().join(format!("demo-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&results).unwrap();
    let mut c = Cluster { procs: vec![], gw, results };
    for (cmd, port_var, port) in [(a, "PORT", pa), (b, "PORT", pb), (gateway, "GATEWAY_PORT", gw)] {
        let mut p = Command::new(env!("CARGO_BIN_EXE_demo"));
        p.arg(cmd)
            .env(port_var, port.to_string())
            .env("DEMO_A_URL", format!("http://127.0.0.1:{pa}"))
            .env("DEMO_B_URL", format!("http://127.0.0.1:{pb}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (k, v) in extra {
            p.env(k, v);
        }
        c.procs.push(p.spawn().unwrap());
    }
    for p in [pa, pb, gw] {
        wait_port(p);
    }
    c
}

fn client(c: &Cluster, args: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_demo"))
        .args(args)
        .env("DEMO_GATEWAY", format!("http://127.0.0.1:{}", c.gw))
        .env("DEMO_RESULTS_DIR", &c.results)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let label = args[args.iter().position(|a| *a == "--save").unwrap() + 1];
    read(&c.results.join(format!("{label}.json")))
}

fn read(p: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

#[test]
fn plain_gateway_combines_both_services() {
    let c = cluster("plain", "gateway", "service-a", "service-b", &[]);
    let r = client(&c, &["test-load", "--requests", "10", "--save", "plain"]);
    assert_eq!(r["summary"]["successRate"], 1.0);
    let keys: Vec<&String> = r.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["label", "scenario", "timestamp", "config", "summary", "results"]);
}

#[test]
fn circuit_breaker_beats_retry_storm() {
    let before = {
        let c = cluster("cb-before", "gateway-retry", "service-a", "service-b-flaky", &[]);
        client(&c, &["test-load", "--requests", "50", "--save", "before-cb"])
    };
    let after = {
        let c = cluster("cb-after", "gateway-retry-cb", "service-a", "service-b-flaky", &[]);
        client(&c, &["test-load", "--requests", "50", "--save", "after-cb"])
    };
    // 3 immediate retries against 60% failures still lose ≈22% of requests;
    // with the breaker, B failures degrade the response instead of failing it
    assert!(before["summary"]["successRate"].as_f64().unwrap() < 1.0, "{}", before["summary"]);
    assert_eq!(after["summary"]["successRate"], 1.0, "{}", after["summary"]);
    assert_eq!(demos::clients::verdict(&before, &after), "VERDICT: System IMPROVED after applying pattern");
}

#[test]
fn saga_compensation_removes_orphaned_orders() {
    let before = {
        let c = cluster("saga-before", "gateway-orders", "service-a-orders", "service-b-payments", &[]);
        client(&c, &["test-orders", "--orders", "20", "--save", "before-saga"])
    };
    let after = {
        let c = cluster("saga-after", "gateway-orders-saga", "service-a-orders", "service-b-payments", &[]);
        client(&c, &["test-orders", "--orders", "20", "--save", "after-saga"])
    };
    // 40% random payment failures: without a saga failed payments leave active orders behind
    assert!(before["summary"]["orphaned"].as_u64().unwrap() > 0, "{}", before["summary"]);
    assert_eq!(before["summary"]["consistent"], false);
    // with compensation every failed payment cancels its order: no orphans, no double charges
    let s = &after["summary"];
    assert_eq!((s["orphaned"].as_u64(), s["duplicates"].as_u64(), &s["consistent"]), (Some(0), Some(0), &Value::Bool(true)), "{s}");
    assert_eq!(s["ordersCancelled"], s["failureCount"]);
}

#[test]
fn double_charge_chaos_creates_phantom_orders() {
    // payments take 3-5 s, gateway gives up after 2 s and re-runs the whole flow
    let c = cluster("double", "gateway-orders", "service-a-orders", "service-b-payments", &[("RETRY", "true"), ("TIMEOUT_MS", "2000"), ("SLOW_MODE", "true")]);
    let r = client(&c, &["test-orders", "--orders", "2", "--save", "double"]);
    let s = &r["summary"];
    assert_eq!(s["successCount"], 0, "{s}");
    assert_eq!(s["ordersCreated"], 4, "every order was created twice: {s}");
    assert_eq!(s["consistent"], false);
}
