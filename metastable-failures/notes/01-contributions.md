# The Four Contributions

Each contribution is paired with the reviewer question it answers and the figure /
artifact that proves it. Together they form the "definition of done" for the paper.

## C1 — Empirical critical-slowing-down precursor on a gossip MAS  ★ headline

**Claim.** Under controlled fault injection, the statistical signature of an
approaching saddle-node bifurcation — **rising variance and rising lag-1
autocorrelation** of live telemetry (goodput, queue length, retry ratio, gossip
staleness) — appears **before** a metastable collapse in a decentralized,
gossip-style multi-agent system, and does so **model-free** (no per-system CTMC
calibration).

- *Reviewer question:* "Is there actually an observable precursor, or do you only
  know in hindsight?"
- *Proof:* time-series figure — EWS indicators rising while goodput is still
  nominal, with the collapse marked; contrasted against a matched stable run where
  they stay flat. ROC/AUC of the EWS as a collapse predictor across many seeds.

## C2 — The abrupt-vs-gradual trigger question  ★ scientific core

**Claim.** CSD theory assumes the control parameter approaches the bifurcation
*slowly*. Real metastable triggers are often **abrupt** (a load surge in seconds).
We characterize *when* a usable precursor exists: we show the **amplification-ramp**
phase (the seconds–minutes while the retry/feedback loop builds after an abrupt
trigger) carries its own slower dynamics that leave a detectable signature — and we
map the lead time as a function of trigger abruptness.

- *Reviewer question:* "CSD needs slow forcing; your triggers are sudden — why would
  it work at all?"
- *Proof:* lead-time vs trigger-rise-time curve; the regime boundary where the
  precursor becomes too short to be actionable. A clean positive *or* a principled
  negative result is a contribution.

## C3 — Resilience configuration as a double-edged sword  ★ narrative hook

**Claim.** The same retry budget / circuit-breaker thresholds / concurrency limits
that *improve* availability under small faults *create and deepen* the metastability
cliff under large ones. We quantify this non-monotonicity over the existing
controls (`lib/circuit-breaker.js`, `lib/concurrency-limiter.js`, retry).

- *Reviewer question:* "Isn't this just bad configuration? What is the general
  lesson?"
- *Proof:* non-monotone curve — outcome (recovers / stuck) vs resilience-aggressiveness
  at fixed trigger; the cliff location as a function of the config. Directly reuses
  and reframes the prior adaptive-resilience work.

## C4 — Online, model-free, EWS-triggered mitigation  ★ actionability

**Claim.** Because the EWS is model-free and online, it can drive a *pre-emptive*
intervention (load shedding / retry-budget throttle / temporary fan-out cut) that
fires before collapse and measurably shrinks the metastable region — unlike the
incumbents' design-time, model-based defenses.

- *Reviewer question:* "So what — what do I do with the warning?"
- *Proof:* with-vs-without-mitigation comparison: recovery rate, recovery time, and
  the shifted tip-in threshold; cost of false-positive interventions accounted for.

## How they compose

C1 establishes the signal exists; C2 bounds *when* it is usable; C3 explains *why*
the system is vulnerable in the first place (and ties to prior work); C4 turns the
signal into action. The light analytical model (a 1-D bistability / fluid model of
retry amplification — see PLAN §8) underpins all four by explaining *why* a
saddle-node bifurcation, and hence CSD, is expected here.
