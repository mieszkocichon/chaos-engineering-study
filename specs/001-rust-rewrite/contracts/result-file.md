# Kontrakt: plik wyniku eksperymentu

Źródło prawdy: `buildResult` i `resultFilename` w `runner/coordinator.js`. Konsumenci: `analyze_results.py` (oba egzemplarze), `compare_impls.py`.

## Nazwa pliku

```
${RESULTS_DIR}/${TOPOLOGY}_${STRATEGY}_${FAULT_BASE}_${FAULT_PROFILE}[+${FAULT_PARTITION}]_${seed}_${Date.now()}.json
```

Dla wariantów (`variants`), przed `${seed}` wstawiany jest suffiks wariantu: `__${variant_name}__`.

## Format

JSON z wcięciem 2 spacji (`JSON.stringify(result, null, 2)`). Kolejność kluczy jak niżej. Liczby formatowane jak w JS (research R7).

```jsonc
{
  "config": {
    "topology": "ring-50",
    "strategy": "mas-agent",
    "faultBase": "no-wan",
    "faultProfile": "baseline-flaky",
    "faultPartition": null,          // string | null
    "seed": 1000,
    "rps": 20,
    "durationSec": 60,
    "warmupSec": 15,
    "softReset": true,
    "loadGenerator": "k6",
    "spof": null                     // null | { "killAtSec": n, "killedAtMs": n | null }
  },
  "experimentStartTime": 1776540455098,
  "summary": {
    "totalRequests": 0,              // round(http_reqs.count)
    "successCount": 0,               // total - failureCount
    "failureCount": 0,               // round(http_req_failed.rate * total)
    "successRate": 0,                // f64; 0 gdy total = 0
    "avgLatencyMs": 0,               // round(http_req_duration.avg)
    "p50Ms": 0,                      // round(p(50) || med)
    "p90Ms": 0,
    "p99Ms": 0,
    "minLatencyMs": 0,
    "maxLatencyMs": 0,
    "wallClockMs": 0
  },
  "k6Metrics": { },                  // surowe `metrics` z podsumowania k6, przepisane bez zmian
  "serviceMetrics": {
    "<serviceId>": {                 // null, jeśli /metrics nie odpowiedziało
      "errorRate": 0, "avgLatency": 0, "p50": 0, "p90": 0, "p99": 0,
      "requestCount": 0, "queueDepth": 0, "windowMs": 10000,
      "strategyStats": null
    }
  },
  "timestamp": "2026-10-19T12:00:00.000Z",
  "spofTest": {                      // opcjonalne (wstrzykiwane przez chaos-run w testach z akcją stop-service)
    "killedService": "svc-03",
    "killAtSec": 30,
    "permanent": true,
    "phase": "phase1",               // opcjonalne
    "note": "..."                    // opcjonalne
  }
}
```

## Reguły zgodności

- Wszystkie pola całkowite powyżej (`round(...)`, liczniki, znaczniki czasu) serializujemy jako liczby całkowite.
- `serviceMetrics[id]` = `snapshot` rozwinięty w obiekt + `strategyStats`. Kolejność kluczy: najpierw pola snapshotu, potem `strategyStats`.
- `serviceMetrics` przechowujemy jako mapę uporządkowaną według kolejności zakończenia zapytań (w JS kolejność wstawiania jest niedeterministyczna). Analizator nie zależy od kolejności, więc **nie** jest ona częścią kontraktu.
- `spofTest`: w eksperymentach z akcją mid-run (`stop-service`, np. `spof-test.json`) `chaos-run` wstrzykuje na poziomie głównym obiekt `spofTest` z polami `killedService`, `killAtSec`, `permanent` (czytanymi przez `analyze_results.py` do weryfikacji hipotezy H2 o braku SPOF).
- Test zgodności (faza 3): `analyze_results.py` przetwarza katalog z plikami Rust bez błędów, a pola `summary` wyliczone przez `build_result` z zapisanego podsumowania k6 są identyczne z historycznym plikiem tej samej komórki.
