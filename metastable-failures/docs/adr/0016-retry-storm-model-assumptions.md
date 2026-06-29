# ADR-0016: Retry-storm model assumptions

- **Status:** Accepted
- **Date:** 2026-06-28

## Context / Background

M1 model (`src/sim.js`) needs to produce *actual* metastability. Not just slow recovery, but a self-sustaining collapse with hysteresis. Early params were weak—surge of 1500 rps vs 500 rps capacity didn't even collapse. Sustaining loop wasn't strong enough. We need the right modeling choices to even see the bistability.

## Decision

Modeling the retry-amplification loop with three linked sustaining mechanisms. These are explicit assumptions:

1. **Client retries on timeout** up to `maxAttempts`, plus backoff. This is the amplification: timed out job comes right back as new load. Retry budget dictates collapsed-state amplification ≈ `maxAttempts`. Recovery point λ_down ≈ capacity / maxAttempts (saw this empirically).
2. **Wasted work** — service completes request *after* client's timeout = wasted, not goodput. Server blowing capacity on doomed work. `cancelQueuedOnTimeout` defaults to **false**. Meaning abandoned *queued* requests still process as wasted work. Makes the sustaining effect stronger (doesn't fake it though) and mimics systems that can't kill queued work.
3. **Shedding triggers fast retries** — request rejected at queue limit retries instantly after backoff. No full timeout. Models fast-fail clients, adds more amplification.

Goodput (completions the client actually wanted per sec) is the **order parameter**.
Service times and inter-arrivals are exponential, all off the single seeded RNG.

## Consequences

**Pros**
- Gives us real bistability with a hysteresis loop we can measure and tune.
- Every mechanism is a knob we can inspect. Sets us up for C3 ablation (figuring out what contributes what).

**Cons**
- Exponential service/arrival dists are an abstraction. Real services have heavier tails. Fine for the model layer; real injection (M3+) will test reality later.
- `cancelQueuedOnTimeout = false` is a heavy sustaining choice. Keeping the alternative as a flag so we can do sensitivity analysis.

## Alternatives

- **No wasted work (cancel all abandoned client stuff).** Dumped as default. Drains way too easy, weakens/kills the metastable trap. Kept as a toggle for ablation.
- **Closed-loop (fixed virtual users) instead of open-loop with retries.** Punting on this. Open-loop + retry budget gives a cleaner knob for amplification.

## Related

- ADR-0009, ADR-0014, ADR-0019.
