#!/bin/bash
set -e


SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR/.."

# scale=50
SCALE=20
STACK_NAME="chaos_experiment"
RESULTS_DIR="./results"


# STRATEGIES=("none" "static" "static-conservative" "central" "mas-agent" "backpressure")
STRATEGIES=("none" "static" "central" "mas-agent" "backpressure")
FAULTS=("baseline-flaky" "burst-failure" "slow-degradation" "cascade-crash")


SEEDS=(1000 2000 3000 4000 5000)

mkdir -p $RESULTS_DIR

echo "=== ROZPOCZYNANIE MACIERZY EKSPERYMENTALNEJ (SWARM) ==="
echo "Skala: $SCALE węzłów"
echo "Strategie: ${STRATEGIES[*]}"
echo "Awarie: ${FAULTS[*]}"
echo "Ziarna: ${SEEDS[*]}"
echo "Łącznie: $((${#STRATEGIES[@]} * ${#FAULTS[@]} * ${#SEEDS[@]})) eksperymentów"
echo "======================================================="


if ! docker info | grep -q "Swarm: active"; then
    docker swarm init || true
fi


echo "[Setup] Budowanie obrazu Docker..."
docker build -t chaos-experiment:latest . > /dev/null

for strategy in "${STRATEGIES[@]}"; do
    for fault in "${FAULTS[@]}"; do
        for seed in "${SEEDS[@]}"; do
        echo ""
        echo ">>> URUCHAMIANIE: Strategy=$strategy | Fault=$fault | Seed=$seed"


        node generate-swarm-topology.js $SCALE $fault $strategy $seed


        docker stack rm $STACK_NAME 2>/dev/null || true
        # Wait until all services from the previous stack are actually removed
        CLEANUP_DEADLINE=$(($(date +%s) + 60))
        while docker service ls --filter "name=${STACK_NAME}" --format "{{.Name}}" 2>/dev/null | grep -q "${STACK_NAME}"; do
            if [ $(date +%s) -gt $CLEANUP_DEADLINE ]; then
                echo "    WARN: Stack cleanup timeout — proceeding anyway"
                break
            fi
            sleep 2
        done
        sleep 3  # brief grace period for network cleanup

        docker stack deploy -c docker-compose-swarm.yml $STACK_NAME > /dev/null
        
        echo "    Czekanie na zakończenie eksperymentu (ok. 100s)..."
        

        START_TIME=$(date +%s)

        while true; do

            STATUS=$(docker service ps ${STACK_NAME}_coordinator --format "{{.CurrentState}}" | head -n 1)
            

            LAST_LOG=$(docker service logs --tail 1 ${STACK_NAME}_coordinator 2>/dev/null | sed 's/^.*| //' | tr -d '\n' | cut -c 1-80)
            ELAPSED=$(($(date +%s) - START_TIME))


            printf "\r    [%3ds] Status: %-20s | Info: %s\033[K" "$ELAPSED" "$STATUS" "$LAST_LOG"

            if [[ "$STATUS" == *"Complete"* ]] || [[ "$STATUS" == *"Shutdown"* ]] || [[ "$STATUS" == *"Failed"* ]] || [[ "$STATUS" == *"Rejected"* ]]; then
                echo ""
                break
            fi
            sleep 5
        done
        
        echo "    Eksperyment zakończony. Status: $STATUS"


        docker stack rm $STACK_NAME > /dev/null
        done
    done
done

echo ""
echo "=== MACIERZ ZAKOŃCZONA ==="
echo "Wyniki znajdują się w $RESULTS_DIR"
