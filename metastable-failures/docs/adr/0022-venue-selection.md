# ADR-0022: Venue selection — DSN / SRDS first, TDSC as the journal alternative

- **Status:** Accepted
- **Date:** 2026-06-28

## Context

The contribution is a dependability result: a model-free early-warning detector for
metastable (congestive-collapse) failures, validated empirically with a light
analytical model. The venue must value rigorous empirical evaluation plus a modest
model, and must be where metastability/dependability work is read and cited (the
incumbents publish at OSDI/HotOS/SRDS).

## Decision

Primary target: **DSN** (natural home for dependability and failure-class work) or
**SRDS** (where Habibi et al.'s queueing analysis of metastability appeared). Journal
alternative: **IEEE TDSC**, which suits the fuller, more complete evaluation a
journal version can carry. Map these to the CORE A / 140-point tier (see ADR-0011).
A* venues (OSDI/NSDI/SOSP/PODC) are out of scope for the first paper and pursued only
under the conditional A* stretch.

## Consequences

**Positive**
- Aligns the paper's shape (empirical + light model) with the venue's expectations.
- Places the work where the relevant community will engage with it.

**Negative**
- DSN/SRDS deadlines and review norms constrain timing and required completeness
  (multi-host, n≥30, generality) — which is why ADR-0012 promotes generality early.

## Related

- ADR-0011, ADR-0012.
