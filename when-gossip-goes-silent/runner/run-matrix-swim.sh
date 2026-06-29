#!/bin/bash
# run-matrix-swim.sh
#
# Rozszerzenie eksperymentu o topologię SWIM-like (random gossip) — odpowiedź
# na upierdliwy zarzut recenzji "ring-50 to najgorszy możliwy przypadek dla gossip".
#
# Uruchamia 3 strategie (mas-agent, mas-agent-swim, backpressure) na topologii
# mesh-50 (K=8 losowych peerów/serwis) z fault baseline-flaky. Wyniki trafiają
# DO ODDZIELNEGO katalogu `results-swim/`, nie naruszając głównej matrycy w
# `results/`.
#
# Usage:
#   bash runner/run-matrix-swim.sh
#   SEEDS="1000,2000,3000" bash runner/run-matrix-swim.sh
#
# Zakłada: chaos-experiment:latest zbudowany (tak jak w głównej macierzy).
# Jeśli nie — skrypt zbuduje go sam.

set -euo pipefail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR/.."

TOPOLOGY=${TOPOLOGY:-mesh-30}
SEEDS=${SEEDS:-"1000,2000,3000,4000,5000,6000,7000,8000,9000,10000,11000,12000,13000,14000,15000,16000,17000,18000,19000,20000"}
RESULTS_DIR="./results-swim"
PROJECT="swim"
COMPOSE="docker-compose-local-${PROJECT}.yml"
SWIM_FANOUT=${SWIM_FANOUT:-3}

SEED_COUNT=$(echo "$SEEDS" | tr ',' '\n' | wc -l | tr -d ' ')

STRATEGIES=("mas-agent" "mas-agent-swim" "backpressure")
BASE="no-wan"
FAULT="baseline-flaky"
PARTITION=""

# Preflight — upewnij się że topologia mesh-50 istnieje (bo skrypty czasem gubią).
if [[ ! -f "config/topologies/${TOPOLOGY}.json" ]]; then
  echo "[SWIM] Brak ${TOPOLOGY}.json — generuję via scripts/generate-mesh-topology.js (tak na wszelki wypadek)"
  node scripts/generate-mesh-topology.js 50 8
fi

TOTAL_CELLS=${#STRATEGIES[@]}
TOTAL_EXP=$(( TOTAL_CELLS * SEED_COUNT ))

mkdir -p "$RESULTS_DIR"

echo "=== SWIM MATRIX (izolowane od głównej macierzy, zeby nic nie zepsuć) ==="
echo "Topology:     $TOPOLOGY   (K=$SWIM_FANOUT dla mas-agent-swim)"
echo "Strategies:   ${STRATEGIES[*]}"
echo "Fault:        $FAULT  (base=$BASE)"
echo "Seeds:        $SEEDS"
echo "Results dir:  $RESULTS_DIR"
echo "Project:      $PROJECT"
echo "Experiments:  $TOTAL_EXP"
echo "===================================================="

# Build image tylko jeśli go nie ma.
if ! docker image inspect chaos-experiment:latest > /dev/null 2>&1; then
  echo "[Setup] Budowanie chaos-experiment:latest..."
  docker build -t chaos-experiment:latest . > /dev/null
fi

# Licznik globalny — progress wypisywany w formacie kompatybilnym z GUI monitorem.
export INITIAL_COUNT
INITIAL_COUNT=$(find "$RESULTS_DIR" -maxdepth 1 -name '*.json' -type f 2>/dev/null | wc -l | tr -d ' ')
: "${INITIAL_COUNT:=0}"

run_cell_swim() {
  local strat="$1"
  local label="SWIM | Strategy=$strat | Fault=$FAULT | Topo=$TOPOLOGY"
  echo ""
  echo ">>> Starting: $label  (seeds=$SEEDS)"

  # Generate compose z RESULTS_DIR_HOST → results-swim.
  # SWIM_FANOUT propaguje się do serwisów, STATIC_PARAMS nie ustawione (default).
  RESULTS_DIR_HOST="$RESULTS_DIR" \
  SWIM_FANOUT="$SWIM_FANOUT" \
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
      echo "    WARN: coordinator disappeared (wtf) after ${elapsed}s"; break
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
  echo ">>> [${n}/${TOTAL_EXP}] Strategy=${strat} | Fault=${FAULT} | Seed=${last_seed} | Topo=${TOPOLOGY}"
}

for s in "${STRATEGIES[@]}"; do
  run_cell_swim "$s"
done

FINAL=$(find "$RESULTS_DIR" -maxdepth 1 -name '*.json' -type f | wc -l | tr -d ' ')
echo ""
echo "=== SWIM MATRIX COMPLETE ==="
echo "Result files w $RESULTS_DIR: $FINAL"
