# ADR-0027: Resilience strategies as open plugins (Open/Closed principle)

- **Status:** Accepted
- **Date:** 2026-10-23

## Context / Background

In the initial Rust port, resilience strategies (`none`, `static`, `static-conservative`,
`central`, `mas-agent`, `mas-agent-swim`, `mas-agent-inbound`, `backpressure`) were implemented
as a single `enum Strategy` in `crates/chaos/src/strategy/mod.rs`. The enum had 9 methods, each
matching over all 8 variants. Adding a new resilience strategy required modifying all 9 methods
across `strategy/mod.rs`, violating the Open/Closed Principle (closed to extension, open to modification).

## Decision

1. Extract strategy behavior into a plugin trait `ResilienceStrategy` with touchpoints for:
   - downstream execution (`call_downstreams`),
   - gossip handling (`has_gossip_receiver`, `receive_gossip`),
   - control updates (`apply_update`),
   - metrics reporting (`stats`),
   - state reset (`reset`),
   - background timer loops (`start`).
   Optional methods provide default no-op / false implementations.
2. For async trait methods (`call_downstreams`), use explicit `Pin<Box<dyn Future<Output = ...> + Send + 'a>>`
   (`BoxFuture`) rather than native `async fn in trait` or external crates like `async-trait`.
   *Rationale:* Dynamic dispatch with trait objects (`dyn ResilienceStrategy`) requires object safety,
   which native `async fn in trait` does not currently provide without compiler nightly features or proc-macro dependencies.
3. Centralize strategy creation in a single factory registry (`create_strategy`) matching `STRATEGY` env
   values to constructors. Unknown names produce the exact existing error message.
4. Move each strategy into its own module under `crates/chaos/src/strategy/`. The three MAS
   flavours share one generic `MasStrategy<V: MasVariant>` (`mas_strategy.rs`); a variant only
   decides gossip targets, extra bookkeeping and extra stats. Shared logic
   (`fetch_json`, `call_pipelined`, `format_pipelined_results`, `parse_gossip_in`, `spawn_every`)
   is shared and reused rather than duplicated.
5. `pub type Strategy = dyn ResilienceStrategy` – services hold `Arc<Strategy>`; no forwarding
   wrapper (amended 2026-10-23: the original wrapper struct only forwarded calls).
6. Verify extensibility by adding `retry-only` strategy as a new module and single-line registry entry.

## Consequences

**Pros**
- Adding a new strategy requires only 1 new file and 1 registration line, with zero modifications to existing strategies or HTTP service wiring.
- `grep` reveals no `match` expressions over strategy variants outside the factory registry.
- 100% backward compatibility: existing test vectors, HTTP contracts, and metrics key orders are preserved.

**Cons**
- Minor runtime overhead of `BoxFuture` allocation per downstream batch call (negligible compared to HTTP network I/O).

## Alternatives

- **Macro-based registration (`inventory` / `linkme`).** Rejected: adds magic, non-deterministic static initialization across platforms, and unnecessary dependencies.
- **`async-trait` crate.** Rejected: adds an unnecessary external procedural macro dependency when `futures::future::BoxFuture` is already available in the Cargo workspace.

## Related

- ADR-0001, ADR-0024, ADR-0026.
