# ADR-0026: Accept the Rust port as equivalent; archive the JS implementation

- **Status:** Accepted
- **Date:** 2026-10-20

## Context / Background

SC-004 (specs/001-rust-rewrite) asked the Rust port to reproduce the historical JS matrix.
The equivalence runs (labbook steps 3–11, `specs/001-rust-rewrite/equivalence-report.md`) showed:

- Against history at the published configuration (gateway limited to 0.25 CPU) Rust was far
  better (+34–45 pp success rate, p90 −8…−12 s). JS re-run on the same machine reproduced history,
  so the gap is the implementation, not the machine.
- The Node gateway is CPU-throttled in ≈96 % of CFS periods at 0.25 CPU (Rust ≈20 %).
- With the gateway limit raised to 4 CPUs (no throttling) the implementations converge:
  success rate equivalent in 5/6 cells, identical strategy ranking, p90 within ≈0.4 s.
- Strategy logic, PRNG, simulator and result files are bit/byte-identical on golden vectors.

## Decision

1. The Rust port is accepted as functionally equivalent. Differences under the published 0.25-CPU
   gateway limit are a performance effect (Node throttling), not a logic difference.
2. SC-004 is amended accordingly: equivalence is judged without gateway CPU throttling
   (`GATEWAY_CPUS`), with the ±7 pp / ±15 % margins and identical ranking; residual small gaps are
   documented, not blocking.
3. The JS implementation is not deleted but **archived in `old-implementation/`** (original layout,
   frozen copy of `config/` and `k6-script.js`, its own Dockerfile). `IMPL=js` keeps working for
   compose/swarm/k8s matrices and the mixed-cluster tests; the default becomes `IMPL=rust`.

## Consequences

**Pros**
- One maintained implementation; the reference stays runnable for future comparisons.

**Cons / open**
- Historical results partly measure Node's CPU throttling at the gateway – their interpretation
  should be revisited (outside this feature), as should quirk Q4 (`cascade-crash` never killed a service).
- `old-implementation/when-gossip-goes-silent/config/` is a frozen snapshot and may drift from
  `when-gossip-goes-silent/config/`.

## Related

- ADR-0024, ADR-0025; specs/001-rust-rewrite (research R10, R15), labbook.md 2026-10-20.
