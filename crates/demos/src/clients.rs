//! Demo clients – ports of `demos/client/{test-load,test-orders,monitor,compare,test}.js`.
//! Report JSON files keep the JS shape (`demos/results/*.json`).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use futures::future::join_all;
use metastable::js::{js_number, js_to_fixed, to_js_json_pretty};
use serde_json::{Value, json};

use crate::{arg, env_or, iso_now, now_ms};

const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const GREY: &str = "\x1b[90m";
const RESET: &str = "\x1b[0m";

pub fn gateway_url() -> String {
    env_or("DEMO_GATEWAY", "http://localhost:3000")
}

/// `demos/results` (from the repo root or `demos/`), overridable with `DEMO_RESULTS_DIR`.
pub fn results_dir() -> PathBuf {
    if let Ok(d) = std::env::var("DEMO_RESULTS_DIR") {
        return PathBuf::from(d);
    }
    for c in ["demos/results", "results", "../results"] {
        if std::path::Path::new(c).is_dir() {
            return PathBuf::from(c);
        }
    }
    PathBuf::from("results")
}

fn save(label: &str, report: &Value) {
    let dir = results_dir();
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(dir.join(format!("{label}.json")), to_js_json_pretty(report)).unwrap();
    println!("\n  Results saved to: results/{label}.json");
}

/// `sorted[floor(n·p)] || 0` – the clients' percentile (not the experiment's).
pub fn pct(sorted: &[u64], p: f64) -> u64 {
    sorted.get((sorted.len() as f64 * p).floor() as usize).copied().unwrap_or(0)
}

fn secs(ms: f64, d: usize) -> String {
    js_to_fixed(ms / 1000.0, d)
}

fn pad_start(s: &str, n: usize) -> String {
    format!("{s:>n$}")
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub id: usize,
    /// HTTP status, or "ERROR" / "TIMEOUT" like the JS clients
    pub status: Value,
    pub duration: u64,
    pub body: String,
    pub timestamp: u64,
}

impl Hit {
    fn ok(&self) -> bool {
        self.status == json!(200)
    }
}

async fn request(client: &reqwest::Client, id: usize, method: reqwest::Method, url: &str, body: Option<String>, timeout: Duration) -> Hit {
    let start = Instant::now();
    let mut req = client.request(method, url).timeout(timeout);
    if let Some(b) = body {
        req = req.header("Content-Type", "application/json").body(b);
    }
    let (status, body) = match req.send().await {
        Ok(res) => {
            let st = res.status().as_u16();
            (json!(st), res.text().await.unwrap_or_default())
        }
        Err(e) if e.is_timeout() => (json!("TIMEOUT"), format!("Request timed out ({}s)", timeout.as_secs())),
        Err(e) => (json!("ERROR"), e.to_string()),
    };
    Hit { id, status, duration: start.elapsed().as_millis() as u64, body, timestamp: now_ms() }
}

/// Histogram buckets of test-load.js: (<200ms, 200ms-1s, 1s-5s, 5s-10s, >10s).
pub fn histogram(durations: &[u64]) -> [usize; 5] {
    let mut b = [0; 5];
    for &d in durations {
        let i = [200, 1000, 5000, 10_000].iter().position(|&m| d < m).unwrap_or(4);
        b[i] += 1;
    }
    b
}

// ------------------------------------------------------------------ test-load

pub async fn test_load(args: &[String]) {
    let n = metastable::js::js_parse_int(&arg(args, "requests", "20")).unwrap_or(20.0) as usize;
    let label = arg_opt(args, "save");
    let url = format!("{}/api/combine", gateway_url());
    println!("\n  Sending {n} concurrent requests to {url}...\n");
    let client = reqwest::Client::new();
    let start = Instant::now();
    let results = join_all((1..=n).map(|i| request(&client, i, reqwest::Method::GET, &url, None, Duration::from_secs(30)))).await;
    let total = start.elapsed().as_millis() as u64;

    let line = "─".repeat(60);
    println!("{line}\n  #   Status   Duration   Result\n{line}");
    let mut ok = 0;
    for r in &results {
        if r.ok() {
            ok += 1;
        }
        let icon = if r.ok() { "✅" } else { "❌" };
        let res = if r.ok() { "OK".to_string() } else { r.body.chars().take(50).collect() };
        println!(
            "{icon} {}   {}   {}     {res}",
            pad_start(&r.id.to_string(), 3),
            pad_start(&crate::js_str(&r.status), 6),
            pad_start(&format!("{}s", secs(r.duration as f64, 1)), 6)
        );
    }
    let fail = n - ok;
    let mut d: Vec<u64> = results.iter().map(|r| r.duration).collect();
    d.sort_unstable();
    let avg = d.iter().sum::<u64>() as f64 / d.len().max(1) as f64;
    let rps = n as f64 / (total as f64 / 1000.0);
    println!("{line}");

    let labels = ["< 200ms", "200ms-1s", "1s-5s  ", "5s-10s ", "> 10s  "];
    let h = histogram(&d);
    let max = (*h.iter().max().unwrap_or(&0)).max(1);
    println!("\n  Response Time Histogram:");
    for (l, c) in labels.iter().zip(h) {
        if c > 0 {
            println!("    {l:<9} : {} {c}", "█".repeat(((c as f64 / max as f64) * 25.0).ceil() as usize));
        }
    }
    let max_dur = results.iter().map(|r| r.duration).max().unwrap_or(0).max(1);
    let timeline: String = results
        .iter()
        .map(|r| {
            let b = BLOCKS[((r.duration as f64 / max_dur as f64) * 7.0).floor().min(7.0) as usize];
            if r.ok() { b.to_string() } else { format!("{RED}{b}{RESET}") }
        })
        .collect();
    println!("\n  Latency Timeline (left=first, height=latency):\n    {timeline}");
    println!("    {:<w$}{}s", "0", secs(max_dur as f64, 1), w = n / 2);

    let eq = "═".repeat(50);
    let dash = "─".repeat(50);
    println!("\n{eq}\n  LOAD TEST SUMMARY\n{eq}");
    println!("  Total Requests : {n}\n  Wall Clock     : {}s\n{dash}", secs(total as f64, 2));
    println!("  Success Rate   : {ok}/{n} ({}%)", js_to_fixed(ok as f64 / n as f64 * 100.0, 0));
    println!("  Throughput     : {} req/sec\n{dash}", js_to_fixed(rps, 2));
    for (l, v) in [("Avg", avg), ("P50", pct(&d, 0.5) as f64), ("P90", pct(&d, 0.9) as f64), ("P99", pct(&d, 0.99) as f64)] {
        println!("  Latency {l}    : {}s", secs(v, 2));
    }
    println!("  Latency Min    : {}s\n  Latency Max    : {}s\n{eq}", secs(d[0] as f64, 2), secs(*d.last().unwrap() as f64, 2));

    match label {
        Some(label) => save(
            &label,
            &json!({
                "label": label,
                "scenario": "circuit-breaker",
                "timestamp": iso_now(),
                "config": { "totalRequests": n, "url": url },
                "summary": {
                    "successRate": ok as f64 / n as f64, "successCount": ok, "failureCount": fail,
                    "rps": metastable::js::js_fixed_num(rps, 2), "avgMs": metastable::js::js_round(avg),
                    "p50": pct(&d, 0.5), "p90": pct(&d, 0.9), "p99": pct(&d, 0.99),
                    "minMs": d[0], "maxMs": d[d.len() - 1], "wallClockMs": total,
                },
                "results": results.iter().map(|r| json!({ "id": r.id, "status": r.status, "duration": r.duration })).collect::<Vec<_>>(),
            }),
        ),
        None => println!("\n  Tip: use --save <label> to save results for comparison"),
    }
    println!();
}

fn arg_opt(args: &[String], name: &str) -> Option<String> {
    let v = arg(args, name, "");
    (!v.is_empty()).then_some(v)
}

// ------------------------------------------------------------------ test-orders

/// Consistency scorecard of test-orders.js.
#[derive(Debug, PartialEq)]
pub struct Consistency {
    pub created: u64,
    pub cancelled: u64,
    pub orphaned: u64,
    pub duplicates: u64,
}

pub fn consistency(orders: &Value, payment_count: u64) -> Consistency {
    let empty = vec![];
    let list = orders.get("orders").and_then(Value::as_array).unwrap_or(&empty);
    let created = list.iter().filter(|o| o["status"] == "created").count() as u64;
    let cancelled = list.iter().filter(|o| o["status"] == "cancelled").count() as u64;
    Consistency { created, cancelled, orphaned: created.saturating_sub(payment_count), duplicates: payment_count.saturating_sub(created) }
}

pub async fn test_orders(args: &[String]) {
    let n = metastable::js::js_parse_int(&arg(args, "orders", "10")).unwrap_or(10.0) as usize;
    let label = arg_opt(args, "save");
    let gw = gateway_url();
    let client = reqwest::Client::new();
    let t0 = Instant::now();
    let line = "─".repeat(70);
    println!("\n  Sending {n} orders to {gw}/api/order...\n\n{line}\n  #    Time     Status   Detail\n{line}");
    let (mut ok, mut fail) = (0, 0);
    let mut rows = vec![];
    for i in 1..=n {
        let item = format!("item-{i}");
        let amount = 10 + rand::random::<u32>() % 90;
        let body = json!({ "item": item, "amount": amount }).to_string();
        let h = request(&client, i, reqwest::Method::POST, &format!("{gw}/api/order"), Some(body), Duration::from_secs(15)).await;
        let data: Value = serde_json::from_str(&h.body).unwrap_or_else(|_| json!(h.body));
        let good = h.ok();
        if good {
            ok += 1;
        } else {
            fail += 1;
        }
        let detail = if good {
            format!("Order #{}, Payment #{}", crate::js_str(&data["order"]["id"]), crate::js_str(&data["payment"]["id"]))
        } else {
            data.get("error").map(crate::js_str).unwrap_or_else(|| crate::js_str(&data))
        };
        println!(
            "{} {}  {}     {}   {detail}",
            if good { "✅" } else { "❌" },
            pad_start(&i.to_string(), 3),
            pad_start(&format!("{}s", secs(h.duration as f64, 2)), 7),
            pad_start(&crate::js_str(&h.status), 3)
        );
        rows.push(json!({
            "order": i, "item": item, "amount": amount, "status": h.status, "duration": h.duration,
            "orderId": if good { data["order"]["id"].clone() } else { Value::Null },
            "paymentId": if good { data["payment"]["id"].clone() } else { Value::Null },
            "error": if good { Value::Null } else { data.get("error").cloned().unwrap_or(Value::Null) },
            "orphanedOrderId": data.get("orphanedOrderId").cloned().unwrap_or(Value::Null),
        }));
    }
    let wall = t0.elapsed().as_millis() as u64;
    println!("{line}");
    let mut d: Vec<u64> = rows.iter().map(|r| r["duration"].as_u64().unwrap()).collect();
    d.sort_unstable();
    let avg = d.iter().sum::<u64>() as f64 / d.len().max(1) as f64;

    println!("\n  Checking actual state of services...\n");
    let get = |p: &str| {
        let (c, u) = (client.clone(), format!("{gw}{p}"));
        async move { request(&c, 0, reqwest::Method::GET, &u, None, Duration::from_secs(15)).await }
    };
    let orders: Value = serde_json::from_str(&get("/api/orders").await.body).unwrap_or(json!({}));
    let payments: Value = serde_json::from_str(&get("/api/payments").await.body).unwrap_or(json!({}));
    let order_count = orders["total"].as_u64().unwrap_or(0);
    let payment_count = payments["total"].as_u64().unwrap_or(0);
    let c = consistency(&orders, payment_count);

    let eq = "═".repeat(55);
    let dash = "─".repeat(55);
    println!("{eq}\n  SAGA CONSISTENCY SCORECARD\n{eq}");
    println!("  Orders Attempted    : {n}\n  Successful Responses: {ok}\n  Failed Responses    : {fail}\n{dash}");
    println!("  DB Orders Created   : {order_count} (created: {}, cancelled: {})\n  DB Payments Processed: {payment_count}\n{dash}", c.created, c.cancelled);
    println!("  Latency Avg         : {}s\n  Latency P50         : {}s\n  Latency P90         : {}s", secs(avg, 2), secs(pct(&d, 0.5) as f64, 2), secs(pct(&d, 0.9) as f64, 2));
    println!("  Total Wall Clock    : {}s\n{dash}", secs(wall as f64, 2));
    if c.created > payment_count {
        println!("  ❌ INCONSISTENCY: {} ORPHANED ORDERS\n     (Active orders exist without matching payment)", c.orphaned);
    } else if payment_count > c.created {
        println!("  ❌ INCONSISTENCY: {} DOUBLE CHARGES\n     (More payments than active orders - duplicates!)", c.duplicates);
    } else if c.created > ok as u64 {
        println!("  ❌ INCONSISTENCY: {} PHANTOM ORDERS\n     (More active orders in DB than successful responses - retries created extras)", c.created - ok as u64);
    } else {
        println!("  ✅ SYSTEM CONSISTENT\n     Orders match Payments perfectly.");
    }
    println!("{eq}");
    match label {
        Some(label) => save(
            &label,
            &json!({
                "label": label,
                "scenario": "saga",
                "timestamp": iso_now(),
                "config": { "totalOrders": n, "gateway": gw },
                "summary": {
                    "successRate": ok as f64 / n as f64, "successCount": ok, "failureCount": fail,
                    "avgMs": metastable::js::js_round(avg), "p50": pct(&d, 0.5), "p90": pct(&d, 0.9), "wallClockMs": wall,
                    "ordersCreated": order_count, "ordersActive": c.created, "ordersCancelled": c.cancelled,
                    "paymentsProcessed": payment_count, "orphaned": c.orphaned, "duplicates": c.duplicates,
                    "consistent": c.orphaned == 0 && c.duplicates == 0 && c.created <= ok as u64,
                },
                "results": rows,
            }),
        ),
        None => println!("\n  Tip: use --save <label> to save results for comparison"),
    }
    println!();
}

// ------------------------------------------------------------------ monitor

pub async fn monitor(args: &[String]) {
    let rps = metastable::js::js_parse_int(&arg(args, "rps", "2")).unwrap_or(2.0).max(1.0);
    let duration = metastable::js::js_parse_int(&arg(args, "duration", "60")).unwrap_or(60.0) as u64;
    let endpoint = arg(args, "endpoint", "/api/combine");
    let method = arg(args, "method", "GET").to_uppercase();
    let body = arg_opt(args, "body");
    let label = arg_opt(args, "save");
    let base = gateway_url();
    println!("\n  Starting monitor: {rps} req/s for {duration}s → {method} {base}{endpoint}\n");

    let results = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Hit>::new()));
    let client = reqwest::Client::new();
    let m: reqwest::Method = method.parse().unwrap_or(reqwest::Method::GET);
    let t0 = Instant::now();
    let mut tick = tokio::time::interval(Duration::from_secs_f64(1.0 / rps));
    tick.tick().await;
    let mut last_dash = Instant::now();
    while t0.elapsed() < Duration::from_secs(duration) {
        tick.tick().await;
        let (c, r, m, u, b) = (client.clone(), results.clone(), m.clone(), format!("{base}{endpoint}"), body.clone());
        tokio::spawn(async move {
            let h = request(&c, 0, m, &u, b, Duration::from_secs(15)).await;
            r.lock().unwrap().push(h);
        });
        if last_dash.elapsed() >= Duration::from_secs(1) {
            last_dash = Instant::now();
            let all = results.lock().unwrap().clone();
            if !all.is_empty() {
                let s = stats(&all);
                println!("{}", "─".repeat(60));
                println!("  LIVE MONITOR  ({}s / {duration}s)  {method} {endpoint}", t0.elapsed().as_secs());
                println!("  Total: {}  OK: {}  Fail: {}  |  Success: {}%", all.len(), s.ok, s.fail, js_to_fixed(s.rate * 100.0, 0));
                println!("  Latency: avg={}s  p50={}s  p90={}s", secs(s.avg, 2), secs(s.p50 as f64, 2), secs(s.p90 as f64, 2));
            }
        }
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    let all = results.lock().unwrap().clone();
    let s = stats(&all);
    let mut d: Vec<u64> = all.iter().map(|r| r.duration).collect();
    d.sort_unstable();
    let eq = "═".repeat(55);
    println!("\n{eq}\n  MONITOR FINAL REPORT\n{eq}");
    println!("  Duration        : {duration}s\n  Target RPS      : {rps}\n  Total Requests  : {}", all.len());
    println!("  Actual RPS      : {}\n{}", js_to_fixed(all.len() as f64 / duration as f64, 2), "─".repeat(55));
    println!("  Success Rate    : {}% ({}/{})", js_to_fixed(s.rate * 100.0, 0), s.ok, all.len());
    println!("  Latency Avg     : {}s\n  Latency P50     : {}s\n  Latency P90     : {}s", secs(s.avg, 2), secs(s.p50 as f64, 2), secs(s.p90 as f64, 2));
    if !d.is_empty() {
        println!("  Latency P99     : {}s\n  Latency Min     : {}s\n  Latency Max     : {}s", secs(pct(&d, 0.99) as f64, 2), secs(d[0] as f64, 2), secs(*d.last().unwrap() as f64, 2));
    }
    println!("{}\n  Per-second success rate:", "─".repeat(55));
    let start_ts = all.first().map_or(0, |r| r.timestamp);
    let mut per_sec: std::collections::BTreeMap<u64, (u64, u64)> = Default::default();
    for r in &all {
        let e = per_sec.entry((r.timestamp - start_ts) / 1000).or_default();
        e.0 += 1;
        e.1 += r.ok() as u64;
    }
    let max_sec = per_sec.keys().max().copied().unwrap_or(0);
    let mut tl = String::from("    ");
    for sec in 0..=max_sec {
        tl += &match per_sec.get(&sec) {
            None => format!("{GREY}·{RESET}"),
            Some((n, okc)) => {
                let rate = *okc as f64 / *n as f64;
                if rate >= 0.8 { format!("{GREEN}█{RESET}") } else if rate >= 0.5 { format!("{YELLOW}▄{RESET}") } else if rate > 0.0 { format!("{RED}▂{RESET}") } else { format!("{RED}▁{RESET}") }
            }
        };
    }
    println!("{tl}\n    {GREEN}█{RESET}=80%+ {YELLOW}▄{RESET}=50%+ {RED}▂{RESET}=<50% {GREY}·{RESET}=no req\n{eq}");
    if let Some(label) = label {
        save(
            &label,
            &json!({
                "label": label,
                "scenario": "circuit-breaker",
                "timestamp": iso_now(),
                "config": { "rps": rps, "duration": duration, "endpoint": endpoint, "method": method },
                "summary": {
                    "successRate": s.rate, "successCount": s.ok, "failureCount": s.fail,
                    "rps": metastable::js::js_fixed_num(all.len() as f64 / duration as f64, 2),
                    "avgMs": metastable::js::js_round(s.avg), "p50": s.p50, "p90": s.p90, "p99": pct(&d, 0.99),
                    "minMs": d.first(), "maxMs": d.last(), "wallClockMs": duration * 1000,
                },
                "results": all.iter().enumerate().map(|(i, r)| json!({ "id": i + 1, "status": r.status, "duration": r.duration, "timestamp": r.timestamp })).collect::<Vec<_>>(),
            }),
        );
    }
    println!();
}

pub struct Stats {
    pub ok: u64,
    pub fail: u64,
    pub rate: f64,
    pub avg: f64,
    pub p50: u64,
    pub p90: u64,
}

pub fn stats(results: &[Hit]) -> Stats {
    if results.is_empty() {
        return Stats { ok: 0, fail: 0, rate: 0.0, avg: 0.0, p50: 0, p90: 0 };
    }
    let ok = results.iter().filter(|r| r.ok()).count() as u64;
    let mut d: Vec<u64> = results.iter().map(|r| r.duration).collect();
    d.sort_unstable();
    Stats {
        ok,
        fail: results.len() as u64 - ok,
        rate: ok as f64 / results.len() as f64,
        avg: d.iter().sum::<u64>() as f64 / d.len() as f64,
        p50: pct(&d, 0.5),
        p90: pct(&d, 0.9),
    }
}

// ------------------------------------------------------------------ compare

pub fn pct_change(before: f64, after: f64) -> f64 {
    if before == 0.0 {
        return if after == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (after - before) / before.abs() * 100.0
}

/// Verdict line of compare.js (without colour codes).
pub fn verdict(a: &Value, b: &Value) -> &'static str {
    let (sa, sb) = (&a["summary"], &b["summary"]);
    let f = |s: &Value, k: &str| s[k].as_f64().unwrap_or(0.0);
    if a["scenario"] == "circuit-breaker" {
        if f(sb, "p90") < f(sa, "p90") * 0.5 || f(sb, "rps") > f(sa, "rps") * 1.5 || f(sb, "successRate") > f(sa, "successRate") {
            "VERDICT: System IMPROVED after applying pattern"
        } else if f(sb, "p90") > f(sa, "p90") * 1.2 {
            "VERDICT: System DEGRADED"
        } else {
            "VERDICT: No significant change"
        }
    } else {
        let was = f(sa, "orphaned") > 0.0 || f(sa, "duplicates") > 0.0;
        let now_ok = f(sb, "orphaned") == 0.0 && f(sb, "duplicates") == 0.0;
        if was && now_ok {
            "VERDICT: Data consistency RESTORED! Saga pattern working."
        } else if !was {
            "VERDICT: Before was already consistent (re-run with more orders?)"
        } else if f(sb, "orphaned") + f(sb, "duplicates") < f(sa, "orphaned") + f(sa, "duplicates") {
            "VERDICT: Improved but still inconsistent"
        } else {
            "VERDICT: Still inconsistent — saga not working yet"
        }
    }
}

pub fn compare(args: &[String]) -> i32 {
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let (Some(fa), Some(fb)) = (files.first(), files.get(1)) else {
        println!("\n  Usage: demo compare <before.json> <after.json>\n");
        println!("  Example:\n    demo compare results/before-cb.json results/after-cb.json\n");
        return 1;
    };
    let load = |p: &str| -> Value { serde_json::from_str(&std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{p}: {e}"))).unwrap() };
    let (a, b) = (load(fa), load(fb));
    let eq = "═".repeat(62);
    println!("\n{eq}\n  COMPARISON: {}  vs  {}\n{eq}", crate::js_str(&a["label"]), crate::js_str(&b["label"]));
    println!("  Scenario  : {}\n  Before    : {}\n  After     : {}\n{eq}", crate::js_str(&a["scenario"]), crate::js_str(&a["timestamp"]), crate::js_str(&b["timestamp"]));
    println!("  {:<22} │ {:>10} │ {:>10} │ {:>10}", "Metric", "Before", "After", "Change");
    let sep = || println!("  {}─┼─{}─┼─{}─┼─{}", "─".repeat(22), "─".repeat(11), "─".repeat(11), "─".repeat(12));
    sep();
    let row = |label: &str, k: &str, unit: &str, higher_better: bool| {
        let (x, y) = (a["summary"][k].as_f64().unwrap_or(0.0), b["summary"][k].as_f64().unwrap_or(0.0));
        let fmt = |v: f64| match unit {
            "%" => pad_start(&format!("{}%", js_to_fixed(v * 100.0, 0)), 10),
            "rps" => pad_start(&format!("{} rps", js_to_fixed(v, 2)), 10),
            "s" => pad_start(&format!("{}s", secs(v, 2)), 10),
            _ => pad_start(&js_number(v), 10),
        };
        let p = pct_change(x, y);
        let ps = if !p.is_finite() {
            "     N/A".to_string()
        } else {
            let s = pad_start(&format!("{}{}%", if p > 0.0 { "+" } else { "" }, js_to_fixed(p, 0)), 8);
            let good = if higher_better { p > 0.0 } else { p < 0.0 };
            if p.abs() < 1.0 { format!("{GREY}{s}{RESET}") } else if good { format!("{GREEN}{s} ✅{RESET}") } else { format!("{RED}{s} ❌{RESET}") }
        };
        println!("  {label:<22} │ {} │ {} │ {ps}", fmt(x), fmt(y));
    };
    row("Success Rate", "successRate", "%", true);
    if a["scenario"] == "circuit-breaker" {
        row("Throughput", "rps", "rps", true);
        for (l, k) in [("Latency Avg", "avgMs"), ("Latency P50", "p50"), ("Latency P90", "p90"), ("Latency P99", "p99"), ("Latency Max", "maxMs")] {
            row(l, k, "s", false);
        }
    } else if a["scenario"] == "saga" {
        for (l, k) in [("Latency Avg", "avgMs"), ("Latency P50", "p50"), ("Latency P90", "p90")] {
            row(l, k, "s", false);
        }
        sep();
        row("Orders Created", "ordersCreated", "int", false);
        row("Payments Processed", "paymentsProcessed", "int", true);
        row("Orphaned Orders", "orphaned", "int", false);
        row("Double Charges", "duplicates", "int", false);
    }
    println!("{eq}");
    let v = verdict(&a, &b);
    let colour = if v.contains("IMPROVED") || v.contains("RESTORED") { GREEN } else if v.contains("DEGRADED") || v.contains("Still") { RED } else { YELLOW };
    println!("{colour}  {v}{RESET}\n{eq}\n");
    0
}

/// client/test.js – one GET to the gateway, body printed.
pub async fn test_once() {
    let url = format!("{}/api/combine", gateway_url());
    match reqwest::get(&url).await {
        Ok(r) => println!("Response from Gateway:\n{}", r.text().await.unwrap_or_default()),
        Err(e) => eprintln!("Connection error: {e}"),
    }
}
