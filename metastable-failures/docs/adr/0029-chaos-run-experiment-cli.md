# ADR-0029: Unified declarative experiment runner CLI (`chaos-run`)

- **Status:** Accepted
- **Date:** 2026-10-23

## Context / Background

Prior to this decision, running resilience experiments required ~1240 lines of bash across 9 scripts in
`when-gossip-goes-silent/runner/*.sh` (`run-matrix-compose.sh`, `run-matrix-inbound.sh`,
`run-matrix-sensitivity.sh`, `run-matrix-spof.sh`, `run-matrix-swim.sh`, `run-matrix-swarm.sh`,
`run-spof-test.sh`, `run-matrix-k8s.sh`, `impl.sh`) and several auxiliary scripts in `scripts/`:

1. **Duplicated logic & hardcoding**: Each runner script embedded hardcoded bash arrays of strategies,
   fault overlays, base network layers, and seeds. Variants differing only by parameters (e.g. `GATEWAY_CPUS`,
   `SPOF_KILL_AT_SEC`, `SWIM_FANOUT`) were maintained as ad-hoc copies or modified using `sed` on generated compose files.
2. **Ignored schema**: `when-gossip-goes-silent/config/experiments/full-matrix.json` existed in the repository
   but was unused by any tooling.
3. **Run collisions**: Running concurrent experiment sessions risked tearing down active clusters (documented
   in `labbook.md` step 7), as runners lacked an exclusivity lock mechanism.
4. **Reproducibility gap**: Result files captured metrics, but the exact commit, Docker image, and run
   configuration were not bundled with result directories.
5. **Portability issues**: Bash scripts relied on MSYS path conversion quirks (`pwd -W`, `MSYS_NO_PATHCONV=1`)
   and could not be executed reliably inside containerized dev environments (`scripts/dev.sh`) without host path translation.

## Decision

1. **Implement `chaos-run` CLI and library in `crates/chaos/src/experiment/`**:
   - `schema.rs`: Declarative JSON experiment schema (unknown keys and non-integer seeds are errors) supporting topologies, strategies, fault bases/profiles,
     seeds (explicit or `seedBase` + `repetitions`), workload (`rps`, `durationSec`, `warmupSec`), resources (`gatewayCpus`),
     strategy variants, `extraCells`, workers count, and implementation (`rust` vs `js`).
   - `validate.rs`: Pre-flight validation ensuring all referenced topologies, strategies, faults, and partitions exist
     on disk or in plugin registries before touching Docker.
   - `cell.rs`: Cartesian product cell generation combined with `extraCells` and `variants`, plus `--resume` check verifying
     complete seed result sets in the target directory.
   - `lock.rs`: OS file lock (`.chaos-run-<projectPrefix>.lock` next to `config/`), one per compose project prefix, because
     the prefix decides which compose projects (`<prefix>1`, `<prefix>2`) a run tears down. The OS drops the lock when the process
     dies, so Ctrl+C leaves nothing stale.
   - `runner.rs`: Orchestrates `docker compose` execution, waits for coordinator container exit, streams progress (1 line per cell),
     handles teardown. A cell fails (and `chaos-run` exits non-zero after the remaining cells) when `compose up` fails, the
     coordinator exits non-zero, disappears for 60 s, exceeds 90 min, or leaves a seed without a result file. After a variant cell
     the result files get `__<label>__` appended, as the old scripts did. A reproducibility manifest (effective experiment, git commit,
     image and image id, time) is written to `<results>/manifests/` - not next to the results, because analyzers read `<results>/*.json`.
   - `bin/chaos-run.rs`: CLI binary supporting `--dry-run`, `--only <pattern>`, `--resume`, `--workers <n>`, `--impl <rust|js>`,
     `--results-dir <path>`, `--topology <name>`, `--seeds <s1,...>`, `--project-prefix <p>` and `--gateway-cpus <n>`.
     Partition cells are ordinary `extraCells`; no fault name is hardcoded in the runner. Strategy names are validated against
     `strategy::STRATEGY_NAMES` (next to the registry), fault names against `find_fault_kind` and `config/faults/`.

2. **Clean Compose Generation**:
   - Extended `generate.rs` (`ComposeOpts`) to accept `gateway_cpus`, `rps`, `duration_sec`, `warmup_sec`, and `config_dir_host`
     directly, eliminating `sed` workarounds while maintaining 100% line-for-line parity with golden JS generator tests when omitted.

3. **Docker-in-Docker / Host Path Translation**:
   - Implemented `resolve_host_path` to translate container paths (`/app/...`) to host paths using `HOST_APP_PATH` or container
     inspection, ensuring bind mounts (`/app/config`, `/results`) resolve seamlessly on Windows Git Bash, Linux hosts, and inside `scripts/dev.sh`.

4. **Experiment File Migration**:
   - Migrated to files in `when-gossip-goes-silent/config/experiments/`: `matrix-compose`, `matrix-inbound`, `matrix-sensitivity`,
     `matrix-spof`, `matrix-swim`, plus `equivalence`, `control-js`, `unthrottled-cpu4` (used by the `scripts/*.sh` of the same names)
     and the smoke test `test-ring5`.
   - `runner/run-matrix-*.sh` became one-line wrappers around `runner/run-experiment.sh` (env -> flags, finds or builds the binary).
   - **Not migrated: `run-matrix-swarm.sh`, `run-spof-test.sh`, `run-matrix-k8s.sh`** stay scripts. Swarm deploys a stack
     (`docker stack deploy`, overlay network, `gen swarm`: RPS 50, warmup 10, one seed per run); the SPOF test stops one service
     (`svc-03`) or the coordinator 30 s into the run and writes `spofTest` into the result. `chaos-run` runs Compose clusters only; an
     experiment file that merely resembled them would run a different experiment (an earlier draft did). Supporting them needs
     a deployment-backend abstraction and a mid-run action hook - a separate step. k8s uses k3d and `kubectl` (no Helm).

5. **Testing & CI Integration**:
   - `crates/chaos/tests/experiment.rs` (15 tests): for each migrated file the cells (order included) and the generated compose
     are compared with what the removed script produced - the expected data is transcribed from those scripts, the `GATEWAY_CPUS`
     `sed` is replayed on the old side. Also validation, seeds, variant tagging, `--resume`, lock, dry-run without side effects.
   - Added container integration test in `tests/compose/chaos-run-test.sh` integrated into `scripts/test.sh`.

## Consequences

**Pros**:
- Defining a new experiment is now strictly declarative: add a JSON file in `config/experiments/`, no code or bash changes needed.
- Two runs with the same project prefix cannot overlap (OS file lock).
- Result directories are self-documenting via `<results>/manifests/`.
- Runs can be safely interrupted and resumed (`--resume`).
- Preserves full backward compatibility with legacy analyzer scripts (`analyze_results.py`, `compare_impls.py`).

**Cons**:
- Running on a host without the binary goes through `scripts/dev.sh` (the dev container with the Docker socket).
- The lock is advisory per host path; it does not stop a run started from another checkout or another machine on the same Docker.
- Project names are `<prefix><worker>` (`sens1`), the old single-project scripts used `sens`; only compose file and network names differ.
- `chaos-run` does not build images (the wrapper does, once, when missing); `SPOF_KILL_AT_SEC_VALUE` is gone, edit `matrix-spof.json`.

## Related

- ADR-0004 (Run everything via Docker)
- ADR-0026 (Rust equivalence and JS archive)
- ADR-0027 (Resilience strategy plugins)
- ADR-0028 (Fault injector plugins)
