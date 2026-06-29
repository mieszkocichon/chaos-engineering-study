# ADR-0006: Select metastable failures (2c) over asymmetry (2a) and partitions (2b)

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

Dumped Frugal Gossip (ADR-0005). Looked at three chaos-engineering-aligned ideas in a breadth survey (`frugal-gossip/docs/direction-survey.md`):

- **2a — Asymmetric / gray-failure observability.** Fits our moat, but heavily occupied: Panorama (OSDI'18), Gray Failure (HotOS'17), the 2026 "Ghost in the Datacenter" paper. High risk of getting scooped.
- **2b — Partition resilience / staleness.** Sandwiched between Age-of-Information theory and systems papers like Nifty (OSDI'20). Needs multi-host day 1. Weak moat.
- **2c — Metastable failures via chaos engineering.** Young field (2021-2026). Methods still open. Perfect moat fit: clean simulators honestly suck at reproducing metastability. Real load + real injection is the way to go. Gets to reuse our ChaosMAS retry/circuit-breaker configs, which are exactly the ingredients for sustaining loops. Can do single-host.

## Decision

Going with **2c — metastable failures**. Maximizes our moat (real testbed + fault injection), least settled field, and we can run it single-host right now.

## Consequences

**Pros**
- Playing to our strengths vs simulator weaknesses.
- Code reuse instead of starting from zero.
- Single-host is fine (unlike partitions which needs a cluster).

**Cons**
- Can't just do "reproduce metastability" (OSDI'22 took that). Need a sharper angle — see ADR-0007.
- Reproducing this stuff reliably is super timing-sensitive. Known risk.

## Alternatives

- 2a and 2b. Both have worse occupancy-to-moat ratios. hard pass.

## Related

- ADR-0005, ADR-0007.
