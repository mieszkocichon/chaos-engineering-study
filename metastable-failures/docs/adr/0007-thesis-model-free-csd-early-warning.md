# ADR-0007: Thesis — model-free online CSD early warning; drop the phase-diagram headline

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

Deep-dive lit review on 2c (`frugal-gossip/docs/depth-survey-2c.md`). Checked the primary sources (HotOS'25 PDF read directly cuz AI summaries kept hallucinating on me — see ADR-0013).

Findings:
- **Taken:** "Vulnerable-region phase diagram + recovery-time prediction" is owned by the model-based crowd — Isaacs/Alvaro (HotOS'25), Habibi (SRDS'24), etc. All model-based, offline, single-service req-resp.
- **Open:** *Model-free*, *online* early warning using critical-slowing-down (CSD) stats (variance + lag-1 autocorrelation). Testing it on *decentralized gossip* with *abrupt triggers*. Everyone else is model-based. Nobody is doing model-free telemetry precursors. HotOS'25 literally calls out gossip and feedback control as future work.

The actual science question here: CSD theory assumes you approach the bifurcation *slowly*. Real triggers are *abrupt*. So does a usable precursor even exist? Actually an open question atm.

## Decision

Thesis: *Model-free, online early-warning signal using CSD stats anticipates metastable collapse in a decentralized gossip system under fault injection.* No phase diagrams or calibrated models (too crowded). Focus on the abrupt-vs-gradual trigger question as the core science.

Four contributions:
C1: Empirical CSD precursor.
C2: Abrupt vs gradual question.
C3: Resilience config as a double-edged sword.
C4: EWS-triggered mitigation.

## Consequences

**Pros**
- Completely orthogonal to incumbents. Model-free, online, gossip, abrupt triggers.
- Leverages our moat (real telemetry). CSD is way more meaningful here than in a clean DES.

**Cons**
- C1 alone is too incremental. We really need C2 to deliver something non-obvious. Bit risky ngl.
- AR(1) might blow up on near-constant signals (yep, confirmed later — ADR-0010).

## Related

- ADR-0006, ADR-0010, ADR-0011, ADR-0012, ADR-0013.
