//! Demo HTTP services – ports of `demos/{gateway,service-a,service-b}/index.js` and
//! `demos/chaos/*.js`. Randomness is intentionally unseeded, like `Math.random()`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use rand::Rng;
use serde_json::{Value, json};

use crate::{env_or, iso_now};

pub fn a_url() -> String {
    env_or("DEMO_A_URL", "http://localhost:3001")
}

pub fn b_url() -> String {
    env_or("DEMO_B_URL", "http://localhost:3002")
}

fn port(var: &str, default: u16) -> u16 {
    std::env::var(var).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

pub async fn serve(router: Router, port: u16, banner: &[String]) {
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.unwrap_or_else(|e| panic!("bind {port}: {e}"));
    for l in banner {
        println!("{l}");
    }
    axum::serve(listener, router).await.unwrap();
}

fn not_found(msg: &'static str) -> Router {
    Router::new().fallback(move || async move { (StatusCode::NOT_FOUND, Json(json!({ "error": msg }))) })
}

fn health() -> Router {
    Router::new().route("/health", get(|| async { "ok" }))
}

fn rand_f() -> f64 {
    rand::thread_rng().r#gen::<f64>()
}

/// `parseFloat(Math.random().toFixed(3))`
fn rand3() -> f64 {
    metastable::js::js_fixed_num(rand_f(), 3)
}

fn data_a(extra: Option<(&str, Value)>) -> Value {
    let mut v = json!({
        "service": "A",
        "message": "Hello from Service A!",
        "value": (rand_f() * 100.0).floor() as i64,
        "time": iso_now(),
    });
    if let Some((k, x)) = extra {
        v[k] = x;
    }
    v
}

fn data_b() -> Value {
    json!({ "service": "B", "message": "Hello from Service B", "status": "active", "random": rand3() })
}

// ------------------------------------------------------------------ plain services

pub async fn service_a() {
    let p = port("PORT", 3001);
    let r = Router::new().route("/data", get(|| async { Json(data_a(None)) })).merge(health()).merge(not_found("Service A - not found"));
    serve(r, p, &[format!("Service A running → http://localhost:{p}/data")]).await
}

pub async fn service_b() {
    let p = port("PORT", 3002);
    let r = Router::new().route("/data", get(|| async { Json(data_b()) })).merge(health()).merge(not_found("Service B - not found"));
    serve(r, p, &[format!("Service B running → http://localhost:{p}/data")]).await
}

pub async fn service_a_slow() {
    let p = port("PORT", 3001);
    let r = Router::new()
        .route(
            "/data",
            get(|| async {
                let delayed = rand_f() < 0.5;
                if delayed {
                    let ms = 5000 + (rand_f() * 10_000.0).floor() as u64;
                    println!("CHAOS: Adding {}s delay to response...", metastable::js::js_to_fixed(ms as f64 / 1000.0, 1));
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                }
                Json(data_a(Some(("delayed", json!(delayed)))))
            }),
        )
        .merge(health())
        .merge(not_found("Service A - not found"));
    serve(r, p, &[format!("Service A (SLOW CHAOS) running → http://localhost:{p}/data"), "  ~50% of requests will have 5-15s random delay".into()]).await
}

pub async fn service_b_flaky() {
    let p = port("PORT", 3002);
    let counters = Arc::new(Mutex::new((0u64, 0u64)));
    let r = Router::new()
        .route(
            "/data",
            get(move || {
                let counters = counters.clone();
                async move {
                    let mut c = counters.lock().unwrap();
                    c.0 += 1;
                    if rand_f() < 0.6 {
                        c.1 += 1;
                        let pct = metastable::js::js_to_fixed(c.1 as f64 / c.0 as f64 * 100.0, 0);
                        println!("CHAOS: Returning 500! (failed {}/{} = {pct}%)", c.1, c.0);
                        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Internal Server Error - service unstable" }))).into_response();
                    }
                    Json(data_b()).into_response()
                }
            }),
        )
        .merge(health());
    serve(r, p, &[format!("Service B (FLAKY CHAOS) running → http://localhost:{p}/data"), "  ~60% of requests will return 500".into()]).await
}

// ------------------------------------------------------------------ orders / payments

#[derive(Default)]
struct Orders {
    list: Vec<Value>,
    next_id: u64,
}

pub async fn service_a_orders() {
    let p = port("PORT", 3001);
    let st = Arc::new(Mutex::new(Orders { list: vec![], next_id: 1 }));
    let r = Router::new()
        .route("/order", post(create_order))
        .route("/orders", get(list_orders))
        .route("/order/{id}", delete(cancel_order))
        .route("/data", get(|| async { Json(data_a(None)) }))
        .with_state(st)
        .merge(health());
    serve(r, p, &[format!("Service A (ORDERS) running → http://localhost:{p}"), "  POST /order, GET /orders, DELETE /order/:id".into()]).await
}

/// `x || default` for a JSON body field.
fn or_default(v: Option<&Value>, d: Value) -> Value {
    v.filter(|x| crate::truthy(x)).cloned().unwrap_or(d)
}

async fn create_order(State(st): State<Arc<Mutex<Orders>>>, body: axum::body::Bytes) -> Response {
    let b: Value = serde_json::from_slice(&body).unwrap_or(json!({}));
    let mut s = st.lock().unwrap();
    let order = json!({
        "id": s.next_id,
        "item": or_default(b.get("item"), json!("unknown-item")),
        "amount": or_default(b.get("amount"), json!(0)),
        "status": "created",
        "createdAt": iso_now(),
    });
    s.next_id += 1;
    println!("ORDER CREATED: #{} - {} (${})", order["id"], crate::js_str(&order["item"]), crate::js_str(&order["amount"]));
    s.list.push(order.clone());
    (StatusCode::CREATED, Json(order)).into_response()
}

async fn list_orders(State(st): State<Arc<Mutex<Orders>>>) -> Json<Value> {
    let s = st.lock().unwrap();
    Json(json!({ "total": s.list.len(), "orders": s.list }))
}

async fn cancel_order(State(st): State<Arc<Mutex<Orders>>>, Path(id): Path<String>) -> Response {
    let id = metastable::js::js_parse_int(&id);
    let mut s = st.lock().unwrap();
    let Some(o) = s.list.iter_mut().find(|o| o["id"].as_f64() == id) else {
        let shown = id.map_or("NaN".to_string(), metastable::js::js_number);
        return (StatusCode::NOT_FOUND, Json(json!({ "error": format!("Order #{shown} not found") }))).into_response();
    };
    o["status"] = json!("cancelled");
    println!("ORDER CANCELLED: #{}", o["id"]);
    Json(o.clone()).into_response()
}

pub async fn service_b_payments() {
    let p = port("PORT", 3002);
    let slow = std::env::var("SLOW_MODE").as_deref() == Ok("true");
    let st = Arc::new(Mutex::new(Orders { list: vec![], next_id: 1 }));
    let r = Router::new()
        .route(
            "/pay",
            post(move |State(st): State<Arc<Mutex<Orders>>>, body: axum::body::Bytes| async move {
                let b: Value = serde_json::from_slice(&body).unwrap_or(json!({}));
                let id = {
                    let mut s = st.lock().unwrap();
                    s.next_id += 1;
                    s.next_id - 1
                };
                if slow {
                    let ms = 3000 + (rand_f() * 2000.0).floor() as u64;
                    println!("CHAOS: Payment #{id} processing slowly ({}s)...", metastable::js::js_to_fixed(ms as f64 / 1000.0, 1));
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                }
                if !slow && rand_f() < 0.4 {
                    println!("CHAOS: Payment #{id} FAILED (random failure)");
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Payment processing failed" }))).into_response();
                }
                let payment = json!({
                    "id": id,
                    "orderId": b.get("orderId").cloned().unwrap_or(Value::Null),
                    "amount": or_default(b.get("amount"), json!(0)),
                    "status": "completed",
                    "processedAt": iso_now(),
                });
                println!("PAYMENT OK: #{id} for order #{} (${})", crate::js_str(&payment["orderId"]), crate::js_str(&payment["amount"]));
                st.lock().unwrap().list.push(payment.clone());
                (StatusCode::CREATED, Json(payment)).into_response()
            }),
        )
        .route("/payments", get(list_orders))
        .route("/data", get(|| async { Json(data_b()) }))
        .with_state(st)
        .merge(health());
    let mut banner = vec![format!("Service B (PAYMENTS) running → http://localhost:{p}"), format!("  SLOW_MODE: {slow}")];
    banner.push(if slow { "  Payments will take 3-5s (causes gateway timeout)".into() } else { "  ~40% of payments will randomly fail".into() });
    serve(r, p, &banner).await
}

// ------------------------------------------------------------------ HTTP helpers

#[derive(Debug)]
pub struct FetchErr {
    pub msg: String,
    pub abort: bool,
}

/// node-fetch + `if (!res.ok) throw new Error(\`HTTP ${status} – ${statusText}\`)`.
pub async fn get_json_status_text(client: &reqwest::Client, url: &str) -> Result<Value, FetchErr> {
    let res = client.get(url).send().await.map_err(|e| FetchErr { msg: node_fetch_error(url, &e), abort: false })?;
    let st = res.status();
    if !st.is_success() {
        return Err(FetchErr { msg: format!("HTTP {} – {}", st.as_u16(), st.canonical_reason().unwrap_or("")), abort: false });
    }
    res.json().await.map_err(|e| FetchErr { msg: e.to_string(), abort: false })
}

/// `fetchWithTimeout` of the orders gateways: abort after `timeout_ms`, `HTTP ${status}: ${body}`.
pub async fn fetch_timeout(client: &reqwest::Client, method: reqwest::Method, url: &str, body: Option<Value>, headers: &[(&str, String)], timeout_ms: u64) -> Result<Value, FetchErr> {
    let mut req = client.request(method, url).timeout(Duration::from_millis(timeout_ms));
    if let Some(b) = body {
        req = req.json(&b);
    }
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let res = req.send().await.map_err(|e| FetchErr {
        msg: if e.is_timeout() { "The operation was aborted.".into() } else { node_fetch_error(url, &e) },
        abort: e.is_timeout(),
    })?;
    let st = res.status();
    let text = res.text().await.map_err(|e| FetchErr { msg: e.to_string(), abort: e.is_timeout() })?;
    if !st.is_success() {
        return Err(FetchErr { msg: format!("HTTP {}: {text}", st.as_u16()), abort: false });
    }
    serde_json::from_str(&text).map_err(|e| FetchErr { msg: e.to_string(), abort: false })
}

fn node_fetch_error(url: &str, _e: &reqwest::Error) -> String {
    format!("request to {url} failed, reason: connect ECONNREFUSED")
}

// ------------------------------------------------------------------ gateways

fn gateway_port() -> u16 {
    port("GATEWAY_PORT", 3000)
}

/// demos/gateway/index.js – no timeout, no retry.
pub async fn gateway() {
    let p = gateway_port();
    let client = reqwest::Client::new();
    let r = Router::new().route(
        "/api/combine",
        get(move || {
            let client = client.clone();
            async move {
                let (ua, ub) = (format!("{}/data", a_url()), format!("{}/data", b_url()));
                let (a, b) = tokio::join!(get_json_status_text(&client, &ua), get_json_status_text(&client, &ub));
                match (a, b) {
                    (Ok(a), Ok(b)) => Json(json!({ "serviceA": a, "serviceB": b, "gateway": "ok", "timestamp": iso_now() })).into_response(),
                    (Err(e), _) | (_, Err(e)) => {
                        eprintln!("[Gateway error] {}", e.msg);
                        let status = if e.abort { StatusCode::GATEWAY_TIMEOUT } else { StatusCode::BAD_GATEWAY };
                        (status, Json(json!({ "error": "One or more services failed", "details": e.msg }))).into_response()
                    }
                }
            }
        }),
    );
    serve(r, p, &[format!("Gateway running → http://localhost:{p}")]).await
}

async fn fetch_with_retry(client: &reqwest::Client, url: &str, retries: u32) -> Result<Value, FetchErr> {
    let mut attempt = 1;
    loop {
        match get_json_status_text(client, url).await {
            Ok(v) => return Ok(v),
            Err(e) => {
                println!("  RETRY {attempt}/{retries} for {url} → {}", e.msg);
                if attempt >= retries {
                    return Err(e);
                }
            }
        }
        attempt += 1;
    }
}

/// demos/chaos/gateway-retry.js – 3 immediate retries, no breaker (retry storm).
pub async fn gateway_retry() {
    let p = gateway_port();
    let client = reqwest::Client::new();
    let r = Router::new().route(
        "/api/combine",
        get(move || {
            let client = client.clone();
            async move {
                println!("\n--- New request to /api/combine ---");
                let (ua, ub) = (format!("{}/data", a_url()), format!("{}/data", b_url()));
                let (a, b) = tokio::join!(fetch_with_retry(&client, &ua, 3), fetch_with_retry(&client, &ub, 3));
                match (a, b) {
                    (Ok(a), Ok(b)) => Json(json!({ "serviceA": a, "serviceB": b, "gateway": "ok", "timestamp": iso_now() })).into_response(),
                    (Err(e), _) | (_, Err(e)) => {
                        eprintln!("[Gateway error] {}", e.msg);
                        (StatusCode::BAD_GATEWAY, Json(json!({ "error": "One or more services failed after retries", "details": e.msg }))).into_response()
                    }
                }
            }
        }),
    );
    serve(r, p, &[format!("Gateway (RETRY STORM) running → http://localhost:{p}"), "  Retries: 3x per service, NO backoff, NO circuit breaker".into()]).await
}

/// The simple breaker of gateway-retry-cb.js (distinct from the experiment's CB).
#[derive(Clone, Debug)]
pub struct DemoBreaker {
    pub name: &'static str,
    pub failure_threshold: u32,
    pub open_timeout_ms: u64,
    pub state: &'static str,
    pub failure_cnt: u32,
    pub next_attempt: u64,
}

impl DemoBreaker {
    pub fn new(name: &'static str, failure_threshold: u32, open_timeout_ms: u64) -> Self {
        DemoBreaker { name, failure_threshold, open_timeout_ms, state: "closed", failure_cnt: 0, next_attempt: 0 }
    }

    /// Gate before the call: `Err` while open, half-open after the timeout.
    pub fn before(&mut self, now: u64) -> Result<(), String> {
        if self.state == "open" {
            if now < self.next_attempt {
                println!("[{}] Circuit breaker OPEN – skipping call", self.name);
                return Err(format!("CircuitBreakerOpen: service {} unavailable", self.name));
            }
            self.state = "half-open";
            println!("[{}] Circuit breaker HALF‑OPEN – retrying once", self.name);
        }
        Ok(())
    }

    pub fn success(&mut self) {
        self.failure_cnt = 0;
        if self.state != "closed" {
            println!("[{}] Circuit breaker CLOSED after success", self.name);
        }
        self.state = "closed";
    }

    /// `now` is the time captured *before* the call, as in JS.
    pub fn failure(&mut self, now: u64) {
        self.failure_cnt += 1;
        println!("[{}] Circuit breaker failure #{}", self.name, self.failure_cnt);
        if self.state == "half-open" || self.failure_cnt >= self.failure_threshold {
            self.state = "open";
            self.next_attempt = now + self.open_timeout_ms;
            println!("[{}] Circuit breaker OPEN – will retry after {} ms", self.name, self.open_timeout_ms);
        }
    }
}

async fn fetch_with_cb(client: &reqwest::Client, url: &str, br: &Mutex<DemoBreaker>) -> Result<Value, String> {
    // MAX_RETRIES = 1: a single attempt through the breaker
    let now = crate::now_ms();
    br.lock().unwrap().before(now)?;
    match get_json_status_text(client, url).await {
        Ok(v) => {
            br.lock().unwrap().success();
            Ok(v)
        }
        Err(e) => {
            br.lock().unwrap().failure(now);
            println!("  RETRY 1/1 for {url} → {}", e.msg);
            Err(e.msg)
        }
    }
}

/// demos/chaos/gateway-retry-cb.js – breaker per service, degraded response for B.
pub async fn gateway_retry_cb() {
    let p = gateway_port();
    let client = reqwest::Client::new();
    let breakers = Arc::new((Mutex::new(DemoBreaker::new("A", 5, 10_000)), Mutex::new(DemoBreaker::new("B", 3, 5000))));
    let r = Router::new().route(
        "/api/combine",
        get(move || {
            let (client, br) = (client.clone(), breakers.clone());
            async move {
                println!("\n--- New request to /api/combine ---");
                let (ua, ub) = (format!("{}/data", a_url()), format!("{}/data", b_url()));
                let (a, b) = tokio::join!(fetch_with_cb(&client, &ua, &br.0), fetch_with_cb(&client, &ub, &br.1));
                let (a, b) = (a.ok(), b.ok());
                if a.is_none() && b.is_none() {
                    return (StatusCode::BAD_GATEWAY, Json(json!({ "error": "All services unavailable", "timestamp": iso_now() }))).into_response();
                }
                let gw = if b.is_some() { "ok" } else { "degraded" };
                let b = b.unwrap_or_else(|| json!({ "service": "B", "message": "Service B unavailable (circuit breaker open)", "status": "degraded" }));
                Json(json!({ "serviceA": a, "serviceB": b, "gateway": gw, "timestamp": iso_now() })).into_response()
            }
        }),
    );
    serve(r, p, &[format!("Gateway (RETRY + CIRCUIT BREAKER) running → http://localhost:{p}"), "  Retries: 1x per service, NO backoff, CIRCUIT BREAKER enabled".into()]).await
}

#[derive(Clone)]
struct OrdersGw {
    client: reqwest::Client,
    retry: bool,
    timeout_ms: u64,
}

fn orders_gw() -> OrdersGw {
    OrdersGw {
        client: reqwest::Client::new(),
        retry: std::env::var("RETRY").as_deref() == Ok("true"),
        timeout_ms: metastable::js::js_parse_int(&env_or("TIMEOUT_MS", "")).filter(|v| *v != 0.0).unwrap_or(5000.0) as u64,
    }
}

impl OrdersGw {
    async fn call(&self, m: reqwest::Method, path: &str, base: &str, body: Option<Value>, headers: &[(&str, String)]) -> Result<Value, FetchErr> {
        fetch_timeout(&self.client, m, &format!("{base}{path}"), body, headers, self.timeout_ms).await
    }
}

async fn passthrough(g: &OrdersGw, base: &str, path: &str) -> Response {
    match g.call(reqwest::Method::GET, path, base, None, &[]).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, Json(json!({ "error": e.msg }))).into_response(),
    }
}

fn order_body(b: &Value) -> (Value, Value) {
    (b.get("item").cloned().unwrap_or(Value::Null), b.get("amount").cloned().unwrap_or(Value::Null))
}

fn json_body(b: &axum::body::Bytes) -> Value {
    serde_json::from_slice(b).unwrap_or(json!({}))
}

/// demos/chaos/gateway-orders.js – no saga; RETRY=true re-runs the whole flow (duplicates).
pub async fn gateway_orders() {
    let p = gateway_port();
    let g = orders_gw();
    let (retry, t) = (g.retry, g.timeout_ms);
    let g1 = g.clone();
    let r = Router::new()
        .route(
            "/api/order",
            post(move |body: axum::body::Bytes| {
                let g = g1.clone();
                async move {
                    let (item, amount) = order_body(&json_body(&body));
                    println!("\n--- New order: {} ${} ---", crate::js_str(&item), crate::js_str(&amount));
                    let order_req = json!({ "item": item, "amount": amount });
                    let order = match g.call(reqwest::Method::POST, "/order", &a_url(), Some(order_req.clone()), &[]).await {
                        Ok(o) => o,
                        Err(e) => {
                            eprintln!("  Step 1 FAILED: Could not create order → {}", e.msg);
                            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Failed to create order", "details": e.msg }))).into_response();
                        }
                    };
                    println!("  Step 1 OK: Order #{} created in Service A", order["id"]);
                    let pay_body = json!({ "orderId": order["id"], "amount": amount });
                    let payment = match g.call(reqwest::Method::POST, "/pay", &b_url(), Some(pay_body.clone()), &[]).await {
                        Ok(pmt) => {
                            println!("  Step 2 OK: Payment #{} processed in Service B", pmt["id"]);
                            pmt
                        }
                        Err(e) => {
                            eprintln!("  Step 2 FAILED: Payment failed → {}", e.msg);
                            if !g.retry {
                                return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
                                    "error": "Payment failed — order is orphaned (no saga rollback!)",
                                    "orphanedOrderId": order["id"],
                                    "details": e.msg,
                                }))).into_response();
                            }
                            println!("  RETRYING entire flow (will cause duplicates)...");
                            let retried = async {
                                let order2 = g.call(reqwest::Method::POST, "/order", &a_url(), Some(order_req), &[]).await?;
                                println!("  Retry: Order #{} created (DUPLICATE!)", order2["id"]);
                                let pmt = g.call(reqwest::Method::POST, "/pay", &b_url(), Some(pay_body), &[]).await?;
                                println!("  Retry: Payment processed (may also be DUPLICATE!)");
                                Ok::<Value, FetchErr>(pmt)
                            }
                            .await;
                            match retried {
                                Ok(pmt) => pmt,
                                Err(re) => {
                                    eprintln!("  Retry also FAILED: {}", re.msg);
                                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
                                        "error": "Order flow failed after retry",
                                        "orphanedOrders": [order["id"]],
                                        "details": re.msg,
                                    }))).into_response();
                                }
                            }
                        }
                    };
                    Json(json!({ "message": "Order completed", "order": order, "payment": payment, "timestamp": iso_now() })).into_response()
                }
            }),
        )
        .merge(order_reads(g.clone()))
        .route(
            "/api/combine",
            get(move || {
                let g = g.clone();
                async move {
                    let (ua, ub) = (a_url(), b_url());
                    let (a, b) = tokio::join!(
                        g.call(reqwest::Method::GET, "/data", &ua, None, &[]),
                        g.call(reqwest::Method::GET, "/data", &ub, None, &[])
                    );
                    match (a, b) {
                        (Ok(a), Ok(b)) => Json(json!({ "serviceA": a, "serviceB": b, "gateway": "ok", "timestamp": iso_now() })).into_response(),
                        (Err(e), _) | (_, Err(e)) => {
                            eprintln!("[Gateway error] {}", e.msg);
                            (StatusCode::BAD_GATEWAY, Json(json!({ "error": "Service failure", "details": e.msg }))).into_response()
                        }
                    }
                }
            }),
        );
    serve(r, p, &[format!("Gateway (ORDERS) running → http://localhost:{p}"), format!("  RETRY: {retry}, TIMEOUT: {t}ms"), "  POST /api/order — no saga, no compensation!".into()]).await
}

fn order_reads(g: OrdersGw) -> Router {
    let g2 = g.clone();
    Router::new()
        .route("/api/orders", get(move || { let g = g.clone(); async move { passthrough(&g, &a_url(), "/orders").await } }))
        .route("/api/payments", get(move || { let g = g2.clone(); async move { passthrough(&g, &b_url(), "/payments").await } }))
}

/// demos/chaos/gateway-orders-saga.js – compensating transaction + idempotency key.
pub async fn gateway_orders_saga() {
    let p = gateway_port();
    let g = orders_gw();
    let (retry, t) = (g.retry, g.timeout_ms);
    let g1 = g.clone();
    let r = Router::new()
        .route(
            "/api/order",
            post(move |body: axum::body::Bytes| {
                let g = g1.clone();
                async move {
                    let (item, amount) = order_body(&json_body(&body));
                    println!("\n--- New order: {} ${} ---", crate::js_str(&item), crate::js_str(&amount));
                    match saga(&g, item, amount).await {
                        Ok((order, payment)) => {
                            Json(json!({ "message": "Order completed", "order": order, "payment": payment, "timestamp": iso_now() })).into_response()
                        }
                        Err(msg) => {
                            eprintln!("  Order saga failed: {msg}");
                            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": msg }))).into_response()
                        }
                    }
                }
            }),
        )
        .merge(order_reads(g));
    serve(r, p, &[format!("Gateway (SAGA) running → http://localhost:{p}"), format!("  RETRY: {retry}, TIMEOUT: {t}ms"), "  POST /api/order — with saga compensation!".into()]).await
}

async fn saga(g: &OrdersGw, item: Value, amount: Value) -> Result<(Value, Value), String> {
    let order = g.call(reqwest::Method::POST, "/order", &a_url(), Some(json!({ "item": item, "amount": amount })), &[]).await.map_err(|e| e.msg)?;
    println!("  Step 1 OK: Order #{} created in Service A", order["id"]);
    let max_retries = if g.retry { 2 } else { 0 };
    let key = format!("pay_order_{}", crate::js_str(&order["id"]));
    let mut attempts = 0;
    loop {
        let r = g
            .call(reqwest::Method::POST, "/pay", &b_url(), Some(json!({ "orderId": order["id"], "amount": amount })), &[("Idempotency-Key", key.clone())])
            .await;
        match r {
            Ok(payment) => {
                println!("  Step 2 OK: Payment #{} processed in Service B", payment["id"]);
                return Ok((order, payment));
            }
            Err(e) => {
                attempts += 1;
                if attempts > max_retries {
                    eprintln!("  Step 2 FAILED: Payment failed → {}", e.msg);
                    cancel(g, &order["id"]).await;
                    return Err(format!("Payment failed after {attempts} attempt(s), order rolled back: {}", e.msg));
                }
                eprintln!("  Payment retry #{attempts} failed: {}", e.msg);
            }
        }
    }
}

async fn cancel(g: &OrdersGw, id: &Value) {
    match g.call(reqwest::Method::DELETE, &format!("/order/{}", crate::js_str(id)), &a_url(), None, &[]).await {
        Ok(_) => println!("  SAGA COMPENSATION: Order #{} cancelled", crate::js_str(id)),
        Err(e) => eprintln!("  SAGA COMPENSATION FAILED: Could not cancel order #{}: {}", crate::js_str(id), e.msg),
    }
}
