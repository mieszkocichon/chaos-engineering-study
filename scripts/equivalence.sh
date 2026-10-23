#!/usr/bin/env bash
# US4 / T080 – LOCAL ONLY, takes hours (ring-50 = 51 containers per worker).
# Runs the Rust port on a subset of the published matrix and writes the equivalence report.
# Run on the HOST shell (Git Bash / Linux), not inside rust-dev: the matrix bind-mounts
# ./config and ./results, which only resolve on the host.
#   bash scripts/equivalence.sh            # WORKERS=1 by default (safe for a laptop)
set -euo pipefail
cd "$(dirname "$0")/../when-gossip-goes-silent"
export WORKERS="${WORKERS:-1}"   # the cells, 30 seeds (1000..30000) and results-rs are in config/experiments/equivalence.json
start=$(date +%s)
bash runner/run-experiment.sh config/experiments/equivalence.json
rm -f docker-compose-local-rs*.yml
echo "[equivalence] Rust matrix finished in $(( $(date +%s) - start ))s"
python3 compare_impls.py results results-rs --out ../specs/001-rust-rewrite/equivalence-report.md
