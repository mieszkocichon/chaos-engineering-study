#!/bin/sh
# Host wrapper (Git Bash / Linux): run a command inside rust-dev with repo + caches mounted (ADR-0004).
#   sh scripts/dev.sh cargo test --workspace
set -eu
ROOT=$(cd "$(dirname "$0")/.." && (pwd -W 2>/dev/null || pwd))
MSYS_NO_PATHCONV=1 exec docker run --rm \
  -e HOST_APP_PATH="$ROOT" \
  -v "$ROOT:/app" -v cargo-registry:/usr/local/cargo/registry -v rust-target:/app/target \
  -v /var/run/docker.sock:/var/run/docker.sock -w /app rust-dev "$@"
