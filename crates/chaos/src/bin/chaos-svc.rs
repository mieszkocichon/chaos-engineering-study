//! Experiment service entry point – port of `runner/service-runner.js`.
//! Env contract: specs/001-rust-rewrite/contracts/cli-env.md.

use std::sync::Arc;

use chaos::service::{AppState, router};
use chaos::strategy::{create_strategy, spawn_every};
use chaos::topology::{config_dir, load_fault_profile, load_topology};
use chaos::{gossip, now_ms};
use metastable::js::js_parse_int;
use serde_json::json;

const LEAF_GOSSIP_INTERVAL_MS: u64 = 1000;

fn env_or(k: &str, d: &str) -> String {
    std::env::var(k).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| d.to_string())
}

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1)
}

#[tokio::main]
async fn main() {
    let service_id = std::env::var("SERVICE_ID").ok().filter(|v| !v.is_empty());
    let Some(service_id) = service_id else { fail("[service-runner] SERVICE_ID env var is required") };
    let topology_name = env_or("TOPOLOGY", "fanout");
    let fault_profile = env_or("FAULT_PROFILE", "baseline-flaky");
    let strategy_name = env_or("STRATEGY", "none");
    let seed = js_parse_int(&env_or("SEED", "42"));
    let base_port = js_parse_int(&env_or("BASE_PORT", "4000")).unwrap_or(4000.0) as u16;

    let dir = config_dir();
    // HOST_OVERRIDE (tests): every peer lives on this host instead of Docker DNS name = id
    let host_override = std::env::var("HOST_OVERRIDE").ok().filter(|v| !v.is_empty());
    let host = |id: &str| host_override.clone().unwrap_or_else(|| id.to_string());
    let topology = load_topology(&dir, &topology_name, base_port, &host).unwrap_or_else(|e| fail(&e));
    // loaded for parity with the JS runner (faults are injected by the coordinator)
    load_fault_profile(&dir, &fault_profile).unwrap_or_else(|e| fail(&e));

    let Some(svc) = topology.service(&service_id).cloned() else {
        let ids: Vec<&str> = topology.services.iter().map(|s| s.id.as_str()).collect();
        eprintln!("[service-runner] SERVICE_ID=\"{service_id}\" not found in topology \"{topology_name}\"");
        fail(&format!("[service-runner] Available: {}", ids.join(", ")));
    };

    let static_params = std::env::var("STATIC_PARAMS").ok().filter(|v| !v.is_empty());
    let strategy = create_strategy(&strategy_name, &svc, static_params.as_deref(), &env_or("SWIM_FANOUT", "3"))
        .unwrap_or_else(|e| fail(&e));

    let seed_json = seed.map_or(json!(null), |s| json!(s as i64));
    let state = AppState::new(&svc.id, svc.downstreams.clone(), svc.processing_delay.clone(), seed_json, strategy.clone());

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", svc.port)).await.unwrap_or_else(|e| fail(&format!("bind {}: {e}", svc.port)));
    println!(
        "[{service_id}] Listening on port {} | topology={topology_name} fault={fault_profile} strategy={strategy_name} seed={}",
        svc.port,
        seed.map_or("NaN".into(), |s| metastable::js::js_number(s))
    );

    let health = {
        let st = state.clone();
        move || st.metrics.lock().unwrap().health_summary(now_ms())
    };
    if let Some(s) = &strategy {
        let st = state.clone();
        s.clone().start(state.client.clone(), Arc::new(health.clone()), Arc::new(move || st.metrics.lock().unwrap().snapshot(now_ms())));
        println!("[{service_id}] Strategy \"{strategy_name}\" started");
    } else if !svc.neighbors.is_empty() {
        // LeafGossipPusher: leaves push their health upstream every second
        let (client, id, nbs) = (state.client.clone(), svc.id.clone(), svc.neighbors.clone());
        spawn_every(LEAF_GOSSIP_INTERVAL_MS, move || {
            let (client, id, nbs, h) = (client.clone(), id.clone(), nbs.clone(), health());
            async move {
                let p = gossip::summary_map(&h);
                futures::future::join_all(nbs.iter().map(|n| gossip::push_to(&client, &n.gossip_url, &id, &p))).await;
            }
        });
        println!("[{service_id}] LeafGossipPusher started (pushing to {} neighbors)", svc.neighbors.len());
    }

    let sigterm_id = service_id.clone();
    tokio::spawn(async move {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
        term.recv().await;
        println!("[{sigterm_id}] SIGTERM received, shutting down");
        std::process::exit(0);
    });

    axum::serve(listener, router(state)).await.unwrap();
}
