# ADR-0005: Abandon Frugal Gossip after the originality survey

- **Status:** Accepted (supersedes [ADR-0003](0003-initial-direction-frugal-gossip-go-simulator.md))
- **Date:** 2026-06-28

## Context / Background

Did the originality survey for Frugal Gossip (ADR-0002) before writing more code. Read actual primary sources, no summaries. Survey is at `frugal-gossip/docs/originality-survey.md`.

## Findings (the occupied territory)

- **Scheduled fan-out for latency** — Verma & Ooi, ICDCS 2005 (offline-computed fan-out; open-loop).
- **Closed-loop fan-out reduction via redundancy** — Carvalho et al., DSN 2007 + Demers PODC 1987. Basically our proposed "novelty signal" inverted.
- **Budget-constrained failure detection with provable optimum** — So & Sirer, 2007. Collides with the proposed title directly.
- O(log N) latency and Θ(n log n) cost are literally textbook.

Only open angle left: online control under non-stationarity with a regret guarantee. Super thin delta, hard to defend. Worse: pivoting to simulation throws away our only moat (chaos engineering/kernel fault injection). We'd just be the weakest player in a sim-heavy field.

## Decision

Killing Frugal Gossip. It is what it is. Keep `frugal-gossip/` code around for reference (DES architecture/determinism are still useful). Pivoting to something that actually uses our chaos-engineering moat. See ADR-0006.

## Consequences

**Pros**
- Dodged a bullet. Only lost a few hours instead of months. phew.
- Back to using our actual hard-to-replicate strengths.

**Cons**
- Go simulator code gets shelved. Not a total waste though, proved out the patterns.

## Related

- ADR-0002, ADR-0003, ADR-0006, ADR-0013.
