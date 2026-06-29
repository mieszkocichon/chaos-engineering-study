# ADR-0013: Verify novelty-critical claims against the primary source

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

While digging into metastability (ADR-0007), a `WebFetch` summary of that HotOS'25 paper ("Analyzing Metastable Failures") threw a wrench in things. It hallucinated quotes claiming the paper *already* had a model-free CSD early-warning signal (variance/autocorr). If true, our whole contribution is dead. The tool did mention the PDF was compressed and it had "decoding constraints" — huge red flag ngl.

Pulled the actual PDF text myself. Total hallucination. The real paper doesn't mention CSD, variance, or autocorrelation at all, not even once. They just do their standard CTMC→DES→Emulator→StressTest model stuff. The gap is still wide open.

## Decision

For any **go/no-go novelty checks**, always verify against the **primary source** (actual PDF/text). Never trust an AI summary for this. If a tool complains about compressed/hard-to-parse docs, treat its summary as garbage for any critical novelty call.

## Consequences

**Pros**
- Saved us from dropping a totally valid project over fake AI output.
- Good rule of thumb going forward so we don't accidentally scoop ourselves.

**Cons**
- Reading papers manually takes way longer than skimming summaries.
  Worth it. Can't afford to burn months of work or kill a great idea over a hallucination.

## Alternatives

- **Just trust the summary tool.** No way. Caught it lying through its teeth this time, lesson learned.

## Related

- ADR-0002, ADR-0005, ADR-0007.
