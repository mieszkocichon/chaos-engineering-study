# ADR-0024: Rewrite the platform in Rust

- **Status:** Accepted
- **Date:** 2026-10-19

## Context / Background

All runnable code (metastable DES, experiment service with 8 strategies, coordinator,
topology/compose/k8s generators, demos) is JavaScript on Node. We want a single
compiled toolchain, but 1,020 historical results must stay comparable.

## Decision

Rewrite all JavaScript in a Cargo workspace (`crates/metastable`, `crates/chaos`,
`crates/demos`), phase by phase, keeping the HTTP/JSON protocol, env vars, config and
result files identical so mixed JS+Rust clusters work at every step. Python analysis,
matrix shell scripts and k6 stay. Seeded RNGs (`seedrandom` ARC4, mulberry32) are
ported bit-exactly; gossip experiments are compared statistically. Details:
`specs/001-rust-rewrite/` (spec, plan, research R1–R18, tasks).

## Consequences

**Pros**
- One toolchain, smaller images, no runtime.
- Every phase ships unit + integration tests (FR-017), runnable in one command.

**Cons**
- Multi-threaded runtime changes timing → equivalence must be shown (US4).
- Known JS quirks are reproduced on purpose until the reference is retired (research R10).

## Alternatives

- **Stay on Node.** Rejected by project owner.
- **Rewrite only the simulator.** Leaves two toolchains forever.

## Related

- ADR-0001, ADR-0004 (images stay the only toolchain), ADR-0014, ADR-0023.
