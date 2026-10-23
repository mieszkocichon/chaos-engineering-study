# Research: Przepisanie platformy na Rust

**Feature**: [spec.md](spec.md) · **Plan**: [plan.md](plan.md) · **Data**: 2026-10-19

Każda decyzja w formacie: Decyzja / Uzasadnienie / Odrzucone alternatywy. Wszystkie wpisy z sekcji Technical Context w planie mają tu rozstrzygnięcie.

---

## R1. Toolchain i uruchamianie

- **Decyzja**: oficjalny obraz `rust:1.97.1-bookworm` (przypięty w T001; ta sama wersja w `rust-toolchain.toml` i CI) (edition 2024). Cache rejestru cargo i katalogu `target/` w nazwanych wolumenach Dockera. Wieloetapowy `Dockerfile`: etap build → `debian:bookworm-slim` + `iproute2`, `iptables`, `curl`, `procps`, k6 v0.55.0 (jak dziś).
- **Uzasadnienie**: ADR-0004 (bez toolchainów na hoście, pinned images). Obraz runtime jest mniejszy niż `node:22-slim`, a binarka nie potrzebuje środowiska uruchomieniowego.
- **Odrzucone**: instalacja rustup na Windows (łamie ADR-0004); obraz `scratch`/musl (brak `tc`/`iptables`, a są wymagane).
- **Uwaga z ADR-0004**: polecenia wieloetapowe uruchamiamy jako `sh scripts/*.sh`, nie `docker run ... sh -c '...'` (PowerShell obcina stdout).

## R2. Układ repozytorium

- **Decyzja**: jeden workspace cargo w katalogu głównym, trzy crate'y:
  - `crates/metastable` – biblioteka (sim, ews) + binarki `run-m1`, `run-m2`, `sweep-hysteresis`;
  - `crates/chaos` – biblioteka (logika rdzenia) + binarki `chaos-svc` (usługa), `chaos-coord` (koordynator), `chaos-gen` (generatory topologii, compose, k8s);
  - `crates/demos` – binarki demo (gateway, service-a/b, warianty chaos, klienci, runnery scenariuszy).
  Kod JS zostaje na miejscu do fazy wycofania. Konfiguracja (`when-gossip-goes-silent/config/**`) zostaje tam, gdzie jest, i jest montowana jako `/app/config`, jak dziś.
- **Uzasadnienie**: trzy niezależne domeny = trzy crate'y. Więcej crate'ów (np. osobny na każdy prymityw) nie daje nic poza narzutem.
- **Odrzucone**: osobne repozytorium (traci wspólną historię i wyniki); jeden crate na wszystko (dema ciągnęłyby zależności rdzenia i odwrotnie).

## R3. Serwer i klient HTTP, runtime asynchroniczny

- **Decyzja**: `tokio` (multi-thread), `axum` (serwer), `reqwest` z `rustls` (klient; timeout na żądanie). `serde` + `serde_json` do JSON.
- **Uzasadnienie**: de facto standard, dojrzały, wspiera wszystko, czego używa referencja (JSON body, parametry ścieżki, timeouty, abort).
- **Odrzucone**: `actix-web` (własny runtime, mniej naturalny z `reqwest`); goły `hyper` (więcej kodu bez korzyści).
- **Ryzyko czasowe**: Node obsługuje żądania na jednym wątku, a tokio na wielu. To zmienia rozkład opóźnień pod obciążeniem. Jest to oczekiwana i akceptowana różnica, mierzona przez SC-004. Jeśli raport równoważności pokaże systematyczne odchylenie, wtedy (i tylko wtedy) test z `current_thread` jako zmienną kontrolną.

## R4. Generator losowy `seedrandom` (FR-005, SC-002)

- **Decyzja**: własny port algorytmu ARC4 z `seedrandom@3.0.5` (domyślny eksport): `mixkey` seeda-napisu (UTF-16 `charCodeAt`, `smear ^= key*19`), RC4 z odrzuceniem 256 bajtów, generacja `double` z 52 bitami znaczącymi (pętla `significance=2^52`, `overflow=2^53`). Na tym: `bernoulli`, `uniform` (włączny, `floor`), `normalRandom` (Box–Muller), `exponential` – 1:1 z `lib/prng.js`.
- **Weryfikacja**: wektory referencyjne generowane przez Node w kontenerze (`node:20-bookworm`) i commitowane do repozytorium: ≥20 seedów (w tym wzorce `"${seed}_${id}_delay"`), ≥10 000 losowań na seed, dla `random()` i każdego rozkładu.
- **Odrzucone**: crate'y `rand`/`rand_chacha` (inna sekwencja, łamie FR-005); FFI do Node (łamie cel przepisania).

## R5. mulberry32 (symulator i SWIM)

- **Decyzja**: jedna funkcja `mulberry32` w `crates/metastable`, używana też przez SWIM w `crates/chaos` (`chaos` zależy od lekkiego `metastable`, z którego bierze też formatter `js`). Arytmetyka `u32` z `wrapping_add`/`wrapping_mul` odpowiada `Math.imul` i `>>> 0`. Seed SWIM: suma kodów znaków `serviceId` XOR `0xC0FFEE`.
- **Weryfikacja**: wektory referencyjne jak w R4.

## R6. Funkcje matematyczne a determinizm bit w bit (FR-006)

- **Problem**: V8 liczy `Math.log`/`Math.cos`/`Math.sqrt` przez port fdlibm (`ieee754::log`, `cos`). `f64::ln` w Rust woła libm platformy (glibc), która może różnić się w ostatnim bicie. `sqrt` jest poprawnie zaokrąglany wszędzie, więc nie stanowi problemu.
- **Decyzja (rozstrzygnięta – ADR-0025: port fdlibm, std różnił się w ~10% wartości)**: najpierw spike (zadanie w fazie 1): porównanie `f64::ln` z wartościami z V8 na 10⁶ wejść z przedziału `(0,1]` (dokładnie te, które daje `1 - rng()`). Jeśli wszystkie są identyczne, zostaje `f64::ln`. Jeśli nie, przenosimy `ieee754::log` i `cos` z V8 (fdlibm, ok. 150 linii łącznie) do modułu `jsmath` i używamy ich w symulatorze i w PRNG.
- **Uzasadnienie**: tylko pomiar mówi, czy problem istnieje. Port fdlibm jest mały i deterministyczny.
- **Odrzucone**: crate `libm` „w ciemno” (port musl, nie fdlibm, więc też nie gwarantuje zgodności z V8).

## R7. Formatowanie liczb i JSON zgodny z JS

- **Problem**: `serde_json` wypisuje `f64` 50.0 jako `50.0`, a JS jako `50`; inaczej formatuje też wykładniki (JS: `1e21`, `1.5e-7`). `toFixed` w JS przy remisie zaokrągla w górę, a `format!("{:.N}")` w Rust do parzystej. Analizator i `gate-a.sh` (grep po `"metastable_stuck": true`) zależą od tekstu.
- **Decyzja**:
  1. Pola, które w JS są wynikiem `Math.round`/liczeniem, mają typy całkowite (`u64`/`i64`).
  2. Pozostałe `f64` są serializowane przez mały formatter `js_number` (cyfry z `ryu` + reguły `Number.prototype.toString`: notacja wykładnicza dla wykładnika ≥21 lub ≤-7, brak `.0` dla całkowitych).
  3. `js_to_fixed(x, n)` odtwarza regułę remisów z ECMAScript.
  4. Wydruki CLI symulatora używają `serde_json::to_string_pretty` (2 spacje, `": "`, tak samo jak `JSON.stringify(x, null, 2)`) + formatter z pkt 2.
- **Weryfikacja**: testy jednostkowe na listach wartości granicznych porównanych z wyjściem Node.
- **Odrzucone**: akceptacja różnic tekstowych (psuje SC-003 i skrypty bramek).

## R8. Czas w logice (testowalność, FR-017)

- **Problem**: referencja woła `Date.now()` wewnątrz CB, metryk, agentów i backpressure, co uniemożliwia deterministyczne testy jednostkowe.
- **Decyzja**: logika rdzenia przyjmuje `now_ms: u64` jako argument (np. `breaker.on_failure(now_ms)`, `metrics.record(now_ms, …)`, `agent.decide(obs, now_ms)`). Tylko warstwa usługi czyta zegar (`SystemTime` w ms od epoki, bo kubełki metryk wyrównują się do `floor(now/1000)` jak w JS). Timery (interwały decyzji i gossipu, slow-degradation) to `tokio::time::interval`; testy timerów używają `tokio::time::pause()`.
- **Uzasadnienie**: przekazanie liczby jest prostsze niż trait zegara z dwiema implementacjami i daje czyste funkcje do testów referencyjnych.
- **Odrzucone**: trait `Clock` (więcej kodu, ten sam efekt); mockowanie czasu globalnie.

## R9. Wspólny potok odporności

- **Obserwacja**: pięć strategii (`static`, `static-conservative`, `central`, `mas-agent*`) kopiuje ten sam kod `limiter → retry → circuit breaker → fetch z timeoutem` oraz identyczne mapowanie wyników (`{ error, service, degraded: true }`).
- **Decyzja**: jeden typ `Pipeline` per downstream (CB + retry + limiter) i jedna funkcja `call_downstreams`. Strategie różnią się tylko **źródłem parametrów** (stałe / `/control` / agent MAS) i hakami (inbound liczy sukcesy i porażki per downstream). Strategie `none` i `backpressure` mają własną, krótką ścieżkę (jak w referencji).
- **Wymóg zgodności**: komunikaty błędów pozostają bajt w bajt (`CircuitBreakerOpen:<name>`, `ConcurrencyLimitExceeded: x/y`, `HTTP <status>: <body>`, `BackpressureThrottled: …`), bo retry rozpoznaje CB po prefiksie, a teksty trafiają do wyników. To samo dotyczy nazw breakerów (`static`/`central`: `<downstreamId>`, MAS: `<serviceId>-><downstreamId>`).
- **Odrzucone**: osobna implementacja na strategię (5× ten sam kod); trait strategii z dynamiczną dyspozycją dla wszystkich ośmiu (zbędne – `enum` z ośmioma wariantami wystarcza).

## R10. Zgodność „bug w bug”

Analiza kodu referencyjnego ujawniła zachowania, które wyglądają na niezamierzone. Porównywalność z 1 020 wynikami wymaga ich **zachowania**:

| # | Zachowanie | Skutek |
|---|---|---|
| Q1 | `createGossipReceiver` przekazuje tylko `errorRate, avgLatency, load, timestamp`, więc pole `inboundReport` jest gubione przed `MASAgentInbound.receiveGossip`. | Mechanizm „inbound” najpewniej nigdy nie podnosi `errorRate` odbiorcy (`reportsReceived` = 0). |
| Q2 | `ConcurrencyLimitExceeded` jest rzucany przed retry, więc odrzucenie przez limiter nie jest ponawiane i nie liczy się jako porażka CB. | Utrwalone jako zachowanie. |
| Q3 | Zmienne `SIMULATE_CLOCK_SKEW`, `SIMULATE_IO_CONTENTION`, `DOWNSTREAM_SERVICES` w compose nie są czytane przez kod. | Zostają ignorowane. |
| Q4 | `cascade-crash` wysyła SIGKILL do własnego PID, ale usługa jest PID 1 w kontenerze (`CMD ["node", …]`), a jądro ignoruje sygnał wysłany przez init przestrzeni nazw do samego siebie. **Proces przeżywa**, odpowiada `injected` i obsługuje dalej ruch (sprawdzone na obrazie JS i Rust, T061). | Profil `cascade-crash` w historycznej macierzy prawdopodobnie nie zabijał żadnej usługi. Wymaga to osobnej analizy wyników. |
| Q5 | Początkowe polecenie `slow-degradation` (`tc qdisc add … delay 0ms 10ms distribution normal`) jest odrzucane przez `tc` („distribution specified but no latency”). Opóźnienie pojawia się dopiero przy pierwszym `tc qdisc replace` po 1 s. | Pierwsza sekunda bez opóźnienia. |

- **Decyzja**: replikujemy Q1, Q2, Q4 i Q5 dokładnie, a testy (jednostkowe i kontenerowe) utrwalają obecne zachowanie. Q1 na danych (T062): w `results*/` nie ma ani jednego przebiegu `mas-agent-inbound` (`results-inbound/` zawiera tylko `mas-agent`), więc Q1 jest potwierdzony wyłącznie na kodzie, przez wektory z `strategies.json`. Poprawki to osobna funkcja po wycofaniu referencji (wymagają nowej serii eksperymentów i ADR). Q3: nie implementujemy.
- **Uzasadnienie**: celem tej funkcji jest wierne przeniesienie; mieszanie z poprawkami uniemożliwia SC-004.

## R11. Wstrzykiwanie awarii

- **Decyzja**: rodzaje awarii jako wtyczki implementujące trait `FaultKind` (ADR-0028: Open/Closed principle). Czysta funkcja `fault_commands(profile, opts) -> Result<Vec<Argv>, String>` (lista poleceń jako argv z walidacją opcji: `lossPct`, `corruptPct`, `maxDelayMs`, `durationMs`, `stepMs`, itp.) + wykonanie przez `tokio::process::Command` **bez powłoki**. `peerIps` są parsowane jako `IpAddr` przed użyciem (referencja wkleja je do polecenia powłoki). `slow-degradation` to zadanie tokio zatrzymywane przez `recover`/`reset` (zarządzane przez `FaultRamp`). `cascade-crash`: `libc::kill(getpid(), SIGKILL)` (kod wyjścia 137 jak dziś). Dodany proof-of-extensibility: `packet-corruption` (`tc … netem corrupt <pct>%`). Błędy poleceń są logowane i połykane jak w referencji, w tym `tc qdisc del` przy braku qdisc.
- **Testy**: jednostkowe na liście poleceń i opcjach dla każdego rodzaju awarii (`tests/faults.rs`); integracyjne w kontenerze z `NET_ADMIN` (`tests/compose/cluster_test.py`), które sprawdzają `tc qdisc show dev eth0` (w tym `packet-corruption`) i `iptables -S` po wstrzyknięciu i po odwołaniu.

## R12. Koordynator i k6

- **Decyzja**: `chaos-coord` odtwarza cykl z `runner/coordinator.js` (te same zmienne env, te same nazwy plików wyników, `CentralController` w tym samym procesie). k6 zostaje procesem zewnętrznym; `runner/k6-script.js` zostaje bez zmian. DNS dla partycji: `tokio::net::lookup_host`.
- **Format wyniku**: struktura z polami w kolejności referencji (`config, experimentStartTime, summary, k6Metrics, serviceMetrics, timestamp`). `k6Metrics` jest przepisywane jako surowy `serde_json::Value`, a `timestamp` w ISO 8601 z milisekundami i `Z` (jak `toISOString`).

## R13. Generatory i healthcheck

- **Decyzja**: `chaos-gen <ring|swarm|mesh|compose|k8s> …` z tymi samymi argumentami pozycyjnymi co skrypty JS. YAML generowany szablonami tekstowymi (jak w JS), więc wynik da się porównać `diff`-em z wyjściem referencji. **Zamierzone różnice (i tylko te, utrwalone w `tests/generators.rs::expected`)**: polecenie kontenera `["node", "runner/service-runner.js"]` → `["chaos-svc"]` (koordynator → `["chaos-coord"]`), healthcheck `node -e fetch(…)` / `wget` → `["CMD", "curl", "-fsS", "http://localhost:<port>/health"]`, domyślny obraz `chaos-experiment:latest` → `chaos-experiment-rs:latest`. Moduł nazywa się `generate`, bo `gen` jest słowem zarezerwowanym w edycji 2024. Skrypty macierzy zmieniają `node generate-compose-topology.js` na `chaos-gen compose` (FR-014).
- **Odrzucone**: `serde_yaml` (crate jest wycofany i nie da wyjścia identycznego z obecnymi plikami).

## R14. Strategia testów (FR-017, FR-018, FR-019)

| Poziom | Co | Jak | Gdzie |
|---|---|---|---|
| Jednostkowe | czyste funkcje: PRNG, mulberry32, jsmath, formatter liczb, metryki, CB, retry (z `tokio::time::pause`), limiter, decyzje każdej strategii, backpressure, inbound, loader topologii, polecenia awarii, sim, ews, generatory | `cargo test`; wektory referencyjne z Node w `crates/*/tests/golden/` | lokalnie + CI |
| Integracyjne lekkie | binarki na `127.0.0.1`: endpointy usługi (kontrakt), gossip między 2–3 usługami, `/control`, `/experiment/reset`, koordynator z atrapą k6 | `cargo test --test '*'` (testy uruchamiają binarki na losowych portach) | lokalnie + CI |
| Integracyjne kontenerowe | `ring-5`: awarie sieci (`tc`/`iptables`, `NET_ADMIN`), klaster mieszany JS+Rust, pełna komórka z prawdziwym k6 (1 seed, krótki czas) | `docker compose -f tests/compose/ring5.yml up --exit-code-from tester` | lokalnie + CI (ring-5) |
| Ciężkie | topologie ≥20 usług, raport równoważności | `#[ignore]` + `scripts/test-heavy.sh` | tylko lokalnie |

- **Wektory referencyjne**: skrypty `tests/golden/gen/*.mjs` uruchamiane w `node:20-bookworm` importują moduły referencji (np. `MASAgent`) z podmienionym `Date.now` i zapisują pary wejście/wyjście do JSON. Pliki wynikowe są commitowane, więc testy Rust nie potrzebują Node. Generatory wektorów zostają w repozytorium po wycofaniu JS jako dokumentacja pochodzenia danych, razem z commitem referencji, z którego pochodzą.
- **Jedno polecenie lokalnie**: `sh scripts/test.sh` (unit + lekkie + kontenerowe ring-5), `sh scripts/test-heavy.sh`. Na Windows przez `docker run` / `docker compose` zgodnie z ADR-0004 (patrz quickstart).
- **CI**: GitHub Actions `ubuntu-latest`, dwa joby: `unit+light` (kontener `rust`) i `ring5` (docker compose z `cap_add: NET_ADMIN`). Cel: < 15 min (SC-009).
- **Odrzucone**: `proptest`/fuzzing (wektory referencyjne z referencji dają silniejszą gwarancję zgodności; można dodać później); `testcontainers-rs` (compose wystarcza i jest już narzędziem projektu).

## R15. Raport równoważności (US4, SC-004)

- **Decyzja**: nowy skrypt `when-gossip-goes-silent/compare_impls.py` (analizator pozostaje bez zmian). Na komórkę liczy: różnicę średnich success rate i p90, 95% CI metodą bootstrap (10 000 prób), Mann–Whitney U i delta Cliffa (ADR-0023), werdykt trójstanowy: równoważne / różne / nierozstrzygnięte, przy marginesach ±7 pp / ±15% (SC-004 zrewidowane 2026-10-20) i zgodność rankingu strategii.
- **Rozmiar próby**: wyniki historyczne mają n=20 seedów na komórkę (seedy 1000–20000). Nowa implementacja jest uruchamiana na **tych samych 20 seedach** (porównanie sparowane po seedzie) **plus** 10 dodatkowych (21000–30000), żeby osiągnąć n=30 z ADR-0023 dla własnych rozkładów. To odstępstwo od n≥30 dla strony historycznej jest uzasadnione w planie.

## R16. Dema

- **Decyzja**: `crates/demos` z `axum` + `reqwest` + `rand` (losowość niedeterministyczna jak `Math.random`). Klienci (`test-load`, `test-orders`, `monitor`, `compare`) jako binarki z tymi samymi flagami (`--requests`, `--save`, …). Runnery scenariuszy uruchamiają binarki zamiast `node`. Format plików `demos/results/*.json` bez zmian.
- **Testy**: jednostkowe na logice (CB, saga, retry w gateway); integracyjne: każdy scenariusz `run_*` z małą liczbą żądań i asercje jakościowe z US5.

## R17. Logowanie

- **Decyzja**: `println!`/`eprintln!` z tymi samymi prefiksami (`[coordinator]`, `[InfraFault]`, `[<SERVICE_ID>]`), bo skrypty macierzy parsują logi koordynatora (`logs --tail 1`). Bez `tracing`.

## R18. Zapis decyzji

- **Decyzja**: ADR-0024 „Przepisanie na Rust” w `metastable-failures/docs/adr/` (zgodnie z ADR-0001), który odsyła do tej specyfikacji. Spike z R6 i każde odstępstwo od zgodności dostają osobne ADR-y.
