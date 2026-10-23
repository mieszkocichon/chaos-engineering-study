# Tasks: Przepisanie platformy chaos-engineering na Rust

**Input**: dokumenty w `specs/001-rust-rewrite/` – [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: wymagane (FR-017, FR-018, FR-019). W każdej fazie testy piszemy **przed** implementacją i muszą najpierw nie przechodzić. Wektory referencyjne generujemy z kodu JS i commitujemy (research R14).

**Organizacja**: fazy według historyjek użytkownika (US1–US5) ze spec.md. Numeracja faz w tym pliku różni się od „Faz 0–6” w plan.md; mapowanie jest podane w nagłówku każdej fazy.

**Zasady dla wykonawcy**:
- Każdy port zaczyna się od przeczytania wskazanego pliku JS. Zachowanie (stałe, kolejność operacji, teksty błędów, nazwy pól JSON) przenosimy 1:1, łącznie z quirkami z research R10.
- Żadnego toolchainu na hoście: każde `cargo`/`node`/`python` uruchamiamy w kontenerze zgodnie z [quickstart.md](quickstart.md) (ADR-0004).
- Faza jest ukończona dopiero, gdy `sh scripts/test.sh` jest zielony (testy tej fazy i wszystkich wcześniejszych).

## Format: `[ID] [P?] [Story] Opis`

- **[P]**: można wykonać równolegle (inne pliki, brak zależności od nieukończonych zadań)
- **[Story]**: US1–US5 według spec.md

---

## Phase 1: Setup (plan: Faza 0, część 1)

**Cel**: pusty workspace buduje się i testuje w kontenerze.

- [X] T001 Utwórz `Dockerfile.dev` (obraz `rust-dev`): `rust:<najnowszy stabilny x.y.z>-bookworm` przypięty co do patcha + `docker-ce-cli` i `docker-compose-plugin` + `iproute2`, `iptables`, `curl`, `procps`, `python3` (do testów analizatora). Wpisz wybraną wersję do `research.md` w R1
- [X] T002 Utwórz workspace `Cargo.toml` (edition 2024, `resolver = "3"`, members `crates/metastable`, `crates/chaos`, `crates/demos`; `[workspace.dependencies]`: `tokio` (full), `axum`, `reqwest` (`rustls-tls`, `json`, bez domyślnych funkcji), `serde` (derive), `serde_json` (`preserve_order`), `ryu`, `libc`, `rand`) i `rust-toolchain.toml` z tą samą wersją co w T001
- [X] T003 [P] Utwórz `crates/metastable/Cargo.toml` + `src/lib.rs` (pusty) + puste `src/bin/run-m1.rs`, `src/bin/run-m2.rs`, `src/bin/sweep-hysteresis.rs`
- [X] T004 [P] Utwórz `crates/chaos/Cargo.toml` + `src/lib.rs` (pusty) + puste `src/bin/chaos-svc.rs`, `src/bin/chaos-coord.rs`, `src/bin/chaos-gen.rs`
- [X] T005 [P] Utwórz `crates/demos/Cargo.toml` + `src/lib.rs` (pusty)
- [X] T006 Utwórz `Dockerfile` (multi-stage: etap `build` na obrazie z T001 → `cargo build --release -p chaos`; etap runtime `debian:bookworm-slim` + `iproute2`, `iptables`, `curl`, `procps`, `ca-certificates`, k6 v0.55.0 dokładnie jak w `when-gossip-goes-silent/Dockerfile`; kopiuje `chaos-svc`, `chaos-coord`, `chaos-gen` do `/usr/local/bin`, `when-gossip-goes-silent/runner/k6-script.js` do `/app/runner/`, `when-gossip-goes-silent/config` do `/app/config`; `CMD ["chaos-svc"]`; `WORKDIR /app`)
- [X] T007 Dopisz do `.gitignore`: `target/`; dodaj `.dockerignore` w katalogu głównym (`target/`, `**/node_modules/`, `**/results*/`, `.git/`)

**Checkpoint**: `docker build -t rust-dev -f Dockerfile.dev .` i `cargo build --workspace` w kontenerze przechodzą.

---

## Phase 2: Foundational (plan: Faza 0, część 2) – blokuje wszystkie historyjki

**Cel**: wspólne narzędzia testowe, formatter liczb JS, skrypty jednego polecenia, CI, ADR.

### Testy

- [X] T008 Utwórz `scripts/golden.sh`: w kontenerze `node:20-bookworm` robi `npm ci` w `when-gossip-goes-silent/` (potrzebny `seedrandom`), a potem uruchamia każdy plik `crates/*/tests/golden/gen/*.mjs` z `cwd` = katalog repozytorium. Każdy skrypt zapisuje JSON obok siebie w `../<nazwa>.json`
- [X] T009 [P] Utwórz `crates/metastable/tests/golden/gen/js_number.mjs`: dla listy ≥50 wartości (całkowite, ułamki, `0.1+0.2`, `1e21`, `1e-7`, `123456789012345680000`, `-0`, `5e-324`, `Number.MAX_VALUE`, wartości z remisem przy `toFixed`: `0.125`, `1.005`, `2.5`) zapisuje `{value, toString, toFixed1, toFixed2, toFixed4}`; wynik do `crates/metastable/tests/golden/js_number.json`
- [X] T010 [P] Napisz test `crates/metastable/tests/js_number.rs`, który porównuje `js_number(x)` i `js_to_fixed(x, n)` z `golden/js_number.json` (musi nie przechodzić przed T011)

### Implementacja

- [X] T011 Zaimplementuj `crates/metastable/src/js.rs`: `pub fn js_number(f64) -> String` (cyfry z `ryu`, reguły `Number.prototype.toString`: notacja wykładnicza dla wykładnika dziesiętnego ≥21 lub ≤−7, `e+` dla dodatnich wykładników, brak `.0`, `-0` → `0`), `pub fn js_to_fixed(f64, u32) -> String` (reguła ECMAScript: przy remisie większe n) oraz `pub fn to_js_json_pretty(&serde_json::Value) -> String` (2 spacje, `": "`, liczby przez `js_number`; całkowite `i64/u64` bez zmian). Re-eksportuj z `lib.rs`
- [X] T012 W `crates/chaos/Cargo.toml` dodaj zależność `metastable = { path = "../metastable" }` (lekki crate bez async). `chaos` używa z niego `js::*` i `sim::mulberry32` (SWIM), więc żaden kod nie jest duplikowany
- [X] T013 Utwórz `scripts/test.sh`: (1) `cargo test --workspace`; (2) jeśli istnieje `tests/compose/ring5.yml`: `docker compose -f tests/compose/ring5.yml up --build --abort-on-container-exit --exit-code-from tester` i `down -v`; kod wyjścia ≠0 przy jakiejkolwiek porażce (`set -eu`)
- [X] T014 [P] Utwórz `scripts/test-heavy.sh`: `cargo test --workspace -- --ignored` + (od US4) uruchomienie porównania równoważności; nagłówek z opisem, że to tylko lokalnie
- [X] T015 [P] Utwórz `.github/workflows/ci.yml`: trigger `push` i `pull_request`; job `unit-light` (kontener `rust:<wersja z T001>`, cache `~/.cargo/registry` i `target`, `cargo test --workspace`); job `ring5` (`ubuntu-latest`, `docker compose -f tests/compose/ring5.yml up --build --abort-on-container-exit --exit-code-from tester`, warunek `hashFiles('tests/compose/ring5.yml') != ''`); `timeout-minutes: 15` dla obu
- [X] T016 [P] Napisz `metastable-failures/docs/adr/0024-rewrite-to-rust.md` (format MADR jak ADR-0001: Status Accepted, Data 2026-10-19, kontekst, decyzja = specs/001-rust-rewrite, konsekwencje, alternatywy: zostać przy Node, przepisać tylko symulator; Related: ADR-0004, 0014, 0023)

**Checkpoint**: `sh scripts/test.sh` w `rust-dev` jest zielony (testy `js_number` przechodzą); CI zielone na pushu.

---

## Phase 3: User Story 1 – Pilot: symulator metastabilności (Priority: P1) 🎯 MVP (plan: Faza 1)

**Goal**: `run-m1`, `run-m2` i `sweep-hysteresis` dają wyjście identyczne bajt w bajt z wersją JS.

**Independent Test**: `gate-a.sh` i `run-m2` na binarkach Rust dają te same wyniki co `metastable-failures/results/m1-gate-a.md` i `m2-gate-b.md`; diff wyjść JS i Rust dla tych samych flag jest pusty.

### Spike

- [X] T017 [US1] Utwórz `crates/metastable/tests/golden/gen/ln_cos.mjs`: dla 10⁶ wartości `1 - mulberry32(seed)()` (seedy 1–10) zapisuje bity wyników `Math.log(x)` jako hex `u64` (plik `golden/ln.json.gz` albo próbkę 10⁵ + sumę kontrolną SHA-256 z całości). To samo dla `Math.cos(2πu)` i `Math.sqrt(-2·ln u)`
- [X] T018 [US1] Napisz test `crates/metastable/tests/jsmath.rs` porównujący `f64::ln`, `f64::cos`, `f64::sqrt` z golden z T017. Wynik zapisz w `metastable-failures/docs/adr/0025-float-math-parity.md`: jeśli wszystko się zgadza → „używamy std”; jeśli nie → T019
- [X] T019 [US1] (tylko gdy T018 wykazał różnice) Zaimplementuj `crates/metastable/src/jsmath.rs`: port `ieee754::log` i `ieee754::cos` z V8 (`src/base/ieee754.cc`, fdlibm) z licencją w nagłówku; test z T018 przełącz na `jsmath::{log, cos}`

### Testy (przed implementacją)

- [X] T020 [P] [US1] Utwórz `crates/metastable/tests/golden/gen/mulberry32.mjs`: 10 000 wyjść dla seedów 0, 1, 42, 0xFFFFFFFF, 2³¹ (kod przepisany z `metastable-failures/src/sim.js`) → `golden/mulberry32.json`
- [X] T021 [P] [US1] Utwórz `crates/metastable/tests/golden/gen/sim.mjs`: importuje `runTrial`, `surgeSchedule` z `metastable-failures/src/sim.js`; dla seedów 1–30 × 3 konfiguracje (domyślna; parametry z `scripts/gate-a.sh` z surge 2000; `cancelQueuedOnTimeout: true`) zapisuje pełne `rows` → `golden/sim.json` (liczby jako hex bitów `f64`, żeby porównanie było bit-exact)
- [X] T022 [P] [US1] Utwórz `crates/metastable/tests/golden/gen/ews.mjs`: dla 5 serii (losowe, stałe, z `null` na początku, rosnąca wariancja, długość < okno) i okien 5/25 zapisuje wyniki `rollingMean`, `detrend`, `rollingVar`, `rollingAR1`, `kendallTau`, `percentile(0.5/0.9/0.99)` z `metastable-failures/src/ews.js` → `golden/ews.json`
- [X] T023 [P] [US1] Utwórz `crates/metastable/tests/golden/gen/cli.mjs`: uruchamia `node metastable-failures/src/run-m1.js`, `run-m2.js`, `sweep-hysteresis.js` dla zestawów flag z `metastable-failures/scripts/*.sh` i `README.md` (min. 10 wywołań, w tym `--json`) i zapisuje `{args, stdout}` → `golden/cli.json` (z `wall_ms` usuniętym, jeśli występuje)
- [X] T024 [P] [US1] Napisz testy jednostkowe `crates/metastable/tests/sim.rs`: mulberry32 vs golden; kolejność kopca przy równych `time` (rosnące `seq`); `run_trial` vs `golden/sim.json` bit w bit; `nominal_capacity_rps`
- [X] T025 [P] [US1] Napisz testy jednostkowe `crates/metastable/tests/ews.rs` vs `golden/ews.json` (porównanie z `null` jako `Option<f64>`)
- [X] T026 [P] [US1] Napisz test integracyjny `crates/metastable/tests/cli.rs`: uruchamia binarki (`env!("CARGO_BIN_EXE_run-m1")` itd.) dla każdego wpisu z `golden/cli.json` i porównuje stdout bajt w bajt

### Implementacja

- [X] T027 [US1] Zaimplementuj `crates/metastable/src/sim.rs`: `mulberry32`, `exp_rand`, kopiec `(time, seq)` 1:1 z klasą `Heap`, `surge_schedule` (zwraca `impl Fn(f64) -> f64`), `TrialConfig` z wartościami domyślnymi z data-model.md, `run_trial(cfg, lambda) -> Trial { rows, cfg }`, `nominal_capacity_rps`. Zachowaj: stan `done` nigdy nieustawiany, `max(1e-9, λ)`, pętla `while (queue.length)` z pomijaniem nie-`queued`
- [X] T028 [US1] Zaimplementuj `crates/metastable/src/ews.rs`: `rolling_mean`, `detrend`, `rolling_var` (Welford, przerwanie przy `None`), `rolling_ar1`, `kendall_tau`, `percentile` (nearest-rank z `round`) na `&[Option<f64>]`
- [X] T029 [US1] Zaimplementuj parser flag w `crates/metastable/src/args.rs` (semantyka z `args()` w `run-m1.js`: `--k v`, `--k` bez wartości = `true`) z testem jednostkowym
- [X] T030 [US1] Zaimplementuj `crates/metastable/src/bin/run-m1.rs` jako port `metastable-failures/src/run-m1.js` (te same flagi, domyślne, obliczenia `post_over_baseline`, `metastable_stuck`, wydruk JSON przez `to_js_json_pretty` i część tekstowa)
- [X] T031 [US1] Zaimplementuj `crates/metastable/src/bin/run-m2.rs` jako port `metastable-failures/src/run-m2.js` (werdykt JSON + wykres ASCII z linią „alarm fires here”)
- [X] T032 [US1] Zaimplementuj `crates/metastable/src/bin/sweep-hysteresis.rs` jako port `metastable-failures/src/sweep-hysteresis.js` (`--json` i wykres U/D)
- [X] T033 [US1] Sparametryzuj skrypty: `gate-a.sh` i `explore-cliff.sh` czytają `M1=${M1:-node src/run-m1.js}`, `find-loop.sh` czyta `SWEEP=${SWEEP:-node src/sweep-hysteresis.js}`. Domyślnie działają na JS jak dotąd; Rust: `M1=/app/target/release/run-m1`
- [X] T034 [US1] Napisz test integracyjny `crates/metastable/tests/gates.rs` (`#[test]`, nie `ignore`): uruchamia `scripts/gate-a.sh` z `RUNNER` na Rust i porównuje jego stdout z wyjściem zapisanym z Node (`golden/gate-a.txt` – dopisz do T023)
- [X] T035 [US1] Zaktualizuj `metastable-failures/README.md`: sekcja „Run (Rust)” z poleceniami z quickstart.md §3 (wersja Node zostaje do fazy Polish)

**Checkpoint**: SC-001 spełnione; `sh scripts/test.sh` zielony. MVP gotowe.

---

## Phase 4: User Story 2 – Usługa eksperymentalna jako zamiennik (Priority: P2) (plan: Faza 2)

**Goal**: obraz `chaos-svc` zastępuje obraz Node w compose (zmiana obrazu i healthcheck) i działa w klastrze mieszanym.

**Independent Test**: `docker compose -f tests/compose/ring5.yml up --exit-code-from tester` zielony: kontrakt HTTP, gossip, `/control`, reset, wszystkie profile awarii, klaster mieszany.

### Wektory referencyjne

- [X] T036 [P] [US2] `crates/chaos/tests/golden/gen/prng.mjs`: z `when-gossip-goes-silent/lib/prng.js` dla ≥20 seedów (`"42"`, `"1000"`, `"1000_gateway_delay"`, `"2000_svc-01_delay"`, `""`, `"zażółć"` itd.) po 10 000 wartości `random()` + po 1 000 `bernoulli(0.3)`, `uniform(10,100)`, `normalRandom(50,10)`, `exponential(0.5)` (każdy rozkład na świeżym PRNG) → `golden/prng.json` (f64 jako hex bitów)
- [X] T037 [P] [US2] `crates/chaos/tests/golden/gen/resilience.mjs`: podmienia `Date.now` na licznik sterowany ze skryptu; scenariusze dla `CircuitBreaker` (sekwencje sukces/porażka/upływ czasu → stan, `stateHistory`, `getStats`), `RetryPolicy` (liczba prób, opóźnienia – podmień `setTimeout` na natychmiastowe z zapisem opóźnień, przerwanie na `CircuitBreakerOpen`), `ConcurrencyLimiter` (acquire/release/updateParams/getStats), `MetricsCollector` (zapisy w różnych sekundach, okno, percentyle, `reset`) → `golden/resilience.json`
- [X] T038 [P] [US2] `crates/chaos/tests/golden/gen/strategies.mjs`: z podmienionym `Date.now` i atrapą `metricsCollector.getSnapshot()`: (a) `MASAgent` – ≥200 kroków `_tick()` na strategię z losowymi (mulberry32, seed stały) obserwacjami: snapshot lokalny, gossip sąsiadów (w tym przeterminowany), otwarte breakery → zapis `params` po każdym kroku i `decisionHistory`; (b) `CentralController._decide` dla ≥200 map snapshotów (w tym `unreachable`); (c) `BackpressureAgent._shouldThrottle` + `_recordLocalFailure/Success` dla ≥200 zdarzeń; (d) `MASAgentInbound` `recordDownstreamResult`, payloady `_gossipPush` (przechwyć `pushTo`) i `receiveGossip` przez `createGossipReceiver` (utrwala quirk Q1); (e) SWIM `pickK` + seed z `serviceId` → `golden/strategies.json`
- [X] T039 [P] [US2] `crates/chaos/tests/golden/gen/topology.mjs`: `loadTopology` dla każdego pliku w `when-gossip-goes-silent/config/topologies/` z `basePort` 4000 i 4040 (sprawdza pomijanie 4045) i `hostResolver = id`; `loadFaultProfile` dla każdego pliku w `config/faults/` → `golden/topology.json` (`portMap` jako obiekt)
- [X] T040 [P] [US2] Uruchom `sh scripts/golden.sh` i zacommituj pliki `crates/chaos/tests/golden/*.json` z T036–T039

### Testy jednostkowe (przed implementacją)

- [X] T041 [P] [US2] `crates/chaos/tests/prng.rs`: wszystkie sekwencje z `golden/prng.json` bit w bit (SC-002); przypadek `u1 = 0` w Box–Muller daje to samo co JS (`Infinity` → `round` → tak jak w JS po `max(0, …)`)
- [X] T042 [P] [US2] `crates/chaos/tests/resilience.rs`: każdy scenariusz z `golden/resilience.json`; quirk Q2 (odrzucenie przez limiter nie jest ponawiane i nie dotyka CB)
- [X] T043 [P] [US2] `crates/chaos/tests/strategies.rs`: każda sekwencja decyzji z `golden/strategies.json` (MAS, Central, Backpressure, Inbound z quirkiem Q1, SWIM); `static`/`static-conservative` – parametry z data-model.md; `STATIC_PARAMS` błędny JSON → wartości domyślne
- [X] T044 [P] [US2] `crates/chaos/tests/topology.rs`: `golden/topology.json`; brak pliku → błąd z nazwą pliku
- [X] T045 [P] [US2] `crates/chaos/tests/faults.rs`: `fault_commands(profile, opts)` dla każdego profilu daje dokładnie listy argv odpowiadające poleceniom z `lib/infrastructure-fault-injector.js` (w tym `wan-baseline` z domyślnymi 50/10 i z opcjami; `network-partition` bez `peerIps` → pusta lista; niepoprawny IP → błąd walidacji); które profile czyszczą najpierw `tc` (`baseline-flaky`, `burst-failure`, `slow-degradation`, `wan-baseline`); `recover_by_type` mapuje typy jak w JS; harmonogram `slow-degradation`: 60 kroków, opóźnienie `round(5000/60·k)`, jitter `round(0.1·d)`

### Implementacja rdzenia (`crates/chaos/src/`)

- [X] T046 [US2] `prng.rs`: port ARC4 `seedrandom@3.0.5` (mixkey UTF-16, RC4-drop[256], `prng()` z `significance = 2^52`, `overflow = 2^53`, `startdenom = 256^6`) + `bernoulli`, `uniform` (włączny, `floor`), `normal_random` (Box–Muller), `exponential` jak w `lib/prng.js`; `pub fn create_prng(seed: &str)`
- [X] T047 [US2] `metrics.rs`: `MetricsCollector` wg data-model.md; wszystkie metody przyjmują `now_ms: u64`; `snapshot()` zwraca strukturę serializowaną do kształtu z contracts/service-http.md
- [X] T048 [US2] `resilience.rs`: `CircuitBreaker`, `RetryPolicy` (asynchroniczny `execute` z `tokio::time::sleep`, rozpoznanie błędu po prefiksie `CircuitBreakerOpen`), `ConcurrencyLimiter`, `Params` + `BOUNDS` + `clamp`, `Pipeline` (CB + retry + limiter per downstream, `get_or_create(id, name, params)`), `call_downstreams(pipelines, downstreams, timeout, client)` mapujące porażki na `{error, service, degraded: true}`; komunikaty błędów bajt w bajt (research R9)
- [X] T049 [US2] `topology.rs`: `load_topology(name, base_port, config_dir)`, `load_fault_profile`, `load_experiment_matrix` wg data-model.md; `config_dir` z `CONFIG_DIR` (domyślnie `/app/config`)
- [X] T050 [US2] `strategy/mas.rs`: `MasAgent` (`observe`, `decide(obs, now_ms)`, `act`, `receive_gossip`, historia, reset), warianty SWIM (`mulberry32`, `pick_k`, fanout) i Inbound (liczniki, raporty, inflacja) wg `lib/strategies/mas-agent*.js`
- [X] T051 [P] [US2] `strategy/central.rs`: `CentralController::decide` (stan per usługa, wpis do historii) + odbiór `/control` (`apply_update`: scalanie i aktualizacja istniejących pipeline'ów) wg `lib/strategies/central.js`
- [X] T052 [P] [US2] `strategy/backpressure.rs`: `BackpressureAgent` wg `when-gossip-goes-silent/agent/backpressure-agent.js` (**nie** wersja z katalogu głównego); format `reason` z `js_to_fixed`
- [X] T053 [US2] `strategy/mod.rs`: `enum Strategy` (8 wariantów), `from_env(name, svc, seed, static_params, swim_fanout)` z błędem dla nieznanej nazwy, `call_downstreams`, `stats() -> serde_json::Value` (kształty z contracts/service-http.md), `reset()`; `None` – `Promise.all` (pierwszy błąd przerywa → 502)
- [X] T054 [US2] `faults.rs`: `fault_commands`, `FaultInjector` (wykonanie argv przez `tokio::process::Command` bez powłoki, logi `[InfraFault] Executed: …` / `Error executing "…"`, błędy połykane, zadanie `slow-degradation` anulowane przy `recover`, `blocked_ips`, `cascade-crash` → `libc::kill(getpid(), SIGKILL)`)
- [X] T055 [US2] `service.rs`: router `axum` z endpointami i kolejnością tras z contracts/service-http.md; stan współdzielony (`Arc` + `Mutex`/`RwLock`, bez trzymania blokady przez `await`); `/data` z opóźnieniem z PRNG; miękki reset (re-seed PRNG `"{seed}_{id}_delay"`, `metrics.reset`, `faults.recover`, `strategy.reset`); `resources` z `/proc/self/statm` i `getrusage`; `uptime` w sekundach f64
- [X] T056 [US2] `gossip.rs`: `push_to(client, url, from_id, summary)` (timeout 1000 ms, błędy połykane), timery agentów (`tokio::time::interval`: MAS decyzja 2000 ms + gossip 3000 ms; backpressure 1000 ms; `LeafGossipPusher` 1000 ms dla usług bez downstreamów z sąsiadami); odbiór `/gossip` jak `createGossipReceiver` (tylko 4 pola – quirk Q1)
- [X] T057 [US2] `src/bin/chaos-svc.rs`: env z contracts/cli-env.md (parsowanie liczb jak `parseInt`), walidacja `SERVICE_ID` (komunikaty jak w `runner/service-runner.js`), start serwera, log startowy, start timerów, obsługa `SIGTERM` (`tokio::signal::unix`) → zatrzymanie i `exit 0`

### Testy integracyjne

- [X] T058 [P] [US2] `crates/chaos/tests/contract.rs`: tabela przypadków dla każdego wiersza contracts/service-http.md (status, `Content-Type`, wymagane klucze i typy, kolejność tras `/faults/recover` vs `/faults/:profile`). Adres usługi z `CONTRACT_TARGET` (domyślnie uruchamia `chaos-svc` z `CONFIG_DIR` na `ring-5` i losowym `BASE_PORT`). Profile `tc`/`iptables` pomija, gdy nie ma `NET_ADMIN` (te pokrywa T061)
- [X] T059 [P] [US2] `crates/chaos/tests/gossip.rs`: 3 procesy `chaos-svc` (`mas-agent`, mała topologia testowa w `crates/chaos/tests/fixtures/topologies/line-3.json`); po ≤10 s `strategyStats.neighborCount > 0`; `/control` na `central` zmienia `currentParams`; `/experiment/reset` z tym samym seedem → identyczna sekwencja `latencyMs` dla stałego obciążenia (± jitter planisty ≤ 5 ms); błędny `SERVICE_ID` → kod ≠ 0 i lista ID na stderr
- [X] T060 [US2] Utwórz `tests/compose/ring5.yml`: `gateway` + `svc-01..05` z obrazu z `Dockerfile` (`cap_add: NET_ADMIN`, healthcheck `curl -fsS`), wariant mieszany przez profil compose `mixed` (gateway i `svc-02` z obrazu Node `when-gossip-goes-silent/Dockerfile`), usługa `tester` (obraz `tests/compose/Dockerfile.tester` ze skryptem `cluster_test.py` – bez bind mountów, więc działa także, gdy compose biegnie wewnątrz `rust-dev`); tryby `MODE=rust|mixed-a|mixed-b` uruchamia `tests/compose/ring5.sh`
- [X] T061 [US2] `tests/compose/cluster_test.py` (uruchamiany w compose): dla każdego profilu: wstrzyknięcie przez `POST /faults/:profile` → sprawdzenie przez `docker exec` (`tc qdisc show dev eth0`, `iptables -S`) → `POST /faults/recover/:type` usuwa tylko swoją warstwę (wan-baseline + partycja razem); `cascade-crash` → `docker inspect` kod 137; `/data` na gateway → 200 z wynikami ze wszystkich downstreamów; ten sam zestaw w profilu `mixed`
- [X] T062 [US2] Sprawdź quirk Q1 na danych: skrypt `when-gossip-goes-silent/scripts/check_inbound_reports.py` sumuje `strategyStats.inbound.reportsReceived` w `results-inbound/*.json` (z obsługą `None`) i wypisuje wynik; dopisz wynik do research.md R10 (potwierdzone/obalone)

**Checkpoint**: SC-002 i scenariusze 1–5 z US2 spełnione; `sh scripts/test.sh` zielony lokalnie i w CI (job `ring5` aktywny).

---

## Phase 5: User Story 3 – Orkiestracja i generatory (Priority: P3) (plan: Faza 3)

**Goal**: komórka eksperymentu i wszystkie generatory działają na Rust; skrypty macierzy używają `chaos-gen` i obrazu Rust.

**Independent Test**: jedna komórka `ring-5` × 1 seed z prawdziwym k6 daje plik zgodny z contracts/result-file.md, który `analyze_results.py` przetwarza bez błędów (SC-003).

### Wektory referencyjne i testy

- [X] T063 [P] [US3] `crates/chaos/tests/golden/gen/generators.mjs`: uruchamia w katalogu tymczasowym `generate-ring-topology.js` (z katalogu głównego), `when-gossip-goes-silent/generate-swarm-topology.js`, `generate-compose-topology.js`, `generate-k8s-manifests.js`, `scripts/generate-mesh-topology.js` dla zestawów argumentów używanych w `runner/*.sh` (odczytaj z wywołań w skryptach) i zapisuje wszystkie pliki wyjściowe → `crates/chaos/tests/golden/generators/<case>/…`
- [X] T064 [P] [US3] `crates/chaos/tests/generators.rs`: `chaos-gen` dla każdego przypadku z T063 daje pliki identyczne poza linią healthcheck (zastąpiona `["CMD","curl","-fsS","http://localhost:<port>/health"]`); test sprawdza też, że to jedyna różnica
- [X] T065 [P] [US3] `crates/chaos/tests/result.rs`: dla ≥5 historycznych plików z `when-gossip-goes-silent/results/` (różne strategie, w tym z partycją) `build_result(k6_metrics_z_pliku, …).summary` == `summary` z pliku (bez `wallClockMs`); `result_filename` dla przypadków z partycją i bez; serializacja przez `to_js_json_pretty` daje poprawny JSON z kluczami w kolejności z kontraktu
- [X] T066 [P] [US3] `crates/chaos/tests/schedule.rs`: `plan_faults(base, overlay, partition)` zwraca listę `(t_ms, akcja, usługa, url, body)` zgodną z `scheduleSingleFault` (inject w `startTimeSec`, recover w `endTimeSec` < 999, `cascade-crash` bez `endTimeSec` → 35 s, `no-wan` → brak warstwy bazowej)
- [X] T067 [P] [US3] `crates/chaos/tests/coordinator.rs`: koordynator na 3 usługach localhost (`line-3.json`), `K6_BIN` = atrapa `crates/chaos/tests/fixtures/fake-k6.sh` (kopiuje zapisane podsumowanie do `SUMMARY_FILE`), `SEEDS=1,2`: powstają 2 pliki wyników, między seedami wywołany `/experiment/reset`; scenariusz timeoutu zdrowia (`HEALTH_TIMEOUT_MS=2000`, jedna usługa nie wystartowała) → kod ≠ 0 i komunikat z nazwą usługi; `SPOF_KILL_AT_SEC` dla `central` → `config.spof.killedAtMs` ustawione

### Implementacja

- [X] T068 [US3] `crates/chaos/src/coordinator.rs`: `wait_for_healthy`, `broadcast_start`, `broadcast_reset`, `collect_metrics`, `plan_faults` + wykonanie harmonogramu (`tokio::time::sleep_until` względem `experimentStartTime`), `resolve_peer_ips` (`lookup_host`), `run_k6` (`K6_BIN`, `K6_SCRIPT`, argumenty `--env` jak w JS, odczyt i usunięcie pliku podsumowania), `build_result`, `result_filename`, pętla seedów, `CentralController` (poll `/metrics` co 2000 ms z timeoutem 1000 ms; brak odpowiedzi → `{errorRate:1, avgLatency:99999, unreachable:true}`; push `/control`) i zabicie SPOF; logi z prefiksem `[coordinator]` jak w JS
- [X] T069 [US3] `crates/chaos/src/bin/chaos-coord.rs`: env z contracts/cli-env.md, kod wyjścia 0/≠0
- [X] T070 [P] [US3] `crates/chaos/src/generate.rs` (ring) – port `generate-ring-topology.js` (katalog główny)
- [X] T071 [P] [US3] `crates/chaos/src/generate.rs` (swarm) – port `when-gossip-goes-silent/generate-swarm-topology.js`
- [X] T072 [P] [US3] `crates/chaos/src/generate.rs` (mesh) – port `when-gossip-goes-silent/scripts/generate-mesh-topology.js` (z jego generatorem losowym 1:1)
- [X] T073 [P] [US3] `crates/chaos/src/generate.rs` (compose) – port `when-gossip-goes-silent/generate-compose-topology.js` (healthcheck przez `curl`, obraz z argumentu lub domyślnie `chaos-experiment-rs:latest`)
- [X] T074 [P] [US3] `crates/chaos/src/generate.rs` (k8s) – port `when-gossip-goes-silent/generate-k8s-manifests.js` (probe przez `curl` lub `httpGet` – wybierz to, co daje najmniejszą różnicę z JS, i opisz w teście T064)
- [X] T075 [US3] `crates/chaos/src/bin/chaos-gen.rs`: podpolecenia `ring|swarm|mesh|compose|k8s` z argumentami pozycyjnymi i ścieżkami wyjściowymi jak w JS
- [X] T076 [US3] Przepnij `when-gossip-goes-silent/runner/*.sh` na wspólny `runner/impl.sh`: `IMPL=js` (domyślnie, zachowanie bez zmian) | `IMPL=rust` (obraz `chaos-experiment-rs:latest` z `Dockerfile` w katalogu głównym, `gen <compose|swarm|k8s|mesh|ring-swarm>` = `chaos-gen` uruchamiany w obrazie, `spof_meta` w Pythonie zamiast `node -e`). Logika skryptów bez zmian
- [X] T077 [US3] Test kontenerowy `tests/compose/cell-ring5.{yml,sh}` (wywoływany przez `scripts/test.sh` i CI): ring-5 Rust + `chaos-coord` z prawdziwym k6 (`SEEDS=1000`, 5+20 s, `mas-agent`, `baseline-flaky` na `tc`) → wolumen `results` → `check_cell.py` sprawdza kontrakt pliku i uruchamia **oba** `analyze_results.py` (kod 0)

**Checkpoint**: US3 scenariusze 1–3 i SC-003 spełnione.

---

## Phase 6: User Story 4 – Walidacja naukowa (Priority: P3) (plan: Faza 4)

**Goal**: raport równoważności Rust vs historyczne wyniki JS.

**Independent Test**: `compare_impls.py` na podzbiorze macierzy generuje `equivalence-report.md` z werdyktem dla każdej komórki.

- [X] T078 [P] [US4] Testy `when-gossip-goes-silent/tests/test_compare_impls.py` (stdlib `unittest`; uruchamiane w `scripts/test.sh` przez `python3 -m unittest`): syntetyczne katalogi wyników – identyczne rozkłady → „równoważne”; przesunięcie success rate o 5 pp → „nierównoważne”; p90 +20% → „nierównoważne”; odwrócony ranking strategii → wykryty; parowanie po seedzie z nazwy pliku
- [X] T079 [US4] `when-gossip-goes-silent/compare_impls.py <dir_ref> <dir_rs> [--out report.md]`: grupowanie po (topology, strategy, faultBase, faultProfile, faultPartition); dla `successRate` i `p90Ms`: średnie, różnica, 95% CI bootstrap (10 000, seed stały), Mann–Whitney U (funkcja z `analyze_results.py` – zaimportuj, nie kopiuj), delta Cliffa, werdykt (±2 pp; ±10% względem średniej referencji), ranking strategii per (profil, base); wyjście Markdown. Zależności: tylko stdlib (+ opcjonalnie scipy jak w analizatorze)
- [X] T080 [US4] `scripts/equivalence.sh`: uruchamia `run-matrix-compose.sh` (wersja Rust) dla strategii `none`, `mas-agent`, `backpressure` × profile `baseline-flaky`, `slow-degradation` × `no-wan` × `ring-50` × `SEEDS=1000..30000` (co 1000) do `when-gossip-goes-silent/results-rs/`, potem `compare_impls.py results results-rs --out specs/001-rust-rewrite/equivalence-report.md`; podłącz do `scripts/test-heavy.sh`
- [X] T081 [US4] `scripts/perf.sh`: dla obu implementacji `ring-50`: czas od `docker compose up` do zdrowia wszystkich usług, `docker stats --no-stream` RAM per usługa, czas komórki 20 seedów; wynik dopisywany do `equivalence-report.md` (SC-005, SC-006)
- [X] T082 [US4] (wykonane 2026-10-20: przebieg Rust, kontrola JS na tej maszynie, porównanie bez dławienia bramki; ADR-0026) Uruchom `scripts/test-heavy.sh` lokalnie, zacommituj `specs/001-rust-rewrite/equivalence-report.md`; dla każdej komórki „nierównoważne” dopisz wyjaśnienie lub ADR (`metastable-failures/docs/adr/00NN-*.md`)

**Checkpoint**: SC-004, SC-005, SC-006 spełnione albo każde odstępstwo udokumentowane w ADR. To warunek wejścia do fazy Polish.

---

## Phase 7: User Story 5 – Dema (Priority: P4) (plan: Faza 5)

**Goal**: wszystkie scenariusze z `demos/` działają na binarkach Rust.

**Independent Test**: `cargo test -p demos` – każdy scenariusz z ≤50 żądaniami spełnia asercje jakościowe z US5.

- [X] T083 [US5] Zinwentaryzuj `demos/` (porty, ścieżki, odpowiedzi, flagi klientów) i zapisz tabelę mapowania JS → binarka w `crates/demos/README.md` (uzupełnia listę z contracts/cli-env.md)
- [X] T084 [P] [US5] Testy jednostkowe `crates/demos/tests/logic.rs`: logika CB i retry gatewaya (`demos/chaos/gateway-retry-cb.js`), kompensacja sagi (`gateway-orders-saga.js`), statystyki klienta (`demos/client/compare.js`, `test-load.js`: percentyle, podsumowanie)
- [X] T085 [P] [US5] Testy integracyjne `crates/demos/tests/scenarios.rs`: `run_cb_comparison` (wariant „po” ma niższe p90 niż „przed”), `run_saga_comparison` (brak podwójnego obciążenia w rozwiązaniu), `saga_*`, `circuit_breaker_*` – po ≤50 żądań na losowych portach
- [X] T086 [P] [US5] `crates/demos/src/services.rs` + multi-call `src/bin/demo.rs` (`demo gateway|service-a|service-b`) – porty z `demos/gateway/index.js`, `demos/service-a/index.js`, `demos/service-b/index.js`
- [X] T087 [P] [US5] `crates/demos/src/services.rs` (`demo gateway-retry…service-b-payments`) – porty 8 plików z `demos/chaos/` (losowość przez `rand`)
- [X] T088 [P] [US5] `crates/demos/src/clients.rs` (`demo test-load|test-orders|monitor|compare|test`) – porty `demos/client/{test-load,test-orders,monitor,compare,test}.js` (format plików `demos/results/*.json` bez zmian)
- [X] T089 [US5] `crates/demos/src/scenarios.rs` (`demo starter|scenario <key>|run-all-chaos|run-cb-comparison|run-saga-comparison`) + scenariusze z `demos/scenarios/*.js` (uruchamiają binarki zamiast `node`)

**Checkpoint**: US5 scenariusze 1–2 spełnione.

---

## Phase 8: Polish & Cross-Cutting (plan: Faza 6 – wycofanie referencji)

**Warunek wejścia**: Phase 6 (US4) zakończona. Stan 2026-10-20: wszystko wykonane (ADR-0026: JS zarchiwizowany zamiast usunięty).

- [X] T090 Zapisz w `crates/chaos/tests/golden/README.md` i `crates/metastable/tests/golden/README.md` hash commita referencji JS, z którego wygenerowano wektory, i instrukcję ich odtworzenia (`git worktree add … <hash>` + `scripts/golden.sh`)
- [X] T091 Przełącz `crates/chaos/tests/contract.rs` i skrypty (`RUNNER`, `GEN`, `IMAGE`) na wartości domyślne Rust; usuń warianty JS z `tests/compose/ring5.yml` (profil `mixed`) *(2026-10-20: domyślnie Rust – `IMPL=rust`, skrypty bramek uruchamiają `../target/release/*`; testy klastra mieszanego zostają celowo, bo JS jest zarchiwizowany, ale uruchamialny)*
- [X] T092 (zmienione decyzją właściciela: **przenieś do `old-implementation/`** zamiast usuwać, ADR-0026) Usuń kod JS: `when-gossip-goes-silent/{lib,agent}/`, `runner/{coordinator,service-runner,workload-driver}.js`, `generate-*.js`, `scripts/generate-mesh-topology.js`, `package*.json`, `Dockerfile`; `metastable-failures/src/*.js`; `demos/**/*.js`, `demos/package*.json`; w katalogu głównym `backpressure-agent.js`, `generate-*-topology.js`, `package*.json`, `run-linux-*.sh`, `docker-compose-swarm.yml` – każdy po `grep` potwierdzającym brak użyć. **Zostają**: `runner/k6-script.js`, `*.py`, `runner/*.sh`, `config/`, wyniki, dokumentacja, `tests/golden/gen/*.mjs` *(2026-10-20: wykonane jako `git mv` 87 plików do `old-implementation/` z zachowaniem układu i historii, plus zamrożona kopia `config/` i `k6-script.js`)*
- [X] T093 Przegeneruj `docker-compose-*.yml` i `k8s/manifests-*.yaml` w `when-gossip-goes-silent/` przez `chaos-gen` (obraz Rust, healthcheck `curl`) *(2026-10-20: compose i k8s są generowane na żądanie przez skrypty macierzy; kopie z ery JS przeniesione do `old-implementation/`, a nowe pliki ignorowane w `.gitignore`)*
- [X] T094 [P] Zaktualizuj `metastable-failures/README.md`, dodaj `README.md` w katalogu głównym (opis workspace + quickstart) i zaktualizuj komentarze w `when-gossip-goes-silent/runner/*.sh` *(2026-10-20: `README.md` w katalogu głównym, `old-implementation/README.md`, `metastable-failures/README.md`)*
- [X] T095 [P] Oznacz ADR-0004 jako częściowo zastąpiony („Superseded in part by ADR-0024” – sekcja Node) zgodnie z ADR-0001 *(2026-10-20)*
- [X] T096 Pełna walidacja na czystym klonie: `sh scripts/test.sh` + `sh scripts/test-heavy.sh` zielone; przejdź quickstart.md §1–§8 krok po kroku i popraw rozbieżności (SC-007, SC-008, SC-009 – zmierz czas CI) *(2026-10-20: `test.sh` zielony: 62 testy cargo, 6 Python, komórka z k6, ring5 rust/mixed-a/mixed-b; SC-007 – odtworzenie przez osobę spoza projektu – nie zostało zweryfikowane)*

---

## Dependencies & Execution Order

### Zależności faz

```
Setup (1) ─► Foundational (2) ─┬─► US1 (3) ─────────────────────────────┐
                               ├─► US2 (4) ─► US3 (5) ─► US4 (6) ───────┼─► Polish (8)
                               └─► US5 (7) ─────────────────────────────┘
```

- **US1** i **US2** są niezależne (różne crate'y). Można je robić równolegle.
- **US3** wymaga US2 (protokół usługi, `topology.rs`, `strategy/central.rs`).
- **US4** wymaga US3 (koordynator + przepięte skrypty macierzy).
- **US5** wymaga tylko fazy 2. Ma najniższy priorytet.
- **Polish** wymaga US4 (zielony raport) i US5.

### Wewnątrz historyjki

Wektory referencyjne → testy (czerwone) → implementacja → testy zielone → testy integracyjne → checkpoint.

### Okazje do równoległości

- Setup: T003, T004, T005.
- Foundational: T009+T010 równolegle z T014, T015, T016.
- US1: T020–T023 (generatory wektorów) równolegle, potem T024–T026 równolegle.
- US2: T036–T039 równolegle; T041–T045 równolegle; T051 i T052 równolegle po T048; T058 i T059 równolegle.
- US3: T063–T067 równolegle; T070–T074 (generatory) równolegle.
- US5: T084–T088 równolegle.

### Przykład równoległego uruchomienia – US2

```text
Równolegle: T036 prng.mjs | T037 resilience.mjs | T038 strategies.mjs | T039 topology.mjs
Potem:      T040 (golden.sh + commit)
Równolegle: T041 prng.rs | T042 resilience.rs | T043 strategies.rs | T044 topology.rs | T045 faults.rs
Kolejno:    T046 → T047 → T048 → (T049 | T050 | T051 | T052) → T053 → T054 → T055 → T056 → T057
Równolegle: T058 contract.rs | T059 gossip.rs
Kolejno:    T060 → T061 → T062
```

---

## Implementation Strategy

### MVP (US1)

1. Phase 1 + Phase 2.
2. Phase 3 (US1) – symulator.
3. **Stop i walidacja**: `gate-a.sh` na Rust = wynik z `m1-gate-a.md`. To potwierdza toolchain, obrazy, generowanie wektorów, CI i spike R6, zanim ruszymy system rozproszony.

### Przyrostowo

1. US1 → MVP.
2. US2 → klaster mieszany działa; każdą usługę można podmienić niezależnie.
3. US3 → cała ścieżka eksperymentu na Rust.
4. US4 → raport równoważności = zgoda na wycofanie JS.
5. US5 → dema.
6. Polish → usunięcie JS.

Każdy krok to osobny PR do gałęzi roboczej z zielonym `scripts/test.sh` i CI.

---

## Notes

- Liczba zadań: 96.
- Quirki R10 (Q1, Q2) mają testy utrwalające. Ich „naprawa” jest poza zakresem tej funkcji.
- Jeśli zadanie nie da się wykonać zgodnie z opisem (np. JS zachowuje się inaczej niż w data-model.md), źródłem prawdy jest **kod JS**. Popraw data-model.md / kontrakt w tym samym commicie.
