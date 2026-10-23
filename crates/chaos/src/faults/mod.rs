//! Infrastructure fault injection – port of `lib/infrastructure-fault-injector.js`.
//! Fault kinds are open plugins implementing the `FaultKind` trait.
//! Commands run as argv without a shell (research R11); peer IPs are validated first.

use std::net::IpAddr;
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;
use indexmap::IndexSet;
use serde_json::Value;
use tokio::task::JoinHandle;

pub mod cascade_crash;
pub mod network_partition;
pub mod packet_corruption;
pub mod packet_loss;
pub mod slow_degradation;
pub mod wan_baseline;

use cascade_crash::CascadeCrashFault;
use network_partition::NetworkPartitionFault;
use packet_corruption::PacketCorruptionFault;
use packet_loss::PacketLossFault;
use slow_degradation::SlowDegradationFault;
use wan_baseline::WanBaselineFault;

pub type Argv = Vec<String>;

pub fn argv(s: &str) -> Argv {
    s.split_whitespace().map(String::from).collect()
}

pub const TC_CLEAR: &str = "tc qdisc del dev eth0 root";

/// The OS or network subsystem layer affected by a fault.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FaultLayer {
    Tc,
    Iptables,
    Process,
}

impl FaultLayer {
    pub fn clears_tc_first(&self) -> bool {
        matches!(self, FaultLayer::Tc)
    }

    pub fn recover_scope(&self) -> RecoverScope {
        match self {
            FaultLayer::Tc => RecoverScope::Tc,
            FaultLayer::Iptables => RecoverScope::Partition,
            FaultLayer::Process => RecoverScope::All,
        }
    }
}

/// Description of repeated background steps for progressive faults.
pub struct FaultRamp {
    pub interval_ms: u64,
    pub steps: Vec<Argv>,
}

/// Plugin trait describing an infrastructure fault kind.
pub trait FaultKind: Send + Sync {
    /// Unique identifier of the fault profile / kind.
    fn name(&self) -> &'static str;

    /// Layer affected by this fault kind.
    fn layer(&self) -> FaultLayer;

    /// Commands to execute at injection time, validating any options.
    fn commands(&self, opts: &Value) -> Result<Vec<Argv>, String>;

    /// Progressive degradation ramp, if any.
    fn ramp(&self, _opts: &Value) -> Result<Option<FaultRamp>, String> {
        Ok(None)
    }

    /// Default coordinator recovery time in seconds if unspecified in schedule (e.g. 35.0 for cascade-crash).
    fn default_recover_after_sec(&self) -> Option<f64> {
        None
    }

    /// Whether coordinator should resolve `peerServices` via DNS before injection.
    fn requires_peer_resolution(&self) -> bool {
        false
    }

    /// Execute the fault injection against the provided injector.
    fn execute<'a>(&'a self, injector: &'a FaultInjector, opts: &'a Value) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let cmds = match self.commands(opts) {
                Ok(cmds) => cmds,
                Err(e) => {
                    eprintln!("[InfraFault] {}: {e}", self.name());
                    return;
                }
            };
            for cmd in &cmds {
                injector.run(cmd).await;
            }
            match self.ramp(opts) {
                Ok(Some(ramp)) => {
                    let me = injector.clone();
                    let handle = tokio::spawn(async move {
                        let mut tick = tokio::time::interval(std::time::Duration::from_millis(ramp.interval_ms));
                        tick.tick().await; // first tick fires immediately; JS setInterval waits one period
                        for cmd in ramp.steps {
                            tick.tick().await;
                            me.run(&cmd).await;
                        }
                    });
                    injector.set_degradation_handle(handle);
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("[InfraFault] {}: {e}", self.name());
                }
            }
        })
    }
}

// Static plugin instances
static BASELINE_FLAKY: PacketLossFault = PacketLossFault::BASELINE_FLAKY;
static BURST_FAILURE: PacketLossFault = PacketLossFault::BURST_FAILURE;
static SLOW_DEGRADATION: SlowDegradationFault = SlowDegradationFault;
static WAN_BASELINE: WanBaselineFault = WanBaselineFault;
static NETWORK_PARTITION: NetworkPartitionFault = NetworkPartitionFault;
static CASCADE_CRASH: CascadeCrashFault = CascadeCrashFault;
static PACKET_CORRUPTION: PacketCorruptionFault = PacketCorruptionFault;

/// Registry lookup for a fault kind plugin by name.
pub fn find_fault_kind(name: &str) -> Option<&'static dyn FaultKind> {
    match name {
        "baseline-flaky" => Some(&BASELINE_FLAKY),
        "burst-failure" => Some(&BURST_FAILURE),
        "slow-degradation" => Some(&SLOW_DEGRADATION),
        "wan-baseline" => Some(&WAN_BASELINE),
        "network-partition" => Some(&NETWORK_PARTITION),
        "cascade-crash" => Some(&CASCADE_CRASH),
        "packet-corruption" => Some(&PACKET_CORRUPTION),
        _ => None,
    }
}

/// Profiles that install a tc qdisc and therefore clear the previous one first.
pub fn clears_tc_first(profile: &str) -> bool {
    find_fault_kind(profile).map_or(false, |k| k.layer().clears_tc_first())
}

/// Which layer `/faults/recover/:type` clears.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum RecoverScope {
    Partition,
    Tc,
    All,
}

pub fn recover_scope(kind: &str) -> RecoverScope {
    find_fault_kind(kind).map(|k| k.layer().recover_scope()).unwrap_or(RecoverScope::All)
}

/// Commands a profile runs right after injection (excluding the tc clear and the
/// slow-degradation ramp). `cascade-crash` and unknown profiles return none.
pub fn fault_commands(profile: &str, opts: &Value) -> Result<Vec<Argv>, String> {
    match find_fault_kind(profile) {
        Some(k) => k.commands(opts),
        None => Ok(vec![]),
    }
}

pub fn peer_ips(opts: &Value) -> Result<Vec<IpAddr>, String> {
    let empty = vec![];
    opts.get("peerIps")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .map(|v| {
            let s = v.as_str().ok_or_else(|| format!("peerIps: not a string: {v}"))?;
            s.parse::<IpAddr>().map_err(|_| format!("peerIps: invalid IP address {s:?}"))
        })
        .collect()
}

pub fn parse_non_negative_number(opts: &Value, key: &str, default: f64) -> Result<f64, String> {
    match opts.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Number(n)) => {
            let val = n.as_f64().ok_or_else(|| format!("{key}: not a valid float: {n}"))?;
            if val < 0.0 {
                return Err(format!("{key} must be non-negative, got {val}"));
            }
            Ok(val)
        }
        Some(Value::String(s)) => {
            let val = s.parse::<f64>().map_err(|_| format!("{key}: invalid number string {s:?}"))?;
            if val < 0.0 {
                return Err(format!("{key} must be non-negative, got {val}"));
            }
            Ok(val)
        }
        Some(other) => Err(format!("{key}: expected number, got {other}")),
    }
}

pub fn parse_percentage(opts: &Value, key: &str, default: f64) -> Result<f64, String> {
    let val = parse_non_negative_number(opts, key, default)?;
    if val > 100.0 {
        return Err(format!("{key} must be <= 100, got {val}"));
    }
    Ok(val)
}

/// Command for slow-degradation step `k` (1-based): delay `round(5000/60·k)`, jitter 10%.
pub fn degradation_step(k: u32) -> Argv {
    SlowDegradationFault::step_for(k, 5000.0, 60)
}

#[derive(Clone, Default)]
pub struct FaultInjector {
    inner: Arc<Mutex<State>>,
    /// Test mode: record commands instead of executing them.
    recorded: Option<Arc<Mutex<Vec<Argv>>>>,
}

#[derive(Default)]
struct State {
    blocked_ips: IndexSet<IpAddr>,
    degradation: Option<JoinHandle<()>>,
}

impl FaultInjector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn recording() -> (Self, Arc<Mutex<Vec<Argv>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        (FaultInjector { recorded: Some(log.clone()), ..Default::default() }, log)
    }

    pub fn is_recording(&self) -> bool {
        self.recorded.is_some()
    }

    pub fn record_cmd(&self, cmd: Argv) {
        if let Some(log) = &self.recorded {
            log.lock().unwrap().push(cmd);
        }
    }

    pub fn add_blocked_ip(&self, ip: IpAddr) {
        self.inner.lock().unwrap().blocked_ips.insert(ip);
    }

    pub fn set_degradation_handle(&self, handle: JoinHandle<()>) {
        self.inner.lock().unwrap().degradation = Some(handle);
    }

    pub async fn run(&self, cmd: &Argv) {
        if let Some(log) = &self.recorded {
            log.lock().unwrap().push(cmd.clone());
            return;
        }
        let line = cmd.join(" ");
        match tokio::process::Command::new(&cmd[0]).args(&cmd[1..]).output().await {
            Ok(o) if o.status.success() => println!("[InfraFault] Executed: {line}"),
            Ok(o) => eprintln!(
                "[InfraFault] Error executing \"{line}\": Command failed: {line}\n{}",
                String::from_utf8_lossy(&o.stderr).trim_end()
            ),
            Err(e) => eprintln!("[InfraFault] Error executing \"{line}\": {e}"),
        }
    }

    pub async fn inject(&self, profile: &str, opts: &Value) {
        println!("[InfraFault] Injecting profile: {profile}");
        let Some(fault) = find_fault_kind(profile) else {
            println!("[InfraFault] Unknown profile '{profile}', doing nothing.");
            return;
        };
        if fault.layer().clears_tc_first() {
            self.clear_network_faults().await;
        }
        fault.execute(self, opts).await;
    }

    pub async fn recover(&self) {
        println!("[InfraFault] Recovering infrastructure...");
        self.clear_network_faults().await;
        self.clear_network_partition().await;
    }

    pub async fn recover_by_type(&self, kind: &str) {
        match recover_scope(kind) {
            RecoverScope::Partition => self.clear_network_partition().await,
            RecoverScope::Tc => self.clear_network_faults().await,
            RecoverScope::All => {
                eprintln!("[InfraFault] recoverByType: unknown type '{kind}', falling back to full recover");
                self.recover().await;
            }
        }
    }

    pub async fn clear_network_faults(&self) {
        if let Some(h) = self.inner.lock().unwrap().degradation.take() {
            h.abort();
        }
        self.run(&argv(TC_CLEAR)).await; // no qdisc installed → error is logged and ignored
    }

    pub async fn clear_network_partition(&self) {
        let ips: Vec<IpAddr> = self.inner.lock().unwrap().blocked_ips.drain(..).collect();
        for ip in ips {
            self.run(&argv(&format!("iptables -D INPUT -s {ip} -j DROP"))).await;
            self.run(&argv(&format!("iptables -D OUTPUT -d {ip} -j DROP"))).await;
        }
    }
}
