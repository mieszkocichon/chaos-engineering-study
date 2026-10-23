#!/usr/bin/env bash
# US4 follow-up: JS vs Rust on THIS machine with the gateway CPU limit raised (no cgroup
# throttling), same subset as equivalence.sh, SEEDS per cell (default 5). Runs JS first, then
# Rust, strictly sequentially; samples cpu.stat of the gateway and svc-01 every 60 s.
# Run on the HOST shell. Results: when-gossip-goes-silent/results-{js,rs}-cpu${GATEWAY_CPUS}/.
set -euo pipefail
cd "$(dirname "$0")/../when-gossip-goes-silent"

# one matrix run at a time on this Docker (two sessions once tore down each other's clusters)
LOCK=../.matrix.lock
if ! mkdir "$LOCK" 2>/dev/null; then
  echo "another matrix run holds $LOCK – refusing to start" >&2
  exit 1
fi
trap 'rmdir "$LOCK" 2>/dev/null || true' EXIT

export GATEWAY_CPUS="${GATEWAY_CPUS:-4}"   # cells and seeds: config/experiments/unthrottled-cpu4.json
export WORKERS=1

for impl in js rust; do
  short=${impl/rust/rs}
  prefix="u${short}"
  (
    export IMPL=$impl RESULTS_DIR="./results-${short}-cpu${GATEWAY_CPUS}" PROJECT_PREFIX=$prefix
    ( while sleep 60; do
        for c in gateway svc-01; do
          s=$(MSYS_NO_PATHCONV=1 docker exec "${prefix}1-${c}-1" cat /sys/fs/cgroup/cpu.stat 2>/dev/null \
              | grep -E '^(nr_periods|nr_throttled|throttled_usec)' | tr '\n' ' ') || true
          [ -n "$s" ] && echo "[cpu.stat $impl $c $(date -u +%H:%M:%S)] $s"
        done
      done ) &
    sampler=$!
    trap 'kill $sampler 2>/dev/null || true' EXIT
    echo "[unthrottled] IMPL=$impl GATEWAY_CPUS=$GATEWAY_CPUS start $(date -u +%FT%TZ)"
    bash runner/run-experiment.sh config/experiments/unthrottled-cpu4.json
    rm -f docker-compose-local-${prefix}*.yml
    echo "[unthrottled] IMPL=$impl done $(date -u +%FT%TZ)"
  )
done
