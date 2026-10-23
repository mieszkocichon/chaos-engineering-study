//! T043: strategy decision logic vs the JS reference (MAS, Central, Backpressure, Inbound
//! incl. quirk Q1, SWIM peer sampling).

mod common;
use chaos::metrics::{HealthSummary, Snapshot};
use chaos::resilience::CbState;
use chaos::strategy::backpressure::BackpressureAgent;
use chaos::strategy::central::CentralDecider;
use chaos::strategy::mas::*;
use chaos::topology::Neighbor;
use common::*;
use indexmap::IndexMap;
use serde_json::{Value, json};

fn snapshot(v: &Value) -> Snapshot {
    serde_json::from_value::<SnapshotIn>(v.clone()).unwrap().into()
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotIn {
    error_rate: f64,
    avg_latency: f64,
    p50: f64,
    p90: f64,
    p99: f64,
    request_count: u64,
    queue_depth: u64,
    window_ms: u64,
}
impl From<SnapshotIn> for Snapshot {
    fn from(s: SnapshotIn) -> Self {
        Snapshot {
            error_rate: s.error_rate, avg_latency: s.avg_latency, p50: s.p50, p90: s.p90, p99: s.p99,
            request_count: s.request_count, queue_depth: s.queue_depth, window_ms: s.window_ms,
        }
    }
}

fn neighbors(ids: &[&str]) -> Vec<Neighbor> {
    ids.iter()
        .map(|id| Neighbor {
            id: id.to_string(),
            gossip_url: format!("http://{id}:4000/gossip"),
            metrics_url: String::new(),
            control_url: String::new(),
        })
        .collect()
}

fn opt(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}

#[test]
fn mas_agent_matches_js() {
    let g = golden("strategies.json");
    for (c, case) in g["mas"].as_array().unwrap().iter().enumerate() {
        let mut agent = MasAgent::new("gateway", neighbors(&["svc-01", "svc-02", "svc-03"]));
        for (i, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let t = step["t"].as_u64().unwrap();
            for gsp in step["gossip"].as_array().unwrap() {
                let gi = GossipIn {
                    error_rate: opt(gsp, "errorRate"),
                    avg_latency: opt(gsp, "avgLatency"),
                    load: opt(gsp, "load"),
                    timestamp: opt(gsp, "timestamp"),
                    inbound_report: None,
                };
                agent.receive_gossip(gsp["from"].as_str().unwrap(), &gi, t);
            }
            for (ds, state) in step["breakers"].as_object().unwrap() {
                agent.get_or_create(ds, t).lock().unwrap().breaker.state =
                    if state == "open" { CbState::Open } else { CbState::Closed };
            }
            agent.tick(snapshot(&step["local"]), t);
            let label = format!("mas case {c} step {i}");
            assert_json_eq(&agent.params.to_json(), &step["params"], &format!("{label} params"));
            assert_json_eq(agent.decision_history.last().unwrap(), &step["history"], &format!("{label} history"));
            let bp: serde_json::Map<String, Value> = agent
                .pipelines
                .0
                .iter()
                .map(|(id, p)| { let p = p.lock().unwrap(); (id.clone(), json!([p.breaker.failure_threshold, p.breaker.open_timeout_ms])) })
                .collect();
            assert_json_eq(&Value::Object(bp), &step["breakerParams"], &format!("{label} breakers"));
        }
    }
}

#[test]
fn central_matches_js() {
    let g = golden("strategies.json");
    let mut d = CentralDecider::default();
    for (i, step) in g["central"].as_array().unwrap().iter().enumerate() {
        let snaps: IndexMap<String, Value> =
            step["snapshots"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let p = d.decide(step["id"].as_str().unwrap(), &snaps, step["t"].as_u64().unwrap());
        assert_json_eq(&p.to_json(), &step["decision"], &format!("central step {i}"));
        assert_json_eq(d.decision_history.last().unwrap(), &step["history"], &format!("central step {i} history"));
    }
}

#[test]
fn backpressure_matches_js() {
    let g = golden("strategies.json");
    let mut bp = BackpressureAgent::new("gateway");
    for (i, step) in g["backpressure"].as_array().unwrap().iter().enumerate() {
        let t = step["t"].as_u64().unwrap();
        let ds = step["ds"].as_str().unwrap();
        match step["op"].as_str().unwrap() {
            "gossip" => {
                let d = &step["data"];
                bp.receive_gossip(ds, opt(d, "errorRate"), opt(d, "avgLatency"), opt(d, "load"), opt(d, "timestamp"), t);
            }
            "fail" => bp.record_local_failure(ds, t),
            "success" => bp.record_local_success(ds, t),
            _ => {
                let r = bp.should_throttle(ds, t);
                let mut got = json!({ "throttle": r.throttle });
                if let Some(s) = r.source {
                    got["source"] = json!(s);
                }
                if let Some(reason) = r.reason {
                    got["reason"] = json!(reason);
                }
                // JS returns {throttle, source, reason} with key order throttle, source, reason
                let exp = &step["result"];
                assert_eq!(got["throttle"], exp["throttle"], "bp step {i}");
                assert_eq!(got.get("source"), exp.get("source"), "bp step {i}");
                assert_eq!(got.get("reason"), exp.get("reason"), "bp step {i}");
            }
        }
    }
}

#[test]
fn inbound_matches_js_including_quirk_q1() {
    let g = golden("strategies.json");
    let mut agent = MasAgent::new("svc-02", neighbors(&["svc-01", "svc-02", "svc-03"]));
    let mut inbound = Inbound::default();
    for (i, step) in g["inbound"].as_array().unwrap().iter().enumerate() {
        let t = step["t"].as_u64().unwrap();
        let label = format!("inbound step {i}");
        match step["op"].as_str().unwrap() {
            "result" => inbound.record_downstream_result(step["ds"].as_str().unwrap(), step["ok"].as_bool().unwrap(), t),
            "receive" => {
                // through the HTTP receiver: only 4 fields survive (quirk Q1)
                let d = &step["data"];
                let gi = GossipIn {
                    error_rate: opt(d, "errorRate"),
                    avg_latency: opt(d, "avgLatency"),
                    load: opt(d, "load"),
                    timestamp: opt(d, "timestamp"),
                    inbound_report: None,
                };
                agent.receive_gossip(d["fromId"].as_str().unwrap(), &gi, t);
                inbound.receive_gossip("svc-02", d["fromId"].as_str().unwrap(), &gi, t);
            }
            "receiveDirect" => {
                let d = &step["data"];
                let r = &d["inboundReport"];
                let gi = GossipIn {
                    error_rate: opt(d, "errorRate"),
                    avg_latency: opt(d, "avgLatency"),
                    load: opt(d, "load"),
                    timestamp: opt(d, "timestamp"),
                    inbound_report: Some(InboundReport { target: r["target"].as_str().unwrap().into(), drop_rate: num(&r["dropRate"]) }),
                };
                agent.receive_gossip("svc-03", &gi, t);
                inbound.receive_gossip("svc-02", "svc-03", &gi, t);
            }
            _ => {
                let h = &step["health"];
                let health = HealthSummary { error_rate: num(&h["errorRate"]), avg_latency: num(&h["avgLatency"]), load: h["load"].as_u64().unwrap() };
                let got: Vec<Value> = inbound
                    .gossip_payloads(&agent.neighbors, &health, t)
                    .into_iter()
                    .map(|(n, p)| json!({ "url": n.gossip_url, "payload": p }))
                    .collect();
                assert_json_eq(&Value::Array(got), &step["pushed"], &format!("{label} pushed"));
            }
        }
        assert_json_eq(&inbound.stats(), &step["stats"], &format!("{label} stats"));
    }
}

#[test]
fn swim_picks_match_js() {
    let g = golden("strategies.json");
    for case in g["swim"].as_array().unwrap() {
        let id = case["serviceId"].as_str().unwrap();
        let n = case["n"].as_u64().unwrap() as usize;
        let nbs: Vec<String> = (0..n).map(|i| format!("http://n{i}/gossip")).collect();
        let mut picker = SwimPicker::new(id, case["fanout"].as_u64().unwrap() as usize);
        for (r, round) in case["rounds"].as_array().unwrap().iter().enumerate() {
            let got: Vec<&String> = picker.pick(&nbs);
            let exp: Vec<&str> = round.as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(got, exp, "swim {id} round {r}");
        }
    }
}
