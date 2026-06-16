#!/bin/bash
set -e

STACK_NAME="chaos_experiment"
SCALE=${1:-20}

echo "=== [1/5] Sprawdzanie statusu Docker Swarm ==="
if ! docker info | grep -q "Swarm: active"; then
    echo "Inicjalizacja Docker Swarm (single-node)..."
    docker swarm init || true
else
    echo "Docker Swarm jest już aktywny."
fi

echo "=== [2/5] Generowanie konfiguracji Swarm ($SCALE węzłów) ==="
node generate-swarm-topology.js $SCALE

echo "=== [3/5] Budowanie obrazu (wymagane dla Swarm) ==="

docker build -t chaos-experiment:latest .

echo "=== [4/5] Wdrażanie stosu (Stack Deploy) ==="

docker stack rm $STACK_NAME 2>/dev/null || true

sleep 3

docker stack deploy -c docker-compose-swarm.yml $STACK_NAME

echo "=== [5/5] Eksperyment uruchomiony ==="
echo "Czekanie na start serwisów... (może to potrwać ok. 30s)"


until docker service logs ${STACK_NAME}_coordinator >/dev/null 2>&1; do
    echo "Oczekiwanie na utworzenie serwisu koordynatora..."
    sleep 2
done

echo "Podłączanie do logów koordynatora. Naciśnij Ctrl+C aby przerwać śledzenie."
docker service logs -f ${STACK_NAME}_coordinator

echo ""
read -p "Czy usunąć stos (sprzątanie)? (y/N) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    docker stack rm $STACK_NAME
    echo "Stos usunięty."
fi