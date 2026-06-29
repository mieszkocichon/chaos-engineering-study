# ADR-0019: Hysteresis measured by a quasi-static up/down sweep; collapse defined relative to load

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

Gate A needs an actual *measurable* hysteresis width, not just a "hey it got stuck" log. Hit two snags during M1:
1. Transient surge-then-remove test at a fixed baseline always got stuck. Baseline (350 rps) was *inside* the bistable loop (λ_down < 350 < λ_up). Physics are right, but doesn't actually give us λ_up and λ_down.
2. First collapse-detection rule (goodput < 0.5 × capacity) fired falsely at low load. Healthy goodput is obviously low at low load (goodput ≈ λ when λ < capacity). Gave fake collapse warnings at λ = 100.

## Decision

1. **Quasi-static sweep** (`src/sweep-hysteresis.js`): One continuous run. Offered rate staircases **up** then **down**, keeping state across levels. Take steady goodput over the last 40% of the dwell per level. λ_up = lowest up-leg level that collapsed; λ_down = highest down-leg level that recovered. **Hysteresis width = λ_up − λ_down**.
2. **Relative collapse definition:** Level counts as collapsed when goodput < 0.5 × min(λ, nominal_capacity). Basically, system is serving less than half of what a healthy system *should* at that load. Fixes the low-load false positive junk.

## Decision outcome

Clean finite loops for maxAttempts ∈ {2,3,5}. e.g., maxAttempts=3 gave λ_up=450, λ_down=100, width=350. For maxAttempts ≥ 8, collapsed state is basically absorbing in the swept range (λ_down hits the floor). That's an informative result on its own.

## Consequences

**Pros**
- Gets us the canonical bistability artifact (hysteresis loop + width metric).
- Relative collapse definition holds up across the entire load spectrum.

**Cons**
- Quasi-static dwell has to be long enough to hit steady state per level. Too short and we bias λ_up/λ_down. Documented the dwell parameter.

## Related

- ADR-0009, ADR-0016; transient probe in `src/run-m1.js`, sweep in `src/sweep-hysteresis.js`.
