# ADR-0012: Generality is a first-class milestone, not an afterthought

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

Reviewers love to kill telemetry stats papers if they just use one custom testbed. "Is this just your toy?" In the old plan, we left multi-host and the second system for the very end. Way too late. We wouldn't be able to bake it into the core claims or the writing (huge mistake).

## Decision

Bumping generality up to a hard gate (M4) right before C3/C4:
- Need to replicate the main result (C1 + C2) on a **second, independent system**.
  Option A: OSDI'22 open-source metastable examples (`github.com/lexiangh/Metastability`). Very credible since it's the incumbents' code. Option B: basic microservice with thread pool + retrying client.
- Moving at least one **multi-host** replication into M4 (Swarm / k8s / PLGrid). Gotta rule out single-host BS like cgroup contention before we start writing, obviously.
- The real contribution here is **cross-system invariance** for EWS. Proving the same indicators spike before a crash regardless of the stack.

## Consequences

**Pros**
- Absolute must-have for a solid CORE A. Kills the easiest rejection reason.
- Takes C1 from "we got lucky once" to "this is a fundamental property of retry storms."

**Cons**
- A lot more eng work up front. But it's the best ROI for paper robustness.
- It's needed for A*, but still **not enough** on its own (see ADR-0011).

## Alternatives

- **Leave generality at the end.** Nope. Too late to fix the paper narrative. Literally begging for the "artifact" rejection.

## Related

- ADR-0011; PLAN §13 and the revised milestone table (M4).
