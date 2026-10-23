pub mod coordinator;
pub mod experiment;
pub mod faults;
pub mod gossip;
pub mod metrics;
pub mod prng;
pub mod resilience;
pub mod service;
pub mod strategy;
pub mod topology;

/// Wall clock in ms since the epoch (`Date.now()`); only the I/O layer reads it (research R8).
pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64
}
pub mod generate;
