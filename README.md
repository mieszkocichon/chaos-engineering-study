# chaos-engineering-study

Chaos-engineering research platform: a multi-agent resilience experiment (gossip, circuit
breakers, backpressure) and a metastable-failure early-warning study. Implemented in Rust;
the original JavaScript implementation is archived in [`old-implementation/`](old-implementation/README.md).

| path | content |
|---|---|
| `crates/chaos` | experiment service `chaos-svc`, coordinator `chaos-coord`, generators `chaos-gen`, runner `chaos-run` |
| `crates/metastable` | retry-storm simulator + CSD early warning: `run-m1`, `run-m2`, `sweep-hysteresis` |
| `crates/demos` | resilience-pattern demos: `demo` |
| `when-gossip-goes-silent/` | experiment configs (`config/experiments/*.json`), runner wrappers (`runner/*.sh`), k6 script, results, analysis (`*.py`) |
| `metastable-failures/` | notes, ADRs, gate scripts, results |
| `specs/001-rust-rewrite/` | spec, plan, contracts, tasks, equivalence report of the rewrite |
| `labbook.md` | implementation log |

## Quick start

Requires Docker only (ADR-0004). See [`specs/001-rust-rewrite/quickstart.md`](specs/001-rust-rewrite/quickstart.md).

```sh
docker build -t rust-dev -f Dockerfile.dev .          # dev image (once)
sh scripts/dev.sh sh scripts/test.sh                  # all tests: unit, localhost, containers
docker build -t chaos-experiment-rs:latest .          # runtime image
cd when-gossip-goes-silent && chaos-run config/experiments/matrix-compose.json   # IMPL=rust by default
```

### Experiment CLI (`chaos-run`)

An experiment is a file in `when-gossip-goes-silent/config/experiments/`; a new experiment needs no code
and no script (schema: `specs/001-rust-rewrite/contracts/cli-env.md`, decisions: ADR-0029, ADR-0030).
`chaos-run <file> --dry-run` lists the cells, `--resume` skips cells whose results exist, `--only <text>` filters.

| Experiment file | Replaces |
|---|---|
| `matrix-compose.json` – 51 cells, 1020 runs | `runner/run-matrix-compose.sh` |
| `matrix-inbound.json`, `matrix-sensitivity.json`, `matrix-spof.json`, `matrix-swim.json` | `runner/run-matrix-<name>.sh` |
| `matrix-swarm.json` – 100 deployments (backend: swarm) | `runner/run-matrix-swarm.sh` |
| `spof-test.json` – 5 cells, `svc-03` stopped at t=30s | `runner/run-spof-test.sh` |
| `equivalence.json`, `control-js.json`, `unthrottled-cpu4.json` | `scripts/{equivalence,control-js,unthrottled}.sh` |
| `test-ring5.json`, `test-action.json` | smoke tests in `tests/compose/chaos-run-test.sh` |

The `runner/run-matrix-*.sh` and `runner/run-spof-test.sh` scripts are thin wrappers around `runner/run-experiment.sh`, which turns the
old variables (`WORKERS`, `IMPL`, `RESULTS_DIR`, `SEEDS`, `PROJECT_PREFIX`, `GATEWAY_CPUS`, `TOPOLOGY`) into flags.
`MATRIX_*`, `SWIM_FANOUT` and `STATIC_PARAMS` are no longer read (the wrapper refuses them): put them in a file.

Not migrated, still a script: `runner/run-matrix-k8s.sh` (k3d cluster driven with `kubectl`).
