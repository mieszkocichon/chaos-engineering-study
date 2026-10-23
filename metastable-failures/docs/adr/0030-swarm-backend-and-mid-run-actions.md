# ADR-0030: Docker Swarm backend and mid-run actions in `chaos-run`

- **Status:** Accepted
- **Date:** 2026-10-23

## Context / Background

ADR-0029 introduced the declarative experiment runner CLI (`chaos-run`) and unified 6 of the 9 runner scripts. However, two important experiment workflows were deferred:

1. **Docker Swarm (`run-matrix-swarm.sh`)**: Deploys a Swarm stack (`docker stack deploy`, overlay network, `gen swarm`: RPS 50, warmup 10, duration 60, scale 20, health timeout 540000ms). In Swarm, there is no soft reset between seeds; each seed must be executed as an independent deployment (5 strategies × 4 faults × 5 seeds = 100 stack deployments).
2. **SPOF Test (`run-spof-test.sh`)**: Runs an extended experiment (duration 90s, RPS 20, warmup 15, bridge network, with `SIMULATE_CLOCK_SKEW: "true"` and `SIMULATE_IO_CONTENTION: "true"`) with mid-run fault actions (stopping `svc-03` in Phase 1 or `coordinator` in Phase 2 at t=30s) and writing `spofTest` metadata directly into the result file for `analyze_results.py` to evaluate hypothesis H2 (single point of failure resilience).

Previously, these runs relied on raw bash scripts with `sed` substitutions on generated compose files and race-prone `jq` modifications on "the newest file in `results/`".

## Decision

1. **`ClusterBackend` Trait (`crates/chaos/src/experiment/backend/`)**:
   - Extracted cluster lifecycle management behind the `ClusterBackend` trait (as in `backend/mod.rs`):
     - `name() -> &'static str`
     - `prepare(&PrepareContext) -> Result<BackendPlan, String>` (writes the compose file / stack file)
     - `start(&BackendPlan) -> Result<(), String>`
     - `check_status(&BackendPlan, started: Instant) -> Result<RunStatus, String>` (`Running`, `Exited(code)`, `Disappeared`)
     - `stop_service(&BackendPlan, target) -> Result<(), String>`
     - `logs_tail(&BackendPlan, lines) -> String`
     - `teardown(&BackendPlan) -> Result<(), String>`
   - **`ComposeBackend`**: Manages Docker Compose projects (`docker compose up -d`, `docker compose ps`, `docker compose stop <target>`, `docker compose down --remove-orphans`).
   - **`SwarmBackend`**: Generates Swarm stack definitions via `generate::swarm_with_opts(...)`, deploys stacks via `docker stack deploy`, monitors service replicas via `docker service ps`, supports service scaling down for `stop_service`, and performs cleanup via `docker stack rm`. Swarm expands to 1 deployment per seed (100 deployments for `matrix-swarm.json`), enabling seed-level `--resume`.
   - Pluggable backend registry via `find_backend(name)`.

2. **Pluggable Mid-Run Actions (`ActionKind` trait in `crates/chaos/src/experiment/action/`)**:
   - `ActionKind`: Defines action execution (`execute(&self, backend, plan, target)`).
   - `StopServiceAction`: Invokes `backend.stop_service(plan, target)` when scheduled time `atSec` is reached during the status poll loop.
   - Action registry via `find_action_kind(name)`.
   - Upon cell completion, `attach_action_metadata` matches the cell and seed file `{file_prefix}{seed}_*.json` deterministically and inserts top-level `spofTest` metadata required by `analyze_results.py`.

3. **Generator Extensions**:
   - `generate::swarm_with_opts`: Parameterizes `scale`, `fault`, `strategy`, `seed`, `seeds`, `rps`, `warmup_sec`, `duration_sec`, `health_timeout_ms`, host mounts, and image. The parameterless `generate::swarm` delegates with defaults, preserving 100% byte-for-byte parity with legacy JS golden tests.
   - `generate::ComposeOpts`: Extended with `extra_env` and `network_driver`, allowing `spof-test.json` to emit `SIMULATE_CLOCK_SKEW` and `SIMULATE_IO_CONTENTION` natively without `sed`.

4. **Experiment Migration & Wrapper Replacement**:
   - Created `when-gossip-goes-silent/config/experiments/matrix-swarm.json` (100 deployments, backend: "swarm", scale: 20, RPS 50, duration 60, warmup 10, health timeout 540000ms).
   - Created `when-gossip-goes-silent/config/experiments/spof-test.json` (5 cells, seed 1000, duration 90s, RPS 20, warmup 15, action `stop-service` on `svc-03` at t=30s). **The old script's second phase (stop the coordinator, seed 2000, central and mas-agent) is not migrated:** the coordinator writes the result files, so a run without it has none (verified: `docker compose stop coordinator` -> exit 137, no result). The old script only annotated whichever file was newest, i.e. a stale one. `validate` rejects actions on `coordinator`. Measuring a controller outage needs a controller separate from the harness - a different change.
   - Replaced `runner/run-matrix-swarm.sh` and `runner/run-spof-test.sh` with 3-line wrappers over `runner/run-experiment.sh`.
   - Added container integration test in `tests/compose/chaos-run-test.sh` verifying `stop-service` execution and `spofTest` consumption by `analyze_results.py`.

## Review fixes (2026-10-23)

- An action that did not fire before the run ended (e.g. `atSec` 30 in a 20 s run) **fails the cell** and adds no `spofTest`; before, the cell was OK and the result claimed a kill that never happened.
- `generate::SwarmOpts.stack_deploy` omits `depends_on`: `docker stack deploy` rejects the map form ("must be a list") and ignores the ordering. The default output (JS parity) is unchanged. A real `chaos-run` of `backend: swarm` (scale 20, 1 seed) now ends with a result file.
- Swarm validation: `scale-<N>.json` must exist in `config/topologies/` (the coordinator exits 1 without it) and `topology` must agree with `scale`.
- Verified: Swarm services get `NET_ADMIN` (`CapEff` has bit 12; `tc qdisc add dev eth0 root netem loss 1%` works in a `chaos_*_svc-a` container).
- **Found: `matrix-swarm.json` injects no faults.** All profiles in `config/faults/` target `svc-01`..`svc-05` (ring/mesh ids), but `scale-20` has `svc-a..d`, `svc-1`...; the coordinator skips a missing target silently. A Swarm cell measures a healthy cluster (a real run: 422/422 requests OK, same as the same cell on Compose). This predates the migration (the JS runs had the same profile files). `chaos-run` now prints `WARNING: fault '...' targets ... none exist in topology ...` (also in `--dry-run`). **Fixed (decision: profiles for `scale-N`):** `config/faults/{baseline-flaky,burst-failure,slow-degradation,cascade-crash}-scale.json` inject the same kinds with the same timing and number of targets as the ring profiles, on the mid-tier services (`svc-a..d`) of `scale-20`/`scale-50`; `matrix-swarm.json` uses them (tests check targets exist and kinds match). Verified on a real Swarm run (`baseline-flaky-scale`, strategy `none`): `netem` on `svc-a/b/c` only, 73 of 420 requests failed (success 82.6%) versus 100% before. Results of the earlier (fault-free) Swarm runs and these are not comparable. The other three `-scale` profiles were not run.

## Consequences

**Pros**:
- `matrix-swarm` and `spof-test` are now fully declarative and executable via `chaos-run`.
- Deterministic metadata injection replaces race-prone `jq` updates on "latest modified file".
- Pre-flight validation checks `backend` and `action` validity before deploying any cluster.
- Swarm multi-seed runs can be safely interrupted and resumed (`--resume`).
- Preserves full parity with legacy scripts and golden generator vectors.

**Cons**:
- Swarm: only a single-deployment run was verified; the 100-deployment matrix was not run.
- `run-matrix-k8s.sh` remains the only legacy runner script requiring a dedicated backend (k3d / `kubectl`).

## Related

- ADR-0004 (Run everything via Docker)
- ADR-0027 (Resilience strategy plugins)
- ADR-0028 (Fault injector plugins)
- ADR-0029 (Unified declarative experiment runner CLI)
