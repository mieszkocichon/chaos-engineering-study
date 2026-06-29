# Project Overview — Metastable Early Warning

**Working paper title:** *Critical Slowing Down as a Model-Free Early-Warning
Signal for Metastable Collapse in Decentralized Systems*

**Branch:** `metastable-early-warning` (off `when-gossip-goes-silent`, to reuse the
ChaosMAS testbed). **Started:** 2026-06-28.

## What a metastable failure is (the intuition)

A highway flows smoothly at 1000 cars/min. A crash (the *trigger*) jams it. The
crash is cleared in 5 minutes — but traffic does **not** recover, because the cars
now stuck drive bumper-to-bumper, brake nervously, and each brake spawns a new jam.
The jam **sustains itself** long after the cause is gone. Clearing it needs a big
intervention (close the on-ramps); removing the original cause is not enough.

That is a metastable failure: the system has a healthy state and a degraded state;
a brief **trigger** pushes it into the degraded state, and a **sustaining effect**
(a positive feedback loop — retry amplification, queue buildup, circuit-breaker
flapping, cache emptying) keeps it stuck. Signature: **hysteresis** — it tips over
at a high stress level but only recovers at a much lower one. Metastable failures
caused at least 4 of 15 major AWS outages in the last decade.

## The thesis (one sentence)

> Prior metastability work predicts vulnerable regions and recovery times from
> **calibrated queueing/CTMC models, offline, for single-service request-response
> systems**; we instead ask whether a **model-free, online early-warning signal
> based on critical-slowing-down statistics (rising variance + lag-1
> autocorrelation of live telemetry)** can anticipate metastable collapse **with
> actionable lead time in a decentralized, gossip-style multi-agent system under
> controlled fault injection** — the regime CTMC abstractions admit they cannot
> capture and that incumbents (HotOS'25) flag as unexplored.

## Why this is *our* project (the moat)

Two assets competitors do not have together:

1. **Real, kernel-level fault injection** (tc netem, Toxiproxy) + chaos-engineering
   competence. Metastability is a *real-system* phenomenon; the sustaining loop
   (retry amplification, queue buildup) is exactly what clean simulators struggle
   to produce. The incumbent model-based line (CTMC) openly concedes it cannot
   model "processor-level contention, thread scheduling, load-balancer behavior"
   and that nothing has practical value "without confirmation against the actual
   system."
2. **An existing decentralized gossip MAS** (`when-gossip-goes-silent/`) whose
   resilience mechanisms — `lib/circuit-breaker.js`, `lib/concurrency-limiter.js`,
   `lib/gossip.js`, retry logic, `backpressure-agent.js` — **are the very
   ingredients of the sustaining loop**. The testbed is almost purpose-built. See
   [03-reusable-assets.md](03-reusable-assets.md).

CSD is a telemetry-statistics method that needs real, noisy dynamics — the clean
exponential distributions of a CTMC/DES are a *weaker* place to test it than a real
system. So the moat and the method reinforce each other.

## The non-obvious result (why it is not a tautology)

The same resilience controls (retry, circuit-breaker, bounded concurrency, gossip)
that protect the system under *small* faults are what *cause* metastable collapse
under *large* ones. That is a measured, non-definitional result — and it reframes
the prior adaptive-resilience work as the *subject* of study, not just the tool.

## The four contributions

See [01-contributions.md](01-contributions.md). In brief:
1. Empirical CSD-precursor study under controlled injection on a gossip MAS.
2. The abrupt-vs-gradual trigger question (the scientific core).
3. Resilience-config as a double-edged sword, quantified.
4. Online, model-free, EWS-triggered mitigation.

## Prior art / delta

See [02-prior-art.md](02-prior-art.md). Headline "phase diagram + validated model"
is **taken** (HotOS'25, SRDS'24, formal'25 — all model-based, offline, single
service). Open lane: **model-free online CSD early-warning on a decentralized
gossip system**, testing the abrupt-trigger regime.

## Venue

DSN (dependability — natural home) or SRDS first; ICDCS/Middleware adjacent; HotOS
for a vision cut.
