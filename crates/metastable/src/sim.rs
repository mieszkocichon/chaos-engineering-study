//! Retry-storm discrete-event simulation, 1:1 port of `metastable-failures/src/sim.js`.
//! Determinism (ADR-0014): one seeded mulberry32, events ordered by (time, seq).

use std::collections::VecDeque;

/// mulberry32 – identical to the JS version (`seed >>> 0`, `Math.imul`).
pub fn mulberry32(seed: f64) -> impl FnMut() -> f64 {
    let mut a = to_uint32(seed);
    move || {
        a = a.wrapping_add(0x6d2b_79f5);
        let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        (t ^ (t >> 14)) as f64 / 4_294_967_296.0
    }
}

/// ECMAScript ToUint32 (`x >>> 0`).
pub fn to_uint32(x: f64) -> u32 {
    if !x.is_finite() {
        return 0;
    }
    let t = x.trunc();
    t.rem_euclid(4_294_967_296.0) as u32
}

fn exp_rand(rng: &mut impl FnMut() -> f64, mean: f64) -> f64 {
    -crate::jsmath::log(1.0 - rng()) * mean
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Fresh,
    Retry(usize),  // job id
    Depart(usize), // attempt id
    Timeout(usize),
}

#[derive(Clone, Copy)]
struct Event {
    time: f64,
    seq: u64,
    kind: Kind,
}

/// Binary min-heap keyed by (time, seq) – same sift rules as the JS `Heap` class.
#[derive(Default)]
struct Heap {
    a: Vec<Event>,
}

impl Heap {
    fn push(&mut self, e: Event) {
        let a = &mut self.a;
        a.push(e);
        let mut i = a.len() - 1;
        while i > 0 {
            let p = (i - 1) >> 1;
            if a[p].time < a[i].time || (a[p].time == a[i].time && a[p].seq <= a[i].seq) {
                break;
            }
            a.swap(p, i);
            i = p;
        }
    }

    fn pop(&mut self) -> Option<Event> {
        let a = &mut self.a;
        if a.is_empty() {
            return None;
        }
        let top = a[0];
        let last = a.pop().unwrap();
        if !a.is_empty() {
            a[0] = last;
            let mut i = 0;
            loop {
                let l = 2 * i + 1;
                let r = l + 1;
                let mut s = i;
                let lt = |x: &Event, y: &Event| x.time < y.time || (x.time == y.time && x.seq < y.seq);
                if l < a.len() && lt(&a[l], &a[s]) {
                    s = l;
                }
                if r < a.len() && lt(&a[r], &a[s]) {
                    s = r;
                }
                if s == i {
                    break;
                }
                a.swap(s, i);
                i = s;
            }
        }
        Some(top)
    }
}

/// `lambda(t)` with a surge window `[surge_start_ms, surge_start_ms + surge_dur_ms)`.
pub fn surge_schedule(baseline: f64, surge: f64, surge_start_ms: f64, surge_dur_ms: f64) -> impl Fn(f64) -> f64 {
    move |t| if t >= surge_start_ms && t < surge_start_ms + surge_dur_ms { surge } else { baseline }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrialConfig {
    pub capacity: f64,
    pub service_mean_ms: f64,
    pub queue_max: f64,
    pub attempt_timeout_ms: f64,
    pub max_attempts: f64,
    pub retry_backoff_ms: f64,
    pub cancel_queued_on_timeout: bool,
    pub total_ms: f64,
    pub tick_ms: f64,
    pub seed: f64,
}

impl Default for TrialConfig {
    fn default() -> Self {
        Self {
            capacity: 50.0,
            service_mean_ms: 100.0,
            queue_max: 200.0,
            attempt_timeout_ms: 500.0,
            max_attempts: 5.0,
            retry_backoff_ms: 50.0,
            cancel_queued_on_timeout: false,
            total_ms: 60000.0,
            tick_ms: 200.0,
            seed: 1.0,
        }
    }
}

/// Per-tick telemetry; rates are per second (count / (tick_ms/1000)).
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub t: f64,
    pub goodput: f64,
    pub offered: f64,
    pub fresh: f64,
    pub retry: f64,
    pub wasted: f64,
    pub shed: f64,
    pub timeout: f64,
    pub queue_len: f64,
    pub in_service: f64,
}

impl Row {
    /// Field by its JS name (used by run-m2 `--signal`).
    pub fn get(&self, name: &str) -> Option<f64> {
        Some(match name {
            "t" => self.t,
            "goodput" => self.goodput,
            "offered" => self.offered,
            "fresh" => self.fresh,
            "retry" => self.retry,
            "wasted" => self.wasted,
            "shed" => self.shed,
            "timeout" => self.timeout,
            "queueLen" => self.queue_len,
            "inService" => self.in_service,
            _ => return None,
        })
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "t": self.t, "goodput": self.goodput, "offered": self.offered, "fresh": self.fresh,
            "retry": self.retry, "wasted": self.wasted, "shed": self.shed, "timeout": self.timeout,
            "queueLen": self.queue_len, "inService": self.in_service,
        })
    }
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    New,
    Queued,
    Serving,
    Rejected,
    Canceled,
    // JS compares against 'done' but never assigns it (quirk kept on purpose).
}

struct Attempt {
    job: usize,
    state: State,
    timeout_at: f64,
}

struct Job {
    attempts: f64,
    succeeded: bool,
}

#[derive(Default, Clone, Copy)]
struct Counters {
    fresh: u64,
    retry: u64,
    offered: u64,
    success: u64,
    wasted: u64,
    shed: u64,
    timeout: u64,
}

pub struct Trial {
    pub rows: Vec<Row>,
    pub cfg: TrialConfig,
}

/// Run one retry-storm trial. Times are milliseconds of virtual time.
pub fn run_trial(cfg: &TrialConfig, lambda: impl Fn(f64) -> f64) -> Trial {
    let mut sim = Sim {
        cfg: cfg.clone(),
        rng: Box::new(mulberry32(cfg.seed)),
        ev: Heap::default(),
        seq: 0,
        now: 0.0,
        in_service: 0,
        queue: VecDeque::new(),
        tick_end: cfg.tick_ms,
        rows: Vec::new(),
        c: Counters::default(),
        jobs: Vec::new(),
        attempts: Vec::new(),
    };
    sim.at(0.0, Kind::Fresh);

    while let Some(e) = sim.ev.pop() {
        if e.time > cfg.total_ms {
            break;
        }
        sim.now = e.time;
        sim.advance_to(sim.now);
        match e.kind {
            Kind::Fresh => {
                let id = sim.jobs.len();
                sim.jobs.push(Job { attempts: 0.0, succeeded: false });
                sim.submit_attempt(id, false);
                let rate = lambda(sim.now).max(1e-9);
                let dt = exp_rand(&mut sim.rng, 1000.0 / rate);
                sim.at(sim.now + dt, Kind::Fresh);
            }
            Kind::Retry(job) => {
                if !sim.jobs[job].succeeded {
                    sim.submit_attempt(job, true);
                }
            }
            Kind::Depart(a) => {
                sim.in_service -= 1;
                let job = sim.attempts[a].job;
                if sim.now <= sim.attempts[a].timeout_at && !sim.jobs[job].succeeded {
                    sim.jobs[job].succeeded = true;
                    sim.c.success += 1;
                } else {
                    sim.c.wasted += 1;
                }
                while let Some(next) = sim.queue.pop_front() {
                    if sim.attempts[next].state == State::Queued {
                        sim.start_service(next);
                        break;
                    }
                }
            }
            Kind::Timeout(a) => {
                if sim.attempts[a].state == State::Rejected {
                    continue;
                }
                sim.c.timeout += 1;
                let job = sim.attempts[a].job;
                if sim.attempts[a].state == State::Queued && cfg.cancel_queued_on_timeout {
                    sim.attempts[a].state = State::Canceled;
                }
                sim.maybe_retry(job);
            }
        }
    }
    sim.advance_to(cfg.total_ms);
    Trial { rows: sim.rows, cfg: cfg.clone() }
}

struct Sim {
    cfg: TrialConfig,
    rng: Box<dyn FnMut() -> f64>,
    ev: Heap,
    seq: u64,
    now: f64,
    in_service: i64,
    queue: VecDeque<usize>,
    tick_end: f64,
    rows: Vec<Row>,
    c: Counters,
    jobs: Vec<Job>,
    attempts: Vec<Attempt>,
}

impl Sim {
    fn at(&mut self, time: f64, kind: Kind) {
        self.ev.push(Event { time, seq: self.seq, kind });
        self.seq += 1;
    }

    fn emit_tick(&mut self) {
        let per = |n: u64, tick: f64| n as f64 / (tick / 1000.0);
        let tick = self.cfg.tick_ms;
        self.rows.push(Row {
            t: self.tick_end,
            goodput: per(self.c.success, tick),
            offered: per(self.c.offered, tick),
            fresh: per(self.c.fresh, tick),
            retry: per(self.c.retry, tick),
            wasted: per(self.c.wasted, tick),
            shed: per(self.c.shed, tick),
            timeout: per(self.c.timeout, tick),
            queue_len: self.queue.len() as f64,
            in_service: self.in_service as f64,
        });
        self.c = Counters::default();
    }

    fn advance_to(&mut self, t: f64) {
        while t >= self.tick_end {
            self.emit_tick();
            self.tick_end += self.cfg.tick_ms;
        }
    }

    fn submit_attempt(&mut self, job: usize, is_retry: bool) {
        self.jobs[job].attempts += 1.0;
        self.c.offered += 1;
        if is_retry {
            self.c.retry += 1;
        } else {
            self.c.fresh += 1;
        }
        let id = self.attempts.len();
        let timeout_at = self.now + self.cfg.attempt_timeout_ms;
        self.attempts.push(Attempt { job, state: State::New, timeout_at });
        self.at(timeout_at, Kind::Timeout(id));
        if (self.in_service as f64) < self.cfg.capacity {
            self.start_service(id);
        } else if (self.queue.len() as f64) < self.cfg.queue_max {
            self.attempts[id].state = State::Queued;
            self.queue.push_back(id);
        } else {
            // shed: immediate reject -> fast retry (amplification)
            self.attempts[id].state = State::Rejected;
            self.c.shed += 1;
            self.maybe_retry(job);
        }
    }

    fn start_service(&mut self, a: usize) {
        self.attempts[a].state = State::Serving;
        self.in_service += 1;
        let dt = exp_rand(&mut self.rng, self.cfg.service_mean_ms);
        self.at(self.now + dt, Kind::Depart(a));
    }

    fn maybe_retry(&mut self, job: usize) {
        let j = &self.jobs[job];
        if !j.succeeded && j.attempts < self.cfg.max_attempts {
            self.at(self.now + self.cfg.retry_backoff_ms, Kind::Retry(job));
        }
    }
}

/// Nominal capacity in req/s: slots / mean service time.
pub fn nominal_capacity_rps(cfg: &TrialConfig) -> f64 {
    cfg.capacity / (cfg.service_mean_ms / 1000.0)
}
