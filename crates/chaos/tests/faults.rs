//! T045: fault commands and injector bookkeeping (no NET_ADMIN needed – recording mode).

use chaos::faults::*;
use serde_json::json;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn cmd(line: &str) -> Vec<String> {
    line.split(' ').map(String::from).collect()
}

#[test]
fn commands_match_injector_js() {
    let none = json!({});
    assert_eq!(fault_commands("baseline-flaky", &none).unwrap(), vec![cmd("tc qdisc add dev eth0 root netem loss 40%")]);
    assert_eq!(fault_commands("burst-failure", &none).unwrap(), vec![cmd("tc qdisc add dev eth0 root netem loss 80%")]);
    assert_eq!(
        fault_commands("slow-degradation", &none).unwrap(),
        vec![cmd("tc qdisc add dev eth0 root netem delay 0ms 10ms distribution normal")]
    );
    assert_eq!(
        fault_commands("wan-baseline", &none).unwrap(),
        vec![cmd("tc qdisc add dev eth0 root netem delay 50ms 10ms distribution normal")]
    );
    assert_eq!(
        fault_commands("wan-baseline", &json!({"delayMs": 120, "jitterMs": 7.5})).unwrap(),
        vec![cmd("tc qdisc add dev eth0 root netem delay 120ms 7.5ms distribution normal")]
    );
    assert!(fault_commands("wan-baseline", &json!({"delayMs": -1})).is_err());
    assert!(fault_commands("wan-baseline", &json!({"jitterMs": -0.5})).is_err());
    // parameterization for slow-degradation
    assert_eq!(
        fault_commands("slow-degradation", &json!({"initialDelayMs": 10, "initialJitterMs": 2})).unwrap(),
        vec![cmd("tc qdisc add dev eth0 root netem delay 10ms 2ms distribution normal")]
    );
    assert!(fault_commands("slow-degradation", &json!({"initialDelayMs": -5})).is_err());
    let custom_ramp = find_fault_kind("slow-degradation")
        .unwrap()
        .ramp(&json!({"maxDelayMs": 1000, "durationMs": 10000, "stepMs": 2000}))
        .unwrap()
        .unwrap();
    assert_eq!(custom_ramp.interval_ms, 2000);
    assert_eq!(custom_ramp.steps.len(), 5);
    assert_eq!(custom_ramp.steps[0], cmd("tc qdisc replace dev eth0 root netem delay 200ms 20ms distribution normal"));
    assert_eq!(custom_ramp.steps[4], cmd("tc qdisc replace dev eth0 root netem delay 1000ms 100ms distribution normal"));
    // parameterization for loss and corruption
    assert_eq!(
        fault_commands("baseline-flaky", &json!({"lossPct": 25.5})).unwrap(),
        vec![cmd("tc qdisc add dev eth0 root netem loss 25.5%")]
    );
    assert_eq!(
        fault_commands("burst-failure", &json!({"lossPct": 90})).unwrap(),
        vec![cmd("tc qdisc add dev eth0 root netem loss 90%")]
    );
    assert!(fault_commands("baseline-flaky", &json!({"lossPct": -5})).is_err());
    assert!(fault_commands("burst-failure", &json!({"lossPct": 105})).is_err());
    assert_eq!(
        fault_commands("packet-corruption", &none).unwrap(),
        vec![cmd("tc qdisc add dev eth0 root netem corrupt 5%")]
    );
    assert_eq!(
        fault_commands("packet-corruption", &json!({"corruptPct": 12.5})).unwrap(),
        vec![cmd("tc qdisc add dev eth0 root netem corrupt 12.5%")]
    );
    assert!(fault_commands("packet-corruption", &json!({"corruptPct": -1})).is_err());
    assert!(fault_commands("packet-corruption", &json!({"corruptPct": 101})).is_err());
    assert_eq!(
        fault_commands("network-partition", &json!({"peerIps": ["10.0.0.3", "10.0.0.5"]})).unwrap(),
        vec![
            cmd("iptables -A INPUT -s 10.0.0.3 -j DROP"),
            cmd("iptables -A OUTPUT -d 10.0.0.3 -j DROP"),
            cmd("iptables -A INPUT -s 10.0.0.5 -j DROP"),
            cmd("iptables -A OUTPUT -d 10.0.0.5 -j DROP"),
        ]
    );
    assert!(fault_commands("network-partition", &none).unwrap().is_empty());
    assert!(fault_commands("network-partition", &json!({"peerIps": ["10.0.0.1; rm -rf /"]})).is_err());
    assert!(fault_commands("cascade-crash", &none).unwrap().is_empty());
    assert!(fault_commands("nope", &none).unwrap().is_empty());
}

#[test]
fn which_profiles_clear_tc_first() {
    for p in ["baseline-flaky", "burst-failure", "slow-degradation", "wan-baseline", "packet-corruption"] {
        assert!(clears_tc_first(p), "{p}");
    }
    for p in ["network-partition", "cascade-crash", "nope"] {
        assert!(!clears_tc_first(p), "{p}");
    }
}

#[test]
fn recover_scope_matches_js() {
    assert_eq!(recover_scope("network-partition"), RecoverScope::Partition);
    for t in ["wan-baseline", "baseline-flaky", "burst-failure", "slow-degradation", "packet-corruption"] {
        assert_eq!(recover_scope(t), RecoverScope::Tc);
    }
    assert_eq!(recover_scope("cascade-crash"), RecoverScope::All);
}

#[test]
fn slow_degradation_ramp() {
    assert_eq!(degradation_step(1), cmd("tc qdisc replace dev eth0 root netem delay 83ms 8ms distribution normal"));
    assert_eq!(degradation_step(6), cmd("tc qdisc replace dev eth0 root netem delay 500ms 50ms distribution normal"));
    assert_eq!(degradation_step(60), cmd("tc qdisc replace dev eth0 root netem delay 5000ms 500ms distribution normal"));
}

#[tokio::test(start_paused = true)]
async fn injector_sequences() {
    let (inj, log) = FaultInjector::recording();
    inj.inject("wan-baseline", &json!({})).await;
    inj.inject("network-partition", &json!({"peerIps": ["10.0.0.3"]})).await;
    // overlay recovery removes only the partition layer
    inj.recover_by_type("network-partition").await;
    inj.recover().await;
    let got = log.lock().unwrap().clone();
    assert_eq!(
        got,
        vec![
            s(&["tc", "qdisc", "del", "dev", "eth0", "root"]),
            cmd("tc qdisc add dev eth0 root netem delay 50ms 10ms distribution normal"),
            cmd("iptables -A INPUT -s 10.0.0.3 -j DROP"),
            cmd("iptables -A OUTPUT -d 10.0.0.3 -j DROP"),
            cmd("iptables -D INPUT -s 10.0.0.3 -j DROP"),
            cmd("iptables -D OUTPUT -d 10.0.0.3 -j DROP"),
            cmd("tc qdisc del dev eth0 root"),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn slow_degradation_steps_every_second_until_recovered() {
    let (inj, log) = FaultInjector::recording();
    inj.inject("slow-degradation", &json!({})).await;
    tokio::time::sleep(std::time::Duration::from_millis(3500)).await;
    inj.recover().await;
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    let got = log.lock().unwrap().clone();
    assert_eq!(got[1], cmd("tc qdisc add dev eth0 root netem delay 0ms 10ms distribution normal"));
    assert_eq!(got[2..5], [degradation_step(1), degradation_step(2), degradation_step(3)]);
    assert_eq!(got[5], cmd("tc qdisc del dev eth0 root")); // recover stops the ramp
    assert_eq!(got.len(), 6, "{got:?}");
}
