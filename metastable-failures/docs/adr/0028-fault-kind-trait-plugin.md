# ADR-0028: Infrastructure fault profiles as open plugins (Open/Closed principle)

- **Status:** Accepted
- **Date:** 2026-10-23

## Context / Background

In the initial Rust port (`crates/chaos`), infrastructure fault types (`baseline-flaky`, `burst-failure`,
`slow-degradation`, `wan-baseline`, `network-partition`, `cascade-crash`) were hardcoded across several
functions:
- `faults.rs`: `clears_tc_first`, `fault_commands`, `recover_scope`, and `FaultInjector::inject` each
  maintained separate `match` statements over string literals; the progressive ramp for `slow-degradation`
  was a special case inside `inject`.
- `coordinator.rs`: `cascade-crash` had a hardcoded default recovery duration of 35 s, and DNS resolution
  of `peerServices` → `peerIps` was executed only when the profile was passed via `FAULT_PARTITION`.
- Parameters were mostly static (loss 40%, loss 80%, degradation ramp fixed 0→5000 ms over 60 s).

Adding a new fault profile required modifying 4–5 locations across two files, violating the Open/Closed
Principle (closed to extension, open to modification).

## Decision

1. Extract fault kind behavior into an open plugin trait `FaultKind`:
   - `name(&self) -> &'static str`: unique fault kind name.
   - `layer(&self) -> FaultLayer`: affected layer (`Tc`, `Iptables`, `Process`). The layer determines whether
     `tc` qdisc must be cleared first (`clears_tc_first`) and what scope is cleared on `/faults/recover/:type` (`recover_scope`).
   - `commands(&self, opts: &Value) -> Result<Vec<Argv>, String>`: validation and generation of argv commands to run upon injection.
   - `ramp(&self, opts: &Value) -> Result<Option<FaultRamp>, String>`: optional specification of progressive background steps.
   - `default_recover_after_sec(&self) -> Option<f64>`: optional coordinator default recovery time (35.0 s for `cascade-crash`, None for others).
   - `requires_peer_resolution(&self) -> bool`: whether coordinator resolves peer DNS prior to injection (true for `network-partition`).
   - `execute<'a>(&'a self, injector: &'a FaultInjector, opts: &'a Value) -> BoxFuture<'a, ()>`: default execution logic runs commands and spawns ramp if present; can be specialized by fault kinds needing custom lifecycle hooks (`cascade-crash` SIGKILL, `network-partition` IP tracking).
2. Centralize fault registration in a single registry `find_fault_kind(name: &str) -> Option<&'static dyn FaultKind>`.
3. Eliminate fault kind comparisons in `FaultInjector` and `coordinator.rs`. Coordinator resolves DNS generically for any fault whose plugin signals `requires_peer_resolution()` or where `peerServices` is provided.
4. Non-breaking parameterization via JSON `options` (`lossPct` for loss profiles, `corruptPct` for corruption, `maxDelayMs`/`durationMs`/`stepMs` for degradation, `delayMs`/`jitterMs` for WAN). Options are strictly validated (range and types).
5. Consolidation rule: variant duplication avoided – `baseline-flaky` and `burst-failure` share `PacketLossFault` with parameterized default loss percentage (40% and 80%).
6. Quirk preservation: preserved Q4 (PID 1 survives SIGKILL on itself) and Q5 (initial 0ms netem rejected by `tc`).
7. Verify extensibility by adding `packet-corruption` (`tc … netem corrupt <pct>%`, default 5%) as a new module and single registry line, covered by unit test, sample profile `when-gossip-goes-silent/config/faults/packet-corruption.json`, and integration test step in `tests/compose/cluster_test.py`.

## Consequences

**Pros**
- Adding a new fault kind requires only 1 new file + 1 line in registry, without editing `coordinator.rs`, `service.rs`, or existing fault modules.
- Zero fault kind name matches in `crates/chaos/src/` outside fault modules and registry.
- 100% backward compatibility: existing test suites, HTTP contracts, and coordinator schedules remain green.

**Cons**
- Minor dynamic dispatch overhead on fault injection and recovery (negligible given infrastructure fault frequency).

## Alternatives

- **Macro-based registration (`inventory` / `linkme`).** Rejected: adds magic, non-deterministic static initialization across platforms, and unnecessary dependencies.

## Related

- ADR-0001, ADR-0024, ADR-0026, ADR-0027.
