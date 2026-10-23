use metastable::js::js_number;
use serde_json::Value;

use crate::faults::{argv, parse_non_negative_number, Argv, FaultKind, FaultLayer};

pub struct WanBaselineFault;

impl FaultKind for WanBaselineFault {
    fn name(&self) -> &'static str {
        "wan-baseline"
    }

    fn layer(&self) -> FaultLayer {
        FaultLayer::Tc
    }

    fn commands(&self, opts: &Value) -> Result<Vec<Argv>, String> {
        let d = parse_non_negative_number(opts, "delayMs", 50.0)?;
        let j = parse_non_negative_number(opts, "jitterMs", 10.0)?;
        Ok(vec![argv(&format!(
            "tc qdisc add dev eth0 root netem delay {}ms {}ms distribution normal",
            js_number(d),
            js_number(j)
        ))])
    }
}
