"""T077: validate one cell produced by chaos-coord with real k6 (contracts/result-file.md),
then run both copies of analyze_results.py on it – they must exit 0."""
import glob
import json
import subprocess
import sys

d = sys.argv[1]
files = sorted(glob.glob(f"{d}/*.json"))
print(f"result files: {[f.split('/')[-1] for f in files]}")
assert len(files) == 1, files
r = json.load(open(files[0]))
assert list(r) == ["config", "experimentStartTime", "summary", "k6Metrics", "serviceMetrics", "timestamp"], list(r)
s = r["summary"]
assert s["totalRequests"] > 0 and s["successCount"] + s["failureCount"] == s["totalRequests"], s
assert set(r["serviceMetrics"]) == {"gateway", "svc-01", "svc-02", "svc-03", "svc-04", "svc-05"}
assert r["serviceMetrics"]["gateway"]["strategyStats"]["strategy"] == "mas-agent"
assert r["config"]["loadGenerator"] == "k6" and r["config"]["seed"] == 1000
print(f"summary: SR={s['successRate']:.3f} p90={s['p90Ms']}ms total={s['totalRequests']}")
for a in ["/check/runner_analyze_results.py", "/check/analyze_results.py"]:
    p = subprocess.run([sys.executable, a, d], capture_output=True, text=True)
    print(f"{a}: exit {p.returncode}, {len(p.stdout.splitlines())} lines")
    if p.returncode != 0:
        print(p.stdout[-2000:], p.stderr[-2000:])
        sys.exit(1)
print("PASS")
