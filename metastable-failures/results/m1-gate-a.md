# M1 — Gate A result (model layer)

**Date:** 2026-06-28 · **Status: GATE A PASSED.** Metastability is inducible on
demand, deterministic, seed-robust, with a measurable hysteresis loop whose width
is a clean function of the retry budget — and that function matches the simple
amplification model. This unblocks M2 (finally).

## Harness

A seeded discrete-event simulation of the retry-amplification loop
([src/sim.js](../src/sim.js)) — the abstract/model layer, in the spirit of the
incumbents' CTMC/DES (HotOS'25). Sustaining effect = client retries on timeout +
server doing **wasted work** on already-abandoned requests + fast retries on shed.
Real network injection on the ChaosMAS services comes in later milestones, don't worry.

- Transient recovery probe: [src/run-m1.js](../src/run-m1.js)
- Quasi-static hysteresis sweep: [src/sweep-hysteresis.js](../src/sweep-hysteresis.js)

## Evidence

**1. Stuck after trigger removal (transient).** capacity 500 rps; baseline 350 rps
(healthy); a 10 s surge to 2000 rps collapses goodput 351→20 rps, and after the
surge is removed (load back to 350) goodput stays at **0** — does not self-recover.
`metastable_stuck: true`. Deterministic (identical md5 across repeats) and stuck
across seeds 1–5.

**2. Measurable hysteresis loop (quasi-static).** Slow load ramp up then down:

| maxAttempts | λ_up | λ_down | width | bistable |
|---|---|---|---|---|
| 2 | 500 | 200 | 300 | ✓ |
| 3 | 450 | 100 | 350 | ✓ |
| 5 | 500 | 50 | 450 | ✓ |
| ≥8 | 500 | < 50 (absorbing in range) | — | — |

The up-leg stays healthy to λ≈450–500, then collapses; the down-leg stays collapsed
until λ falls to λ_down before recovering. λ_down < λ_up ⇒ genuine bistable
hysteresis.

## Why this matters beyond Gate A

- **Model validation (PLAN §7).** Recovery happens near **λ_down ≈ capacity /
  maxAttempts** — exactly the simple bistability prediction (collapsed-state
  amplification ≈ maxAttempts; recovery needs amplified load below capacity). The
  napkin model actually has empirical support now.
- **C3 seed (double-edged sword).** The retry budget *sets the size of the trap*:
  more aggressive retry (higher maxAttempts) widens the hysteresis loop and, past
  ~8, makes the collapsed state effectively absorbing in the operating range. The
  same knob that helps under small faults deepens the metastable cliff, which is wild.

## Reproduce

```sh
# in node:20-bookworm, cwd = repo root
sh scripts/gate-a.sh        # determinism + tipping + seed robustness
sh scripts/find-loop.sh     # hysteresis width vs retry budget
node src/sweep-hysteresis.js --attempts 3   # one labelled loop + ASCII plot
```

## Canonical M1 config (lock for downstream milestones)

`capacity=50 slots, serviceMeanMs=100 (=> 500 rps), timeout=300ms, backoff=30ms,
qmax=500`, retry budget `maxAttempts` as the C3 axis. Operating point for the
metastable regime: baseline 350 rps inside the loop [λ_down, λ_up].

## Next (M2 — Gate B)

Compute, on the live goodput/queue/retry telemetry these runs already emit, the
**critical-slowing-down indicators** (rolling variance + lag-1 autocorrelation +
Kendall-τ) and test whether they **rise before** the collapse vs a matched stable
run. That is the headline (C1), fingers crossed.
