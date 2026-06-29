# ADR-0011: Target CORE A (140) first; A* (200) only conditionally

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

Maintainer keeps asking if this is "good enough for 140 / 200 points". (classic) Look, points come from *results and how deep/surprising the contribution is*, not the doc. Tweaking the plan doesn't raise the ceiling of the core idea. The C1 contribution (taking a known CSD technique to a new domain with solid rigor) is CORE A material. It's not A*. Getting to A* means a massive jump in scope, which is a pain.

## Decision

Aiming for **CORE A / 140** (DSN, SRDS, or maybe TDSC) as the main goal. **A* / 200 is just a conditional stretch**. We only hit it if the science goes crazy well—like if C2 (abrupt triggers) gives us a weird, general, theoretically-provable result. Like a lead-time bound for CSD under sudden loads, or a cross-system law we can validate at scale, or if we actually deploy a detector. Designing it so we *can* pivot to A* without betting the house on it.

## Consequences

**Pros**
- Keeps things realistic. Honest framing for the maintainer.
- "A* stretch" is just an option (PLAN §14), no promises.

**Cons**
- Not gunning for A* from day 1 gives the big labs first-mover advantage.
  Fine with this. Fighting them at A* on their own model-heavy turf is a 12+ month grind with zero guarantees.

## Alternatives

- **Aim straight for 200.** Rejected. We don't have the generality, theory, or scale right now tbh. Probably just results in a fast "Reject: lacks depth".

## Related

- ADR-0007, ADR-0012; PLAN §11–§14.
