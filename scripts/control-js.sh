#!/usr/bin/env bash
# Control run for US4: the JS reference on THIS machine, same subset as equivalence.sh but
# fewer seeds, so Rust-vs-JS can be separated from this-machine-vs-historical-host.
# While each cell runs, samples the gateway's CPU throttling (cgroup cpu.stat) every 60 s.
# Run on the HOST shell. Results: when-gossip-goes-silent/results-js-local/.
set -euo pipefail
cd "$(dirname "$0")/../when-gossip-goes-silent"
export WORKERS=1   # cells, 5 seeds and results-js-local are in config/experiments/control-js.json
( while sleep 60; do
    s=$(MSYS_NO_PATHCONV=1 docker exec js1-gateway-1 cat /sys/fs/cgroup/cpu.stat 2>/dev/null | tr '\n' ' ') || true
    [ -n "$s" ] && echo "[cpu.stat js-gateway $(date -u +%H:%M:%S)] $s"
  done ) &
sampler=$!
trap 'kill $sampler 2>/dev/null || true' EXIT
bash runner/run-experiment.sh config/experiments/control-js.json
rm -f docker-compose-local-js*.yml
