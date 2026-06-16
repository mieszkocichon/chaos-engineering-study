#!/bin/bash
# run-matrix-compose.sh
#
# Full experiment matrix with soft-reset cells and optional parallel workers.
#
#   MAIN  : 6 strategies × 2 bases (no-wan, wan-50ms) × 4 overlays × 20 seeds
#          = 48 cells × 20 seeds = 960 experiments
#   PARTITION : 3 strategies × no-wan × baseline-flaky × network-partition × 20 seeds
#          = 3 cells × 20 seeds = 60 experiments
#   TOTAL : 51 cells × 20 seeds = 1020 experiments
#
# Each cell runs 20 seeds in a single cluster lifecycle via POST /experiment/reset
# (soft reset) between seeds, eliminating docker compose up/down overhead per seed.
#
# Usage:
#   bash runner/run-matrix-compose.sh [topology]
#   WORKERS=2 bash runner/run-matrix-compose.sh ring-50
#   SEEDS="1000,2000,3000,4000,5000" WORKERS=2 bash runner/run-matrix-compose.sh
#
# Requirements:
#   - Docker Desktop running
#   - Node.js >= 22 in PATH
#   - Run from when-gossip-goes-silent/ or anywhere else
#
set -euo pipefail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR/.."

# ── Parameters ────────────────────────────────────────────────────────────────
TOPOLOGY=${1:-ring-50}
WORKERS=${WORKERS:-2}
SEEDS=${SEEDS:-"1000,2000,3000,4000,5000,6000,7000,8000,9000,10000,11000,12000,13000,14000,15000,16000,17000,18000,19000,20000"}
RESULTS_DIR="./results"

SEED_COUNT=$(echo "$SEEDS" | tr ',' '\n' | wc -l | tr -d ' ')

STRATEGIES=("none" "static" "static-conservative" "central" "mas-agent" "backpressure")
BASES=("no-wan" "wan-50ms")
FAULTS=("baseline-flaky" "burst-failure" "slow-degradation" "cascade-crash")
PARTITION_STRATEGIES=("central" "mas-agent" "backpressure")

# ── Preflight ─────────────────────────────────────────────────────────────────
if [[ ! -f "config/topologies/${TOPOLOGY}.json" ]]; then
  echo "ERROR: Topology not found: config/topologies/${TOPOLOGY}.json" >&2
  exit 1
fi

# ── Build cell list ───────────────────────────────────────────────────────────
# Each cell encoded as "strategy|base|fault|partition".
CELLS=()
for s in "${STRATEGIES[@]}"; do
  for b in "${BASES[@]}"; do
    for f in "${FAULTS[@]}"; do
      CELLS+=("$s|$b|$f|")
    done
  done
done
for s in "${PARTITION_STRATEGIES[@]}"; do
  CELLS+=("$s|no-wan|baseline-flaky|network-partition")
done

TOTAL_CELLS=${#CELLS[@]}
TOTAL_EXP=$(( TOTAL_CELLS * SEED_COUNT ))

mkdir -p "$RESULTS_DIR"

echo "=== MATRIX (soft-reset cells | $WORKERS worker(s)) ==="
echo "Topology:     $TOPOLOGY"
echo "Seeds:        $SEEDS ($SEED_COUNT per cell)"
echo "Cells:        $TOTAL_CELLS"
echo "Experiments:  $TOTAL_EXP"
echo "======================================================"

# ── Build image once ──────────────────────────────────────────────────────────
echo "[Setup] Building chaos-experiment:latest..."
docker build -t chaos-experiment:latest . > /dev/null
echo "[Setup] Image ready."

# ── Shared progress counter (mkdir-based lock for cross-platform compatibility) ──
COUNTER_FILE="$(mktemp)"
COUNTER_LOCK="${COUNTER_FILE}.lock.d"
echo 0 > "$COUNTER_FILE"
trap 'rm -f "$COUNTER_FILE"; rmdir "$COUNTER_LOCK" 2>/dev/null || true' EXIT

# Snapshot initial file count ONCE — progress = (files_now - INITIAL_COUNT)
# Use find (exit 0 even if empty) to avoid pipefail issues.
INITIAL_COUNT=$(find "$RESULTS_DIR" -maxdepth 1 -name '*.json' -type f 2>/dev/null | wc -l | tr -d ' ')
: "${INITIAL_COUNT:=0}"
export INITIAL_COUNT

# ── Cell executor ─────────────────────────────────────────────────────────────
run_cell() {
  local cell="$1"
  local worker="$2"
  local strat base fault part
  IFS='|' read -r strat base fault part <<< "$cell"

  local project="w${worker}"
  local compose="docker-compose-local-${project}.yml"
  local label="Strategy=$strat | Fault=$fault | Base=$base | Partition=${part:-none}"

  echo ""
  echo "[W${worker}] >>> Starting cell: $label  (seeds=$SEEDS)"

  # Generate per-worker compose file with project-scoped network name.
  node generate-compose-topology.js "$TOPOLOGY" "$fault" "$strat" "$SEEDS" "$base" "$part" "$project" > /dev/null

  # Ensure no leftover state for this worker.
  docker compose -p "$project" -f "$compose" down --remove-orphans > /dev/null 2>&1 || true

  # Start cluster for this cell.
  docker compose -p "$project" -f "$compose" up -d > /dev/null

  # Wait for coordinator to finish (max 90 min per cell — WAN/partition add overhead).
  local start_time
  start_time=$(date +%s)
  local timeout=5400
  local last_log_emit=0
  while true; do
    local status last_log elapsed
    status=$(docker compose -p "$project" -f "$compose" ps --all coordinator --format "{{.Status}}" 2>/dev/null | head -n 1 || echo "")
    last_log=$(docker compose -p "$project" -f "$compose" logs --tail 1 coordinator 2>/dev/null | tail -n 1 | cut -c 1-80 || echo "")
    elapsed=$(( $(date +%s) - start_time ))

    # Heartbeat to stdout every 10s so the monitor GUI stays responsive.
    if (( elapsed - last_log_emit >= 10 )); then
      echo "    [W${worker}] [${elapsed}s] ${status} | ${last_log}"
      last_log_emit=$elapsed
    fi

    if [[ "$status" == *Exit* || "$status" == *Exited* ]]; then
      break
    fi
    if [[ -z "$status" ]] && (( elapsed > 60 )); then
      echo "    [W${worker}] WARN: coordinator disappeared (OOM?) after ${elapsed}s"
      break
    fi
    if (( elapsed > timeout )); then
      echo "    [W${worker}] TIMEOUT after ${elapsed}s"
      break
    fi
    sleep 3
  done

  # Teardown.
  docker compose -p "$project" -f "$compose" down --remove-orphans > /dev/null 2>&1 || true

  # Atomic counter bump + progress emission that the Python monitor parses.
  # Emits SEED_COUNT progress units per completed cell (all seeds finish together).
  # Use mkdir-based locking (atomic across all platforms, no flock dependency).
  local lock_acquired=0
  local lock_attempts=0
  while (( lock_attempts < 100 )); do
    if mkdir "$COUNTER_LOCK" 2>/dev/null; then
      lock_acquired=1
      break
    fi
    sleep 0.05
    lock_attempts=$((lock_attempts + 1))
  done

  if (( lock_acquired )); then
    # Count absolute progress from actual files (subtract initial count).
    # This avoids race conditions between parallel workers.
    # Use find (exit 0 even if empty) to avoid pipefail issues.
    local files_now
    files_now=$(find "$RESULTS_DIR" -maxdepth 1 -name '*.json' -type f 2>/dev/null | wc -l | tr -d ' ')
    : "${files_now:=0}"
    local n=$(( files_now - INITIAL_COUNT ))
    echo "$n" > "$COUNTER_FILE"
    
    # The Python monitor parses `Strategy=... | Fault=... | Seed=\d+` for labels
    # and `[N/TOTAL]` for the progress bar — we emit both in the same line.
    # Seed field holds the last seed of the cell for display purposes.
    local last_seed
    last_seed=$(echo "$SEEDS" | awk -F, '{print $NF}')
    echo ">>> [${n}/${TOTAL_EXP}] Strategy=${strat} | Fault=${fault} | Seed=${last_seed} | Base=${base} | Partition=${part:-none}"
    rmdir "$COUNTER_LOCK"
  fi
}

# ── Round-robin worker loop ───────────────────────────────────────────────────
run_worker() {
  local worker_id="$1"
  local num_workers="$2"
  local idx=0
  for cell in "${CELLS[@]}"; do
    if (( idx % num_workers == worker_id - 1 )); then
      run_cell "$cell" "$worker_id"
    fi
    idx=$((idx + 1))
  done
}

# ── Launch ────────────────────────────────────────────────────────────────────
if (( WORKERS <= 1 )); then
  run_worker 1 1
else
  pids=()
  for w in $(seq 1 "$WORKERS"); do
    run_worker "$w" "$WORKERS" &
    pids+=("$!")
  done
  wait "${pids[@]}"
fi

# ── Summary ───────────────────────────────────────────────────────────────────
FINAL=$(cat "$COUNTER_FILE")
echo ""
echo "=== MATRIX COMPLETE ==="
echo "Completed:    $FINAL/$TOTAL_EXP experiments"
RESULT_COUNT=$(ls "$RESULTS_DIR"/*.json 2>/dev/null | wc -l | tr -d ' ')
echo "Result files: $RESULT_COUNT"
