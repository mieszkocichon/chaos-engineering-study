# ADR-0017: Abrupt-vs-gradual trigger is the central experimental axis

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

CSD (critical slowing down) theory assumes control param hits the bifurcation **slowly**. Precursor stuff (rising variance / autocorrelation) relies on *slow* forcing. But real metastable triggers? Often **abrupt**. Like a load surge in seconds. So, whether a usable precursor even exists for sudden triggers is a legit open question. This uncertainty is exactly what elevates this from a mechanical port of a known technique to an actual scientific result (contribution C2).

Gate B result so far only used a *gradual* ramp. Basically the easy mode.

## Decision

Making **trigger rise-time (abrupt ↔ gradual)** the core experimental axis for M3 (the scientific meat). Specifically:
- Sweep trigger rise-time from near-instant surge down to slow ramp.
- Measure lead time as a function of rise-time. Pinpoint the boundary where the precursor gets too short to be useful.
- Hypothesis: even with an abrupt trigger, the **amplification-ramp phase** (the seconds/mins where the retry loop builds up) has its own slower dynamics that leave a detectable signature.
- A clean positive *or* a solid negative result here is a win.

Current fault profiles map cleanly to this axis: `config/faults/burst-failure.json` and `adaptive-surge.json` are abrupt; `slow-degradation.json` is gradual.

## Consequences

**Pros**
- Pushes the work from "CSD applied to new domain" to answering a question the EWS lit hasn't touched for software metastability.
- Lead-time-vs-rise-time curve will be our money-shot figure.

**Cons**
- Uncertain result. Abrupt triggers might just give zero usable lead time. I'm okay with this—it's an honest question, negative result still publishable.

## Related

- ADR-0007, ADR-0010, ADR-0015; PLAN §3, §6 (figure 4), §8 (M3).
