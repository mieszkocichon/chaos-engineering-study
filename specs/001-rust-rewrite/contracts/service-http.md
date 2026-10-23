# Kontrakt: HTTP usługi eksperymentalnej (`chaos-svc`)

Źródło prawdy: `when-gossip-goes-silent/lib/generic-service.js`. Wszystkie odpowiedzi mają `Content-Type: application/json`. Ciała żądań POST są parsowane jako JSON; puste lub brakujące ciało = `{}`.

Testy kontraktowe (`crates/chaos/tests/contract.rs`) uruchamiamy na obu implementacjach do fazy 6: ta sama tabela przypadków, a adres usługi podajemy parametrem.

| Metoda | Ścieżka | Ciało żądania | Odpowiedź 2xx | Błędy |
|---|---|---|---|---|
| GET | `/data` | — | `200 {service, latencyMs, downstreamResults, timestamp}` | `502 {error, service}` gdy strategia rzuci wyjątek |
| GET | `/health` | — | `200 {status:"ok", service, uptime}` (`uptime` w sekundach, f64) | — |
| GET | `/metrics` | — | `200 {service, snapshot, resources, healthSummary, strategyStats, timestamp}` | — |
| POST | `/gossip` | GossipMessage | `200 {ok:true}` | — (brak handlera = no-op) |
| POST | `/control` | ControlUpdate | `200 {ok:true}` | — |
| POST | `/start` | `{experimentStartTime?: number}` | `200 {ok:true, experimentStartTime, service}` (brak/nieliczba → `Date.now()`) | — |
| POST | `/experiment/reset` | `{seed?: number}` | `200 {ok:true, service, seed}` (po zakończeniu resetu) | — |
| POST | `/faults/recover` | — | `200 {status:"recovered"}` | — |
| POST | `/faults/recover/:type` | — | `200 {status:"recovered", type}` | nieznany typ → pełny recover, nadal 200 |
| POST | `/faults/:profile` | opcje profilu (`delayMs`, `jitterMs`, `lossPct`, `corruptPct`, `maxDelayMs`, `durationMs`, `stepMs`, `peerIps[]`) | `200 {status:"injected", profile}` | nieznany profil → log, nadal 200; `cascade-crash` → SIGKILL do siebie, który jako PID 1 w kontenerze jest ignorowany, więc odpowiedź `injected` i proces działa dalej (quirk Q4, identycznie w JS); obsługiwane rodzaje: `baseline-flaky`, `burst-failure`, `slow-degradation`, `wan-baseline`, `network-partition`, `cascade-crash`, `packet-corruption` (ADR-0028) |

**Kolejność dopasowania tras**: `/faults/recover` i `/faults/recover/:type` mają pierwszeństwo przed `/faults/:profile`.

## `/data` – szczegóły

1. `inFlight += 1`; odczekanie `processingDelay` (z PRNG seedowanego `"${seed}_${id}_delay"`).
2. Jeśli usługa ma downstreamy i strategię → `downstreamResults = strategy.callDownstreams(downstreams)`; inaczej `null`.
3. Każdy element `downstreamResults` to odpowiedź JSON downstreamu albo (strategie z pipeline'em) `{error, service, degraded:true}`; backpressure: `{error, service, throttled, degraded: !throttled}`. Strategia `none` nie łapie błędów: pierwszy błąd → `502`.
4. Metryka zapisywana z `success = true` przy 200, `false` przy 502; `latencyMs = now − start`.

## `/metrics` – kształt

```json
{
  "service": "svc-01",
  "snapshot": { "errorRate": 0, "avgLatency": 0, "p50": 0, "p90": 0, "p99": 0,
                "requestCount": 0, "queueDepth": 0, "windowMs": 10000 },
  "resources": { "memoryMb": 12, "cpuUser": 0, "cpuSystem": 0, "uptime": 1.23 },
  "healthSummary": { "errorRate": 0, "avgLatency": 0, "load": 0 },
  "strategyStats": null,
  "timestamp": 1776540455098
}
```

- `resources.memoryMb`: w Rust RSS procesu w MB (`/proc/self/statm`). Semantyka różni się od `heapUsed` Node. Pole jest tylko informacyjne: koordynator nie przepisuje `resources` do pliku wyników, a analizator go nie czyta (sprawdzone).
- `resources.cpuUser`/`cpuSystem`: mikrosekundy z `getrusage` (jak `process.cpuUsage()`).
- `strategyStats`: kształt per strategia jak w `getStats()` referencji. Klucze: `strategy`, a dla pipeline'u per downstream `{circuitBreaker, retry, concurrency}`; MAS: `serviceId, currentParams, decisionCount, neighborCount, history`; inbound dodatkowo `inbound`; SWIM dodatkowo `gossipFanout`; backpressure: `throttledCount, throttledByGossip, throttledByLocalFails, passedCount, throttleRate, gossipReceived, neighborStates, recentThrottles`.

## Gossip wychodzący

POST `<neighbor>/gossip` z `{fromId, ...healthSummary, timestamp}`, timeout 1000 ms, błędy połykane. Interwały: MAS 3000 ms (SWIM: losowe `fanout` sąsiadów, domyślnie 3), backpressure 1000 ms, leaf 1000 ms.

## Kontrola zdrowia kontenera

`curl -fsS http://localhost:<port>/health` (zamiast `node -e fetch(...)`, research R13).
