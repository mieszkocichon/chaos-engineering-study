#!/usr/bin/env bash
# run-matrix-spof.sh - thin wrapper, the matrix is config/experiments/matrix-spof.json
exec bash "$(dirname "${BASH_SOURCE[0]}")/run-experiment.sh" config/experiments/matrix-spof.json "$@"
