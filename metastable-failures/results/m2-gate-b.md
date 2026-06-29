# M2 — Gate B result (model layer)

**Date:** 2026-06-28 · **Status: GATE B PASSED (first form, n=1).** A model-free
critical-slowing-down precursor appears **before** metastable collapse, with
meaningful lead time, and is cleanly discriminative from a matched stable run.
Both project kill-gates (A and B) are now green. Huge relief.

## Setup

Canonical M1 config (capacity 500 rps, timeout 300 ms, retry budget 3, qmax 500).
Two scenarios over the same horizon:
- **Ramp:** offered load increased slowly 200 → 560 rps over 60 s (a *gradual*
  approach to the tipping point λ_up ≈ 450) — the regime CSD theory expects a
  precursor in (textbook case).
- **Stable:** load held at 260 rps (clearly inside the healthy basin).

Online indicators on the live `queueLen` telemetry: trailing rolling **variance**
and **lag-1 autocorrelation** of the detrended signal (window 5 s); Kendall-τ trend
test over the pre-collapse window. Code: [src/ews.js](../src/ews.js),
[src/run-m2.js](../src/run-m2.js).

## Result

| indicator | ramp (pre-collapse) | stable |
|---|---|---|
| Kendall-τ of rolling **variance** | **0.591** | **0.000** |
| Kendall-τ of rolling AR(1) | 0.781 | 0.000 |
| collapse time | 46.4 s | — (no collapse) |
| **alarm lead time** | **14.4 s** before collapse | — |

`gate_b_pass: true`. The variance of queue length stays flat for the whole healthy
phase of the ramp, then rises sharply ~14 s before goodput collapses; the alarm
(variance exceeding the stable run's 95th percentile, sustained) fires with 14.4 s
of lead. The matched stable run shows zero trend → the signal is not just some artifact of
elapsed time.

## Honest caveats (drive the next milestones)

1. **Model layer, not real injection.** This is the DES; real `tc netem` injection
   on the ChaosMAS services is M3+.
2. **Gradual trigger only.** A slow ramp is the *easy* case for CSD. The scientific
   core (**C2**) is the **abrupt** trigger — CSD theory assumes slow forcing, so
   whether a usable precursor survives a sudden surge is the real open question. M3.
3. **Variance is the workhorse; AR(1) is not robust here.** On the near-constant
   stable signal, AR(1) is ill-conditioned (total mess). Report variance as primary, AR(1) as
   supplementary — and revisit AR(1) on signals with more dynamic range.
4. **n = 1.** This is a gate probe. The claim needs n ≥ 30 seeds with CI on lead
   time and a ROC/AUC of the alarm across collapsing vs stable runs.

## Next

- **M3 (the real test):** abrupt-vs-gradual trigger sweep — lead time vs trigger
  rise-time; find where the precursor becomes too short to act on. ROC/AUC over
  n ≥ 30. This is C1 + C2, the scientific core of the whole thing.
- Then generality (second system, M4) and the C3 double-edged-sword sweep (the
  retry-budget → hysteresis-width relation already observed in M1 is the seed).
