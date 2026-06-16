#!/usr/bin/env bash


set -euo pipefail


SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR/.."

SCALE=20
IMAGE="chaos-experiment:latest"
CLUSTER_NAME="chaos-experiment"
NAMESPACE="chaos-experiment"
RESULTS_DIR="$(pwd)/results"
K8S_DIR="$(pwd)/k8s"

STRATEGIES=("none" "static" "central" "mas-agent" "backpressure")
FAULTS=("baseline-flaky" "burst-failure" "slow-degradation" "cascade-crash")

EXPERIMENT_TIMEOUT=360
CLEANUP_TIMEOUT=60


missing=()
for cmd in docker k3d kubectl; do
    command -v "$cmd" &>/dev/null || missing+=("$cmd")
done

if [[ ${
    echo "[ERROR] Brakujące narzędzia: ${missing[*]}"
    echo "  Instalacja: nix profile install nixpkgs#k3d nixpkgs#kubectl"
    exit 1
fi

if ! docker info &>/dev/null; then
    echo "[ERROR] Docker daemon nie działa."
    exit 1
fi


mkdir -p "$RESULTS_DIR" "$K8S_DIR"

echo "=== MACIERZ EKSPERYMENTALNA (Kubernetes / k3d) ==="
echo "Skala:     $SCALE węzłów"
echo "Strategie: ${STRATEGIES[*]}"
echo "Awarie:    ${FAULTS[*]}"
echo "Wyniki:    $RESULTS_DIR"
echo "==================================================="
echo ""


echo "[1/4] Budowanie obrazu Docker..."
docker build -t "$IMAGE" . --quiet
echo "      Obraz: $IMAGE"


echo ""
echo "[2/4] Tworzenie klastra k3d '$CLUSTER_NAME'..."


if k3d cluster list 2>/dev/null | grep -q "$CLUSTER_NAME"; then
    echo "      Usuwam istniejący klaster '$CLUSTER_NAME'..."
    k3d cluster delete "$CLUSTER_NAME" 2>/dev/null || true
fi

k3d cluster create "$CLUSTER_NAME" \
    --volume "${RESULTS_DIR}:/results-host@server:0" \
    --wait \
    --timeout 120s \
    --k3s-arg "--disable=traefik@server:0"

echo "      Klaster gotowy."


echo ""
echo "[3/4] Ładowanie obrazu '$IMAGE' do klastra k3d..."
k3d image import "$IMAGE" --cluster "$CLUSTER_NAME"
echo "      Obraz załadowany."


TOTAL=$(( ${
CURRENT=0

echo ""
echo "[4/4] Uruchamianie macierzy ($TOTAL kombinacji)..."

for strategy in "${STRATEGIES[@]}"; do
    for fault in "${FAULTS[@]}"; do
        CURRENT=$(( CURRENT + 1 ))
        echo ""
        echo ">>> [$CURRENT/$TOTAL] Strategy=$strategy | Fault=$fault"


        echo "    Generowanie manifestów..."
        docker run --rm \
            -v "$(pwd):/app" \
            "$IMAGE" \
            node generate-k8s-manifests.js "$SCALE" "$fault" "$strategy"

        MANIFEST="${K8S_DIR}/manifests-${fault}-${strategy}.yaml"


        if kubectl get namespace "$NAMESPACE" &>/dev/null; then
            echo "    Czyszczenie poprzedniego namespace..."
            kubectl delete namespace "$NAMESPACE" \
                --wait=true --timeout="${CLEANUP_TIMEOUT}s" 2>/dev/null || true
        fi


        echo "    Wdrażanie (namespace + 20 serwisów + koordynator)..."
        kubectl apply -f "$MANIFEST"


        echo "    Czekanie na eksperyment (max ${EXPERIMENT_TIMEOUT}s)..."
        START_TIME=$(date +%s)
        JOB_STATUS="running"

        while true; do
            ELAPSED=$(( $(date +%s) - START_TIME ))

            COMPLETE=$(kubectl get job coordinator -n "$NAMESPACE" \
                -o jsonpath='{.status.conditions[?(@.type=="Complete")].status}' \
                2>/dev/null || echo "")
            FAILED=$(kubectl get job coordinator -n "$NAMESPACE" \
                -o jsonpath='{.status.conditions[?(@.type=="Failed")].status}' \
                2>/dev/null || echo "")

            LAST_LOG=$(kubectl logs job/coordinator -n "$NAMESPACE" \
                --tail=1 2>/dev/null | tr -d '\n' | cut -c1-75 || echo "...")
            printf "\r    [%3ds] %s\033[K" "$ELAPSED" "$LAST_LOG"

            if [[ "$COMPLETE" == "True" ]]; then
                JOB_STATUS="complete"; echo ""; break
            fi
            if [[ "$FAILED" == "True" ]]; then
                JOB_STATUS="failed"; echo ""; break
            fi
            if [[ "$ELAPSED" -gt "$EXPERIMENT_TIMEOUT" ]]; then
                JOB_STATUS="timeout"; echo ""; break
            fi

            sleep 4
        done

        echo "    Status: $JOB_STATUS"
        if [[ "$JOB_STATUS" != "complete" ]]; then
            echo "    --- Logi koordynatora (ostatnie 20 linii) ---"
            kubectl logs job/coordinator -n "$NAMESPACE" --tail=20 2>/dev/null || true
            echo "    --- Pody namespace ---"
            kubectl get pods -n "$NAMESPACE" 2>/dev/null || true
            echo "    ---------------------------------------------"
        else
            kubectl logs job/coordinator -n "$NAMESPACE" --tail=3 2>/dev/null || true
        fi


        echo "    Sprzątanie namespace..."
        kubectl delete namespace "$NAMESPACE" \
            --wait=true --timeout="${CLEANUP_TIMEOUT}s" 2>/dev/null || true
    done
done


echo ""
echo "=== MACIERZ ZAKOŃCZONA ==="
echo "Wyniki JSON: $RESULTS_DIR"
echo "Manifesty K8s: $K8S_DIR"
echo ""
echo "Klaster k3d '$CLUSTER_NAME' pozostaje aktywny."
echo "Aby usunąć:  k3d cluster delete $CLUSTER_NAME"
echo ""
echo "Aby przeanalizować wyniki:"
echo "  python3 runner/analyze_results.py $RESULTS_DIR"
