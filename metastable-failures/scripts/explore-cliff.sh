#!/bin/sh
# Find a regime with a real recover->stuck cliff (not monotone fragility).
# Grid over queue bound (basin size) x surge magnitude. Look for a row that
# transitions recover(false) -> stuck(true) as surge grows.
set -e
# M1: command to run. Default = Rust (../target/release/run-m1, build: cargo build --release -p metastable);
# archived JS: M1="node ../old-implementation/metastable-failures/src/run-m1.js"
M1=${M1:-../target/release/run-m1}
BASE="--baseline 350 --surge-dur 8000 --timeout 300 --attempts 30 --backoff 30 --seed 1"

for q in 150 300 500 800; do
  printf "qmax=%-5s : " "$q"
  for s in 500 700 900 1100 1400 1800; do
    out=$($M1 $BASE --qmax $q --surge $s)
    stuck=$(echo "$out" | grep -oE '"metastable_stuck": (true|false)' | awk '{print $2}')
    if [ "$stuck" = "true" ]; then mark="X"; else mark="."; fi
    printf "%s%s " "$s" "$mark"
  done
  printf "   (X=stuck . =recovered)\n"
done
