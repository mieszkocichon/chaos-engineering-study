# metastable-failures

Model-free **early warning** for metastable failures (congestive collapse) in
distributed systems. A retry-storm discrete-event simulator produces the failure,
and an online detector watches **Critical Slowing Down** (rising variance) to raise
an alarm *before* the system tips over.

See [`PLAN.md`](PLAN.md) for the roadmap, [`notes/`](notes/) for the four
contributions and prior art, and [`docs/adr/`](docs/adr/) for the decision log.

## Layout

```
src/        sim.js (retry-storm DES), ews.js (CSD signals),
            run-m1.js, run-m2.js, sweep-hysteresis.js
scripts/    gate-a.sh, find-loop.sh, explore-cliff.sh
results/    dated evidence (m1-gate-a.md, m2-gate-b.md)
notes/      overview, contributions, prior-art, reusable-assets
docs/adr/   architecture decision records
```

Pure Node, no dependencies (standard library only) — no `npm install` needed (thank god).

## Run (Docker only)

No host toolchain required. Everything runs inside `node:20-bookworm` so it just works everywhere.

```sh
# from the metastable-failures/ directory
docker run --rm -v "$PWD":/app -w /app node:20-bookworm sh scripts/gate-a.sh
docker run --rm -v "$PWD":/app -w /app node:20-bookworm node src/run-m2.js
docker run --rm -v "$PWD":/app -w /app node:20-bookworm node src/sweep-hysteresis.js
```

PowerShell: replace `"$PWD"` with `"${PWD}"` (windows stuff...).

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
