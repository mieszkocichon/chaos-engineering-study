//! Sliding-window request metrics, port of `lib/metrics-collector.js`.
//! 10 one-second buckets; every method takes `now_ms` (research R8).

use serde::Serialize;

#[derive(Clone, Default)]
struct Bucket {
    count: u64,
    errors: u64,
    latencies: Vec<f64>,
    timestamp: i64, // floor(now_ms / bucket_ms); 0 = never used
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub error_rate: f64,
    pub avg_latency: f64,
    pub p50: f64,
    pub p90: f64,
    pub p99: f64,
    pub request_count: u64,
    pub queue_depth: u64,
    pub window_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthSummary {
    pub error_rate: f64,
    pub avg_latency: f64,
    pub load: u64,
}

pub struct MetricsCollector {
    window_size_ms: u64,
    bucket_ms: u64,
    buckets: Vec<Bucket>,
    in_flight: u64,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new(10_000, 1_000)
    }
}

impl MetricsCollector {
    pub fn new(window_size_ms: u64, bucket_ms: u64) -> Self {
        let n = window_size_ms.div_ceil(bucket_ms) as usize;
        MetricsCollector { window_size_ms, bucket_ms, buckets: vec![Bucket::default(); n], in_flight: 0 }
    }

    /// Soft reset between seeds.
    pub fn reset(&mut self) {
        self.in_flight = 0;
        let n = self.buckets.len();
        self.buckets = vec![Bucket::default(); n];
    }

    pub fn start_request(&mut self) {
        self.in_flight += 1;
    }

    pub fn end_request(&mut self) {
        self.in_flight = self.in_flight.saturating_sub(1);
    }

    pub fn record_request(&mut self, now_ms: u64, latency_ms: f64, success: bool) {
        self.end_request();
        let now = (now_ms / self.bucket_ms) as i64;
        let idx = (now as usize) % self.buckets.len();
        let b = &mut self.buckets[idx];
        if b.timestamp != now {
            *b = Bucket { timestamp: now, ..Bucket::default() };
        }
        b.count += 1;
        if !success {
            b.errors += 1;
        }
        b.latencies.push(latency_ms);
    }

    pub fn snapshot(&self, now_ms: u64) -> Snapshot {
        let now = (now_ms / self.bucket_ms) as i64;
        let n = self.buckets.len() as i64;
        let active = self.buckets.iter().filter(|b| b.timestamp > 0 && now - b.timestamp < n);
        let (mut count, mut errors, mut lat) = (0u64, 0u64, Vec::new());
        let mut any = false;
        for b in active {
            any = true;
            count += b.count;
            errors += b.errors;
            lat.extend_from_slice(&b.latencies);
        }
        if !any {
            return Snapshot {
                error_rate: 0.0, avg_latency: 0.0, p50: 0.0, p90: 0.0, p99: 0.0,
                request_count: 0, queue_depth: self.in_flight, window_ms: self.window_size_ms,
            };
        }
        lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let error_rate = if count > 0 { errors as f64 / count as f64 } else { 0.0 };
        let avg = if lat.is_empty() { 0.0 } else { lat.iter().fold(0.0, |s, v| s + v) / lat.len() as f64 };
        Snapshot {
            error_rate,
            avg_latency: metastable::js::js_round(avg),
            p50: percentile(&lat, 0.5),
            p90: percentile(&lat, 0.9),
            p99: percentile(&lat, 0.99),
            request_count: count,
            queue_depth: self.in_flight,
            window_ms: self.window_size_ms,
        }
    }

    pub fn health_summary(&self, now_ms: u64) -> HealthSummary {
        let s = self.snapshot(now_ms);
        HealthSummary { error_rate: s.error_rate, avg_latency: s.avg_latency, load: s.queue_depth }
    }
}

/// `sorted[max(0, ceil(n·p) − 1)]`.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (sorted.len() as f64 * p).ceil() as i64 - 1;
    sorted[idx.max(0) as usize]
}
