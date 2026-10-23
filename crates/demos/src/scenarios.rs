//! Scenario starters and comparison runners – ports of `demos/scenarios/*.js` and
//! `demos/starter.js`. Services are started as `demo <service>` child processes with
//! coloured, prefixed output; runners start them directly (no intermediate starter process).

use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

const RESET: &str = "\x1b[0m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";

pub struct Svc {
    pub name: &'static str,
    pub cmd: &'static str,
    pub color: &'static str,
    pub env: &'static [(&'static str, &'static str)],
}

pub struct Scenario {
    pub key: &'static str,
    pub intro: &'static [&'static str],
    pub services: [Svc; 3],
    pub hint: &'static str,
}

const fn svc(name: &'static str, cmd: &'static str, color: &'static str) -> Svc {
    Svc { name, cmd, color, env: &[] }
}

pub const SCENARIOS: [Scenario; 7] = [
    Scenario {
        key: "starter",
        intro: &["Starting all microservices..."],
        services: [svc("Gateway", "gateway", CYAN), svc("Service A", "service-a", GREEN), svc("Service B", "service-b", YELLOW)],
        hint: "To test → open: http://localhost:3000/api/combine",
    },
    Scenario {
        key: "circuit-breaker-timeout",
        intro: &["⚡ CHAOS: Circuit Breaker — Cascading Timeout", "", "Service A will randomly delay responses by 5-15 seconds.", "Gateway has NO circuit breaker → all requests will suffer."],
        services: [svc("Gateway", "gateway", CYAN), svc("Service A (SLOW)", "service-a-slow", GREEN), svc("Service B", "service-b", YELLOW)],
        hint: "Test with: demo test-load",
    },
    Scenario {
        key: "circuit-breaker-cascade",
        intro: &["⚡ CHAOS: Circuit Breaker — Retry Storm", "", "Service B fails ~60% of requests.", "Gateway retries 3x with NO backoff → retry storm!"],
        services: [svc("Gateway (RETRY)", "gateway-retry", CYAN), svc("Service A", "service-a", GREEN), svc("Service B (FLAKY)", "service-b-flaky", YELLOW)],
        hint: "Test with: demo test-load",
    },
    Scenario {
        key: "circuit-breaker-cascade-solution",
        intro: &["✅ SOLUTION: Circuit Breaker Enabled"],
        services: [svc("Gateway (CB)", "gateway-retry-cb", CYAN), svc("Service A", "service-a", GREEN), svc("Service B (FLAKY)", "service-b-flaky", YELLOW)],
        hint: "Test with: demo test-load --save after-cb",
    },
    Scenario {
        key: "saga-lost-payment",
        intro: &["⚡ CHAOS: Saga — Orphaned Orders", "", "Order is created in A, then payment is attempted in B.", "~40% of payments fail → order is left orphaned (no rollback!)."],
        services: [svc("Gateway (ORDERS)", "gateway-orders", CYAN), svc("Service A (ORDERS)", "service-a-orders", GREEN), svc("Service B (PAYMENTS)", "service-b-payments", YELLOW)],
        hint: "Test with: demo test-orders",
    },
    Scenario {
        key: "saga-lost-payment-solution",
        intro: &["✅ SOLUTION: Saga — Orphaned Orders (Compensating Transaction)", "", "Gateway will now ROLLBACK orders if payment fails."],
        services: [svc("Gateway (SAGA)", "gateway-orders-saga", CYAN), svc("Service A (ORDERS)", "service-a-orders", GREEN), svc("Service B (PAYMENTS)", "service-b-payments", YELLOW)],
        hint: "Test with: demo test-orders",
    },
    Scenario {
        key: "saga-double-charge",
        intro: &["⚡ CHAOS: Saga — Double Charge", "", "Service B processes payments slowly (3-5s).", "Gateway timeout is 2s → thinks payment failed → retries.", "Result: duplicate orders + double charges!"],
        services: [
            Svc { name: "Gateway (RETRY+TIMEOUT)", cmd: "gateway-orders", color: CYAN, env: &[("RETRY", "true"), ("TIMEOUT_MS", "2000")] },
            svc("Service A (ORDERS)", "service-a-orders", GREEN),
            Svc { name: "Service B (SLOW PAY)", cmd: "service-b-payments", color: YELLOW, env: &[("SLOW_MODE", "true")] },
        ],
        hint: "Test with: demo test-orders",
    },
];

/// `saga_double_charge_solution.js`: same services as the double-charge chaos, saga gateway.
pub const DOUBLE_CHARGE_SOLUTION: Scenario = Scenario {
    key: "saga-double-charge-solution",
    intro: &["✅ SOLUTION: Saga — Double Charge (Idempotency)", "", "Gateway sends Idempotency-Key. Service B should reject duplicates."],
    services: [
        Svc { name: "Gateway (SAGA+IDEM)", cmd: "gateway-orders-saga", color: CYAN, env: &[("RETRY", "true"), ("TIMEOUT_MS", "2000")] },
        svc("Service A (ORDERS)", "service-a-orders", GREEN),
        Svc { name: "Service B (SLOW PAY)", cmd: "service-b-payments", color: YELLOW, env: &[("SLOW_MODE", "true")] },
    ],
    hint: "Test with: demo test-orders",
};

pub fn find(key: &str) -> Option<&'static Scenario> {
    SCENARIOS.iter().chain(std::iter::once(&DOUBLE_CHARGE_SOLUTION)).find(|s| s.key == key)
}

/// Start the three services; their stdout/stderr is echoed with a coloured prefix.
pub fn start(sc: &Scenario) -> Vec<Child> {
    for l in sc.intro {
        println!("\n{l}");
    }
    println!();
    let exe = std::env::current_exe().unwrap();
    sc.services
        .iter()
        .map(|s| {
            let mut c = Command::new(&exe);
            c.arg(s.cmd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
            for (k, v) in s.env {
                c.env(k, v);
            }
            let mut child = c.spawn().expect("spawn demo service");
            let prefix = format!("{}[{}]{RESET} ", s.color, s.name);
            println!("{prefix}started → PID {}", child.id().unwrap_or(0));
            for (pipe, err) in [(child.stdout.take().map(Pipe::Out), false), (child.stderr.take().map(Pipe::Err), true)] {
                let prefix = prefix.clone();
                if let Some(p) = pipe {
                    tokio::spawn(async move {
                        let mut lines = p.lines();
                        while let Ok(Some(l)) = lines.next_line().await {
                            if err { eprintln!("{prefix}\x1b[31m{l}{RESET}") } else { println!("{prefix}{l}") }
                        }
                    });
                }
            }
            child
        })
        .collect()
}

enum Pipe {
    Out(tokio::process::ChildStdout),
    Err(tokio::process::ChildStderr),
}

impl Pipe {
    fn lines(self) -> tokio::io::Lines<BufReader<Box<dyn tokio::io::AsyncRead + Unpin + Send>>> {
        let r: Box<dyn tokio::io::AsyncRead + Unpin + Send> = match self {
            Pipe::Out(o) => Box::new(o),
            Pipe::Err(e) => Box::new(e),
        };
        BufReader::new(r).lines()
    }
}

/// Run a starter in the foreground until Ctrl+C.
pub async fn run_starter(sc: &Scenario) {
    let _children = start(sc);
    println!("\n{}", sc.hint);
    println!("Stop with: Ctrl+C\n");
    let _ = tokio::signal::ctrl_c().await;
}

async fn stop(children: Vec<Child>) {
    for mut c in children {
        let _ = c.kill().await;
    }
    println!("[Runner] Waiting 3s for ports to clear...");
    tokio::time::sleep(Duration::from_secs(3)).await;
}

/// One measured step: start services, wait 5 s, run a client, stop.
pub async fn step(label: &str, sc: &Scenario, client: &[&str]) {
    let eq = "=".repeat(60);
    println!("\n{eq}\n🚀 {label}\n{eq}");
    println!("[Runner] Starting environment: {}...", sc.key);
    let children = start(sc);
    println!("[Runner] Waiting 5s for services to boot...");
    tokio::time::sleep(Duration::from_secs(5)).await;
    println!("\n[Runner] Running client: demo {}...", client.join(" "));
    let status = Command::new(std::env::current_exe().unwrap()).args(client).status().await;
    if !status.is_ok_and(|s| s.success()) {
        eprintln!("[Runner] Test execution failed.");
    }
    println!("\n[Runner] Stopping environment...");
    stop(children).await;
}

pub async fn run_all_chaos() {
    for (label, key, client) in [
        ("SCENARIO: CIRCUIT BREAKER: CASCADING TIMEOUT", "circuit-breaker-timeout", ["test-load", "--requests", "20"]),
        ("SCENARIO: CIRCUIT BREAKER: RETRY STORM", "circuit-breaker-cascade", ["test-load", "--requests", "20"]),
        ("SCENARIO: SAGA: ORPHANED ORDERS", "saga-lost-payment", ["test-orders", "--orders", "10"]),
        ("SCENARIO: SAGA: DOUBLE CHARGE", "saga-double-charge", ["test-orders", "--orders", "10"]),
    ] {
        step(label, find(key).unwrap(), &client).await;
    }
    println!("\n✅ All chaos scenarios completed.");
}

/// `run_cb_comparison.js` / `run_saga_comparison.js`; `n` = requests or orders.
pub async fn run_comparison(kind: &str, n: &str) {
    let (before, after, client, flag, b_label, a_label) = match kind {
        "cb" => ("circuit-breaker-cascade", "circuit-breaker-cascade-solution", "test-load", "--requests", "before-cb", "after-cb"),
        _ => ("saga-lost-payment", "saga-lost-payment-solution", "test-orders", "--orders", "before-saga", "after-saga"),
    };
    let what = if kind == "cb" { ("Retry Storm", "Circuit Breaker") } else { ("No Saga", "With Saga") };
    step(&format!("STEP: MEASURE \"BEFORE\" ({})", what.0), find(before).unwrap(), &[client, flag, n, "--save", b_label]).await;
    step(&format!("STEP: MEASURE \"AFTER\" ({})", what.1), find(after).unwrap(), &[client, flag, n, "--save", a_label]).await;
    let eq = "=".repeat(60);
    println!("\n{eq}\n📊 COMPARISON REPORT\n{eq}");
    let dir = crate::clients::results_dir();
    let (fa, fb) = (dir.join(format!("{b_label}.json")), dir.join(format!("{a_label}.json")));
    crate::clients::compare(&[fa.to_string_lossy().into(), fb.to_string_lossy().into()]);
}
