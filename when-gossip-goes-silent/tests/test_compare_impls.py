"""T078: compare_impls.py on synthetic result directories (stdlib unittest)."""
import json
import os
import random
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
import compare_impls  # noqa: E402


def write_cell(d, strategy, srs, p90s, profile='baseline-flaky', seeds=None):
    os.makedirs(d, exist_ok=True)
    for i, (sr, p90) in enumerate(zip(srs, p90s)):
        seed = (seeds or range(1000, 1000 * (len(srs) + 1), 1000))[i]
        r = {'config': {'topology': 'ring-50', 'strategy': strategy, 'faultBase': 'no-wan', 'faultProfile': profile,
                        'faultPartition': None, 'seed': seed},
             'summary': {'successRate': sr, 'p90Ms': p90}}
        with open(os.path.join(d, f'ring-50_{strategy}_no-wan_{profile}_{seed}_{i}.json'), 'w') as fh:
            json.dump(r, fh)


def noisy(rng, center, sd, n=20):
    return [center + rng.gauss(0, sd) for _ in range(n)]


class CompareImpls(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.ref, self.rs = os.path.join(self.tmp, 'ref'), os.path.join(self.tmp, 'rs')
        self.rng = random.Random(1)

    def run_cmp(self):
        rows, flips = compare_impls.compare(compare_impls.load(self.ref), compare_impls.load(self.rs))
        return rows, flips

    def test_identical_distributions_are_equivalent(self):
        for s, c in [('none', 0.65), ('mas-agent', 0.80)]:
            srs, p90s = noisy(self.rng, c, 0.005), noisy(self.rng, 3000, 30)
            write_cell(self.ref, s, srs, p90s)
            write_cell(self.rs, s, srs, p90s)
        rows, flips = self.run_cmp()
        self.assertEqual(len(rows), 2)
        self.assertTrue(all(r['sr']['verdict'] == r['p90']['verdict'] == 'equivalent' for r in rows))
        self.assertEqual(flips, [])
        self.assertEqual(rows[0]['paired'], 20)

    def test_ten_pp_shift_is_different(self):
        write_cell(self.ref, 'none', noisy(self.rng, 0.65, 0.01), noisy(self.rng, 3000, 30))
        write_cell(self.rs, 'none', noisy(self.rng, 0.75, 0.01), noisy(self.rng, 3000, 30))
        rows, _ = self.run_cmp()
        self.assertEqual(rows[0]['sr']['verdict'], 'different')
        self.assertEqual(rows[0]['p90']['verdict'], 'equivalent')
        self.assertGreater(rows[0]['sr']['delta'], 0.9)

    def test_p90_plus_20_percent_is_not_equivalent(self):
        write_cell(self.ref, 'none', noisy(self.rng, 0.65, 0.005), noisy(self.rng, 3000, 30))
        write_cell(self.rs, 'none', noisy(self.rng, 0.65, 0.005), noisy(self.rng, 3600, 30))
        rows, _ = self.run_cmp()
        self.assertEqual(rows[0]['p90']['verdict'], 'different')

    def test_ranking_flip_is_detected(self):
        write_cell(self.ref, 'none', noisy(self.rng, 0.60, 0.005), noisy(self.rng, 3000, 30))
        write_cell(self.ref, 'mas-agent', noisy(self.rng, 0.70, 0.005), noisy(self.rng, 3000, 30))
        write_cell(self.rs, 'none', noisy(self.rng, 0.72, 0.005), noisy(self.rng, 3000, 30))
        write_cell(self.rs, 'mas-agent', noisy(self.rng, 0.70, 0.005), noisy(self.rng, 3000, 30))
        _, flips = self.run_cmp()
        self.assertEqual(len(flips), 1)
        self.assertEqual(flips[0][1], ['mas-agent', 'none'])

    def test_cli_exit_code_and_report(self):
        srs, p90s = noisy(self.rng, 0.65, 0.005), noisy(self.rng, 3000, 30)
        write_cell(self.ref, 'none', srs, p90s)
        write_cell(self.rs, 'none', srs, p90s)
        out = os.path.join(self.tmp, 'report.md')
        p = subprocess.run([sys.executable, os.path.join(os.path.dirname(HERE), 'compare_impls.py'), self.ref, self.rs, '--out', out],
                           capture_output=True, text=True)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        with open(out, encoding='utf-8') as fh:
            text = fh.read()
        self.assertIn('| ring-50/none/no-wan/baseline-flaky |', text)
        self.assertIn('1 cells compared, 0 with a different metric, 0 inconclusive, 0 ranking flips', text)

    def test_high_seed_variance_is_inconclusive_not_different(self):
        # historical-like spread (sd 0.12): identical generating process, n=20 vs 30
        write_cell(self.ref, 'none', noisy(self.rng, 0.65, 0.12), noisy(self.rng, 3000, 500))
        write_cell(self.rs, 'none', noisy(self.rng, 0.65, 0.12, 30), noisy(self.rng, 3000, 500, 30),
                   seeds=list(range(1000, 31000, 1000)))
        rows, _ = self.run_cmp()
        self.assertEqual(rows[0]['sr']['verdict'], 'inconclusive')


if __name__ == '__main__':
    unittest.main()
