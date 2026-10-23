# old-implementation – archived JavaScript reference

The original Node.js implementation, archived on 2026-10-20 after the Rust port was accepted as
equivalent (ADR-0026, `specs/001-rust-rewrite/`). Nothing here is maintained; it is kept
runnable as a reference for comparisons and for regenerating golden test vectors.

The layout mirrors the original repository:

| path | what it was |
|---|---|
| `when-gossip-goes-silent/{lib,agent,runner/*.js,generate-*.js,scripts/}` | experiment service, strategies, coordinator, generators |
| `when-gossip-goes-silent/Dockerfile`, `package*.json` | the `chaos-experiment:latest` image |
| `when-gossip-goes-silent/config/`, `runner/k6-script.js` | **frozen snapshot** of the config and k6 script it ran with (the live copies stay in `../when-gossip-goes-silent/`) |
| `when-gossip-goes-silent/docker-compose-*.yml`, `k8s/` | compose/k8s files generated in the JS era |
| `metastable-failures/src/` | retry-storm DES + early-warning signals |
| `demos/` | circuit breaker / saga demos |
| `*.js`, `run-linux-*.sh`, `docker-compose-swarm.yml`, `config/` (root) | early repo-root experiments |

The Rust replacements: `crates/chaos` (`chaos-svc`, `chaos-coord`, `chaos-gen`),
`crates/metastable` (`run-m1`, `run-m2`, `sweep-hysteresis`), `crates/demos` (`demo`).

## Running it

```sh
# experiment matrices with the JS implementation (from when-gossip-goes-silent/)
IMPL=js bash runner/run-matrix-compose.sh ring-50
# the JS image alone
docker build -t chaos-experiment:latest old-implementation/when-gossip-goes-silent
# metastable gates with JS
cd metastable-failures && M1="node ../old-implementation/metastable-failures/src/run-m1.js" sh scripts/gate-a.sh
# golden vectors for the Rust tests
docker run --rm -v "${PWD}:/app" -w /app node:20-bookworm sh scripts/golden.sh
```

Not supported for `IMPL=js` any more: `gen mesh`, `gen ring-swarm`, `gen ring` (those generators
write next to themselves, i.e. inside this folder) – use `IMPL=rust`.

Known behaviour reproduced by the Rust port on purpose (research R10): inbound reports dropped by
the gossip receiver (Q1), `cascade-crash` never kills PID 1 (Q4), the first slow-degradation `tc`
command is rejected (Q5).
