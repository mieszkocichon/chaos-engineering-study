#!/bin/bash
# run-matrix-sensitivity.sh
#
# Sensitivity sweep parametrów StaticStrategy — odpowiedź na zarzut recenzji
# "information leakage: parametry Static dobierane pod profile błędów" (recenzent kazał to zrobić).
# Uruchamia 4 warianty Static z parametrami NIE tunowanymi pod testy:
#   - r4j-default : odpowiednik Resilience4j defaults (retry=3, CB=50%, conc=25)
#   - aggressive  : retry=5, cb=3, concurrency=80
#   - conservative: retry=1, cb=10, concurrency=20
#   - matched     : dotychczasowe defaults (retry=3, cb=5, conc=50) — baseline
#
# Wyniki w `results-sensitivity/`, izolowane od głównej macierzy.
#
# Usage:
#   bash runner/run-matrix-sensitivity.sh
#   SEEDS="1000,2000,3000" bash runner/run-matrix-sensitivity.sh

set -euo pipefail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR/.."

TOPOLOGY=${TOPOLOGY:-ring-50}
SEEDS=${SEEDS:-"1000,2000,3000,4000,5000,6000,7000,8000,9000,10000,11000,12000,13000,14000,15000,16000,17000,18000,19000,20000"}
RESULTS_DIR="./results-sensitivity"
PROJECT="sens"
COMPOSE="docker-compose-local-${PROJECT}.yml"

SEED_COUNT=$(echo "$SEEDS" | tr ',' '\n' | wc -l | tr -d ' ')

BASE="no-wan"
FAULT="baseline-flaky"
PARTITION=""

# Warianty — każdy kodowany jako "label|JSON_PARAMS". Taki prosty hack.
# JSON przekazywany do StaticStrategy via STATIC_PARAMS env.
VARIANTS=(
  "r4j-default|{\"retryCount\":3,\"retryBackoff\":2,\"cbThreshold\":5,\"cbTimeoutMs\":10000,\"requestTimeoutMs\":2000,\"maxConcurrent\":25}"
  "aggressive|{\"retryCount\":5,\"retryBackoff\":1.5,\"cbThreshold\":3,\"cbTimeoutMs\":5000,\"requestTimeoutMs\":3000,\"maxConcurrent\":80}"
  "conservative|{\"retryCount\":1,\"retryBackoff\":3,\"cbThreshold\":10,\"cbTimeoutMs\":20000,\"requestTimeoutMs\":3000,\"maxConcurrent\":20}"
  "matched|{\"retryCount\":3,\"retryBackoff\":2,\"cbThreshold\":5,\"cbTimeoutMs\":10000,\"requestTimeoutMs\":3000,\"maxConcurrent\":50}"
)

TOTAL_CELLS=${#VARIANTS[@]}
TOTAL_EXP=$(( TOTAL_CELLS * SEED_COUNT ))

mkdir -p "$RESULTS_DIR"

echo "=== STATIC SENSITIVITY MATRIX (izolowana) ==="
echo "Topology:     $TOPOLOGY"
echo "Fault:        $FAULT  (base=$BASE)"
echo "Seeds:        $SEEDS"
echo "Results dir:  $RESULTS_DIR"
echo "Project:      $PROJECT"
echo "Variants:     ${#VARIANTS[@]}"
echo "Experiments:  $TOTAL_EXP"
echo "============================================="

if ! docker image inspect chaos-experiment:latest > /dev/null 2>&1; then
  echo "[Setup] Budowanie chaos-experiment:latest..."
  docker build -t chaos-experiment:latest . > /dev/null
fi

export INITIAL_COUNT
INITIAL_COUNT=$(find "$RESULTS_DIR" -maxdepth 1 -name '*.json' -type f 2>/dev/null | wc -l | tr -d ' ')
: "${INITIAL_COUNT:=0}"

run_variant() {
  local label="$1"
  local params_json="$2"
  echo ""
  echo ">>> Sensitivity variant: $label"
  echo "    STATIC_PARAMS=$params_json"

  # Nazwa wariantu trafia do FAULT_PROFILE? Nie — FAULT=baseline-flaky. Zamiast tego
  # osadzamy label w nazwie pliku wyniku przez override STRATEGY — ale to zmieniłoby
  # case w service-runner. Prostsze: zapisujemy po fakcie (poniżej).
  RESULTS_DIR_HOST="$RESULTS_DIR" \
  STATIC_PARAMS="$params_json" \
    node generate-compose-topology.js "$TOPOLOGY" "$FAULT" "static" "$SEEDS" "$BASE" "$PARTITION" "$PROJECT" > /dev/null

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
      echo "    WARN: coordinator disappeared, no i znowu po ${elapsed}s"; break
    fi
    if (( elapsed > timeout )); then echo "    TIMEOUT after ${elapsed}s"; break; fi
    sleep 3
  done

  docker compose -p "$PROJECT" -f "$COMPOSE" down --remove-orphans > /dev/null 2>&1 || true

  # Przemianuj świeżo utworzone pliki: doklej wariant do nazwy dla analizy.
  # Pliki mają prefix "${TOPOLOGY}_static_"; dodajemy "__${label}__" przed timestampem.
  for f in "$RESULTS_DIR"/${TOPOLOGY}_static_${BASE}_${FAULT}_*.json; do
    [[ -e "$f" ]] || continue
    # Tylko pliki które jeszcze nie zawierają __<label>__ (nie zostały już przemianowane).
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
  IFS='|' read -r label params <<< "$v"
  run_variant "$label" "$params"
done

FINAL=$(find "$RESULTS_DIR" -maxdepth 1 -name '*.json' -type f | wc -l | tr -d ' ')
echo ""
echo "=== SENSITIVITY COMPLETE ==="
echo "Result files w $RESULTS_DIR: $FINAL"
echo "Pliki oznaczone sufiksem __<variant>__ dla porównania."
