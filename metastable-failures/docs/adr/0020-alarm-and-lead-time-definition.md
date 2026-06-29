# ADR-0020: Alarm and lead-time definition

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

"Variance goes up before collapse" is nice, but we need a concrete, falsifiable detector with a hard lead time number. Otherwise the headline claim (C1) is fluff. Detector needs to avoid two traps: firing on random transient noise (false alarms), and relying on info you wouldn't have online.

## Decision

- **Baseline reference:** 95th percentile of the rolling-variance from a **matched stable run** (same model, load held in healthy basin). This is our "normal fluctuation" envelope.
- **Alarm condition:** On the approaching run, alarm trips at the first tick where rolling variance crosses that 95th-percentile baseline **and stays above it for ≥ 5 consecutive ticks (1 s)**. Sustain requirement kills off single-tick noise spikes.
- **Lead time = collapse_time − alarm_time.** Collapse time is first tick of sustained goodput collapse (relative-to-load logic, see ADR-0019). Gotta have a positive lead time for Gate B.

## Decision outcome

On the gradual ramp, alarm fired 14.4s before collapse. On the matched stable run, variance trend stayed flat (no false alarms). Logged in `results/m2-gate-b.md`.

## Consequences

**Pros**
- Turns the precursor theory into an actual deployable binary detector. Gets us a measurable lead time and bounds the false-positives.
- Stable-run baseline makes "false-positive rate on stable runs" a legit, reportable metric for the n≥30 study.

**Cons**
- 95th-percentile threshold and 5-tick sustain are basically hyperparams. Detector ROC needs to be reported by sweeping them, can't just cherry-pick one operating point.

## Alternatives

- **Fixed absolute variance threshold.** Scrapped. Doesn't port across systems or scales. Relative threshold generalizes way better.
- **Single-tick trigger.** Scrapped. Way too many false alarms on noise.

## Related

- ADR-0010, ADR-0018; ROC/AUC over n≥30 planned in M3.
