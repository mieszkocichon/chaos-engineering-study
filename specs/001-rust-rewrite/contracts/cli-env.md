# Kontrakt: zmienne środowiskowe i CLI

Nazwy, wartości domyślne i semantyka są identyczne z referencją (FR-004). Liczby są parsowane jak `parseInt(x, 10)`: wiodące cyfry, reszta ignorowana.

## `chaos-svc` (źródło: `runner/service-runner.js`)

| Zmienna | Domyślnie | Uwagi |
|---|---|---|
| `SERVICE_ID` | — (wymagana) | brak → `exit 1`; nieznane ID → `exit 1` + lista dostępnych ID na stderr |
| `TOPOLOGY` | `fanout` | nazwa pliku w `config/topologies/` |
| `FAULT_PROFILE` | `baseline-flaky` | ładowany; awarie wstrzykuje koordynator |
| `STRATEGY` | `none` | `none|static|static-conservative|central|mas-agent|mas-agent-swim|mas-agent-inbound|backpressure|retry-only`; inna → błąd startu |
| `SEED` | `42` | seed PRNG opóźnień |
| `BASE_PORT` | `4000` | |
| `STATIC_PARAMS` | — | JSON dla `static`; błędny → ostrzeżenie, wartości domyślne |
| `SWIM_FANOUT` | `3` | min 1 |
| `CONFIG_DIR` | `/app/config` | **nowa**, opcjonalna; referencja liczyła ścieżkę względem pliku. Potrzebna do testów na localhost |
| `HOST_OVERRIDE` | — | **nowa**, opcjonalna (testy): wszystkie adresy sąsiadów/downstreamów wskazują ten host zamiast nazwy DNS = id |

Sygnały: `SIGTERM` → zatrzymanie timerów, `exit 0`. Log startowy: `[<id>] Listening on port <p> | topology=… fault=… strategy=… seed=…`.

## `chaos-coord` (źródło: `runner/coordinator.js`)

| Zmienna | Domyślnie |
|---|---|
| `TOPOLOGY` | `ring-50` |
| `STRATEGY` | `none` |
| `FAULT_BASE` | `no-wan` (`no-wan` lub pusta = brak warstwy bazowej) |
| `FAULT_PROFILE` | `baseline-flaky` |
| `FAULT_PARTITION` | `` (brak) |
| `SEEDS` | `1000,2000,3000,4000,5000` (pusta/niepoprawna → `exit 1`) |
| `RPS` | `20` |
| `DURATION_SEC` | `60` |
| `WARMUP_SEC` | `15` |
| `BASE_PORT` | `4000` |
| `RESULTS_DIR` | `/results` |
| `HEALTH_TIMEOUT_MS` | `300000` |
| `INTER_SEED_SETTLE_MS` | `2000` |
| `SPOF_KILL_AT_SEC` | `0` (tylko `central`) |
| `K6_SCRIPT` | `/app/runner/k6-script.js` (**nowa**, opcjonalna; w testach wskazuje atrapę) |
| `K6_BIN` | `k6` (**nowa**, opcjonalna; w testach wskazuje atrapę) |

Kody wyjścia: `0` – komórka zakończona; `≠0` – timeout zdrowia, błąd k6, błąd parsowania podsumowania. Linia logu po seedzie: `[coordinator] seed=<s> SR=<x.x>% P90=<n>ms` (parsowana przez skrypty macierzy).

## `chaos-gen` (źródło: `generate-*.js`, `scripts/generate-mesh-topology.js`, `generate-k8s-manifests.js`)

```
chaos-gen ring                         # jak generate-ring-topology.js (katalog główny, bez argumentów)
chaos-gen ring-swarm [N] [FAULT] [STRATEGY] [SEED]   # jak generate-swarm-topology.js z katalogu głównego (uruchamiać z katalogu głównego)
chaos-gen swarm   <args jak JS>
chaos-gen mesh    <args jak JS>
chaos-gen compose <TOPOLOGY> <FAULT> <STRATEGY> <SEEDS> <BASE> <PARTITION> <PROJECT>
chaos-gen k8s     <args jak JS>
```
Argumenty pozycyjne i ścieżki wyjściowe są dokładnie takie, jak w odpowiednich skryptach JS (do odczytania z kodu w fazie 3 i utrwalenia testem diff). Wyjście różni się od JS **tylko** poleceniem healthcheck.

## `chaos-run` (zastępuje `runner/run-matrix-*.sh`, ADR-0029, ADR-0030)

```
chaos-run <ścieżka-do-eksperymentu.json> [opcje]
```

Kolejność: flaga > zmienna środowiskowa (te z dawnych skryptów) > plik eksperymentu.

| Flaga | Zmienna | Znaczenie |
|---|---|---|
| `--dry-run` | | wypisuje komórki i liczbę eksperymentów; niczego nie tworzy |
| `--only <tekst>` | | komórki, których etykieta lub topologia zawiera tekst |
| `--resume` | | pomija komórki, które mają plik wyniku dla każdego seeda (z uwzględnieniem wariantu) |
| `--workers <n>` | `WORKERS` | równoległe projekty compose `<prefix>1..n` |
| `--impl <rust|js>` | `IMPL` | obraz `chaos-experiment-rs:latest` albo `chaos-experiment:latest` (nadpisuje `IMAGE`) |
| `--results-dir <ścieżka>` | `RESULTS_DIR` | katalog wyników |
| `--topology <nazwa>` | | topologia |
| `--seeds <s1,s2,...>` | `SEEDS` | seedy; nie-liczba to błąd |
| `--project-prefix <p>` | `PROJECT_PREFIX` | prefiks projektów compose i klucz blokady |
| `--gateway-cpus <n>` | `GATEWAY_CPUS` | limit CPU bramki (zamiast `sed`) |
| `--config-dir <ścieżka>` | `CONFIG_DIR` | katalog `config/` |

Kod wyjścia: `0` – wszystkie komórki się udały; `1` – błąd walidacji albo co najmniej jedna komórka nie
(`compose up` / `stack deploy` nie wstał, koordynator zakończył się kodem ≠ 0, zniknął po 60 s, przekroczył 90 min albo brakuje pliku wyniku seeda); `2` – błąd użycia.

### Schemat pliku eksperymentu (`config/experiments/*.json`)

Nieznany klucz jest błędem. Wszystkie pola opcjonalne poza tym, co daje co najmniej jedną komórkę i seed.

```json
{
  "name": "matrix-compose",
  "description": "…",
  "topology": "ring-50",                       // albo "topologies": [...]
  "strategies": ["none", "central", "mas-agent"],
  "faultBases": ["no-wan", "wan-50ms"],        // domyślnie ["no-wan"]
  "faultProfiles": ["baseline-flaky", "cascade-crash"],
  "seeds": [1000, 2000],                       // albo "1000,2000", albo "seedBase": 1000 + "repetitions": 5
  "workload": { "rps": 20, "durationSec": 60, "warmupSec": 15 },
  "resources": { "gatewayCpus": "4" },
  "staticParams": {}, "swimFanout": 3, "spofKillAtSec": 30,   // dla wszystkich komórek
  "variants": [                                // komórki o tej samej strategii; pliki wyników dostają `__<name>__`
    { "name": "aggressive", "strategy": "static", "staticParams": { "retryCount": 5 } }
  ],
  "extraCells": [                              // pojedyncze komórki poza iloczynem (np. partycja sieci)
    { "strategy": "central", "base": "no-wan", "fault": "baseline-flaky", "partition": "network-partition" }
  ],
  "backend": "compose",                        // "compose" (domyślnie) lub "swarm" (1 wdrożenie na seed)
  "scale": 20,                                 // skala klastra Swarm (min. 5)
  "healthTimeoutMs": 300000,                   // timeout healthchecka w ms (np. 540000 dla Swarm)
  "env": { "SIMULATE_CLOCK_SKEW": "true" },    // zmienne środowiskowe do kontenerów
  "network": "bridge",                         // sterownik sieci (np. "bridge" zamiast "overlay")
  "actions": [                                 // akcje mid-run (np. SPOF testy)
    { "kind": "stop-service", "target": "svc-03", "atSec": 30 }
  ],
  "resultsDir": "./results", "projectPrefix": "w", "workers": 2, "impl": "rust"
}
```

Komórki: dla każdej topologii iloczyn strategie × bazy × profile, potem `variants` × bazy × profile, potem `extraCells`.
Dla backendu `swarm`, komórki są rozwijane do 1 wdrożenia na każdy seed (brak współdzielonego stanu / soft resetu między seedami).
`staticParams` jako obiekt jest serializowany zwarto (`{"retryCount":3}`), jako napis – bez zmian.
Po wykonaniu akcji (np. `stop-service`), do pliku wyniku wstawiany jest bezpośrednio blok `spofTest` (`killedService`, `killAtSec`, `permanent`), czytany przez `runner/analyze_results.py`.

Wyłączność: blokada pliku OS `.chaos-run-<projectPrefix>.lock` obok `config/` (znika razem z procesem). Powtarzalność:
`<results>/manifests/<name>-<ms>.json` (efektywny eksperyment, commit, obraz i jego id, czas) – poza `<results>/*.json`, które czytają analizatory.

## Symulator (`crates/metastable`, źródło: `src/run-m1.js`, `run-m2.js`, `sweep-hysteresis.js`)

- Parser flag jak w JS: `--klucz wartość`; `--klucz` bez wartości (albo przed kolejnym `--`) = `true`.
- Te same flagi i domyślne wartości co w JS (m.in. `--baseline --surge --surge-dur --timeout --attempts --backoff --qmax --seed --json --signal --window`).
- Wyjście: `JSON.stringify(x, null, 2)` + wykresy ASCII, bajt w bajt (formatter liczb – research R7).

## Dema (`crates/demos`)

Jedna binarka `demo <polecenie>` z poleceniami odpowiadającymi plikom JS 1:1. Mapowanie, flagi i nowe zmienne env (`DEMO_A_URL`, `DEMO_B_URL`, `DEMO_GATEWAY`, `DEMO_RESULTS_DIR`) są opisane w `crates/demos/README.md`.
