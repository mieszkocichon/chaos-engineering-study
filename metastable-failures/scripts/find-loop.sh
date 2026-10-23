#!/bin/sh
# Sweep retry budget (maxAttempts) to find a regime with a FINITE, measurable
# hysteresis loop. Theory: collapsed-state amplification ~ maxAttempts, so
# lambda_down ~ capacity / maxAttempts. Small attempts => loop closes (recoverable);
# large attempts => collapsed state is absorbing (lambda_down -> 0).
set -e
# SWEEP: command to run. Default = Rust (../target/release/sweep-hysteresis, build: cargo build --release -p metastable);
# archived JS: SWEEP="node ../old-implementation/metastable-failures/src/sweep-hysteresis.js"
SWEEP=${SWEEP:-../target/release/sweep-hysteresis}
SW="--lmin 50 --lmax 800 --steps 16 --dwell 4000 --timeout 300 --backoff 30 --qmax 500 --seed 1"
echo "attempts | lambda_up | lambda_down | width | bistable"
for a in 2 3 5 8 12 20; do
  out=$($SWEEP $SW --attempts $a)
  up=$(echo "$out"   | grep -oE '"lambda_up": [0-9a-z]+'        | awk '{print $2}')
  dn=$(echo "$out"   | grep -oE '"lambda_down": [0-9a-z]+'      | awk '{print $2}')
  w=$(echo "$out"    | grep -oE '"hysteresis_width": [0-9a-z]+' | awk '{print $2}')
  b=$(echo "$out"    | grep -oE '"bistable": (true|false)'      | awk '{print $2}')
  printf "%-8s | %-9s | %-11s | %-5s | %s\n" "$a" "$up" "$dn" "$w" "$b"
done
