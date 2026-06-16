#!/bin/bash


set -e

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR/.."

SCALE=${1:-5}
COMPOSE_FILE="docker-compose-local.yml"
RESULTS_DIR="./results"
KILL_TARGET="svc-03"
KILL_AT_SEC=30

mkdir -p "$RESULTS_DIR"

STRATEGIES=("none" "static" "central" "mas-agent" "backpressure")

echo "=== SPOF TEST (N=$SCALE, kill $KILL_TARGET at t=${KILL_AT_SEC}s) ==="
echo "Strategie: ${STRATEGIES[*]}"
echo "Łącznie: ${#STRATEGIES[@]} eksperymentów"
echo "==========================================================="


echo "[Setup] Budowanie obrazu Docker..."
docker build -t chaos-experiment:latest . > /dev/null

for strategy in "${STRATEGIES[@]}"; do
    echo ""
    echo ">>> SPOF: Strategy=$strategy | kill $KILL_TARGET at t=${KILL_AT_SEC}s"


    node ../generate-swarm-topology.js $SCALE baseline-flaky $strategy 1000

    sed -e 's/driver: overlay/driver: bridge/' \
        -e '/environment:/a\      SIMULATE_CLOCK_SKEW: "true"\n      SIMULATE_IO_CONTENTION: "true"' \
        ../docker-compose-swarm.yml > $COMPOSE_FILE


    docker compose -f $COMPOSE_FILE down --remove-orphans > /dev/null 2>&1 || true


    docker compose -f $COMPOSE_FILE up -d > /dev/null

    echo "    Czekanie na start koordynatora..."
    START_TIME=$(date +%s)


    (
        sleep $KILL_AT_SEC
        echo ""
        echo "    [t=${KILL_AT_SEC}s] >>> ZABIJAM $KILL_TARGET (docker stop — brak restartu) <<<"
        docker stop "experiment-${KILL_TARGET}-1" > /dev/null 2>&1 || true
        echo "    [t=${KILL_AT_SEC}s] $KILL_TARGET zatrzymany."
    ) &
    KILLER_PID=$!


    while true; do
        STATUS=$(docker compose -f $COMPOSE_FILE ps --all coordinator --format "{{.Status}}" 2>/dev/null | head -n 1)
        LAST_LOG=$(docker compose -f $COMPOSE_FILE logs --tail 1 coordinator 2>/dev/null | tail -n 1 | cut -c 1-80)
        ELAPSED=$(($(date +%s) - START_TIME))

        printf "\r    [%3ds] Status: %-25s | %s\033[K" "$ELAPSED" "$STATUS" "$LAST_LOG"

        if [[ "$STATUS" == *"Exit"* ]] || [[ "$STATUS" == *"Exited"* ]]; then
            echo ""
            break
        fi

        if [ $ELAPSED -gt 300 ]; then
            echo ""
            echo "    TIMEOUT po ${ELAPSED}s"
            kill $KILLER_PID 2>/dev/null || true
            break
        fi

        sleep 3
    done


    wait $KILLER_PID 2>/dev/null || true

    echo "    Eksperyment zakończony. Status: $STATUS"


    RESULT_FILE=$(ls -t "$RESULTS_DIR"/*.json 2>/dev/null | head -n 1)
    if [ -n "$RESULT_FILE" ]; then

        node -e "
const fs = require('fs');
const f = '$RESULT_FILE';
const d = JSON.parse(fs.readFileSync(f, 'utf8'));
d.spofTest = {
  killedService: '$KILL_TARGET',
  killAtSec: $KILL_AT_SEC,
  permanent: true,
  note: 'docker stop (SIGTERM) — no restart due to on-failure policy'
};
fs.writeFileSync(f, JSON.stringify(d, null, 2));
console.log('[spof] Metadane SPOF dopisane do ' + f);
" 2>/dev/null || true
    fi


    docker compose -f $COMPOSE_FILE down --remove-orphans > /dev/null 2>&1


    if [[ "$strategy" == "central" || "$strategy" == "mas-agent" ]]; then
        echo ""
        echo ">>> SPOF-COORD: Strategy=$strategy | kill coordinator at t=${KILL_AT_SEC}s"

        node ../generate-swarm-topology.js $SCALE baseline-flaky $strategy 2000

        sed -e 's/driver: overlay/driver: bridge/' \
            -e '/environment:/a\      SIMULATE_CLOCK_SKEW: "true"\n      SIMULATE_IO_CONTENTION: "true"' \
            ../docker-compose-swarm.yml > $COMPOSE_FILE

        docker compose -f $COMPOSE_FILE down --remove-orphans > /dev/null 2>&1 || true
        docker compose -f $COMPOSE_FILE up -d > /dev/null

        echo "    Czekanie na start (faza coordinator-kill)..."
        START_TIME=$(date +%s)

        KILL_TARGET_2="coordinator"
        (
            sleep $KILL_AT_SEC
            echo ""
            echo "    [t=${KILL_AT_SEC}s] >>> ZABIJAM $KILL_TARGET_2 — test prawdziwego SPOF <<<"
            docker stop "experiment-${KILL_TARGET_2}-1" > /dev/null 2>&1 || true
            echo "    [t=${KILL_AT_SEC}s] $KILL_TARGET_2 zatrzymany."
        ) &
        KILLER2_PID=$!

        while true; do
            STATUS=$(docker compose -f $COMPOSE_FILE ps --all coordinator --format "{{.Status}}" 2>/dev/null | head -n 1)
            LAST_LOG=$(docker compose -f $COMPOSE_FILE logs --tail 1 coordinator 2>/dev/null | tail -n 1 | cut -c 1-80)
            ELAPSED=$(($(date +%s) - START_TIME))

            printf "\r    [%3ds] Status: %-25s | %s\033[K" "$ELAPSED" "$STATUS" "$LAST_LOG"

            if [[ "$STATUS" == *"Exit"* ]] || [[ "$STATUS" == *"Exited"* ]]; then
                echo ""
                break
            fi

            if [ $ELAPSED -gt 300 ]; then
                echo ""
                echo "    TIMEOUT po ${ELAPSED}s"
                kill $KILLER2_PID 2>/dev/null || true
                break
            fi

            sleep 3
        done

        wait $KILLER2_PID 2>/dev/null || true

        RESULT_FILE=$(ls -t "$RESULTS_DIR"/*.json 2>/dev/null | head -n 1)
        if [ -n "$RESULT_FILE" ]; then
            node -e "
const fs = require('fs');
const f = '$RESULT_FILE';
const d = JSON.parse(fs.readFileSync(f, 'utf8'));
d.spofTest = {
  killedService: 'coordinator',
  killAtSec: $KILL_AT_SEC,
  permanent: true,
  phase: 'coordinator-kill',
  note: 'Prawdziwy SPOF: awaria coordinator — central zamraża parametry, mas-agent działa autonomicznie'
};
fs.writeFileSync(f, JSON.stringify(d, null, 2));
console.log('[spof-coord] Metadane SPOF dopisane do ' + f);
" 2>/dev/null || true
        fi

        docker compose -f $COMPOSE_FILE down --remove-orphans > /dev/null 2>&1
    fi
done

echo ""
echo "=== SPOF TEST ZAKOŃCZONY ==="
echo "Wyniki w: $RESULTS_DIR/spof_*.json (szukaj pola 'spofTest' w JSON)"
echo ""
echo "Interpretacja:"
echo "  - Porównaj successRate między strategiami"
echo "  - MAS-agent powinien osiągnąć wyższy SR niż central/static/none"
echo "    (szybsza detekcja awarii przez gossip od sąsiadów svc-02, svc-04)"
echo "  - Brak restartu svc-03 weryfikuje główną tezę: system bez SPOF"
echo "    kontynuuje obsługę przez pozostałe 4 z 5 węzłów"
