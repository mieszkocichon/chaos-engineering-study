#!/bin/bash
set -e

echo "=== [1/4] Generowanie topologii pierścienia (20 węzłów) ==="
node generate-ring-topology.js

echo "=== [2/4] Budowanie i uruchamianie kontenerów ==="

docker compose -f docker-compose-scale.yml up --build -d

echo "=== [3/4] Eksperyment w toku... ==="
echo "Śledzenie logów koordynatora. Naciśnij Ctrl+C aby przerwać śledzenie (eksperyment będzie trwał)."
docker compose -f docker-compose-scale.yml logs -f coordinator

echo ""
echo "=== [4/4] Sprzątanie po eksperymencie ==="
read -p "Czy chcesz zatrzymać i usunąć kontenery? (y/N) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]
then
    docker compose -f docker-compose-scale.yml down
    echo "Środowisko wyczyszczone."
else
    echo "Kontenery nadal działają. Użyj 'docker compose -f docker-compose-scale.yml down' aby je zatrzymać."
fi
