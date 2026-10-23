# Sourced by runner/*.sh (cwd = when-gossip-goes-silent): selects the implementation.
#   IMPL=rust (default) – Rust port (specs/001-rust-rewrite): chaos-gen inside the image
#                         chaos-experiment-rs:latest built from the repo-root Dockerfile;
#                         no Node needed on the host.
#   IMPL=js             – archived Node reference in ../old-implementation/ (ADR-0026): node
#                         generators on the host (they read their frozen config snapshot),
#                         image chaos-experiment:latest built from the archived Dockerfile.
IMPL="${IMPL:-rust}"
OLD=../old-implementation/when-gossip-goes-silent

if [ "$IMPL" = "rust" ]; then
  EXP_IMAGE="${EXP_IMAGE:-chaos-experiment-rs:latest}"
  build_experiment_image() { docker build -t "$EXP_IMAGE" -f ../Dockerfile ..; }
  gen() {
    # Git Bash on Windows: pass a Windows path and stop MSYS from rewriting "/w" and ":/w"
    local src; src=$(pwd -W 2>/dev/null || pwd)
    MSYS_NO_PATHCONV=1 docker run --rm -u "$(id -u):$(id -g)" -v "$src:/w" -w /w \
      -e IMAGE -e REGISTRY -e STATIC_PARAMS -e SWIM_FANOUT -e SPOF_KILL_AT_SEC \
      -e RESULTS_DIR_HOST -e RESULTS_DIR_CONTAINER \
      "$EXP_IMAGE" chaos-gen "$@"
  }
  # spof_meta FILE KILLED_SERVICE KILL_AT_SEC PHASE NOTE – add the spofTest block to a result
  spof_meta() {
    python3 - "$@" <<'PY'
import json, sys
f, killed, at, phase, note = sys.argv[1:6]
d = json.load(open(f, encoding='utf-8'))
meta = {'killedService': killed, 'killAtSec': int(at), 'permanent': True}
if phase:
    meta['phase'] = phase
meta['note'] = note
d['spofTest'] = meta
json.dump(d, open(f, 'w', encoding='utf-8'), indent=2, ensure_ascii=False)
print('[spof] Metadane SPOF dopisane do ' + f)
PY
  }
else
  EXP_IMAGE="${EXP_IMAGE:-chaos-experiment:latest}"
  build_experiment_image() { docker build -t "$EXP_IMAGE" "$OLD"; }
  gen() {
    local g=$1; shift
    case "$g" in
      compose) node "$OLD/generate-compose-topology.js" "$@" ;;
      swarm)   node "$OLD/generate-swarm-topology.js" "$@" ;;
      k8s)     node "$OLD/generate-k8s-manifests.js" "$@" ;;
      # these write next to the script (inside old-implementation/), not into the working tree
      mesh|ring-swarm|ring)
        echo "gen $g: not supported for the archived JS implementation – use IMPL=rust" >&2; return 1 ;;
      *) echo "gen: unknown generator $g" >&2; return 1 ;;
    esac
  }
  spof_meta() {
    node -e "
const fs = require('fs');
const [f, killed, at, phase, note] = process.argv.slice(1);
const d = JSON.parse(fs.readFileSync(f, 'utf8'));
d.spofTest = { killedService: killed, killAtSec: Number(at), permanent: true, ...(phase ? { phase } : {}), note };
fs.writeFileSync(f, JSON.stringify(d, null, 2));
console.log('[spof] Metadane SPOF dopisane do ' + f);
" "$@"
  }
fi
echo "[impl] IMPL=$IMPL image=$EXP_IMAGE"
