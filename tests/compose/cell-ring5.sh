#!/bin/sh
# T077: one experiment cell on Rust only (real k6, real tc via baseline-flaky) → analyzer.
# Services run detached; coordinator and checker run in the foreground so their exit codes
# are the script's (no --abort-on-container-exit race).
set -eu
F=tests/compose/cell-ring5.yml
down() { docker compose -f $F down -v --remove-orphans >/dev/null 2>&1 || true; }
trap down EXIT
docker compose -f tests/compose/ring5.yml build build-rs
docker compose -f $F build checker
docker compose -f $F up -d gateway svc-01 svc-02 svc-03 svc-04 svc-05
docker compose -f $F run --rm --no-deps coordinator
docker compose -f $F run --rm --no-deps checker
