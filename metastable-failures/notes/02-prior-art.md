# Prior Art & Delta (condensed)

Full depth survey with primary-source verification lives at
`../frugal-gossip/docs/depth-survey-2c.md` (created before the rename; the analysis
carries over). This is the distilled version.

## What is TAKEN (do not headline)

| Territory | Owner | Nature |
|---|---|---|
| Vulnerable-region "phase diagram" + recovery-time prediction | **Isaacs/Alvaro/Majumdar — HotOS'25 (AWS+MPI-SWS)** | CTMC→DES→Emulator→StressTest suite; **model-based, offline, design-time**, single-service request-response |
| Queueing-based prediction of metastability | **Habibi et al. — SRDS'24** | CTMC, replicated storage |
| Escape-probability / eigenvalue characterization, recovery-time | **Formal Analysis, arXiv:2510.03551 (2025)** | model-based |
| Incident study + controlled reproduction + taxonomy | **Huang et al. — OSDI'22 ("Wild")** | reproduction was "heroic"; visibility was the hard part |
| Original definition | **Bronson et al. — HotOS'21** | qualitative |
| Self-sustaining cascade detection via causal stitching | **CSnake, arXiv:2509.26529 (2025)** | causal-graph, not statistical precursor |

⚠️ Mirrors the Frugal-vs-So/Sirer trap: the model-based boundary/prediction space is
occupied by a well-funded team actively pushing it. Do **not** compete there.

## What is OPEN (verified)

**Model-free early warning via critical slowing down (CSD).** Rising **variance** and
**lag-1 autocorrelation** as a saddle-node bifurcation is approached — mature in
ecology, climate, finance, and **road-traffic congestion onset** (arXiv:2401.09364),
but **never applied to software / distributed metastable failures.** The whole
metastability line is model-based; none use a model-free telemetry-statistics
precursor. HotOS'25 itself names two unpursued doors:
- "the application of [feedback/control] techniques to metastable failures has not
  been explored";
- plans to extend beyond request-response to **"decentralized, gossip-style
  systems"** — exactly our testbed.

## Why it is not a mechanical port

CSD assumes the control parameter approaches the bifurcation **slowly**; metastable
triggers are often **abrupt**. Whether a usable precursor exists for abrupt-trigger
collapse is genuinely open — that uncertainty *is* the research question (C2).

## The delta sentence

> Prior work predicts vulnerable regions and recovery times from calibrated
> queueing/CTMC models, offline, for single-service request-response systems; we ask
> whether a model-free, online early-warning signal based on critical-slowing-down
> statistics can anticipate metastable collapse with actionable lead time in a
> decentralized gossip-style system under controlled injection — the regime CTMC
> abstractions cannot capture and incumbents flag as unexplored.

## Method lesson banked

A WebFetch summary **hallucinated** that HotOS'25 already proposes CSD early warning.
Reading the PDF directly disproved it. **Verify novelty-critical claims against the
primary source, never a summary.**

## Core references

- Bronson et al., *Metastable Failures in Distributed Systems*, HotOS'21.
- Huang et al., *Metastable Failures in the Wild*, OSDI'22.
- Isaacs, Alvaro, Majumdar et al., *Analyzing Metastable Failures*, HotOS'25.
- Habibi et al., *Queuing-based Analysis and Prediction of Metastable Failures...*, SRDS'24.
- *Formal Analysis of Metastable Failures in Software Systems*, arXiv:2510.03551, 2025.
- Scheffer et al., *Early-warning signals for critical transitions*, Nature 2009.
- Dakos et al., *Methods for detecting early warnings of critical transitions*, PLoS ONE 2012.
- *Anticipating Tipping Points for Disordered Traffic: CSD on the Onset of Congestion*, arXiv:2401.09364, 2024.
- Bury et al., *Deep learning for early warning signals of tipping points*, PNAS 2021.
