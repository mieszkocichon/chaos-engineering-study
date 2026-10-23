#!/usr/bin/env python3
"""
compare_impls.py — equivalence report: Rust port vs JS reference (specs/001-rust-rewrite, US4).

    python3 compare_impls.py <ref_dir> <rust_dir> [--out report.md]

Cells = (topology, strategy, faultBase, faultProfile, faultPartition). For successRate and
p90Ms per cell: means, difference (rust − ref), 95% bootstrap CI of the difference (10 000
resamples, fixed seed), Mann–Whitney U p (from analyze_results.py) and Cliff's δ
(ADR-0023). Per metric the verdict is
  equivalent   – the whole CI lies inside the margin (±7 pp successRate, ±15 % of the
                 reference p90 mean; SC-004 revised 2026-10-20),
  different    – the CI excludes 0 and |difference| exceeds the margin,
  inconclusive – neither: the sample is too small for the seed-to-seed variance to decide
                 (with the historical spread, SR sd ≈ 0.12, n=20/30 gives a CI of ≈ ±6.6 pp).
Strategy ranking (by mean successRate) is compared per (topology, base, profile, partition).
Exit code 1 if any metric is *different* or a ranking flips, else 0.
"""
import argparse
import glob
import json
import os
import random
import sys
from collections import defaultdict

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), 'runner'))
from analyze_results import mann_whitney_p  # noqa: E402  (shared, not copied)

# SC-004 as revised 2026-10-20 (labbook): margins on the scale of the seed-to-seed spread
SR_MARGIN = 0.07
P90_REL_MARGIN = 0.15
BOOT = 10_000


def load(d):
    cells = defaultdict(dict)  # cell -> seed -> (sr, p90)
    for f in glob.glob(os.path.join(d, '*.json')):
        with open(f, encoding='utf-8') as fh:
            r = json.load(fh)
        c, s = r.get('config', {}), r.get('summary', {})
        if 'successRate' not in s:
            continue
        key = (c.get('topology'), c.get('strategy'), c.get('faultBase'), c.get('faultProfile'), c.get('faultPartition') or '')
        cells[key][c.get('seed')] = (s['successRate'], s.get('p90Ms', 0))
    return cells


def mean(xs):
    return sum(xs) / len(xs) if xs else float('nan')


def cliffs_delta(a, b):
    if not a or not b:
        return float('nan')
    gt = sum(1 for x in a for y in b if x > y)
    lt = sum(1 for x in a for y in b if x < y)
    return (gt - lt) / (len(a) * len(b))


def boot_ci(a, b, rng):
    """95% percentile CI of mean(b) − mean(a)."""
    diffs = []
    for _ in range(BOOT):
        ra = [a[rng.randrange(len(a))] for _ in a]
        rb = [b[rng.randrange(len(b))] for _ in b]
        diffs.append(mean(rb) - mean(ra))
    diffs.sort()
    return diffs[int(0.025 * BOOT)], diffs[int(0.975 * BOOT) - 1]


def compare(ref, rs):
    rng = random.Random(20261019)
    rows = []
    for key in sorted(set(ref) & set(rs), key=lambda k: tuple(str(x) for x in k)):
        a, b = ref[key], rs[key]
        row = {'cell': key, 'n_ref': len(a), 'n_rs': len(b), 'paired': len(set(a) & set(b))}
        for i, metric in enumerate(['sr', 'p90']):
            xa, xb = [v[i] for v in a.values()], [v[i] for v in b.values()]
            lo, hi = boot_ci(xa, xb, rng)
            margin = SR_MARGIN if metric == 'sr' else P90_REL_MARGIN * abs(mean(xa))
            row[metric] = {
                'ref': mean(xa), 'rs': mean(xb), 'diff': mean(xb) - mean(xa), 'ci': (lo, hi),
                'p': mann_whitney_p(xa, xb), 'delta': cliffs_delta(xb, xa), 'margin': margin,
                'verdict': 'equivalent' if -margin <= lo and hi <= margin
                else 'different' if (lo > 0 or hi < 0) and abs(mean(xb) - mean(xa)) > margin
                else 'inconclusive',
            }
        rows.append(row)
    # ranking per scenario
    flips = []
    scen = defaultdict(list)
    for r in rows:
        t, s, base, prof, part = r['cell']
        scen[(t, base, prof, part)].append(r)
    for k, rs_rows in scen.items():
        if len(rs_rows) < 2:
            continue
        order_ref = [r['cell'][1] for r in sorted(rs_rows, key=lambda r: -r['sr']['ref'])]
        order_rs = [r['cell'][1] for r in sorted(rs_rows, key=lambda r: -r['sr']['rs'])]
        if order_ref != order_rs:
            flips.append((k, order_ref, order_rs))
    return rows, flips


def report(rows, flips, only_ref, only_rs):
    out = ['# Equivalence report: Rust vs JS reference', '',
           f'Margins: successRate ±{SR_MARGIN * 100:.0f} pp, p90 ±{P90_REL_MARGIN * 100:.0f}% of the reference mean '
           f'(95% bootstrap CI of the difference must lie inside). Mann–Whitney U, Cliff\'s δ (rust vs ref).', '',
           '| cell (topology/strategy/base/profile/partition) | n ref/rs (paired) | SR ref | SR rs | ΔSR [95% CI] | p | δ | p90 ref | p90 rs | Δp90 [95% CI] | p | δ | verdict |',
           '|---|---|---|---|---|---|---|---|---|---|---|---|---|']
    for r in rows:
        sr, p9 = r['sr'], r['p90']
        verdict = 'different' if 'different' in (sr['verdict'], p9['verdict']) else             'inconclusive' if 'inconclusive' in (sr['verdict'], p9['verdict']) else 'equivalent'
        out.append(
            f"| {'/'.join(str(x) for x in r['cell'] if x)} | {r['n_ref']}/{r['n_rs']} ({r['paired']}) "
            f"| {sr['ref']:.3f} | {sr['rs']:.3f} | {sr['diff']:+.3f} [{sr['ci'][0]:+.3f}, {sr['ci'][1]:+.3f}] | {sr['p']:.3f} | {sr['delta']:+.2f} "
            f"| {p9['ref']:.0f} | {p9['rs']:.0f} | {p9['diff']:+.0f} [{p9['ci'][0]:+.0f}, {p9['ci'][1]:+.0f}] | {p9['p']:.3f} | {p9['delta']:+.2f} "
            f"| {'**different**' if verdict == 'different' else verdict} (SR {sr['verdict']}, p90 {p9['verdict']}) |")
    out += ['', '## Strategy ranking (by mean successRate)', '']
    out += ['All rankings agree.'] if not flips else [
        f"- **flip** {'/'.join(str(x) for x in k if x)}: ref {' > '.join(a)} vs rust {' > '.join(b)}" for k, a, b in flips]
    if only_rs:
        out += ['', '## Rust cells without a reference', ''] + [f'- {"/".join(str(x) for x in c if x)}' for c in only_rs]
    if only_ref:
        out += ['', f'({len(only_ref)} reference cells have no counterpart and were skipped.)']
    def count(v):
        return sum(1 for r in rows if v in (r['sr']['verdict'], r['p90']['verdict']))

    n_diff, n_inc = count('different'), count('inconclusive')
    out += ['', f'**Summary:** {len(rows)} cells compared, {n_diff} with a different metric, '
                f'{n_inc} inconclusive, {len(flips)} ranking flips.', '']
    return '\n'.join(out), n_diff == 0 and not flips


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    ap.add_argument('ref_dir')
    ap.add_argument('rust_dir')
    ap.add_argument('--out')
    a = ap.parse_args()
    ref, rs = load(a.ref_dir), load(a.rust_dir)
    rows, flips = compare(ref, rs)
    text, ok = report(rows, flips, sorted(set(ref) - set(rs), key=str), sorted(set(rs) - set(ref), key=str))
    if a.out:
        with open(a.out, 'w', encoding='utf-8') as fh:
            fh.write(text)
    try:
        sys.stdout.reconfigure(encoding='utf-8', errors='replace')
    except AttributeError:
        pass
    print(text)
    sys.exit(0 if ok else 1)


if __name__ == '__main__':
    main()
