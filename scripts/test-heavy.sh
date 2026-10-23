#!/bin/sh
# Heavy tests – LOCAL ONLY (FR-019): ignored cargo tests, topologies >= 20 services,
# equivalence report (US4). Runs inside rust-dev:
#   sh scripts/dev.sh sh scripts/test-heavy.sh
set -eu
cargo test --workspace -- --ignored --skip cluster
if [ -f scripts/equivalence.sh ]; then
  sh scripts/equivalence.sh
fi
