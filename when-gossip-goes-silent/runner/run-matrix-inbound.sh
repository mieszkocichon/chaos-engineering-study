#!/usr/bin/env bash
# run-matrix-inbound.sh - thin wrapper, the matrix is config/experiments/matrix-inbound.json
exec bash "$(dirname "${BASH_SOURCE[0]}")/run-experiment.sh" config/experiments/matrix-inbound.json "$@"
