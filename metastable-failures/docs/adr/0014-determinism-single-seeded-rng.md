# ADR-0014: Determinism via a single seeded RNG and (Time, seq) event ordering

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

Gotta have reproducible runs for these venues and for our own sanity during sweeps (trust me). The old Go sim and the new metastable retry-storm model are discrete-event sims. We need bit-identical outputs for a given seed. Can't have nondeterministic event firing or wall-clock BS messing up the hysteresis width or lead time measurements.

## Decision

Baking determinism in from the ground up:
- **Exactly one** explicitly seeded PRNG per run. Thread it everywhere (topology, arrivals, service times, network/loss). Absolutely zero global RNGs. None. No `time.Now()` or `Date.now()` in the sim logic. (Go: `*math/rand.Rand`; Node: `mulberry32`).
- Event queue has to be a binary min-heap ordered by **(Time, seq)**. `seq` is just an incrementing counter on insertion to break timestamp ties predictably. Guarantees stable firing order.
- Only nondeterministic thing allowed in output is `wall_ms` (for perf profiling, not science). Strip it before hashing/diffing.

Tested it: same seed gives identical md5 hashes on both the Go shell and the metastable harness. Different seeds change the output. The "stuck" state triggers reliably on seeds 1-5.

## Consequences

**Pros**
- Sweeps and gate checks are perfectly reproducible. Debugging is a breeze.
- Easy pass for artifact-evaluation. We can regen figures perfectly.

**Cons**
- Strict discipline required. If anyone slips a `Math.random()` in there, reproducibility is shot completely.
  We'll catch it with determinism checks in the `scripts/` and code review.

## Related

- ADR-0003, ADR-0009.
