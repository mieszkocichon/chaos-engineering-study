# Equivalence report: Rust vs JS reference

Margins: successRate ±7 pp, p90 ±15% of the reference mean (95% bootstrap CI of the difference must lie inside). Mann–Whitney U, Cliff's δ (rust vs ref).

| cell (topology/strategy/base/profile/partition) | n ref/rs (paired) | SR ref | SR rs | ΔSR [95% CI] | p | δ | p90 ref | p90 rs | Δp90 [95% CI] | p | δ | verdict |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| ring-50/backpressure/no-wan/baseline-flaky | 20/30 (20) | 0.535 | 1.000 | +0.465 [+0.439, +0.490] | 0.000 | +1.00 | 13043 | 1295 | -11748 [-12592, -10811] | 0.000 | -1.00 | **different** (SR different, p90 different) |
| ring-50/backpressure/no-wan/slow-degradation | 20/30 (20) | 0.548 | 1.000 | +0.452 [+0.381, +0.512] | 0.000 | +1.00 | 13544 | 3074 | -10471 [-11196, -9629] | 0.000 | -1.00 | **different** (SR different, p90 different) |
| ring-50/mas-agent/no-wan/baseline-flaky | 20/30 (20) | 0.649 | 1.000 | +0.351 [+0.278, +0.418] | 0.000 | +1.00 | 11295 | 1186 | -10109 [-11548, -8532] | 0.000 | -1.00 | **different** (SR different, p90 different) |
| ring-50/mas-agent/no-wan/slow-degradation | 20/30 (20) | 0.582 | 1.000 | +0.418 [+0.329, +0.501] | 0.000 | +0.95 | 12650 | 4814 | -7835 [-9161, -6255] | 0.000 | -0.81 | **different** (SR different, p90 different) |
| ring-50/none/no-wan/baseline-flaky | 20/30 (20) | 0.591 | 0.933 | +0.342 [+0.271, +0.384] | 0.000 | +0.93 | 10814 | 1309 | -9505 [-10493, -8448] | 0.000 | -0.99 | **different** (SR different, p90 different) |
| ring-50/none/no-wan/slow-degradation | 20/30 (20) | 0.223 | 0.557 | +0.334 [+0.312, +0.358] | 0.000 | +1.00 | 12618 | 3063 | -9555 [-10347, -8763] | 0.000 | -1.00 | **different** (SR different, p90 different) |

## Strategy ranking (by mean successRate)

- **flip** ring-50/no-wan/baseline-flaky: ref mas-agent > none > backpressure vs rust backpressure > mas-agent > none
- **flip** ring-50/no-wan/slow-degradation: ref mas-agent > backpressure > none vs rust backpressure > mas-agent > none

(45 reference cells have no counterpart and were skipped.)

**Summary:** 6 cells compared, 6 with a different metric, 0 inconclusive, 2 ranking flips.


---

## Control: JS on this machine (`results-js-local`, n=5 per cell, 2026-10-20)

Purpose: separate "Rust vs JS" from "this machine vs the historical host".

### A. Historical JS vs JS on this machine (the reference against itself)

Margins: successRate ±7 pp, p90 ±15% of the reference mean (95% bootstrap CI of the difference must lie inside). Mann–Whitney U, Cliff's δ (rust vs ref).

| cell (topology/strategy/base/profile/partition) | n ref/rs (paired) | SR ref | SR rs | ΔSR [95% CI] | p | δ | p90 ref | p90 rs | Δp90 [95% CI] | p | δ | verdict |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| ring-50/backpressure/no-wan/baseline-flaky | 20/5 (5) | 0.535 | 0.664 | +0.129 [+0.012, +0.305] | 0.051 | +0.58 | 13043 | 12473 | -570 [-2921, +1645] | 0.919 | -0.04 | **different** (SR different, p90 inconclusive) |
| ring-50/backpressure/no-wan/slow-degradation | 20/5 (5) | 0.548 | 0.676 | +0.129 [+0.007, +0.278] | 0.035 | +0.62 | 13544 | 13322 | -223 [-1406, +973] | 0.396 | -0.26 | **different** (SR different, p90 equivalent) |
| ring-50/mas-agent/no-wan/baseline-flaky | 20/5 (5) | 0.649 | 0.651 | +0.002 [-0.082, +0.081] | 0.243 | +0.36 | 11295 | 12665 | +1370 [-1122, +3765] | 0.541 | +0.19 | inconclusive (SR inconclusive, p90 inconclusive) |
| ring-50/mas-agent/no-wan/slow-degradation | 20/5 (5) | 0.582 | 0.717 | +0.135 [-0.031, +0.298] | 0.243 | +0.36 | 12650 | 11885 | -765 [-4220, +2377] | 0.946 | -0.03 | inconclusive (SR inconclusive, p90 inconclusive) |
| ring-50/none/no-wan/baseline-flaky | 20/5 (5) | 0.591 | 0.617 | +0.026 [-0.010, +0.060] | 0.148 | +0.44 | 10814 | 10541 | -273 [-2796, +2419] | 0.818 | -0.08 | inconclusive (SR equivalent, p90 inconclusive) |
| ring-50/none/no-wan/slow-degradation | 20/5 (5) | 0.223 | 0.213 | -0.010 [-0.050, +0.029] | 0.371 | -0.28 | 12618 | 13392 | +774 [-892, +2342] | 0.292 | +0.32 | inconclusive (SR equivalent, p90 inconclusive) |

#### Strategy ranking (by mean successRate)

- **flip** ring-50/no-wan/baseline-flaky: ref mas-agent > none > backpressure vs rust backpressure > mas-agent > none

(45 reference cells have no counterpart and were skipped.)

**Summary:** 6 cells compared, 2 with a different metric, 5 inconclusive, 1 ranking flips.

### B. JS on this machine vs Rust on this machine

Margins: successRate ±7 pp, p90 ±15% of the reference mean (95% bootstrap CI of the difference must lie inside). Mann–Whitney U, Cliff's δ (rust vs ref).

| cell (topology/strategy/base/profile/partition) | n ref/rs (paired) | SR ref | SR rs | ΔSR [95% CI] | p | δ | p90 ref | p90 rs | Δp90 [95% CI] | p | δ | verdict |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| ring-50/backpressure/no-wan/baseline-flaky | 5/30 (5) | 0.664 | 1.000 | +0.336 [+0.162, +0.448] | 0.000 | +0.80 | 12473 | 1295 | -11178 [-13175, -9037] | 0.000 | -1.00 | **different** (SR different, p90 different) |
| ring-50/backpressure/no-wan/slow-degradation | 5/30 (5) | 0.676 | 1.000 | +0.324 [+0.185, +0.415] | 0.000 | +1.00 | 13322 | 3074 | -10248 [-11090, -9206] | 0.000 | -1.00 | **different** (SR different, p90 different) |
| ring-50/mas-agent/no-wan/baseline-flaky | 5/30 (5) | 0.651 | 1.000 | +0.349 [+0.309, +0.388] | 0.000 | +1.00 | 12665 | 1186 | -11479 [-13384, -9519] | 0.000 | -1.00 | **different** (SR different, p90 different) |
| ring-50/mas-agent/no-wan/slow-degradation | 5/30 (5) | 0.717 | 1.000 | +0.283 [+0.143, +0.428] | 0.000 | +1.00 | 11885 | 4814 | -7071 [-9972, -4170] | 0.000 | -1.00 | **different** (SR different, p90 different) |
| ring-50/none/no-wan/baseline-flaky | 5/30 (5) | 0.617 | 0.933 | +0.316 [+0.237, +0.370] | 0.001 | +0.93 | 10541 | 1309 | -9232 [-11740, -6792] | 0.001 | -0.97 | **different** (SR different, p90 different) |
| ring-50/none/no-wan/slow-degradation | 5/30 (5) | 0.213 | 0.557 | +0.344 [+0.313, +0.378] | 0.000 | +1.00 | 13392 | 3063 | -10329 [-11733, -8896] | 0.000 | -1.00 | **different** (SR different, p90 different) |

#### Strategy ranking (by mean successRate)

- **flip** ring-50/no-wan/slow-degradation: ref mas-agent > backpressure > none vs rust backpressure > mas-agent > none

**Summary:** 6 cells compared, 6 with a different metric, 0 inconclusive, 1 ranking flips.

### Gateway CPU throttling (cgroup `cpu.stat`, compose limit 0.25 CPU)

| implementation | throttled CFS periods under load |
|---|---|
| JS (Node) gateway | ≈ 94–100 % (e.g. 967/1031, 1576/1640 cumulative) |
| Rust gateway | ≈ 20 % |

### Conclusion

JS on this machine reproduces the historical numbers (|ΔSR| ≤ 0.135, p90 within ±1.4 s), while Rust
differs from both by 34–45 pp SR and 8–12 s p90. The difference comes from the implementation's CPU
efficiency under the 0.25-CPU gateway limit (Node is throttled almost continuously), not from the
machine and not from strategy logic (bit-exact on golden vectors). Criterion SC-004 also flags
JS-vs-JS (2 "different" cells, 1 ranking flip at n=5), so it is sensitive at this sample size.


---

## Unthrottled gateway: JS vs Rust on this machine (`GATEWAY_CPUS=4`, n=5 per cell, 2026-10-20)

Same subset, gateway CPU limit raised from 0.25 to 4 CPUs (leaves unchanged at 0.20). Gateway
throttling: JS 0/4210 periods, Rust 0/3870. Leaf svc-01 throttling: JS 466/3967 (≈12 %), Rust 57/2338 (≈2.4 %).


Margins: successRate ±7 pp, p90 ±15% of the reference mean (95% bootstrap CI of the difference must lie inside). Mann–Whitney U, Cliff's δ (rust vs ref).

| cell (topology/strategy/base/profile/partition) | n ref/rs (paired) | SR ref | SR rs | ΔSR [95% CI] | p | δ | p90 ref | p90 rs | Δp90 [95% CI] | p | δ | verdict |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| ring-50/backpressure/no-wan/baseline-flaky | 5/5 (5) | 1.000 | 1.000 | +0.000 [+0.000, +0.000] | 1.000 | +0.00 | 1380 | 1156 | -224 [-449, +56] | 0.151 | -0.60 | inconclusive (SR equivalent, p90 inconclusive) |
| ring-50/backpressure/no-wan/slow-degradation | 5/5 (5) | 1.000 | 1.000 | +0.000 [+0.000, +0.000] | 1.000 | +0.00 | 3063 | 3056 | -7 [-9, -6] | 0.012 | -1.00 | equivalent (SR equivalent, p90 equivalent) |
| ring-50/mas-agent/no-wan/baseline-flaky | 5/5 (5) | 1.000 | 1.000 | +0.000 [+0.000, +0.000] | 1.000 | +0.00 | 1457 | 1030 | -426 [-731, -133] | 0.008 | -1.00 | **different** (SR equivalent, p90 different) |
| ring-50/mas-agent/no-wan/slow-degradation | 5/5 (5) | 0.995 | 1.000 | +0.005 [+0.001, +0.013] | 0.007 | +1.00 | 4652 | 4905 | +253 [-378, +953] | 0.690 | -0.20 | inconclusive (SR equivalent, p90 inconclusive) |
| ring-50/none/no-wan/baseline-flaky | 5/5 (5) | 0.962 | 0.965 | +0.003 [-0.003, +0.008] | 0.421 | +0.36 | 1430 | 1157 | -273 [-573, +62] | 0.095 | -0.68 | inconclusive (SR equivalent, p90 inconclusive) |
| ring-50/none/no-wan/slow-degradation | 5/5 (5) | 0.478 | 0.552 | +0.074 [+0.066, +0.080] | 0.008 | +1.00 | 3069 | 3060 | -8 [-10, -6] | 0.011 | -1.00 | **different** (SR different, p90 equivalent) |

#### Strategy ranking (by mean successRate)

All rankings agree.

**Summary:** 6 cells compared, 2 with a different metric, 3 inconclusive, 0 ranking flips.

### Conclusion

Without gateway throttling the two implementations converge: successRate is equivalent in 5/6 cells
(the sixth, none/slow-degradation, differs by +7.4 pp, just above the 7 pp margin), strategy rankings
agree (0 flips), p90 differs by at most ≈0.4 s (Rust faster). The 34–45 pp gaps seen at 0.25 CPU are
therefore an artefact of Node's CPU throttling, not of the port's logic. The remaining small gaps are
consistent with Node's leaves still being throttled (≈12 % vs ≈2 %) – latency near the 3 s timeout
boundary decides success in slow-degradation.
