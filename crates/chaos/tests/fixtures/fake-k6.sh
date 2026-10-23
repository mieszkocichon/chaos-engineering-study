#!/bin/sh
# Stand-in for `k6 run …` in coordinator tests: copies a recorded end-of-test summary to the
# SUMMARY_FILE passed via `--env`, after FAKE_K6_SLEEP seconds (default 0).
for a in "$@"; do
  case "$a" in SUMMARY_FILE=*) out="${a#SUMMARY_FILE=}" ;; esac
done
sleep "${FAKE_K6_SLEEP:-0}"
cp "$(dirname "$0")/k6-summary.json" "$out"
echo "fake k6 run: $*"
