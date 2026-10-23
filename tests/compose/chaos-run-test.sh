#!/bin/sh
# Integration test for chaos-run: executes a small experiment (ring-5, 1 cell, 1 seed, short time)
# and checks that analyze_results.py reads the resulting file.
set -eu
RDIR="when-gossip-goes-silent/results-test-chaos-run"
RDIR_ACT="when-gossip-goes-silent/results-test-chaos-run-action"
cleanup() {
  rm -rf "$RDIR" "$RDIR_ACT"
  docker compose -p crt1 -f when-gossip-goes-silent/docker-compose-local-crt1.yml down -v --remove-orphans >/dev/null 2>&1 || true
  docker compose -p cta1 -f when-gossip-goes-silent/docker-compose-local-cta1.yml down -v --remove-orphans >/dev/null 2>&1 || true
  rm -f when-gossip-goes-silent/docker-compose-local-crt1.yml when-gossip-goes-silent/.chaos-run-crt.lock
  rm -f when-gossip-goes-silent/docker-compose-local-cta1.yml when-gossip-goes-silent/.chaos-run-cta.lock
}
trap cleanup EXIT

docker compose -f tests/compose/ring5.yml build build-rs
export IMAGE=chaos-experiment-rs:test
cargo build -p chaos --bin chaos-run

cleanup

(cd when-gossip-goes-silent && ../target/debug/chaos-run config/experiments/test-ring5.json)

# Check that at least one result JSON was produced
count=$(find "$RDIR" -maxdepth 1 -name "ring-5_*.json" | wc -l)
if [ "$count" -lt 1 ]; then
  echo "ERROR: no result file generated in $RDIR" >&2
  exit 1
fi

# Reproducibility manifest next to the results, but not among the result files
ls "$RDIR"/manifests/test-ring5-*.json >/dev/null
if [ "$(find "$RDIR" -maxdepth 1 -name '*.json' | wc -l)" -ne 1 ]; then
  echo "ERROR: expected exactly the one result file in $RDIR" >&2
  exit 1
fi

# A second run with --resume must find nothing to do
(cd when-gossip-goes-silent && ../target/debug/chaos-run config/experiments/test-ring5.json --resume) | grep -q "No cells to run"

# Run analyzer
python3 when-gossip-goes-silent/runner/analyze_results.py "$RDIR"

# Test mid-run action and spofTest metadata
(cd when-gossip-goes-silent && ../target/debug/chaos-run config/experiments/test-action.json)
grep -q '"spofTest"' "$RDIR_ACT"/ring-5_*.json
grep -q '"killedService": "svc-03"' "$RDIR_ACT"/ring-5_*.json
python3 when-gossip-goes-silent/runner/analyze_results.py "$RDIR_ACT" | grep -q "TEST SPOF"

echo "[chaos-run-test.sh] PASS"
