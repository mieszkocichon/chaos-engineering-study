#!/usr/bin/env bash
# run-matrix-compose.sh - thin wrapper, the matrix is config/experiments/matrix-compose.json
exec bash "$(dirname "${BASH_SOURCE[0]}")/run-experiment.sh" config/experiments/matrix-compose.json "$@"
