"""Container integration test for ring-5 (tasks T060/T061, run by tests/compose/ring5.yml).

MODE=rust     : every fault profile injected/recovered on real eth0 (tc, iptables), layer-scoped
                recovery, cascade-crash exit code, gateway /data across the ring.
MODE=mixed-a  : Rust gateway (mas-agent) with JS leaves svc-02/svc-04.
MODE=mixed-b  : JS gateway (mas-agent) with Rust leaves.
"""
import json
import os
import socket
import subprocess
import sys
import time
import urllib.request

MODE = os.environ.get("MODE", "rust")
PORT = {"gateway": 4000, **{f"svc-0{i}": 4000 + i for i in range(1, 6)}}
failures = []


def url(svc, path):
    return f"http://{svc}:{PORT[svc]}{path}"


def http(method, svc, path, body=None, timeout=30):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url(svc, path), data=data, method=method, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.status, json.loads(r.read())
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read())


def ctl(svc, path, body=None):
    """Fault control via loopback inside the container: eth0 faults (80% loss, partitions)
    would otherwise drop the control request/response itself."""
    args = ["exec", f"ring5-{svc}", "curl", "-s", "-m", "10", "-X", "POST", "-H", "Content-Type: application/json"]
    if body is not None:
        args += ["-d", json.dumps(body)]
    out = sh(*args, f"http://localhost:{PORT[svc]}{path}")
    return json.loads(out) if out.strip() else None


def sh(*args):
    return subprocess.run(["docker", *args], capture_output=True, text=True).stdout


def tc(svc):
    return sh("exec", f"ring5-{svc}", "tc", "qdisc", "show", "dev", "eth0")


def ipt(svc):
    return sh("exec", f"ring5-{svc}", "iptables", "-S")


def check(cond, msg):
    print(("  ok   " if cond else "  FAIL ") + msg)
    if not cond:
        failures.append(msg)


def wait_until(pred, timeout=15):
    end = time.time() + timeout
    while time.time() < end:
        if pred():
            return True
        time.sleep(0.5)
    return False


def data_ok(expect_degraded=()):
    status, body = http("GET", "gateway", "/data")
    res = body.get("downstreamResults") or []
    degraded = {r.get("service") for r in res if r.get("degraded")}
    return status, res, degraded


def faults_suite():
    print("== baseline: gateway reaches all 5 leaves")
    status, res, degraded = data_ok()
    check(status == 200 and len(res) == 5 and not degraded, f"/data 200 with 5 healthy results (got {status}, degraded={degraded})")

    print("== tc profiles on svc-01 (inject → tc qdisc show → recover by type)")
    profiles = [
        ("baseline-flaky", "loss 40%"),
        ("burst-failure", "loss 80%"),
        ("wan-baseline", "delay 50ms"),
    ]
    if MODE == "rust":
        profiles.append(("packet-corruption", "corrupt 5%"))
    for profile, needle in profiles:
        ctl("svc-01", f"/faults/{profile}", {})
        check(needle in tc("svc-01"), f"{profile}: '{needle}' installed")
        ctl("svc-01", f"/faults/recover/{profile}")
        check("netem" not in tc("svc-01"), f"{profile}: netem removed by /faults/recover/{profile}")

    print("== slow-degradation ramps the delay every second")
    ctl("svc-01", "/faults/slow-degradation", {})
    # quirk Q5 (same in JS): the initial `delay 0ms 10ms distribution normal` is rejected by tc,
    # so there is no qdisc until the first `tc qdisc replace` step one second later
    check("netem" not in tc("svc-01"), "Q5: initial zero-delay qdisc is rejected by tc")
    time.sleep(3.5)
    q = tc("svc-01")
    check("delay 250ms" in q or "delay 333ms" in q or "delay 167ms" in q, f"ramp advanced (tc: {q.strip()})")
    ctl("svc-01", "/faults/recover")
    time.sleep(1.5)
    check("netem" not in tc("svc-01"), "full recover stops the ramp and clears tc")

    print("== WAN baseline + partition overlay on gateway; partition-only recovery")
    ip1, ip3 = socket.gethostbyname("svc-01"), socket.gethostbyname("svc-03")
    ctl("gateway", "/faults/wan-baseline", {"delayMs": 50, "jitterMs": 10})
    ctl("gateway", "/faults/network-partition", {"peerIps": [ip1, ip3]})
    rules = ipt("gateway")
    check(f"-A INPUT -s {ip1}/32 -j DROP" in rules and f"-A OUTPUT -d {ip3}/32 -j DROP" in rules, "iptables DROP rules for both peers")
    _, _, degraded = data_ok()
    check({"svc-01", "svc-03"} <= degraded, f"partitioned leaves degraded in /data (degraded={degraded})")
    ctl("gateway", "/faults/recover/network-partition")
    check(ip1 not in ipt("gateway"), "partition rules removed")
    check("delay 50ms" in tc("gateway"), "WAN baseline layer survives partition-only recovery")
    ctl("gateway", "/faults/recover")
    check("netem" not in tc("gateway"), "full recover clears WAN layer")

    print("== invalid peer IP is rejected (no shell injection)")
    ctl("gateway", "/faults/network-partition", {"peerIps": ["1.2.3.4; touch /pwned"]})
    check("1.2.3.4" not in ipt("gateway"), "no rule for invalid peer")
    check(sh("exec", "ring5-gateway", "ls", "/pwned").strip() == "", "no command injection")

    print("== soft reset clears active faults")
    ctl("svc-02", "/faults/burst-failure", {})
    ctl("svc-02", "/experiment/reset", {"seed": 2000})
    check("netem" not in tc("svc-02"), "reset → tc cleared")

    cascade_crash_is_a_noop("svc-05")


def cascade_crash_is_a_noop(svc):
    # quirk Q4 (same in JS): the service is PID 1 in its container and the kernel ignores a
    # SIGKILL that the namespace init sends to itself – the process keeps serving.
    print(f"== cascade-crash on {svc}: PID 1 survives its own SIGKILL (quirk Q4)")
    r = ctl(svc, "/faults/cascade-crash", {})
    check(r == {"status": "injected", "profile": "cascade-crash"}, f"{svc} answers 'injected' (got {r})")
    time.sleep(1)
    running = sh("inspect", "-f", "{{.State.Running}}", f"ring5-{svc}").strip()
    check(running == "true", f"{svc} still running")
    status, _ = http("GET", svc, "/health")
    check(status == 200, f"{svc} still healthy")


def mixed_suite():
    print(f"== mixed cluster ({MODE})")
    for _ in range(3):
        status, res, degraded = data_ok()
    check(status == 200 and len(res) == 5 and not degraded, f"/data across implementations (status={status}, degraded={degraded})")
    leaf_impl = {r["service"]: r for r in res}
    check(all("latencyMs" in leaf_impl[s] for s in leaf_impl), "every leaf answered with the same JSON shape")

    def gossip_seen():
        _, m = http("GET", "gateway", "/metrics")
        return (m.get("strategyStats") or {}).get("neighborCount", 0) >= 2

    check(wait_until(gossip_seen, 20), "leaf gossip reaches the gateway across implementations")
    _, m = http("GET", "gateway", "/metrics")
    check(m["strategyStats"]["strategy"] == "mas-agent", "gateway runs mas-agent")
    for svc in ("svc-02", "svc-03"):  # one JS and one Rust leaf in mixed-a; both Rust in mixed-b
        cascade_crash_is_a_noop(svc)


if __name__ == "__main__":
    print(f"cluster_test MODE={MODE}")
    faults_suite() if MODE == "rust" else mixed_suite()
    print(f"\n{'PASS' if not failures else 'FAIL'} ({len(failures)} failures)")
    sys.exit(1 if failures else 0)
