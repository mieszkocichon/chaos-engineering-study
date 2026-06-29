# ADR-0008: Branch off `when-gossip-goes-silent` and reuse the ChaosMAS testbed

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

Branching stuff from the session:
1. Cut `frugal-gossip` off `main`. Checkout got blocked by an untracked `README.md` (classic) — backed it up, nuked it, branched from `origin/main`.
2. Metastable project needs the ChaosMAS code. `main` has the base (`c0fcbb7`), but `when-gossip-goes-silent` has `feb0444` which actually contains the multi-agent/gossip stuff plus `lib/*` and `config/faults/*`.

The 2c thesis needs those resilience mechanisms (`circuit-breaker.js`, `concurrency-limiter.js`, etc.). They are the sustaining-loop ingredients.

## Decision

Cut `metastable-early-warning` branch off `origin/when-gossip-goes-silent` instead of `main`. Gets us the full ChaosMAS code for reuse. Put all new work in `metastable-failures/` to keep it clean. Just reference or port testbed code as needed.

## Consequences

**Pros**
- Fault profiles and sustaining-loop stuff are right there.
- New code is isolated in `metastable-failures/`.

**Cons**
- Pulls in old papers and results, so the branch is a bit noisy. Whatever, untracked files linger anyway and the reuse is def worth the clutter.

## Alternatives

- **Branch off `main`.** Nope. Missing `feb0444`'s gossip code, so that's a no go.
- **Branch off `frugal-gossip`.** Nope. Still missing the commit, and mixes up the shelved Go code with the new stuff.

## Related

- ADR-0006, ADR-0009.
