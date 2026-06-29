# ADR-0002: Run an originality survey before writing code

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

Targeting CORE A/A* venues. Main reason systems papers get rejected: core idea is just a known result renamed. (been there done that) Same failure mode we hit with the ECMS "gossip silence" framing. Sinking weeks into a simulator before checking the literature is a great way to waste months on a contribution that doesn't exist.

## Decision

Before writing real code, do a lit survey. Deliverable: **one-sentence delta** vs closest prior art. Classify GREEN/AMBER/RED. Save it in repo as a markdown artifact. Only build if delta is verified open.

## Consequences

**Pros**
- Killed Frugal Gossip fast (hours instead of months) — see ADR-0005.
- Forces crisp, defensible contribution from day 1.
- Free related-work material for the paper.

**Cons**
- Front-loading lit review feels super slow tbh.
- Might miss stuff. Mitigated by ADR-0013 (check primary sources).

## Alternatives

- **Build first, survey later.** Nope. Finding out it's not novel after writing code is exactly what we're trying to avoid, trust me.

## Related

- ADR-0005 (abandon Frugal Gossip), ADR-0006 (direction selection), ADR-0013 (verify primary sources).
