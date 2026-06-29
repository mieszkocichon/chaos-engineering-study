# ADR-0009: M1 model layer = seeded discrete-event retry-storm; defer real injection

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

Need to pass Gate A for M1: prove we can trigger metastability on demand and build a recovery/hysteresis harness. Yeah, the thesis needs *real* injection eventually (ADR-0007), but whatever. But cheapest way to clear Gate A and tweak the cliff? A solid model of the retry-amplification loop. Needs to run in ms for sweeps.

## Decision

Building M1 as a seeded discrete-event sim of the retry-storm (`metastable-failures/src/sim.js`). Basically a binary-heap event loop + `mulberry32` PRNG. Sustaining effect needs three parts: (1) clients retry on timeout (amplification), (2) server does **wasted work** on dead requests, (3) shedding causes fast retries. Goodput is our order parameter. This is just the *abstract* layer, kinda like the CTMC/DES stuff the incumbents do. Pushing real `tc netem` injection on ChaosMAS to M3+ and multi-host to M4.

## Decision outcome (Gate A passed)

Got metastability on demand. Deterministic. Stuck across seeds 1–5. Did a quasi-static load sweep and got a measurable hysteresis loop. Width scales cleanly with retry budget: **λ_down ≈ capacity / maxAttempts**. Matches the basic amplification model perfectly. With maxAttempts ≥ 8, the collapsed state just absorbs everything in the normal operating range. Logged in `results/m1-gate-a.md`.

## Consequences

**Pros**
- Cleared Gate A fast. Sweeps take ms.
- The retry-budget → hysteresis-width thing is the seed for C3. First real validation of the analytical model (PLAN §7).

**Cons**
- It's just a model, not real injection. "Just a simulation" critique still applies till M3+.
  We'll mitigate this by confirming on actual ChaosMAS services later. Being upfront that M1 is just a model.

## Alternatives

- **Start with fully networked real service (Docker, HTTP) + tc netem.** No go for M1. Way too slow to iterate when we just need to know "does this even cause metastability and can we measure hysteresis?". Kicking it down the road for now, not dropping it completely.

## Related

- ADR-0004, ADR-0007, ADR-0012, ADR-0014.
