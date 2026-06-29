# ADR-0010: Variance is the primary CSD indicator; AR(1) is supplementary

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

CSD early warning usually needs two things: rising **variance** and rising **lag-1 autocorrelation (AR(1))** of a state variable right before a bifurcation. M2 pipeline (`src/ews.js`) calcs both online over a trailing window on detrended telemetry. Uses Kendall-τ for trend testing.

Ran first Gate B test (gradual ramp to tipping point vs stable run) on `queueLen`:
- variance: Kendall-τ = 0.591 (ramp) vs 0.000 (stable). Super discriminative, 14.4s lead time.
- AR(1): Kendall-τ = 0.781 (ramp) BUT also 0.873 (stable, before tightening baseline). **Total garbage for discrimination** (literally useless). If the signal is near-constant healthy (queue ≈ 0), detrended residuals are basically noise. Trailing-window AR(1) goes ill-conditioned and drifts up even if we aren't crashing.

## Decision

Making **variance the main CSD indicator**. AR(1) is just **supplementary** now. Gate B pass criteria is all about variance: ramp variance trend needs to be strongly positive, lead time > 0, clear margin over stable run. We'll only look at AR(1) again on signals with more dynamic range. Can't let a busted AR(1) math artifact veto a legit variance signal, that'd be dumb.

## Consequences

**Pros**
- Gate B actually uses the signal that works.
- Honest reporting. AR(1) sucking is a finding, we aren't hiding it.

**Cons**
- One indicator isn't as bulletproof as two.
  We'll still report AR(1) and test other signals later (goodput, in-flight retries) to see if we can fix AR(1) conditioning.

## Alternatives

- **Require both variance and AR(1) to pass.** Rejected. Would've tanked a totally valid result just because AR(1) breaks on a flat signal. It's a math glitch, not a missing precursor tbh.

## Related

- ADR-0007, ADR-0009; result in `results/m2-gate-b.md`.
