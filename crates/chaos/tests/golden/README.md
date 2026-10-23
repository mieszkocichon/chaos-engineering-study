# Golden vectors

JSON files here are recorded from the JavaScript reference implementation and are the
ground truth for the Rust parity tests in `../*.rs`. The generators are in `gen/*.mjs`.

- **Reference code:** commit `ef78fe86575f210bdb6c153f4618785e1484ac23` (2026-06-29) – the last
  commit that changed the JS sources (`when-gossip-goes-silent/lib`, `agent`, `runner/coordinator.js`,
  generators, `metastable-failures/src`, `demos`). Later commits only touched shell scripts and docs.
- **Runtime:** `node:20-bookworm` (V8 `Math.log`/`Math.cos` = fdlibm, see ADR-0025), `seedrandom@3.0.5`.

The JS sources now live in `old-implementation/` (ADR-0026); the generators import them from there.
Regenerate from the repo root:

```sh
docker run --rm -v "${PWD}:/app" -w /app node:20-bookworm sh scripts/golden.sh
```

Commit regenerated files together with the test that consumes them.
