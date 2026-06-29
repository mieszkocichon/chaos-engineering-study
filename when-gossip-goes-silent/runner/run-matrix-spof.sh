#!/bin/bash
# run-matrix-spof.sh
#
# SPOF test dla Central Controller — odpowiedź na zarzut recenzji
# "Central dominuje, pokaż koszt SPOF" (no bo recenzent narzekał, że to dominuje). Porównuje 3 konfiguracje:
#   - central-baseline : Central bez ingerencji (baseline)
#   - central-spof30   : Central zabity T+30s od startu workloadu
#   - mas-agent-ref    : MAS-ODA (decentralized, brak SPOF) dla porównania
#
# Każda konfiguracja uruchamia 20 seedów na ring-50 pod baseline-flaky / no-wan.
# Wyniki w `results-spof/`, izolowane od głównej macierzy.
#
# Usage:
#   bash runner/run-matrix-spof.sh
#   SEEDS="1000,2000,3000" bash runner/run-matrix-spof.sh

set -euo pipefail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR/.."

TOPOLOGY=${TOPOLOGY:-ring-50}
SEEDS=${SEEDS:-"1000,2000,3000,4000,5000,6000,7000,8000,9000,10000,11000,12000,13000,14000,15000,16000,17000,18000,19000,20000"}
RESULTS_DIR="./results-spof"
PROJECT="spof"
COMPOSE="docker-compose-local-${PROJECT}.yml"
SPOF_KILL_AT_SEC_VALUE=${SPOF_KILL_AT_SEC_VALUE:-30}

SEED_COUNT=$(echo "$SEEDS" | tr ',' '\n' | wc -l | tr -d ' ')

BASE="no-wan"
FAULT="baseline-flaky"
PARTITION=""

# Warianty: "label|strategy|spof_kill_at_sec"
VARIANTS=(
  "central-baseline|central|0"
  "central-spof30|central|${SPOF_KILL_AT_SEC_VALUE}"
  "mas-agent-ref|mas-agent|0"
)

TOTAL_CELLS=${#VARIANTS[@]}
TOTAL_EXP=$(( TOTAL_CELLS * SEED_COUNT ))

mkdir -p "$RESULTS_DIR"

echo "=== SPOF MATRIX (izolowana coby nie bruździć) ==="
echo "Topology:     $TOPOLOGY"
echo "Fault:        $FAULT  (base=$BASE)"
echo "Seeds:        $SEEDS"
echo "Kill at:      T+${SPOF_KILL_AT_SEC_VALUE}s from workload start"
echo "Results dir:  $RESULTS_DIR"
echo "Project:      $PROJECT"
echo "Variants:     ${#VARIANTS[@]}"
echo "Experiments:  $TOTAL_EXP"
echo "================================="

if ! docker image inspect chaos-experiment:latest > /dev/null 2>&1; then
  echo "[Setup] Budowanie chaos-experiment:latest..."
  docker build -t chaos-experiment:latest . > /dev/null
fi

export INITIAL_COUNT
INITIAL_COUNT=$(find "$RESULTS_DIR" -maxdepth 1 -name '*.json' -type f 2>/dev/null | wc -l | tr -d ' ')
: "${INITIAL_COUNT:=0}"

run_variant() {
  local label="$1"
  local strat="$2"
  local kill_at="$3"
  echo ""
  echo ">>> Variant: $label | strategy=$strat | kill_at=$kill_at"

  local spof_env=""
  if [[ "$kill_at" != "0" ]]; then
    spof_env="SPOF_KILL_AT_SEC=$kill_at"
  fi

  RESULTS_DIR_HOST="$RESULTS_DIR" \
  SPOF_KILL_AT_SEC="$kill_at" \
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
      echo "    WARN: coordinator zniknął (jak zwykle) po ${elapsed}s"; break
    fi
    if (( elapsed > timeout )); then echo "    TIMEOUT after ${elapsed}s"; break; fi
    sleep 3
  done

  docker compose -p "$PROJECT" -f "$COMPOSE" down --remove-orphans > /dev/null 2>&1 || true

  # Przemianowanie plików: doklej label do nazwy dla analizy.
  for f in "$RESULTS_DIR"/${TOPOLOGY}_${strat}_${BASE}_${FAULT}_*.json; do
    [[ -e "$f" ]] || continue
    if [[ "$f" != *"__${label}__"* && "$f" != *"__"*"__"* ]]; then
      mv "$f" "${f%.json}__${label}__.json"
    fi
  done

  local files_now
  files_now=$(find "$RESULTS_DIR" -maxdepth 1 -name '*.json' -type f 2>/dev/null | wc -l | tr -d ' ')
  : "${files_now:=0}"
  local n=$(( files_now - INITIAL_COUNT ))
  local last_seed
  last_seed=$(echo "$SEEDS" | awk -F, '{print $NF}')
  echo ">>> [${n}/${TOTAL_EXP}] Variant=${label} | Fault=${FAULT} | Seed=${last_seed}"
}

for v in "${VARIANTS[@]}"; do
  IFS='|' read -r label strat kill_at <<< "$v"
  run_variant "$label" "$strat" "$kill_at"
done

FINAL=$(find "$RESULTS_DIR" -maxdepth 1 -name '*.json' -type f | wc -l | tr -d ' ')
echo ""
echo "=== SPOF COMPLETE ==="
echo "Result files w $RESULTS_DIR: $FINAL"
echo "Pliki oznaczone sufiksem __<label>__ dla porównania."
