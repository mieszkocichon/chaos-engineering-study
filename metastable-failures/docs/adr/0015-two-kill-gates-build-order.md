# ADR-0015: Two kill-gates drive the build order

- **Status:** Accepted
- **Date:** 2026-06-28

## Context

The project has two ways to fail that are *existential* — if either is true, there is
no paper — and both are empirical (results, not framing):
1. Metastability might not be reproducible on demand in our setup.
2. The critical-slowing-down precursor might not appear before collapse.

A naive build order (build everything, then check) risks discovering an existential
failure after months (which would suck). The cheapest possible path tests the riskiest assumptions
first.

## Decision

Structure the milestones around two explicit kill-gates, and build nothing
downstream until both are green:
- **Gate A (M1):** an inducible *stuck* state that does not self-recover when the
  trigger is removed, with measurable hysteresis width.
- **Gate B (M2):** the CSD indicators (variance / AR(1)) rise pre-collapse versus a
  matched stable run, with non-zero lead time.

If Gate A fails: no metastability, no project. period. If Gate B fails: the headline (C1) is
dead; pivot to C3 (the double-edged-sword measurement) or report a principled
negative.

## Decision outcome

Both gates passed on 2026-06-28 (model layer): Gate A with λ_down ≈ capacity /
maxAttempts and a measurable loop; Gate B with variance Kendall-τ = 0.591 (ramp) vs
0.000 (stable) and 14.4 s lead time. See `results/m1-gate-a.md`, `results/m2-gate-b.md`.

## Consequences

**Positive**
- Existential risk retired in one session instead of over months. phew.
- A clear, defensible narrative: the riskiest claims were tested first.

**Negative**
- Gate results are from the model layer and n=1; they de-risk the project but are not
  yet the publishable claim (needs real injection, n≥30, and the abrupt-trigger test).

## Related

- ADR-0009, ADR-0010, ADR-0017; PLAN §0 and §8.
