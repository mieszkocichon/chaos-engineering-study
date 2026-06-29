# ADR-0023: Carry over the statistical-rigor standard

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

Metastability repro is super sensitive to timing. Gate results right now are n=1. Target venues want actual quantified uncertainty and effect sizes. The old ECMS work already set a solid rigor standard, might as well keep it instead of reinventing the wheel.

## Decision

Locking in this standard for the whole project:
- **n ≥ 30 seeds per matrix cell**. Confidence intervals on every figure. Cheap deterministic sims make this trivial; ADR-0014 guarantees reproducibility per seed.
- **Mann–Whitney U + Cliff's δ** for main comparisons. Reusing/extending the old `analyze_results.py`. Report effect size, not just p-values.
- **Explicit split between "statistically significant" and "practically relevant".** Prior work did this well, definitely keeping it.
- For the detector: report **ROC / AUC** of the alarm on collapsing vs stable runs, plus **false-positive rate on stable runs**. Don't just cherry-pick one operating point (ties into ADR-0020).

## Consequences

**Pros**
- Checks the box for reviewer expectations on uncertainty/effect size out of the gate.
- Gate probes (n=1) are labeled as such and will get the n≥30 treatment before we make any actual claims.

**Cons**
- More runs, more analysis. Cheap at the model layer, but gonna cost us later with real injection (M3+). Budgeted for it.

## Related

- ADR-0014, ADR-0020; PLAN §5, §10.
