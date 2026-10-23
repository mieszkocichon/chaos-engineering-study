# ADR-0025: Port V8's fdlibm `log`/`cos` instead of using Rust's libm

- **Status:** Accepted
- **Date:** 2026-10-19

## Context / Background

FR-005/FR-006 (specs/001-rust-rewrite) require seeded runs to be bit-identical to the
JS reference. The simulator draws exponential times with `Math.log`, the service PRNG
draws normals with Box–Muller (`Math.log`, `Math.cos`, `Math.sqrt`). Spike R6 compared
Rust's `f64::ln/cos/sqrt` (glibc, rust:1.97.1-bookworm) with V8 (node:20) on 10⁶ inputs
of exactly the shape the code produces (`1 - mulberry32()`, `2πu`).

## Decision

Result: std `ln` differed from V8 in the last bit for ~10% of inputs, `cos` for a few %
(sqrt only through `ln`). The 90 golden simulator trials still matched, but only because
per-tick counters are insensitive to 1-ulp time shifts – not a guarantee.
We ported V8's `src/base/ieee754.cc` (`log`, `cos` with `__kernel_cos/sin` and the
medium-range `__ieee754_rem_pio2`) to `crates/metastable/src/jsmath.rs`. After the
port all 10⁶ values match for ln, cos and sqrt (`tests/sim.rs::jsmath_matches_v8`).

## Consequences

**Pros**
- Bit-exact parity is guaranteed, not lucky.

**Cons**
- ~250 lines of numeric code to keep. Arguments > 2^19·π/2 fall back to libm for `cos`
  (never happens: callers pass 2πu, u ∈ [0,1)).

## Alternatives

- **Keep `f64::ln`.** Rejected: 10% last-bit mismatch.
- **`libm` crate.** Port of musl, not fdlibm – same class of risk.

## Related

- ADR-0014, ADR-0024; specs/001-rust-rewrite/research.md R6.
