#!/bin/sh
# One-command test run (FR-018). Runs inside rust-dev:
#   sh scripts/dev.sh sh scripts/test.sh
# 1) unit + light integration tests, 2) container integration scripts in tests/compose/*.sh.
set -eu
cargo test --workspace
if [ -f when-gossip-goes-silent/tests/test_compare_impls.py ]; then
  python3 -m unittest discover -s when-gossip-goes-silent/tests -p 'test_*.py'
fi
for t in tests/compose/*.sh; do
  sh "$t"   # a failing script must fail the run (no `[ -f ] &&`, which hides errors under set -e)
done
echo "[test.sh] all green"
