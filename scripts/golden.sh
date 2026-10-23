#!/bin/sh
# Regenerates golden test vectors from the archived JS reference in old-implementation/ (research R14).
# Run in node:20-bookworm with the repo mounted at /app:
#   docker run --rm -v "${PWD}:/app" -w /app node:20-bookworm sh scripts/golden.sh
set -eu
(cd old-implementation/when-gossip-goes-silent && npm ci --silent)
for f in crates/*/tests/golden/gen/[!_]*.mjs; do
  echo "[golden] $f"
  node "$f"
done
