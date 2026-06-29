# ADR-0003: Initial direction — Frugal Gossip adaptive-gossip simulator in Go

- **Status:** Superseded by [ADR-0005](0005-abandon-frugal-gossip.md)
- **Date:** 2026-06-28

## Context / Background

Needed to pivot from the ECMS single-host gossip study to a CORE A/A* paper. Idea: **Frugal Gossip**. It's an adaptive controller tuning fan-out/period based on local novelty and queue congestion. Claims O(log N) bounded detection latency at 1M nodes. ECMS testbed (Node/Docker, N≈50) can't hit that scale. Decided to write a from-scratch discrete-event simulator (DES) in Go. Node testbed gets demoted to validation.

## Decision

Build Go DES (`frugal-gossip/`). Single event loop, binary-heap priority queue keyed on `(Time, seq)`. Flat per-node structs, static gossip baseline. Pluggable `Strategy` interface so we can swap adaptive controller and Plumtree without touching core logic. Run via Docker (`golang:1.22`) because I don't have Go installed locally.

## Decision outcome (M1 built and validated)

Built and ran the skeleton. At N=10^4, converged to 99.9% in ~6.2 rounds vs analytical `log_{fanout+1}(N)=6.64`. Bit-identical runs per seed. Pareto corners spread as expected. Engineering is solid.

## Consequences

- Proved out the DES architecture, determinism rules, and Docker workflow. Ported concepts to the metastable project later.
- Kept `frugal-gossip/` code around for reference. Not deleting.

## Why superseded

Ran the originality survey (ADR-0002). Core idea is already taken. Adaptive-fanout-on-redundancy = Carvalho & Pereira (DSN'07) + Demers (PODC'87). Budget-optimal failure detection with provable tradeoffs = So & Sirer (2007). Dead end. See ADR-0005.

## Related

- ADR-0004 (Docker), ADR-0005 (abandon), ADR-0014 (determinism).
