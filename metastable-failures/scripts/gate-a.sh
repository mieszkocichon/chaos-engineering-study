#!/bin/sh
# Gate A confirmation: determinism + a real tipping threshold + seed robustness.
set -e
P="--baseline 350 --surge-dur 10000 --timeout 300 --attempts 30 --backoff 30 --qmax 2000"

echo "=== DETERMINISM (same seed x2, md5 must match) ==="
node src/run-m1.js $P --surge 2000 --seed 1 | grep -E 'post_over_baseline|metastable_stuck' | md5sum
node src/run-m1.js $P --surge 2000 --seed 1 | grep -E 'post_over_baseline|metastable_stuck' | md5sum

echo ""
echo "=== TIPPING THRESHOLD (sweep surge; expect recover -> stuck cliff) ==="
for s in 600 800 1000 1200 1500 2000; do
  out=$(node src/run-m1.js $P --surge $s --seed 1)
  stuck=$(echo "$out" | grep -oE '"metastable_stuck": (true|false)' | awk '{print $2}')
  ratio=$(echo "$out" | grep -oE '"post_over_baseline": [0-9.]+' | awk '{print $2}')
  printf "surge=%-5s  post/baseline=%-6s  stuck=%s\n" "$s" "$ratio" "$stuck"
done

echo ""
echo "=== SEED ROBUSTNESS at surge=2000 (expect stuck across seeds) ==="
for seed in 1 2 3 4 5; do
  out=$(node src/run-m1.js $P --surge 2000 --seed $seed)
  stuck=$(echo "$out" | grep -oE '"metastable_stuck": (true|false)' | awk '{print $2}')
  ratio=$(echo "$out" | grep -oE '"post_over_baseline": [0-9.]+' | awk '{print $2}')
  printf "seed=%-2s  post/baseline=%-6s  stuck=%s\n" "$seed" "$ratio" "$stuck"
done
