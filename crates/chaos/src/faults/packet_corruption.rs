use metastable::js::js_number;
use serde_json::Value;

use crate::faults::{argv, parse_percentage, Argv, FaultKind, FaultLayer};

pub struct PacketCorruptionFault;

impl FaultKind for PacketCorruptionFault {
    fn name(&self) -> &'static str {
        "packet-corruption"
    }

    fn layer(&self) -> FaultLayer {
        FaultLayer::Tc
    }

    fn commands(&self, opts: &Value) -> Result<Vec<Argv>, String> {
        let pct = parse_percentage(opts, "corruptPct", 5.0)?;
        Ok(vec![argv(&format!("tc qdisc add dev eth0 root netem corrupt {}%", js_number(pct)))])
    }
}
