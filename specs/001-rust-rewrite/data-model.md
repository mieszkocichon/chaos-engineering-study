# Data Model: Przepisanie platformy na Rust

Encje odpowiadają strukturom z kodu referencyjnego. Nazwy pól w JSON zostają w camelCase (`#[serde(rename_all = "camelCase")]`). „Źródło” wskazuje plik referencji, z którego wynika semantyka.

## Konfiguracja (wejście, tylko odczyt)

### Topology — `config/topologies/<name>.json` (źródło: `lib/topology-loader.js`)

| Pole | Typ | Reguły |
|---|---|---|
| `name`, `description` | string | — |
| `services[]` | `{ id, role?, port?, processingDelay? }` | `id` unikalne; `role` ∈ `entry` \| `service` (domyślnie `service`); dokładnie jedna usługa `entry` (inaczej `entryPoint = null`). Pole `port` w pliku jest **ignorowane** (porty nadaje loader) |
| `edges[]` | `{ from, to, kind? }` | `kind = "gossip"` → krawędź tylko dla sąsiedztwa, nie downstream |
| `defaults.processingDelayMs` | ProcessingDelay | domyślnie `{distribution:"normal", mean:50, std:10}` |

**Wyliczane (ResolvedService)**:
- `port`: kolejne od `BASE_PORT` w kolejności `services[]`, z pominięciem 4045, 3659, 5060, 5061;
- `downstreams[]`: `{id, url: http://<host>:<port>/data}` dla krawędzi `from = id` i `kind ≠ gossip`;
- `neighbors[]`: unikalne id z krawędzi w obu kierunkach (w kolejności pierwszego wystąpienia), z `gossipUrl`, `metricsUrl`, `controlUrl`;
- `host` = id usługi (DNS Dockera).

### ProcessingDelay
`number` (stała, ms) | `{distribution:"normal", mean=50, std=10}` → `max(0, round(normal))` | `{distribution:"uniform", min=10, max=100}` → `uniform` włącznie | inna wartość → `mean ?? 50`.

### FaultProfile — `config/faults/<name>.json`

| Pole | Typ | Reguły |
|---|---|---|
| `name`, `description` | string | — |
| `faults[]` | Fault | — |

**Fault**: `targetService` (id z topologii; nieznane → pomijane), `type` ∈ {`baseline-flaky`, `burst-failure`, `slow-degradation`, `cascade-crash`, `wan-baseline`, `network-partition`}, `startTimeSec` (domyślnie 0), `endTimeSec?` (`≥ 999` lub brak = trwała do resetu; `cascade-crash` bez `endTimeSec` → recover po 35 s), `options?` (`delayMs`, `jitterMs` dla wan-baseline), `peerServices?` (tylko partycja; koordynator zamienia je na `options.peerIps`).

### ExperimentMatrix — `config/experiments/<name>.json`
`topologies[]`, `faultProfiles[]`, `strategies[]`, `workload{rps, durationSec, warmupSec}`, `repetitions`, `seedBase`.

## Stan usługi

### MetricsCollector (źródło: `lib/metrics-collector.js`)
- 10 kubełków po 1000 ms; kubełek = `{count, errors, latencies[], timestamp}` gdzie `timestamp = floor(now_ms/1000)`; indeks `timestamp % 10`.
- Aktywne: `timestamp > 0 && now_s − timestamp < 10`.
- `inFlight`: `+1` na start żądania, `−1` (min 0) na zapis.
- **Snapshot**: `errorRate` (f64), `avgLatency` (round), `p50/p90/p99` (nearest-rank `ceil(n·p)−1`), `requestCount`, `queueDepth = inFlight`, `windowMs = 10000`. Brak aktywnych → zera.
- **HealthSummary**: `{errorRate, avgLatency, load = queueDepth}`.
- `reset()`: zeruje `inFlight` i kubełki.

### CircuitBreaker (źródło: `lib/circuit-breaker.js`)
Stany i przejścia:

```
closed ──(failureCount ≥ threshold)──► open
open ──(now − lastStateChange ≥ openTimeoutMs, przy wywołaniu)──► half-open
half-open ──(sukces)──► closed (failureCount = 0)
half-open ──(porażka)──► open
open (przed timeoutem) → błąd "CircuitBreakerOpen:<name>" bez wywołania
```
Pola: `state`, `failureCount` (sukces zeruje), `successCount`, `lastFailureTime?`, `lastStateChange`, `stateHistory[] {from,to,timestamp,failureCount}`. Parametry `failureThreshold`, `openTimeoutMs` zmieniane w locie.

### RetryPolicy
`maxRetries`, `baseDelayMs`, `backoffMultiplier`, `maxDelayMs=10000`; opóźnienie `min(base·mult^attempt, max)`; błąd z prefiksem `CircuitBreakerOpen` przerywa natychmiast. Liczniki `totalAttempts`, `totalRetries`.

### ConcurrencyLimiter
`maxConcurrent`, `current`, `totalAcquired`, `totalRejected`, `shedRate`. Odmowa → `ConcurrencyLimitExceeded: <current>/<max>`.

### ResilienceParams i granice (BOUNDS, źródło: `mas-agent.js`, `central.js`)

| Parametr | min | max | domyślnie |
|---|---|---|---|
| `retryCount` | 0 | 5 | 3 |
| `backoffMultiplier` | 1.0 | 4.0 | 2.0 |
| `cbThreshold` | 2 | 10 | 5 |
| `cbTimeoutMs` | 2000 | 30000 | 10000 |
| `concurrencyLimit` | 5 | 100 | 50 |
| `timeoutMs` | 1000 | 10000 | 3000 |

Static: `retryCount=3, retryBaseMs=200, retryBackoff=2, cbThreshold=5, cbTimeoutMs=10000, requestTimeoutMs=3000, maxConcurrent=50` (nadpisywalne przez `STATIC_PARAMS`). Static-conservative: `retryCount=1, cbThreshold=15, cbTimeoutMs=5000, maxConcurrent=100, retryBaseMs=500`. Uwaga: `cbThreshold=15` wykracza poza BOUNDS – to poprawne, bo static nie jest ograniczany.

### Strategy (enum, 8 wariantów)
`None{timeoutMs=3000}` · `Static(params)` · `StaticConservative` · `Central(params z /control)` · `Mas(agent)` · `MasSwim(agent, fanout)` · `MasInbound(agent, counters)` · `Backpressure(agent)`. Usługa bez downstreamów nie ma strategii; jeśli ma sąsiadów → `LeafGossipPusher` (co 1000 ms).

### MasAgent
`params`, `neighborState: id → {errorRate, avgLatency, load, timestamp}` (wygasa po `3 × gossipIntervalMs` = 9000 ms), `prevP90`, `decisionHistory[]` `{timestamp, observation{errorRate,p90,requestCount,queueDepth,neighborCount,openBreakers[]}, decision}`. Interwały: decyzja 2000 ms, gossip 3000 ms. Reset → wartości domyślne.

### Inbound (rozszerzenie MasAgent)
Liczniki per downstream `{ok, fail, windowStart}` (okno 8000 ms), `selfInboundReport {reporterId, dropRate, timestamp}` (TTL 10000 ms), `inboundStats {reportsEmitted, reportsReceived, selfInboundInflations}`. Raport wysyłany, gdy `samples ≥ 5 && rate ≥ 0.3`; `dropRate` jako `toFixed(4)`. Quirk Q1: odbiorca nie dostaje `inboundReport` (research R10).

### BackpressureAgent
Progi: `errorRate > 0.3`, `load > 15`, `avgLatency > 2000` (gossip świeższy niż 5000 ms); lokalnie: ≥3 porażki w oknie 8000 ms; sonda co 5000 ms. Gossip co 1000 ms, timeout żądania 3000 ms, log ≤500 wpisów (w stats ostatnie 20).

### FaultInjector
`activeFaults`, `blockedIps: Set<IpAddr>`, zadanie `slow-degradation` (0→5000 ms przez 60 s, krok 1 s, jitter 10%).

## Komunikaty

### GossipMessage (POST `/gossip`)
`{fromId, errorRate, avgLatency, load, timestamp, inboundReport?{target, dropRate, samples}, inboundInflatedBy?}`. Odbiór ignoruje wiadomości bez `fromId`.

### ControlUpdate (POST `/control`)
Podzbiór ResilienceParams; scalany z bieżącymi parametrami (`Central`).

## Wynik

### ExperimentResult — patrz [contracts/result-file.md](contracts/result-file.md)

## Symulator metastabilności (źródło: `metastable-failures/src/sim.js`)

### TrialConfig
`capacity=50, serviceMeanMs=100, queueMax=200, attemptTimeoutMs=500, maxAttempts=5, retryBackoffMs=50, cancelQueuedOnTimeout=false, lambda: fn(t)→req/s, totalMs=60000, tickMs=200, seed=1`.

### Event
`{time: f64, seq: u64, type ∈ FRESH|RETRY|DEPART|TIMEOUT, payload}`; kolejność `(time, seq)`.

### Attempt — stany
```
new ─► serving ─► (DEPART) done*
new ─► queued ─► serving
new ─► rejected (shed, natychmiastowy retry)
queued ─► canceled (TIMEOUT && cancelQueuedOnTimeout)
```
*W referencji stan `done` nigdy nie jest ustawiany; TIMEOUT po DEPART liczy się więc jako timeout. To zachowanie zachowujemy (wpływa na liczniki).

### TickRow
`{t, goodput, offered, fresh, retry, wasted, shed, timeout, queueLen, inService}` (wartości na sekundę = licznik / (tickMs/1000)).
