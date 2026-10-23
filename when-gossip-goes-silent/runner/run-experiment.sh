#!/usr/bin/env bash
# run-experiment.sh <experiment.json> [topology] [chaos-run flags]
# Finds (or, via scripts/dev.sh, builds) chaos-run, makes sure the image exists and passes the
# old scripts' environment variables on as flags. The matrix definition lives in the JSON file.
set -euo pipefail
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR/.."

for v in MATRIX_STRATEGIES MATRIX_BASES MATRIX_FAULTS MATRIX_PARTITION_STRATEGIES SPOF_KILL_AT_SEC_VALUE SWIM_FANOUT STATIC_PARAMS; do
  if [[ -n "${!v:-}" ]]; then
    echo "ERROR: $v is no longer read - put it in an experiment file (config/experiments/*.json)" >&2
    exit 1
  fi
done

EXPERIMENT="$1"; shift
ARGS=()
[[ -n "${WORKERS:-}" ]] && ARGS+=(--workers "$WORKERS")
[[ -n "${IMPL:-}" ]] && ARGS+=(--impl "$IMPL")
[[ -n "${RESULTS_DIR:-}" ]] && ARGS+=(--results-dir "$RESULTS_DIR")
[[ -n "${SEEDS:-}" ]] && ARGS+=(--seeds "$SEEDS")
[[ -n "${PROJECT_PREFIX:-}" ]] && ARGS+=(--project-prefix "$PROJECT_PREFIX")
[[ -n "${GATEWAY_CPUS:-}" ]] && ARGS+=(--gateway-cpus "$GATEWAY_CPUS")
TOPOLOGY="${TOPOLOGY:-}"
if [[ $# -ge 1 && "$1" != -* ]]; then TOPOLOGY="$1"; shift; fi
[[ -n "$TOPOLOGY" ]] && ARGS+=(--topology "$TOPOLOGY")

# the image of the selected implementation (IMPL=rust|js), built once if missing
source "$SCRIPT_DIR/impl.sh"
docker image inspect "$EXP_IMAGE" > /dev/null 2>&1 || build_experiment_image

for bin in "${CHAOS_RUN:-}" ../target/release/chaos-run ../target/debug/chaos-run; do
  if [[ -n "$bin" && -x "$bin" ]]; then exec "$bin" "$EXPERIMENT" ${ARGS[@]+"${ARGS[@]}"} "$@"; fi
done
if command -v chaos-run > /dev/null 2>&1; then exec chaos-run "$EXPERIMENT" ${ARGS[@]+"${ARGS[@]}"} "$@"; fi
# no binary on the host (e.g. Windows): build and run it in the dev container (ADR-0004)
exec sh ../scripts/dev.sh sh -c 'cd when-gossip-goes-silent && exec cargo run -q -p chaos --bin chaos-run -- "$@"' sh "$EXPERIMENT" ${ARGS[@]+"${ARGS[@]}"} "$@"
