#!/usr/bin/env bash
# US4 / T081 – LOCAL ONLY. SC-005: time until a ring-50 cluster is healthy and memory per
# service, JS image vs Rust image. Appends a section to specs/001-rust-rewrite/equivalence-report.md.
# Run on the HOST shell (compose bind mounts must resolve on the host).
set -euo pipefail
cd "$(dirname "$0")/../when-gossip-goes-silent"
REPORT=../specs/001-rust-rewrite/equivalence-report.md
{
  echo ""
  echo "## Performance (SC-005), $(date -u +%Y-%m-%dT%H:%MZ)"
  echo ""
  echo "| implementation | time to 51/51 healthy | mean RSS per service | max RSS |"
  echo "|---|---|---|---|"
} >> "$REPORT"
for impl in js rust; do
  (
    export IMPL=$impl
    source runner/impl.sh
    build_experiment_image > /dev/null
    project="perf-$impl"
    gen compose ring-50 baseline-flaky mas-agent 1000 no-wan "" "$project" > /dev/null
    f="docker-compose-local-$project.yml"
    services=$(docker compose -f "$f" -p "$project" config --services | grep -v '^coordinator$')
    t0=$(date +%s)
    docker compose -f "$f" -p "$project" up -d $services > /dev/null 2>&1
    until [ "$(docker compose -f "$f" -p "$project" ps --format '{{.Health}}' | grep -c '^healthy$')" -ge 51 ]; do sleep 2; done
    t=$(( $(date +%s) - t0 ))
    sleep 20  # let gossip and timers settle before sampling memory
    mem=$(docker stats --no-stream --format '{{.MemUsage}}' $(docker compose -f "$f" -p "$project" ps -q) \
      | awk '{v=$1; u=v; gsub(/[0-9.]/,"",u); gsub(/[A-Za-z]/,"",v); m=(u=="GiB")?v*1024:(u=="KiB")?v/1024:v; s+=m; if(m>x)x=m} END {printf "%.1f MiB | %.1f MiB", s/NR, x}')
    echo "| $impl | ${t}s | $mem |" >> "$REPORT"
    docker compose -f "$f" -p "$project" down -v > /dev/null 2>&1
    rm -f "$f"
  )
done
tail -n 4 "$REPORT"
