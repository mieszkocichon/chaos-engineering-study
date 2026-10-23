# Implementation Plan: Przepisanie platformy chaos-engineering na Rust

**Branch**: `when-gossip-goes-silent` | **Date**: 2026-10-19 | **Spec**: [spec.md](spec.md)
**Input**: Feature specification from `specs/001-rust-rewrite/spec.md`

## Summary

Przenosimy cały kod JavaScript (symulator metastabilności, usługę eksperymentalną z 8 strategiami, koordynator, generatory topologii i manifestów oraz dema) do jednego workspace'u cargo z trzema crate'ami. Python (analizator), skrypty powłoki macierzy i k6 zostają bez zmian. Migracja przebiega etapami i zachowuje zgodność protokołu HTTP/JSON, plików konfiguracji i wyników. Dzięki temu na każdym etapie działa klaster mieszany JS+Rust, a referencję JS wycofujemy dopiero po raporcie równoważności. Determinizm generatorów losowych jest odtworzony bit w bit (port `seedrandom` ARC4 i mulberry32). Każdy etap kończy się zielonymi testami jednostkowymi i integracyjnymi uruchamianymi lokalnie jednym poleceniem w kontenerach. Lżejsza część tych testów chodzi też w CI. Decyzje techniczne: [research.md](research.md).

## Technical Context

**Language/Version**: Rust (edition 2024), wersja toolchainu przypięta w obrazie `rust:<x.y.z>-bookworm` (R1)
**Primary Dependencies**: `tokio`, `axum`, `reqwest` (rustls), `serde`, `serde_json`, `ryu` (formatter liczb JS), `libc` (SIGKILL), `rand` (tylko dema). Bez frameworka CLI: argumenty parsowane ręcznie jak w JS (≤10 flag na binarkę) (R3, R7, R11, R16)
**Storage**: pliki: konfiguracja JSON (`when-gossip-goes-silent/config/**`), wyniki JSON (`results*/`), wektory referencyjne testów (`crates/*/tests/golden/`)
**Testing**: `cargo test` (jednostkowe + lekkie integracyjne na localhost), `docker compose` (integracyjne kontenerowe ring-5 z `NET_ADMIN`), `#[ignore]` + skrypt dla testów ciężkich; wektory referencyjne generowane jednorazowo z kodu JS w `node:20-bookworm` (R14)
**Target Platform**: kontenery Linux (Debian bookworm) – Docker Compose / Swarm / Kubernetes; host deweloperski Windows 11 bez toolchainów (ADR-0004)
**Project Type**: workspace: biblioteki + usługi sieciowe + narzędzia CLI
**Performance Goals**: klaster 50 usług osiąga stan zdrowy nie wolniej niż referencja; komórka 20 seedów nie dłużej niż referencja (SC-005, SC-006)
**Constraints**: zgodność protokołu, env, plików konfiguracyjnych i wyników z referencją (FR-002–FR-004); ≤256 MB RAM / 0,25 CPU na usługę (limity z compose); bit-exact dla symulatora i PRNG (FR-005, FR-006); testy w CI < 15 min (SC-009)
**Scale/Scope**: ~6–7 tys. linii JS → szacunkowo 5–6 tys. linii Rust + testy; 8 strategii, 6 profili awarii, 5 generatorów, topologie do 50 usług, 1 020 historycznych wyników jako punkt odniesienia

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Projekt nie ma pliku `.specify/memory/constitution.md`. Rolę zasad pełnią zaakceptowane ADR-y w `metastable-failures/docs/adr/`:

| Zasada | Źródło | Status przed | Status po projekcie | Jak spełniona |
|---|---|---|---|---|
| Wszystko w przypiętych obrazach Docker, zero toolchainów na hoście; polecenia wieloetapowe jako `sh scripts/*.sh` | ADR-0004 | ✅ | ✅ | R1, R14; quickstart używa wyłącznie `docker` |
| Jeden seedowany PRNG na przebieg, kolejka zdarzeń (time, seq), zero zegara ściennego w logice symulatora | ADR-0014 | ✅ | ✅ | R5, R6; port 1:1 kopca i pętli zdarzeń, testy md5 z `gate-a.sh` |
| n ≥ 30 seedów, Mann–Whitney U + delta Cliffa, rozróżnienie istotności statystycznej i praktycznej | ADR-0023 | ⚠️ | ✅ (uzasadnione) | R15: strona historyczna ma n=20, więc porównanie sparowane na 20 seedach + 10 dodatkowych dla nowej implementacji. Patrz Complexity Tracking |
| Każda istotna decyzja jako ADR, decyzje zastąpione oznaczane, nie kasowane | ADR-0001 | ✅ | ✅ | R18: ADR-0024 + osobne ADR-y dla spike'a R6 i każdego odstępstwa |

**Wynik bramki**: PASS (jedno uzasadnione odstępstwo).

## Fazy realizacji

Każda faza ma **Definition of Done** wspólne z FR-017 i SC-008: wszystkie testy jednostkowe i integracyjne tej fazy **oraz wszystkich wcześniejszych** przechodzą w `sh scripts/test.sh` (lokalnie, w kontenerach). Lżejsza część przechodzi też w CI. Każda faza kończy się osobnym commitem lub PR-em do gałęzi roboczej.

### Faza 0 – Fundamenty (blokuje wszystkie kolejne)

Cel: da się zbudować i przetestować pusty workspace jednym poleceniem lokalnie i w CI.

1. Workspace `Cargo.toml` + `crates/{metastable,chaos,demos}` z pustymi bibliotekami/binarkami.
2. `Dockerfile` (multi-stage: build → `debian:bookworm-slim` + `iproute2`, `iptables`, `curl`, `procps`, k6 0.55.0), `Dockerfile.dev` (obraz `rust-dev` do testów), nazwane wolumeny cache cargo.
3. `scripts/test.sh` (unit → lekkie integracyjne → kontenerowe ring-5), `scripts/test-heavy.sh`, `scripts/golden.sh` (generacja wektorów w `node:20-bookworm`).
4. `.github/workflows/ci.yml`: joby `unit+light` i `ring5` (szkielet; ring-5 aktywny od fazy 2).
5. Wspólne narzędzia testowe: formatter `js_number` + `js_to_fixed` (R7), z testami jednostkowymi na wartościach granicznych porównanych z Node.
6. ADR-0024.

**Testy**: jednostkowe – formatter liczb (≥50 przypadków: całkowite, wykładniki ±21/−7, remisy `toFixed`); integracyjne – „smoke” (`scripts/test.sh` buduje obraz i uruchamia pustą binarkę, kod wyjścia 0).

### Faza 1 – Symulator metastabilności (US1, P1)

1. Spike R6: porównanie `f64::ln` z V8 na 10⁶ wejść → ADR z decyzją (zostaje `f64::ln` albo port fdlibm `jsmath`).
2. `mulberry32`, kopiec (time, seq), `run_trial`, `surge_schedule`, `nominal_capacity_rps` – port 1:1 z `sim.js`.
3. `ews`: `rolling_mean`, `detrend`, `rolling_var`, `rolling_ar1`, `kendall_tau`, `percentile`.
4. Binarki `run-m1`, `run-m2`, `sweep-hysteresis` z tymi samymi flagami i formatem wyjścia (`JSON.stringify(x, null, 2)`).
5. Przepięcie `metastable-failures/scripts/*.sh` z `node src/…` na binarki (wersja JS zostaje do fazy 6).

**Testy jednostkowe**: wektory mulberry32; kopiec (kolejność przy remisach czasu); `run_trial` dla ≥30 seedów × 3 konfiguracje – porównanie wszystkich wierszy telemetrii z referencją; każda funkcja `ews` na wektorach referencyjnych (w tym `null` w oknie).
**Testy integracyjne**: `gate-a.sh` na binarkach daje identyczne md5 i te same tabele co `results/m1-gate-a.md`; `run-m2` daje lead time i brak fałszywego alarmu jak `results/m2-gate-b.md`; diff tekstowy wyjścia CLI JS vs Rust = pusty (po usunięciu `wall_ms`).
**Kryterium**: SC-001.

### Faza 2 – Rdzeń usługi i strategie (US2, P2)

Kolejność wewnątrz fazy (każdy punkt z własnymi testami jednostkowymi przed następnym):

1. PRNG `seedrandom` ARC4 + rozkłady (R4) → SC-002.
2. `MetricsCollector` (kubełki 1 s × 10, percentyle `ceil(n·p)−1`, `reset`) z `now_ms` jako argumentem (R8).
3. `CircuitBreaker`, `RetryPolicy` (rozpoznawanie `CircuitBreakerOpen` po prefiksie), `ConcurrencyLimiter`, `Pipeline` (R9).
4. Loader topologii (porty z pominięciem 4045/3659/5060/5061, downstreamy bez krawędzi `gossip`, sąsiedzi dwukierunkowi, domyślny `processingDelay`) i profili awarii.
5. Decyzje strategii: `MASAgent::decide` (granice `BOUNDS`, trend p90, sąsiedzi > 0.3, wygaśnięcie gossipu 3× interwał), `CentralController::decide`, `Backpressure::should_throttle` (gossip ≤5 s, lokalne porażki 3/8 s, sonda co 5 s), inbound (okno 8 s, próg 0.3, min. 5 prób, TTL 10 s), SWIM `pick_k`, static i static-conservative.
6. `fault_commands` dla 6 profili + wykonawca bez powłoki (R11).
7. Binarka `chaos-svc`: env (FR-004), endpointy (contracts/service-http.md), timery agentów, `LeafGossipPusher`, miękki reset, SIGTERM.
8. Obraz `chaos-svc` + plik `tests/compose/ring5.yml`.

**Testy jednostkowe**: wektory referencyjne dla każdej jednostki z pkt 1–6. Dla strategii: sekwencje ≥200 obserwacji na strategię generowane z kodu JS z podmienionym `Date.now`, porównanie każdej decyzji. Przypadki brzegowe ze spec (Edge Cases): `u1=0` w Box–Muller, nieprawidłowy `STATIC_PARAMS`, `tc qdisc del` bez qdisc, gossip starszy niż 9 s, quirki Q1–Q2 (research R10).
**Testy integracyjne (localhost)**: każdy endpoint – kształt odpowiedzi i kody statusu zgodne z kontraktem (testy kontraktowe uruchamiane **także na usłudze JS**, żeby kontrakt był sprawdzony po obu stronach); gossip 3 usług; `/control` zmienia parametry pipeline'u; `/experiment/reset` przeseedowuje PRNG (te same opóźnienia po resecie z tym samym seedem); błędny `SERVICE_ID` → kod ≠ 0 i lista ID.
**Testy integracyjne (kontenery, ring-5)**: każdy profil awarii wstrzyknięty i odwołany (`tc qdisc show`, `iptables -S`); odwołanie po typie usuwa tylko swoją warstwę; `cascade-crash` → kontener kończy się kodem 137; klaster mieszany (gateway JS + usługi Rust i odwrotnie) obsługuje `/data` z sukcesem.
**Kryteria**: SC-002, US2 scenariusze 1–5.

### Faza 3 – Koordynator i generatory (US3, P3)

1. `chaos-coord`: health-wait, `/start`, harmonogram awarii (base, overlay, partycja z DNS), `CentralController` + SPOF kill, uruchomienie k6, zbiór metryk, `buildResult`, nazwy plików, miękki reset między seedami.
2. `chaos-gen` (ring, swarm, mesh, compose, k8s) – szablony 1:1, healthcheck przez `curl` (R13).
3. Przepięcie `runner/run-matrix-*.sh` na `chaos-gen` i obrazy Rust (zmiana poleceń, logika skryptów bez zmian).

**Testy jednostkowe**: `build_result` na zapisanych podsumowaniach k6 (porównanie z wynikami historycznymi tej samej komórki co do pól `summary`); `result_filename`; harmonogram awarii (kiedy inject, kiedy recover, `cascade-crash` → recover 35 s, `endTimeSec ≥ 999` = trwałe); generatory – diff z wyjściem JS dla wszystkich topologii w `config/topologies/` (jedyna różnica: healthcheck).
**Testy integracyjne**: koordynator z atrapą k6 (skrypt zwracający zapisane podsumowanie) na 3 usługach localhost; timeout zdrowia → błąd z listą usług; kontenerowo: pełna komórka ring-5 × 1 seed × 20 s z prawdziwym k6 → plik przechodzi przez `analyze_results.py` bez błędów (SC-003).
**Kryteria**: US3 scenariusze 1–3, SC-003.

### Faza 4 – Walidacja naukowa (US4, P3)

1. `compare_impls.py` (R15).
2. Uruchomienie podzbioru: strategie `none`, `mas-agent`, `backpressure` (min. 3) × profile `baseline-flaky`, `slow-degradation` × `ring-50` × seedy 1000–20000 (sparowane) + 21000–30000.
3. Pomiar SC-005/SC-006 (czas do zdrowia klastra 50 usług, RAM per usługa, czas komórki) dla obu implementacji.
4. Raport `specs/001-rust-rewrite/equivalence-report.md`: każda komórka poza progiem → wyjaśnienie lub ADR.

**Testy jednostkowe**: `compare_impls.py` na syntetycznych danych (identyczne rozkłady → równoważne; przesunięcie o 5 pp → nierównoważne; odwrócony ranking → wykryty).
**Testy integracyjne**: skrypt na 2 rzeczywistych plikach wyników na implementację → raport się generuje (ciężki przebieg jest oznaczony i uruchamiany tylko lokalnie).
**Kryteria**: SC-004, SC-005, SC-006.

### Faza 5 – Dema (US5, P4)

1. Gateway, service-a/b, warianty `chaos/*` (retry, retry+CB, orders, saga, slow, flaky, payments).
2. Klienci: `test-load`, `test-orders`, `monitor`, `compare`; runnery scenariuszy `run_all_chaos`, `run_cb_comparison`, `run_saga_comparison` i skrypty `saga_*`/`circuit_breaker_*`.

**Testy jednostkowe**: logika CB i sagi w gateway, statystyki klienta (percentyle, porównanie).
**Testy integracyjne**: każdy scenariusz z ≤50 żądaniami na localhost. Asercje jakościowe z US5 (CB obniża opóźnienia, saga nie obciąża podwójnie).

### Faza 6 – Wycofanie referencji (po zielonym raporcie z fazy 4)

1. Usunięcie kodu JS, `package*.json`, `node_modules` z `.dockerignore` itd.; usunięcie duplikatów w katalogu głównym (`backpressure-agent.js`, `generate-*-topology.js`, `run-linux-*.sh`, root `docker-compose-swarm.yml`) po potwierdzeniu, że nie są używane.
2. Skrypty generacji wektorów (`tests/golden/gen/*.mjs`) **zostają** jako dokumentacja pochodzenia danych (uruchamiane z checkoutu commita referencji, zapisanego w README katalogu golden).
3. Aktualizacja README obu podprojektów, ADR-0004 → „Superseded by 0024” w części dotyczącej Node.
4. Testy kontraktowe uruchamiane dotąd także na JS → tylko Rust.

**Testy**: pełny `scripts/test.sh` + `scripts/test-heavy.sh` na czystym checkoucie; SC-007 (osoba spoza projektu odtwarza bramki A/B i komórkę z README).

### Kolejność i równoległość

```
Faza 0 ─► Faza 1 ─────────────────────────────┐
      └─► Faza 2 ─► Faza 3 ─► Faza 4 ─────────┼─► Faza 6
                          └─► Faza 5 ─────────┘
```

Faza 1 i Faza 2 są niezależne (różne crate'y) i mogą iść równolegle. Faza 5 zależy tylko od Fazy 0, ale ma najniższy priorytet.

## Project Structure

### Documentation (this feature)

```text
specs/001-rust-rewrite/
├── spec.md
├── plan.md              # ten plik
├── research.md          # decyzje R1–R18
├── data-model.md        # encje i reguły walidacji
├── quickstart.md        # jak zbudować, przetestować, zwalidować
├── contracts/
│   ├── service-http.md  # endpointy usługi
│   ├── cli-env.md       # zmienne env i CLI binarek
│   └── result-file.md   # schemat pliku wyników
├── checklists/requirements.md
└── equivalence-report.md   # powstaje w fazie 4
```

### Source Code (repository root)

```text
Cargo.toml                      # workspace
Dockerfile                      # obraz runtime chaos-svc / chaos-coord
Dockerfile.dev                  # obraz deweloperski rust-dev (toolchain + klient Dockera + tc/iptables)
crates/
├── metastable/
│   ├── src/{lib.rs, sim.rs, ews.rs, js.rs, args.rs, jsmath.rs?}   # js: formatter liczb JS (używany też przez chaos); jsmath tylko jeśli spike R6 tego wymaga
│   ├── src/bin/{run-m1.rs, run-m2.rs, sweep-hysteresis.rs}
│   └── tests/{golden/, cli.rs}
├── chaos/
│   ├── src/
│   │   ├── lib.rs
│   │   ├── prng.rs            # seedrandom ARC4 + rozkłady
│   │   ├── metrics.rs
│   │   ├── resilience.rs      # CircuitBreaker, RetryPolicy, ConcurrencyLimiter, Pipeline
│   │   ├── topology.rs        # loader topologii, profili, macierzy
│   │   ├── faults.rs          # fault_commands + wykonawca
│   │   ├── strategy/{mod.rs, mas.rs, central.rs, backpressure.rs}
│   │   ├── service.rs         # router axum, miękki reset, gossip
│   │   ├── coordinator.rs
│   │   └── gen/{ring.rs, swarm.rs, mesh.rs, compose.rs, k8s.rs}
│   ├── src/bin/{chaos-svc.rs, chaos-coord.rs, chaos-gen.rs}
│   └── tests/{golden/, golden/gen/*.mjs, contract.rs, gossip.rs, coordinator.rs}
└── demos/
    ├── src/bin/…              # gateway, service-a, service-b, chaos-*, client-*, run-*
    └── tests/scenarios.rs
scripts/{test.sh, test-heavy.sh, golden.sh}
tests/compose/ring5.yml         # integracja kontenerowa (NET_ADMIN, klaster mieszany)
.github/workflows/ci.yml
when-gossip-goes-silent/        # config/, results*/, runner/*.sh, k6-script.js, *.py – zostają
metastable-failures/            # docs/, notes/, results/, scripts/ – zostają (scripts przepięte)
```

**Structure Decision**: jeden workspace w katalogu głównym, trzy crate'y odpowiadające trzem domenom (R2). `chaos` zależy od `metastable` (lekki, bez async) w zakresie `js` i `mulberry32`. Konfiguracja, wyniki, skrypty powłoki i Python zostają w obecnych miejscach, więc ścieżki w skryptach i montowania wolumenów `/app/config` się nie zmieniają.

## Ryzyka i środki zaradcze

| Ryzyko | P-stwo | Wpływ | Środek |
|---|---|---|---|
| `f64::ln`/`cos` ≠ V8 w ostatnim bicie | średnie | łamie FR-006 | spike R6 na początku fazy 1; port fdlibm (~150 linii) |
| Inna charakterystyka czasowa (wielowątkowy runtime) przesuwa wyniki gossip | średnie | SC-004 nie przechodzi | porównanie sparowane; diagnostyka z `current_thread`; ADR przy uzasadnionej różnicy |
| Ukryte zależności od formatu tekstu (logi, liczby w JSON) | średnie | psuje skrypty / analizator | R7, R17; analizator na plikach Rust w testach fazy 3 |
| Zachowania niezamierzone w JS (R10) poprawione „przy okazji” | niskie | nieporównywalne wyniki | testy utrwalające quirki; poprawki dopiero po fazie 6 |
| Testy kontenerowe z `NET_ADMIN` niestabilne w CI | niskie | fałszywe czerwone | w CI tylko ring-5 i krótkie przebiegi; retry całego joba tylko ręcznie |

## Complexity Tracking

| Odstępstwo | Dlaczego potrzebne | Dlaczego prostsza alternatywa odrzucona |
|---|---|---|
| Porównanie równoważności na n=20 seedach historycznych zamiast n≥30 (ADR-0023) | Historyczna macierz ma 20 seedów na komórkę; dogenerowanie 10 seedów w JS oznacza kolejne godziny klastra referencyjnego | Ponowne uruchomienie całej referencji jest droższe niż porównanie sparowane na 20 seedach + 10 dodatkowych dla Rust. Jeśli wynik jest graniczny, dogenerowujemy seedy referencji tylko dla tych komórek |
| Własne porty `seedrandom` ARC4 i formatera liczb JS zamiast gotowych crate'ów | FR-005, SC-003 wymagają identycznych sekwencji i tekstu | Gotowe crate'y (`rand`, domyślny `serde_json`) dają inne sekwencje i inny tekst |
