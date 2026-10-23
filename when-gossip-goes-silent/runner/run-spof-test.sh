#!/usr/bin/env bash
# run-spof-test.sh - thin wrapper, the matrix is config/experiments/spof-test.json
exec bash "$(dirname "${BASH_SOURCE[0]}")/run-experiment.sh" config/experiments/spof-test.json "$@"
