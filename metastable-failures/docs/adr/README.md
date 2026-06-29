# Architecture Decision Records

Logging the major calls made while putting together the research project in `metastable-failures/`. Left the **superseded** ones in here on purpose—dead ends are part of the process and explain why things are the way they are right now.

Format: lightweight MADR. Status, Context, Decision, Consequences, Alternatives, and related links per record.

| ADR | Title | Status |
|-----|-------|--------|
| [0001](0001-record-architecture-decisions.md) | Record architecture decisions | Accepted |
| [0002](0002-originality-survey-before-code.md) | Run an originality survey before writing code | Accepted |
| [0003](0003-initial-direction-frugal-gossip-go-simulator.md) | Initial direction: Frugal Gossip adaptive-gossip simulator in Go | Superseded by 0005 |
| [0004](0004-run-everything-via-docker.md) | Run all builds and simulations via Docker, no host toolchains | Accepted |
| [0005](0005-abandon-frugal-gossip.md) | Abandon Frugal Gossip after the originality survey | Accepted |
| [0006](0006-select-direction-2c-metastable.md) | Select metastable failures (2c) over asymmetry (2a) and partitions (2b) | Accepted |
| [0007](0007-thesis-model-free-csd-early-warning.md) | Thesis: model-free online CSD early warning; drop the phase-diagram headline | Accepted |
| [0008](0008-branch-and-reuse-strategy.md) | Branch off `when-gossip-goes-silent` and reuse the ChaosMAS testbed | Accepted |
| [0009](0009-m1-model-layer-des-retry-storm.md) | M1 model layer = seeded discrete-event retry-storm; defer real injection | Accepted |
| [0010](0010-variance-primary-csd-indicator.md) | Variance is the primary CSD indicator; AR(1) is supplementary | Accepted |
| [0011](0011-target-core-a-140-first.md) | Target CORE A (140) first; A* (200) only conditionally | Accepted |
| [0012](0012-generality-first-class-milestone.md) | Generality is a first-class milestone, not an afterthought | Accepted |
| [0013](0013-verify-novelty-against-primary-source.md) | Verify novelty-critical claims against the primary source | Accepted |
| [0014](0014-determinism-single-seeded-rng.md) | Determinism via a single seeded RNG and (Time, seq) event ordering | Accepted |
| [0015](0015-two-kill-gates-build-order.md) | Two kill-gates drive the build order | Accepted |
| [0016](0016-retry-storm-model-assumptions.md) | Retry-storm model assumptions (amplification, wasted work, shedding) | Accepted |
| [0017](0017-abrupt-vs-gradual-trigger-axis.md) | Abrupt-vs-gradual trigger is the central experimental axis | Accepted |
| [0018](0018-ews-signal-and-trend-test.md) | EWS signal selection, online detrending, Kendall-τ trend test | Accepted |
| [0019](0019-hysteresis-measurement-quasi-static-sweep.md) | Hysteresis via quasi-static sweep; collapse defined relative to load | Accepted |
| [0020](0020-alarm-and-lead-time-definition.md) | Alarm and lead-time definition | Accepted |
| [0021](0021-repository-layout.md) | Self-contained `metastable-failures/` repository layout | Accepted |
| [0022](0022-venue-selection.md) | Venue selection — DSN / SRDS first, TDSC alternative | Accepted |
| [0023](0023-statistical-rigor-standard.md) | Carry over the statistical-rigor standard (n≥30, MWU + Cliff's δ) | Accepted |

Numbering is chronological by the order decisions were taken on 2026-06-28.
