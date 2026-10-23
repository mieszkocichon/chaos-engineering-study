# metastable-failures

Model-free **early warning** for metastable failures (congestive collapse) in
distributed systems. A retry-storm discrete-event simulator produces the failure,
and an online detector watches **Critical Slowing Down** (rising variance) to raise
an alarm *before* the system tips over.

See [`PLAN.md`](PLAN.md) for the roadmap, [`notes/`](notes/) for the four
contributions and prior art, and [`docs/adr/`](docs/adr/) for the decision log.

## Layout

```
../crates/metastable/   Rust implementation: sim.rs (retry-storm DES), ews.rs (CSD signals),
                        bins run-m1, run-m2, sweep-hysteresis (ADR-0024, ADR-0025)
scripts/    gate-a.sh, find-loop.sh, explore-cliff.sh
results/    dated evidence (m1-gate-a.md, m2-gate-b.md)
notes/      overview, contributions, prior-art, reusable-assets
docs/adr/   architecture decision records
```

The original Node implementation (`src/*.js`) is archived in
`../old-implementation/metastable-failures/src/` (ADR-0026); the Rust port prints byte-identical output.

## Run (Docker only)

No host toolchain required. From the repo root (needs the `rust-dev` image, see
`specs/001-rust-rewrite/quickstart.md`):

```sh
sh scripts/dev.sh cargo build --release -p metastable
sh scripts/dev.sh sh -c 'cd metastable-failures && sh scripts/gate-a.sh'
sh scripts/dev.sh cargo run -q --release -p metastable --bin run-m2
sh scripts/dev.sh cargo run -q --release -p metastable --bin sweep-hysteresis -- --attempts 3
```

`scripts/gate-a.sh` and `explore-cliff.sh` run `../target/release/run-m1` (override with `M1=...`),
`find-loop.sh` runs `../target/release/sweep-hysteresis` (`SWEEP=...`). The archived JS:
`M1="node ../old-implementation/metastable-failures/src/run-m1.js"` (inside `node:20-bookworm`).

## The two gates

The project is structured around two kill-gates that had to pass before any claim
was made (to not waste our time):

- **Gate A — the failure is real.** Metastability reproduces deterministically with
  measurable **hysteresis** (tip-in load > tip-out load), so it is a genuine
  bistable collapse, not transient overload. Run `scripts/gate-a.sh`; evidence in
  [`results/m1-gate-a.md`](results/m1-gate-a.md).
- **Gate B — the precursor exists.** On a gradual ramp, rolling variance rises and
  the alarm fires **before** collapse (lead time > 0), while a matched stable run
  stays flat (no false alarm). Run `src/run-m2.js`; evidence in
  [`results/m2-gate-b.md`](results/m2-gate-b.md).

Both gates passed. Next milestone (M3): abrupt-vs-gradual trigger sweep and ROC/AUC
over n ≥ 30 seeds. Should be fun.
