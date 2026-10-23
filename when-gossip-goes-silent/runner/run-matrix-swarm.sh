#!/usr/bin/env bash
# run-matrix-swarm.sh - thin wrapper, the matrix is config/experiments/matrix-swarm.json
exec bash "$(dirname "${BASH_SOURCE[0]}")/run-experiment.sh" config/experiments/matrix-swarm.json "$@"
