# ADR-0021: Self-contained `metastable-failures/` repository layout

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

Working branch has a ton of old baggage (papers, the shelved `frugal-gossip/` Go project, ChaosMAS testbed). We need the new project artifacts (code, notes, plan, results, ADRs) to be easy to find, reason about, and package up later for artifact evaluation. Don't want it tangled up with legacy stuff.

## Decision

Dumping all new work into a single self-contained folder: `metastable-failures/`:

```text
metastable-failures/
  PLAN.md              project plan (milestones, gates, DoD)
  notes/               00-overview, 01-contributions, 02-prior-art, 03-reusable-assets
  src/                 sim.js, ews.js, run-m1.js, run-m2.js, sweep-hysteresis.js
  scripts/             gate-a.sh, find-loop.sh, explore-cliff.sh (run via Docker)
  results/             m1-gate-a.md, m2-gate-b.md (dated evidence)
  docs/adr/            this decision log
  package.json         type: module
```

Reusable ChaosMAS bits (`when-gossip-goes-silent/lib/*`, `config/faults/*`) just referenced in place and ported as needed. No wholesale copying.

## Consequences

**Pros**
- Everything is in one folder for artifact evaluation. Easy to zip up.
- Hard boundary from the dead Go project and old paper tree.
- Separation of concerns: `notes/` for durable rationale, `results/` for dated proof, `docs/adr/` for decisions.

**Cons**
- Some code duplication with legacy testbed when porting parts. Fine with this—keeps the artifact self-contained and drops coupling.

## Related

- ADR-0008, ADR-0023.
