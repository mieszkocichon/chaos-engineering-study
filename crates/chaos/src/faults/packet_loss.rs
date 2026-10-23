use metastable::js::js_number;
use serde_json::Value;

use crate::faults::{argv, parse_percentage, Argv, FaultKind, FaultLayer};

pub struct PacketLossFault {
    name: &'static str,
    default_loss_pct: f64,
}

impl PacketLossFault {
    pub const BASELINE_FLAKY: Self = Self {
        name: "baseline-flaky",
        default_loss_pct: 40.0,
    };

    pub const BURST_FAILURE: Self = Self {
        name: "burst-failure",
        default_loss_pct: 80.0,
    };
}

impl FaultKind for PacketLossFault {
    fn name(&self) -> &'static str {
        self.name
    }

    fn layer(&self) -> FaultLayer {
        FaultLayer::Tc
    }

    fn commands(&self, opts: &Value) -> Result<Vec<Argv>, String> {
        let pct = parse_percentage(opts, "lossPct", self.default_loss_pct)?;
        Ok(vec![argv(&format!("tc qdisc add dev eth0 root netem loss {}%", js_number(pct)))])
    }
}
