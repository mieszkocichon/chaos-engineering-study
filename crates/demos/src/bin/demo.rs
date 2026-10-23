//! `demo <command> [args]` – see crates/demos/README.md for the JS → command mapping.

use demos::{clients, scenarios, services};

const USAGE: &str = "usage: demo <command> [args]
services : gateway service-a service-b gateway-retry gateway-retry-cb gateway-orders
           gateway-orders-saga service-a-orders service-a-slow service-b-flaky service-b-payments
clients  : test-load [--requests N] [--save L] | test-orders [--orders N] [--save L]
           monitor [--rps R --duration S --endpoint P --method M --body B --save L]
           compare <before.json> <after.json> | test
scenarios: starter | scenario <key> | run-all-chaos | run-cb-comparison | run-saga-comparison
           keys: circuit-breaker-timeout circuit-breaker-cascade circuit-breaker-cascade-solution
                 saga-lost-payment saga-lost-payment-solution saga-double-charge saga-double-charge-solution";

#[tokio::main]
async fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let rest = argv.get(1..).unwrap_or_default();
    match argv.first().map(String::as_str) {
        Some("gateway") => services::gateway().await,
        Some("service-a") => services::service_a().await,
        Some("service-b") => services::service_b().await,
        Some("gateway-retry") => services::gateway_retry().await,
        Some("gateway-retry-cb") => services::gateway_retry_cb().await,
        Some("gateway-orders") => services::gateway_orders().await,
        Some("gateway-orders-saga") => services::gateway_orders_saga().await,
        Some("service-a-orders") => services::service_a_orders().await,
        Some("service-a-slow") => services::service_a_slow().await,
        Some("service-b-flaky") => services::service_b_flaky().await,
        Some("service-b-payments") => services::service_b_payments().await,
        Some("test-load") => clients::test_load(rest).await,
        Some("test-orders") => clients::test_orders(rest).await,
        Some("monitor") => clients::monitor(rest).await,
        Some("compare") => std::process::exit(clients::compare(rest)),
        Some("test") => clients::test_once().await,
        Some("starter") => scenarios::run_starter(scenarios::find("starter").unwrap()).await,
        Some("scenario") => match rest.first().and_then(|k| scenarios::find(k)) {
            Some(sc) => scenarios::run_starter(sc).await,
            None => {
                eprintln!("{USAGE}");
                std::process::exit(1);
            }
        },
        Some("run-all-chaos") => scenarios::run_all_chaos().await,
        Some("run-cb-comparison") => scenarios::run_comparison("cb", "200").await,
        Some("run-saga-comparison") => scenarios::run_comparison("saga", "20").await,
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(1);
        }
    }
}
