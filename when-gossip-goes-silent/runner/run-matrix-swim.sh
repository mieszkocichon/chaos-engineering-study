#!/usr/bin/env bash
# run-matrix-swim.sh - thin wrapper, the matrix is config/experiments/matrix-swim.json
exec bash "$(dirname "${BASH_SOURCE[0]}")/run-experiment.sh" config/experiments/matrix-swim.json "$@"
