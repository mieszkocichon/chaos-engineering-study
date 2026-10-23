#!/usr/bin/env bash
# run-matrix-sensitivity.sh - thin wrapper, the matrix is config/experiments/matrix-sensitivity.json
exec bash "$(dirname "${BASH_SOURCE[0]}")/run-experiment.sh" config/experiments/matrix-sensitivity.json "$@"
