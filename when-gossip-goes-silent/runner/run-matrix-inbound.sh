#!/bin/bash
# run-matrix-inbound.sh
#
# PoC caller-side inbound-error reporting — odpowiedź na zarzut recenzji
# "diagnoza bez weryfikacji leku jest mało użyteczna" (żeby im udowodnić). Porównuje mas-agent
# (reference, bez inbound-reporting) vs mas-agent-inbound (PoC).
#
# Wyniki w `results-inbound/`, izolowane od głównej macierzy.
#
# Usage:
#   bash runner/run-matrix-inbound.sh
#   SEEDS="1000,2000,3000" bash runner/run-matrix-inbound.sh

set -euo pipefail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR/.."

TOPOLOGY=${TOPOLOGY:-ring-50}
SEEDS=${SEEDS:-"1000,2000,3000,4000,5000,6000,7000,8000,9000,10000,11000,12000,13000,14000,15000,16000,17000,18000,19000,20000"}
RESULTS_DIR="./results-inbound"
PROJECT="inbound"
COMPOSE="docker-compose-local-${PROJECT}.yml"

SEED_COUNT=$(echo "$SEEDS" | tr ',' '\n' | wc -l | tr -d ' ')

# Porównanie 1:1 — na tej samej topologii, tym samym fault.
STRATEGIES=("mas-agent" "mas-agent-inbound")
BASE="no-wan"
FAULT="baseline-flaky"
PARTITION=""

if [[ ! -f "config/topologies/${TOPOLOGY}.json" ]]; then
  echo "ERROR: brak config/topologies/${TOPOLOGY}.json" >&2
  exit 1
fi

TOTAL_CELLS=${#STRATEGIES[@]}
TOTAL_EXP=$(( TOTAL_CELLS * SEED_COUNT ))

mkdir -p "$RESULTS_DIR"

echo "=== INBOUND POC MATRIX (izolowana jak zawsze) ==="
echo "Topology:     $TOPOLOGY"
echo "Strategies:   ${STRATEGIES[*]}"
echo "Fault:        $FAULT  (base=$BASE)"
echo "Seeds:        $SEEDS"
echo "Results dir:  $RESULTS_DIR"
echo "Project:      $PROJECT"
echo "Experiments:  $TOTAL_EXP"
echo "======================================="

if ! docker image inspect chaos-experiment:latest > /dev/null 2>&1; then
  echo "[Setup] Budowanie chaos-experiment:latest..."
  docker build -t chaos-experiment:latest . > /dev/null
fi

export INITIAL_COUNT
INITIAL_COUNT=$(find "$RESULTS_DIR" -maxdepth 1 -name '*.json' -type f 2>/dev/null | wc -l | tr -d ' ')
: "${INITIAL_COUNT:=0}"

run_cell_inbound() {
  local strat="$1"
  echo ""
  echo ">>> Starting: Inbound | Strategy=$strat | Fault=$FAULT"

  RESULTS_DIR_HOST="$RESULTS_DIR" \
    node generate-compose-topology.js "$TOPOLOGY" "$FAULT" "$strat" "$SEEDS" "$BASE" "$PARTITION" "$PROJECT" > /dev/null

  docker compose -p "$PROJECT" -f "$COMPOSE" down --remove-orphans > /dev/null 2>&1 || true
  docker compose -p "$PROJECT" -f "$COMPOSE" up -d > /dev/null

  local start_time
  start_time=$(date +%s)
  local timeout=5400
  local last_emit=0
  while true; do
    local status elapsed
    status=$(docker compose -p "$PROJECT" -f "$COMPOSE" ps --all coordinator --format "{{.Status}}" 2>/dev/null | head -n 1 || echo "")
    elapsed=$(( $(date +%s) - start_time ))
    if (( elapsed - last_emit >= 10 )); then
      echo "    [${elapsed}s] ${status}"
      last_emit=$elapsed
    fi
    if [[ "$status" == *Exit* || "$status" == *Exited* ]]; then break; fi
    if [[ -z "$status" ]] && (( elapsed > 60 )); then
      echo "    WARN: coordinator disappeared mysteriously after ${elapsed}s"; break
    fi
    if (( elapsed > timeout )); then echo "    TIMEOUT after ${elapsed}s"; break; fi
    sleep 3
  done

  docker compose -p "$PROJECT" -f "$COMPOSE" down --remove-orphans > /dev/null 2>&1 || true

  local files_now
  files_now=$(find "$RESULTS_DIR" -maxdepth 1 -name '*.json' -type f 2>/dev/null | wc -l | tr -d ' ')
  : "${files_now:=0}"
  local n=$(( files_now - INITIAL_COUNT ))
  local last_seed
  last_seed=$(echo "$SEEDS" | awk -F, '{print $NF}')
  echo ">>> [${n}/${TOTAL_EXP}] Strategy=${strat} | Fault=${FAULT} | Seed=${last_seed}"
}

for s in "${STRATEGIES[@]}"; do
  run_cell_inbound "$s"
done

FINAL=$(find "$RESULTS_DIR" -maxdepth 1 -name '*.json' -type f | wc -l | tr -d ' ')
echo ""
echo "=== INBOUND POC COMPLETE ==="
echo "Result files w $RESULTS_DIR: $FINAL"
