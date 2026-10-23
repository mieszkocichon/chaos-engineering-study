//! MAS strategies (`mas-agent`, `mas-agent-swim`, `mas-agent-inbound`): one runtime around
//! [`MasAgent`], specialised by a [`MasVariant`] that only decides what differs – gossip targets,
//! extra bookkeeping and extra stats. A new MAS flavour = one new `MasVariant` impl.

use std::sync::{Arc, Mutex};

use futures::future::join_all;
use serde_json::{Map, Value, json};

use super::mas::{GossipIn, Inbound, MasAgent, SwimPicker};
use super::{BoxFuture, ResilienceStrategy, call_pipelined, format_pipelined_results, parse_gossip_in, spawn_every};
use crate::metrics::{HealthSummary, Snapshot};
use crate::topology::{Downstream, Neighbor};
use crate::{gossip, now_ms};

pub type Payload = Map<String, Value>;

/// What a MAS flavour adds to the plain agent. Called with the agent locked where the agent is
/// passed in (lock order: agent → variant state).
pub trait MasVariant: Send + Sync + 'static {
    /// `strategyStats.strategy`.
    fn name(&self) -> &'static str;

    /// Gossip round recipients and payloads; default: every neighbor gets the health summary.
    fn gossip_targets(&self, agent: &MasAgent, base: &HealthSummary) -> Vec<(Neighbor, Payload)> {
        let plain = gossip::summary_map(base);
        agent.neighbors.iter().map(|n| (n.clone(), plain.clone())).collect()
    }

    /// After each downstream call (agent not locked).
    fn on_downstream_result(&self, _ds: &str, _ok: bool) {}

    /// After the agent recorded incoming gossip.
    fn on_gossip(&self, _agent: &MasAgent, _from: &str, _g: &GossipIn) {}

    /// Keys placed before / after the agent's own stats (JS spread order).
    fn stats_before(&self, _m: &mut Payload) {}
    fn stats_after(&self, _m: &mut Payload) {}

    fn reset(&self) {}
}

pub struct MasStrategy<V: MasVariant> {
    pub agent: Mutex<MasAgent>,
    pub variant: V,
}

impl<V: MasVariant> MasStrategy<V> {
    pub fn new(service_id: &str, neighbors: Vec<Neighbor>, variant: V) -> Self {
        MasStrategy { agent: Mutex::new(MasAgent::new(service_id, neighbors)), variant }
    }

    async fn gossip_round(&self, client: &reqwest::Client, base: &HealthSummary) {
        let (from, targets) = {
            let a = self.agent.lock().unwrap();
            (a.service_id.clone(), self.variant.gossip_targets(&a, base))
        };
        join_all(targets.iter().map(|(n, p)| gossip::push_to(client, &n.gossip_url, &from, p))).await;
    }
}

impl<V: MasVariant> ResilienceStrategy for MasStrategy<V> {
    fn call_downstreams<'a>(
        &'a self,
        client: &'a reqwest::Client,
        downstreams: &'a [Downstream],
    ) -> BoxFuture<'a, Result<Vec<Value>, String>> {
        Box::pin(async move {
            let calls = downstreams.iter().map(|ds| async move {
                let pl = self.agent.lock().unwrap().get_or_create(&ds.id, now_ms());
                // re-read on every attempt, like JS `_fetch` (the decision tick may change it)
                let r = call_pipelined(client, pl, &ds.url, || self.agent.lock().unwrap().params.timeout_ms).await;
                self.variant.on_downstream_result(&ds.id, r.is_ok());
                r
            });
            Ok(format_pipelined_results(downstreams, join_all(calls).await))
        })
    }

    fn has_gossip_receiver(&self) -> bool {
        true
    }

    fn receive_gossip(&self, body: &Value) {
        let Some((from, g)) = parse_gossip_in(body) else { return };
        let mut a = self.agent.lock().unwrap();
        a.receive_gossip(&from, &g, now_ms());
        self.variant.on_gossip(&a, &from, &g);
    }

    fn stats(&self) -> Value {
        let a = self.agent.lock().unwrap();
        let mut m = Map::new();
        m.insert("strategy".into(), json!(self.variant.name()));
        self.variant.stats_before(&mut m);
        m.extend(a.stats());
        self.variant.stats_after(&mut m);
        Value::Object(m)
    }

    fn reset(&self) {
        self.agent.lock().unwrap().reset();
        self.variant.reset();
    }

    fn start(
        self: Arc<Self>,
        client: reqwest::Client,
        health: Arc<dyn Fn() -> HealthSummary + Send + Sync>,
        snapshot: Arc<dyn Fn() -> Snapshot + Send + Sync>,
    ) {
        let (decide_ms, gossip_ms) = {
            let a = self.agent.lock().unwrap();
            (a.decision_interval_ms, a.gossip_interval_ms)
        };
        let me = self.clone();
        spawn_every(decide_ms, move || {
            let (me, snap) = (me.clone(), snapshot());
            async move { me.agent.lock().unwrap().tick(snap, now_ms()) }
        });
        let me = self;
        spawn_every(gossip_ms, move || {
            let (me, client, health) = (me.clone(), client.clone(), health.clone());
            async move { me.gossip_round(&client, &health()).await }
        });
    }
}

// ------------------------------------------------------------------ variants

/// `mas-agent`: gossip to every neighbor.
pub struct Plain;

impl MasVariant for Plain {
    fn name(&self) -> &'static str {
        "mas-agent"
    }
}

/// `mas-agent-swim`: gossip to `fanout` seeded-random neighbors per round.
pub struct Swim(pub Mutex<SwimPicker>);

impl Swim {
    pub fn new(service_id: &str, swim_fanout: &str) -> Self {
        let fanout = metastable::js::js_parse_int(swim_fanout).unwrap_or(3.0).max(1.0) as usize;
        Swim(Mutex::new(SwimPicker::new(service_id, fanout)))
    }
}

impl MasVariant for Swim {
    fn name(&self) -> &'static str {
        "mas-agent-swim"
    }

    fn gossip_targets(&self, agent: &MasAgent, base: &HealthSummary) -> Vec<(Neighbor, Payload)> {
        let plain = gossip::summary_map(base);
        self.0.lock().unwrap().pick(&agent.neighbors).into_iter().map(|n| (n.clone(), plain.clone())).collect()
    }

    fn stats_before(&self, m: &mut Payload) {
        m.insert("gossipFanout".into(), json!(self.0.lock().unwrap().fanout));
    }
}

/// `mas-agent-inbound`: per-downstream fail rates, inbound reports in gossip.
#[derive(Default)]
pub struct InboundReports(pub Mutex<Inbound>);

impl MasVariant for InboundReports {
    fn name(&self) -> &'static str {
        "mas-agent-inbound"
    }

    fn gossip_targets(&self, agent: &MasAgent, base: &HealthSummary) -> Vec<(Neighbor, Payload)> {
        self.0.lock().unwrap().gossip_payloads(&agent.neighbors, base, now_ms())
    }

    fn on_downstream_result(&self, ds: &str, ok: bool) {
        self.0.lock().unwrap().record_downstream_result(ds, ok, now_ms());
    }

    fn on_gossip(&self, agent: &MasAgent, from: &str, g: &GossipIn) {
        self.0.lock().unwrap().receive_gossip(&agent.service_id, from, g, now_ms());
    }

    fn stats_after(&self, m: &mut Payload) {
        m.insert("inbound".into(), self.0.lock().unwrap().stats());
    }

    fn reset(&self) {
        self.0.lock().unwrap().reset();
    }
}
