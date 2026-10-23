#!/bin/sh
# Container integration (T060/T061): ring-5 in three modes – Rust only (all faults on real
# tc/iptables), mixed-a (Rust gateway + JS leaves), mixed-b (JS gateway + Rust leaves).
# Services start detached and must become healthy; the tester runs in the foreground and
# its exit code is the script's.
set -eu
F=tests/compose/ring5.yml
SVCS="gateway svc-01 svc-02 svc-03 svc-04 svc-05"
down() { docker compose -f $F down -v --remove-orphans >/dev/null 2>&1 || true; }
trap down EXIT
docker compose -f $F --profile build build
docker compose -f $F build tester
run() {
  m=$1; shift
  echo "[ring5] MODE=$m"
  env MODE="$m" "$@" docker compose -f $F up -d --wait $SVCS
  env MODE="$m" "$@" docker compose -f $F run --rm --no-deps tester
  down
}
run rust
run mixed-a LEAF_JS_IMAGE=chaos-experiment:test GATEWAY_STRATEGY=mas-agent
run mixed-b GATEWAY_IMAGE=chaos-experiment:test GATEWAY_STRATEGY=mas-agent
