//! Circuit breaker, retry policy, concurrency limiter and parameter bounds – ports of
//! `lib/circuit-breaker.js`, `lib/retry.js`, `lib/concurrency-limiter.js` and the `BOUNDS`
//! tables of `mas-agent.js` / `central.js`. Error strings are byte-identical (research R9).

use std::future::Future;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

// ---------------------------------------------------------------- circuit breaker

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CbState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transition {
    pub from: CbState,
    pub to: CbState,
    pub timestamp: u64,
    pub failure_count: u64,
}

#[derive(Clone, Debug)]
pub struct CircuitBreaker {
    pub failure_threshold: f64,
    pub open_timeout_ms: f64,
    pub name: String,
    pub state: CbState,
    pub failure_count: u64,
    pub success_count: u64,
    pub last_failure_time: Option<u64>,
    pub last_state_change: u64,
    pub state_history: Vec<Transition>,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: f64, open_timeout_ms: f64, name: impl Into<String>, now_ms: u64) -> Self {
        CircuitBreaker {
            failure_threshold,
            open_timeout_ms,
            name: name.into(),
            state: CbState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure_time: None,
            last_state_change: now_ms,
            state_history: Vec::new(),
        }
    }

    /// Gate at the start of `call()`: open → half-open after the timeout, else reject.
    pub fn before_call(&mut self, now_ms: u64) -> Result<(), String> {
        if self.state == CbState::Open {
            if (now_ms as i64 - self.last_state_change as i64) as f64 >= self.open_timeout_ms {
                self.transition(CbState::HalfOpen, now_ms);
            } else {
                return Err(format!("CircuitBreakerOpen:{}", self.name));
            }
        }
        Ok(())
    }

    pub fn on_success(&mut self, now_ms: u64) {
        self.failure_count = 0;
        self.success_count += 1;
        if self.state == CbState::HalfOpen {
            self.transition(CbState::Closed, now_ms);
        }
    }

    pub fn on_failure(&mut self, now_ms: u64) {
        self.failure_count += 1;
        self.last_failure_time = Some(now_ms);
        if self.state == CbState::HalfOpen || self.failure_count as f64 >= self.failure_threshold {
            self.transition(CbState::Open, now_ms);
        }
    }

    fn transition(&mut self, to: CbState, now_ms: u64) {
        if self.state != to {
            self.state_history.push(Transition { from: self.state, to, timestamp: now_ms, failure_count: self.failure_count });
            self.state = to;
            self.last_state_change = now_ms;
            if to == CbState::Closed {
                self.failure_count = 0;
            }
        }
    }

    pub fn update_params(&mut self, failure_threshold: Option<f64>, open_timeout_ms: Option<f64>) {
        if let Some(v) = failure_threshold {
            self.failure_threshold = v;
        }
        if let Some(v) = open_timeout_ms {
            self.open_timeout_ms = v;
        }
    }

    pub fn stats(&self) -> Value {
        json!({
            "state": self.state,
            "failureCount": self.failure_count,
            "successCount": self.success_count,
            "lastFailureTime": self.last_failure_time,
            "lastStateChange": self.last_state_change,
            "stateHistory": self.state_history,
        })
    }
}

// ---------------------------------------------------------------- retry

#[derive(Clone, Debug)]
pub struct RetryPolicy {
    pub max_retries: f64,
    pub base_delay_ms: f64,
    pub backoff_multiplier: f64,
    pub max_delay_ms: f64,
    pub total_attempts: u64,
    pub total_retries: u64,
}

impl RetryPolicy {
    pub fn new(max_retries: f64, base_delay_ms: f64, backoff_multiplier: f64) -> Self {
        RetryPolicy { max_retries, base_delay_ms, backoff_multiplier, max_delay_ms: 10_000.0, total_attempts: 0, total_retries: 0 }
    }

    /// `min(base · mult^attempt, max)`.
    pub fn delay_ms(&self, attempt: u32) -> f64 {
        (self.base_delay_ms * self.backoff_multiplier.powf(attempt as f64)).min(self.max_delay_ms)
    }

    pub fn update_params(&mut self, max_retries: Option<f64>, backoff_multiplier: Option<f64>) {
        if let Some(v) = max_retries {
            self.max_retries = v;
        }
        if let Some(v) = backoff_multiplier {
            self.backoff_multiplier = v;
        }
    }

    pub fn stats(&self) -> Value {
        json!({
            "totalAttempts": self.total_attempts,
            "totalRetries": self.total_retries,
            "currentParams": {
                "maxRetries": self.max_retries,
                "baseDelayMs": self.base_delay_ms,
                "backoffMultiplier": self.backoff_multiplier,
            },
        })
    }

    pub fn reset(&mut self) {
        self.total_attempts = 0;
        self.total_retries = 0;
    }
}

/// Bookkeeping hooks so the policy can live behind a lock while attempts run unlocked.
pub trait RetryState {
    fn with_policy<R>(&self, f: impl FnOnce(&mut RetryPolicy) -> R) -> R;
}

/// `RetryPolicy.execute(fn)`: attempts 0..=maxRetries; an error starting with
/// `CircuitBreakerOpen` stops immediately. Parameters are re-read every attempt, like JS.
pub async fn execute<T, F, Fut>(state: &impl RetryState, mut f: F) -> Result<T, String>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let mut last_err = String::new();
    let mut attempt: u32 = 0;
    while attempt as f64 <= state.with_policy(|p| p.max_retries) {
        state.with_policy(|p| p.total_attempts += 1);
        match f(attempt).await {
            Ok(v) => return Ok(v),
            Err(e) => {
                if e.starts_with("CircuitBreakerOpen") {
                    return Err(e);
                }
                last_err = e;
                let delay = state.with_policy(|p| {
                    if (attempt as f64) < p.max_retries {
                        p.total_retries += 1;
                        Some(p.delay_ms(attempt))
                    } else {
                        None
                    }
                });
                if let Some(ms) = delay {
                    tokio::time::sleep(Duration::from_secs_f64(ms.max(0.0) / 1000.0)).await;
                }
            }
        }
        attempt += 1;
    }
    Err(last_err)
}

// ---------------------------------------------------------------- concurrency limiter

#[derive(Clone, Debug)]
pub struct ConcurrencyLimiter {
    pub max_concurrent: f64,
    pub current: u64,
    pub total_acquired: u64,
    pub total_rejected: u64,
}

impl ConcurrencyLimiter {
    pub fn new(max_concurrent: f64) -> Self {
        ConcurrencyLimiter { max_concurrent, current: 0, total_acquired: 0, total_rejected: 0 }
    }

    pub fn acquire(&mut self) -> Result<(), String> {
        if (self.current as f64) < self.max_concurrent {
            self.current += 1;
            self.total_acquired += 1;
            Ok(())
        } else {
            self.total_rejected += 1;
            Err(format!(
                "ConcurrencyLimitExceeded: {}/{}",
                self.current,
                metastable::js::js_number(self.max_concurrent)
            ))
        }
    }

    pub fn release(&mut self) {
        self.current = self.current.saturating_sub(1);
    }

    pub fn update_params(&mut self, max_concurrent: Option<f64>) {
        if let Some(v) = max_concurrent {
            self.max_concurrent = v;
        }
    }

    pub fn stats(&self) -> Value {
        let total = self.total_acquired + self.total_rejected;
        json!({
            "current": self.current,
            "max": self.max_concurrent,
            "totalAcquired": self.total_acquired,
            "totalRejected": self.total_rejected,
            "shedRate": if total > 0 { self.total_rejected as f64 / total as f64 } else { 0.0 },
        })
    }

    pub fn reset(&mut self) {
        self.current = 0;
        self.total_acquired = 0;
        self.total_rejected = 0;
    }
}

// ---------------------------------------------------------------- adaptive parameters

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Params {
    pub retry_count: f64,
    pub backoff_multiplier: f64,
    pub cb_threshold: f64,
    pub cb_timeout_ms: f64,
    pub concurrency_limit: f64,
    pub timeout_ms: f64,
}

pub struct Bound {
    pub min: f64,
    pub max: f64,
}

/// `BOUNDS` (same table in mas-agent.js and central.js), in declaration order.
pub const BOUNDS: Params = Params {
    retry_count: 3.0,
    backoff_multiplier: 2.0,
    cb_threshold: 5.0,
    cb_timeout_ms: 10_000.0,
    concurrency_limit: 50.0,
    timeout_ms: 3000.0,
};
pub const RETRY: Bound = Bound { min: 0.0, max: 5.0 };
pub const BACKOFF: Bound = Bound { min: 1.0, max: 4.0 };
pub const CB_THRESHOLD: Bound = Bound { min: 2.0, max: 10.0 };
pub const CB_TIMEOUT: Bound = Bound { min: 2000.0, max: 30_000.0 };
pub const CONCURRENCY: Bound = Bound { min: 5.0, max: 100.0 };
pub const TIMEOUT: Bound = Bound { min: 1000.0, max: 10_000.0 };

impl Default for Params {
    fn default() -> Self {
        BOUNDS
    }
}

impl Params {
    /// Final clamp loop over all bounds.
    pub fn clamp(mut self) -> Self {
        let c = |v: f64, b: &Bound| b.min.max(b.max.min(v));
        self.retry_count = c(self.retry_count, &RETRY);
        self.backoff_multiplier = c(self.backoff_multiplier, &BACKOFF);
        self.cb_threshold = c(self.cb_threshold, &CB_THRESHOLD);
        self.cb_timeout_ms = c(self.cb_timeout_ms, &CB_TIMEOUT);
        self.concurrency_limit = c(self.concurrency_limit, &CONCURRENCY);
        self.timeout_ms = c(self.timeout_ms, &TIMEOUT);
        self
    }

    pub fn to_json(self) -> Value {
        serde_json::to_value(self).unwrap()
    }
}

/// `Math.max(a, b)` / `Math.min(a, b)` for the finite values used here.
pub fn jmax(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}

pub fn jmin(a: f64, b: f64) -> f64 {
    if b < a { b } else { a }
}
