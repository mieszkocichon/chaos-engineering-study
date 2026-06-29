# ADR-0018: EWS signal selection, online detrending, and Kendall-τ trend test

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

The early-warning pipeline (`src/ews.js`) has to run online. Needs to compute from live telemetry, zero lookahead, and turn a noisy stream into a solid "is it going up?" call. Bundled a few choices together here.

## Decision

1. **State variable = queue length (`queueLen`) as primary signal.** Near saddle-node, fluctuation recovery slows down. Queue length fluctuations grow and stretch out. Queue length gave us a clean, obvious variance spike (Kendall-τ 0.591 ramp vs 0.000 stable). Goodput and in-flight retries are secondary signals we'll test later.
2. **Trailing-window detrend.** Subtracting a *trailing* rolling mean (5s window = 25 ticks at 200ms) before doing variance/AR(1). Ensures indicators just reflect fluctuations, not slow drift. Only uses past data (realistic for online), unlike a centred/Gaussian filter that cheats with future samples.
3. **Kendall-τ for trend testing.** Computing Kendall-τ of the rolling-variance over the pre-collapse window. Standard, robust "is it rising?" check. Differentiates ramp (high τ) from stable (τ ≈ 0).

## Consequences

**Pros**
- 100% online and deployable. No lookahead, no per-system model needed.
- Kendall-τ is rank-based. Super robust against wonky, non-normal telemetry.

**Cons**
- Trailing windows lag behind a centred filter, eats into lead time a bit. Price of being actually deployable.
- Window length is a hyperparam. Need to report sensitivity on it later.

## Alternatives

- **Centred/Gaussian detrend.** Tossed for the online detector (needs future data). Might use it for offline analysis graphs though.
- **Raw variance without detrending.** Nope. Conflates the trend with the actual fluctuation growth.

## Related

- ADR-0010 (variance primary, AR(1) supplementary), ADR-0020 (alarm/lead time).
