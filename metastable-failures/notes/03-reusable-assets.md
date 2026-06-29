# Reusable Assets from ChaosMAS

The existing testbed (branch `when-gossip-goes-silent`, now the base of this branch)
already contains the sustaining-loop ingredients and the injection machinery. M1
audits these; this note maps what each gives us. **Paths are tracked files.**

## The sustaining-loop ingredients (what causes metastability)

| File | Role in this project |
|---|---|
| `when-gossip-goes-silent/lib/circuit-breaker.js` | CB flapping = a sustaining effect; the threshold is a C3 knob |
| `when-gossip-goes-silent/lib/concurrency-limiter.js` | bounded concurrency / queue buildup = the queue dimension of the state space |
| `when-gossip-goes-silent/lib/gossip.js` | gossip traffic that *adds load under stress* — a decentralized-specific amplifier |
| `when-gossip-goes-silent/agent/backpressure-agent.js` | the ODA agent with retry/backpressure = the retry-amplification loop |
| `when-gossip-goes-silent/lib/generic-service.js` | the work-doing service whose goodput is the order parameter |
| `when-gossip-goes-silent/lib/leaf-gossip-pusher.js` | gossip push path (telemetry source) |

`demos/` also has directly relevant scenarios: `scenarios/circuit_breaker_cascade.js`,
`demos/chaos/gateway-retry-cb.js`, `demos/chaos/gateway-retry.js` — existing
cascade/retry setups to adapt.

## The injection machinery (triggers)

| File | Role |
|---|---|
| `when-gossip-goes-silent/lib/infrastructure-fault-injector.js` | programmatic fault injection |
| `when-gossip-goes-silent/config/faults/*.json` | ready trigger profiles: `burst-failure`, `adaptive-surge`, `slow-degradation`, `baseline-flaky`, `network-partition`, `wan-50ms` |
| `tc netem` / Toxiproxy (external) | latency / loss / asymmetric injection |

`config/faults/adaptive-surge.json` and `burst-failure.json` are natural **abrupt**
triggers; `slow-degradation.json` is a natural **gradual** trigger → directly feeds
the C2 abrupt-vs-gradual axis.

## Topologies & orchestration

- `when-gossip-goes-silent/config/topologies/{ring,mesh,scale}-{5,20,30,50}.json`
- `docker-compose-local*.yml` (single-host), `docker-compose-swarm.yml`, k8s
  manifest generators — path to multi-host (PLGrid) later.

## Measurement & analysis discipline (carry over 1:1)

- `analyze_results.py` (Mann–Whitney U + Cliff's δ, seed discipline) — extend with
  the EWS metrics (variance/autocorrelation, lead time, ROC).
- `demos/client/monitor.js`, `compare.js` — load generation & monitoring.

## What must be ADDED (M1/M2)

1. A **goodput meter** with a clean steady-state definition (the order parameter).
2. A **recovery/hysteresis harness**: inject trigger → remove it → classify
   recovered vs stuck; sweep tip-in vs tip-out levels.
3. The **online CSD pipeline**: rolling-window variance + lag-1 autocorrelation +
   Kendall-τ trend test on the telemetry streams, with lead-time accounting.
4. A reliable, parameterized way to **induce metastability on demand** (the M1 gate).
