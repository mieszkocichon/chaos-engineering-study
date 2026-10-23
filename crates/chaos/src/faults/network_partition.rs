use futures::future::BoxFuture;
use serde_json::Value;

use crate::faults::{argv, peer_ips, Argv, FaultInjector, FaultKind, FaultLayer};

pub struct NetworkPartitionFault;

impl FaultKind for NetworkPartitionFault {
    fn name(&self) -> &'static str {
        "network-partition"
    }

    fn layer(&self) -> FaultLayer {
        FaultLayer::Iptables
    }

    fn requires_peer_resolution(&self) -> bool {
        true
    }

    fn commands(&self, opts: &Value) -> Result<Vec<Argv>, String> {
        let ips = peer_ips(opts)?;
        Ok(ips
            .iter()
            .flat_map(|ip| [argv(&format!("iptables -A INPUT -s {ip} -j DROP")), argv(&format!("iptables -A OUTPUT -d {ip} -j DROP"))])
            .collect())
    }

    fn execute<'a>(&'a self, injector: &'a FaultInjector, opts: &'a Value) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            let ips = match peer_ips(opts) {
                Ok(ips) => ips,
                Err(e) => {
                    eprintln!("[InfraFault] network-partition: {e}");
                    return;
                }
            };
            if ips.is_empty() {
                eprintln!("[InfraFault] network-partition: no peerIps provided, no-op");
                return;
            }
            // same argv as `commands()` (INPUT then OUTPUT per peer), then remember the peer
            let cmds = self.commands(opts).unwrap_or_default();
            for (ip, pair) in ips.iter().zip(cmds.chunks(2)) {
                for cmd in pair {
                    injector.run(cmd).await;
                }
                injector.add_blocked_ip(*ip);
            }
            let list: Vec<String> = ips.iter().map(ToString::to_string).collect();
            println!("[InfraFault] Partitioned from {} peers: {}", ips.len(), list.join(", "));
        })
    }
}
