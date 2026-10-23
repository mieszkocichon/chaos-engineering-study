use metastable::js::{js_number, js_round};
use serde_json::Value;

use crate::faults::{argv, parse_non_negative_number, Argv, FaultKind, FaultLayer, FaultRamp};

pub struct SlowDegradationFault;

impl SlowDegradationFault {
    pub fn step_for(k: u32, max_delay_ms: f64, total_steps: u32) -> Argv {
        let per_step = max_delay_ms / total_steps as f64;
        let d = max_delay_ms.min(js_round(per_step * k as f64));
        let j = js_round(d * 0.1);
        argv(&format!(
            "tc qdisc replace dev eth0 root netem delay {}ms {}ms distribution normal",
            js_number(d),
            js_number(j)
        ))
    }
}

impl FaultKind for SlowDegradationFault {
    fn name(&self) -> &'static str {
        "slow-degradation"
    }

    fn layer(&self) -> FaultLayer {
        FaultLayer::Tc
    }

    fn commands(&self, opts: &Value) -> Result<Vec<Argv>, String> {
        let initial_delay = parse_non_negative_number(opts, "initialDelayMs", 0.0)?;
        let initial_jitter = parse_non_negative_number(opts, "initialJitterMs", 10.0)?;
        Ok(vec![argv(&format!(
            "tc qdisc add dev eth0 root netem delay {}ms {}ms distribution normal",
            js_number(initial_delay),
            js_number(initial_jitter)
        ))])
    }

    fn ramp(&self, opts: &Value) -> Result<Option<FaultRamp>, String> {
        let max_delay = parse_non_negative_number(opts, "maxDelayMs", 5000.0)?;
        let duration = parse_non_negative_number(opts, "durationMs", 60000.0)?;
        let step_ms = parse_non_negative_number(opts, "stepMs", 1000.0)? as u64;
        let step_ms = step_ms.max(1);
        let total_steps = ((duration / step_ms as f64).round() as u32).max(1);

        let steps = (1..=total_steps)
            .map(|k| Self::step_for(k, max_delay, total_steps))
            .collect();

        Ok(Some(FaultRamp {
            interval_ms: step_ms,
            steps,
        }))
    }
}
