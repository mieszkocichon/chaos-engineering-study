# Labbook – przepisanie na Rust (specs/001-rust-rewrite)

Dziennik prac implementacyjnych. Najnowsze wpisy na dole. Każdy wpis: data, co zrobiono, wynik testów, decyzje i problemy.

## 2026-10-19

### Start implementacji
- Checklist `checklists/requirements.md`: 16/16 ✓ – brak blokad.
- Docker Desktop nie działał – uruchomiony.
- Na hoście są `cargo` i `node`, mimo że ADR-0004 mówi o hoście bez toolchainów. Trzymamy się ADR-0004: oficjalne przebiegi testów idą w kontenerach.
- Commit dokumentów spec/plan/tasks przed kodem.

### Docker Desktop – awaria startu
- Pierwszy start Docker Desktop 4.76.0 zgłosił błąd: `initializing Inference manager: … remove …\Docker\run\dockerInference: The file cannot be accessed by the system`. Okno błędu zostało zamknięte (Quit).
- Po kilku minutach demon wstał sam (Engine 29.5.2). Na hoście nic nie zmieniałem. Gdyby błąd wrócił, przyczyną jest pozostały plik gniazda w `%LOCALAPPDATA%\Docker\run\`.

### Faza 1 – Setup (T001–T007) ✅
- Toolchain: **Rust 1.97.1** (najnowszy stabilny zainstalowany lokalnie, z 2026-07-14). Ta sama wersja w `rust-toolchain.toml`, `Dockerfile`, `Dockerfile.dev` i CI. Pierwotnie wpisałem 1.89.0, ale rustup miał uszkodzony wpis tej wersji („Missing manifest”), więc zmieniłem na 1.97.1.
- Workspace: `crates/{metastable,chaos,demos}`; `chaos` zależy od `metastable` (`js`, `mulberry32`).
- `Dockerfile.dev` → obraz `rust-dev` (rust:1.97.1-bookworm + docker CLI 27.3.1 + compose v2.29.7 + iproute2/iptables/curl/python3).
- `scripts/dev.sh` – nowy wrapper hosta: `sh scripts/dev.sh <cmd>` uruchamia komendę w `rust-dev` z repo i cache (wolumeny `cargo-registry`, `rust-target`). Używa `pwd -W` + `MSYS_NO_PATHCONV=1` dla Git Bash na Windows.

### Faza 2 – Foundational (T008–T016) ✅
- `metastable::js`: `js_number`, `js_to_fixed`, `to_js_json_pretty`. Wektory z Node: 56 wartości × (toString, toFixed 0/1/2/4) – **wszystkie zgodne za pierwszym razem**.
  - `toFixed` przy remisie: wykrywamy dokładny remis na pełnym rozwinięciu dziesiętnym (1100 cyfr) i inkrementujemy łańcuch cyfr. Arytmetyka f64 (`a + 0.005`) mogłaby zaokrąglić w złą stronę.
- `scripts/golden.sh` (node:20-bookworm), `scripts/test.sh`, `scripts/test-heavy.sh`, `.github/workflows/ci.yml`, ADR-0024.
- Wynik: `sh scripts/dev.sh sh scripts/test.sh` → **all green**.

### Faza 3 – US1, część 1: symulator i EWS (T017–T022, T024–T025, T027–T028) ✅
- Wektory z Node (node:20-bookworm, ~1,5 min): `mulberry32` (6 seedów × 10⁴), `sim` (3 konfiguracje × 30 seedów, skróty SHA-256 wierszy + pełne wiersze dla seeda 1), `ews` (5 serii × okna 5/25), `cli` (33 wywołania CLI), `ln_cos` (spike R6).
- **Spike R6 – wynik ważny:** `f64::ln` (glibc) ≠ V8 `Math.log` w ostatnim bicie dla ~10% wejść, `cos` dla kilku %. Co ciekawe, 90 przebiegów symulatora i tak było identycznych: zliczenia per tick nie reagują na przesunięcie czasu o 1 ulp. To jednak szczęście, nie gwarancja.
  → **ADR-0025**: port `log` i `cos` z V8 (fdlibm) w `metastable::jsmath`. Po porcie: 10⁶/10⁶ zgodnych dla ln, cos, sqrt.
- `sim.rs`: port 1:1 (kopiec (time, seq), arena prób zamiast obiektów współdzielonych przez zdarzenia, quirk „done nigdy nieustawiane” zachowany). Wszystkie 90 przebiegów bit w bit.
- `ews.rs`: wszystkie funkcje bit w bit (w tym `**2` → `powi(2)` – zgodne).
- Pomocnicze: `js_round` (Math.round: połówki w stronę +∞), `js_fixed_num` (`Number(x.toFixed(n))`), `js_to_number`, parser flag `args.rs`.

### Faza 3 – US1, część 2: CLI i bramki (T023, T026, T029–T035) ✅ → **US1 / MVP gotowe**
- `run-m1`, `run-m2`, `sweep-hysteresis`: **wszystkie 33 nagrane wywołania JS dają wyjście identyczne bajt w bajt**, łącznie z wykresami ASCII (`▁▂▃▄▅▆▇█`), `--json` z 300 wierszami telemetrii i `null`-ami w werdykcie M2.
  - Pułapki JS odtworzone celowo: `null > x` w pętli alarmu M2 porównuje `0 > x`; `Number(true)` = 1 dla flagi bez wartości; `x/100 | 0`; `Math.round` dla połówek.
- Skrypty `gate-a.sh`, `explore-cliff.sh` (`M1=…`) i `find-loop.sh` (`SWEEP=…`) działają teraz na obu implementacjach (domyślnie JS).
- **Problem znaleziony po drodze:** skrypty `.sh` w working copy miały CRLF (git `autocrlf=true` na Windows), więc `sh` w kontenerze wywalał `set: Illegal option -`. Naprawa: `.gitattributes` (`* text=auto eol=lf`) + konwersja edytowanych skryptów. Inne `.sh` w repo mogą mieć ten sam problem w working copy (w repozytorium są OK, problem dotyczy tylko checkoutu na Windows).
- Test `gates.rs`: `gate-a.sh` z `M1=<binarka Rust>` daje identyczny stdout (md5 determinizmu, próg, seedy 1–5) jak z Node → **SC-001 spełnione**.
- `[profile.dev.package.metastable] opt-level = 3` → testy CLI z 43 s spadły do kilku sekund w trybie debug.
- `sh scripts/dev.sh sh scripts/test.sh` → **all green**.

### Faza 4 – US2, część 1: wektory, PRNG, prymitywy odporności (T036–T042, T046–T047) ✅
- Wektory z JS (scripts/golden.sh): `prng.json` (20 seedów w tym `""`, `"zażółć gęślą jaźń"`, emoji, 300×`a`; sekwencje skracane SHA-256 + prefiks), `resilience.json` (CB 20×200 kroków z zegarem sterowanym, retry 28 scenariuszy, limiter 300 kroków, metryki 5×400 kroków), `strategies.json` (MAS 8×200 ticków, Central 200, Backpressure 400, Inbound 300 z przechwyconym `pushTo`, SWIM 5 konfiguracji × 20 rund), `topology.json` (wszystkie topologie/profile).
- `chaos::prng` – port `seedrandom@3.0.5` ARC4: **wszystkie 20 seedów × (10⁴ random + 4 rozkłady) bit w bit** → **SC-002 spełnione**. Box–Muller używa `metastable::jsmath` (ADR-0025).
- `chaos::metrics`, `chaos::resilience` (CB, retry, limiter, BOUNDS) – zgodne z wektorami.
- **Pułapka znaleziona przez testy:** `serde_json` domyślnie parsuje liczby zmiennoprzecinkowe *niedokładnie* (np. `0.36363636363636365` → o 1 ulp obok). Włączona funkcja `float_roundtrip` w workspace. Ma to znaczenie także dla parsowania podsumowań k6 i configów w koordynatorze.
- Retry: czas mierzony w testach na `tokio::time::pause` – opóźnienia 1:1 z JS.

### Faza 4 – US2, część 2: topologie i logika strategii (T043–T044, T049–T052) ✅
- `chaos::topology`: loader zgodny z JS dla wszystkich topologii (ring-5/50, scale-20/50, mesh-30/50), łącznie z pomijaniem portów 4045/3659/5060/5061 i kolejnością sąsiadów.
- `chaos::strategy::{pipeline, mas, central, backpressure}`: czysta logika decyzyjna, zegar w argumentach. **Wszystkie sekwencje decyzji identyczne z JS**: MAS 8×200 ticków (params, wpis historii, parametry breakerów po `_act`), Central 200, Backpressure 400 (w tym teksty `reason`, np. `errorRate=75.0%>30.000000000000004%`, bo w JS `0.3*100` daje taki float), Inbound 300, SWIM 5×20 rund.
- **Quirk Q1 potwierdzony na wektorach:** przez `createGossipReceiver` pole `inboundReport` ginie, a `reportsReceived` rośnie tylko przy bezpośrednim wywołaniu `receiveGossip`. Odtworzone 1:1 (`GossipIn.inbound_report = None` z HTTP).
- Odtworzony też detal JS: `selfInflations` pojawia się w statystykach Inbound dopiero po pierwszej inflacji (`(x || 0) + 1`).

### Faza 4 – US2, część 3: awarie, runtime strategii, usługa HTTP (T045, T048, T053–T059) ✅
- `chaos::faults`: polecenia `tc`/`iptables` 1:1 z JS jako argv (bez powłoki). `peerIps` walidowane jako `IpAddr`: wejście `10.0.0.1; rm -rf /` jest odrzucane, a w JS trafiłoby do `exec`. Rampa slow-degradation co 1 s jako zadanie tokio przerywane przez `recover`. Testy w trybie nagrywania, bez NET_ADMIN.
- `chaos::strategy` (runtime): wspólny `call_pipelined` (limiter → retry → CB → fetch). Pipeline'y współdzielone (`Arc<Mutex<_>>`) jak obiekty JS, więc żądanie w locie przeżywa miękki reset. Błędy fetch w brzmieniu Node (`This operation was aborted`, `fetch failed`, `HTTP <code>: <body>`), bo trafiają do `downstreamResults` i do ciał 502.
- `chaos::service` (axum) + binarka `chaos-svc`: wszystkie endpointy z kontraktu; nowe opcjonalne zmienne env `CONFIG_DIR` i `HOST_OVERRIDE` (tylko testy na localhost; dopisane do contracts/cli-env.md).
- Testy integracyjne na prawdziwych procesach (localhost, losowe porty):
  - `contract.rs`: kształt i kolejność kluczy każdej odpowiedzi, pierwszeństwo `/faults/recover` przed `/faults/:profile`, 502 dla `none` przy martwych downstreamach, degradacja dla `static`. Ten sam test działa na JS po ustawieniu `CONTRACT_TARGET_LEAF/GATEWAY`.
  - `gossip.rs` (fixture `line-3`): leaf gossip → MAS gateway (`neighborCount` = 2), `/control` aktualizuje parametry i istniejące pipeline'y w `central`, **miękki reset przeseedowuje PRNG** (opóźnienia = `normal(50,10)` z `"7000_svc-01_delay"`, identycznie w dwóch rundach), walidacja startu (`SERVICE_ID`, nieznana strategia, zły `STATIC_PARAMS`).
- `sh scripts/dev.sh sh scripts/test.sh` → **all green**. Zostały testy kontenerowe ring-5 (T060–T062).

### Faza 4 – US2, część 4: testy kontenerowe ring-5 (T060–T062) ✅ → **US2 gotowe**
- `tests/compose/ring5.yml` + `ring5.sh`: 6 usług z NET_ADMIN, tester jako osobny obraz (Python + docker CLI). Nie ma bind mountów, więc całość działa także z wnętrza `rust-dev` przez socket Dockera. Tryby:
  - `rust`: każdy profil na prawdziwym `eth0` (`tc qdisc show`, `iptables -S`), WAN + partycja na gateway z odwołaniem tylko partycji (WAN zostaje), odrzucenie `peerIps` z próbą wstrzyknięcia polecenia, reset czyści awarie;
  - `mixed-a` (Rust gateway mas-agent + JS liście) i `mixed-b` (JS gateway mas-agent + Rust liście): `/data` bez degradacji, ten sam kształt JSON, gossip dochodzi przez granicę implementacji.
  - Wszystko **PASS**. Pełny `test.sh` (unit + localhost + 3 tryby) trwa ok. 1,5 min.
- **Odkrycie 1 (Q4, ważne dla wyników badań):** `cascade-crash` **nie zabija usługi ani w JS, ani w Rust**. Proces jest PID 1 w kontenerze, a jądro ignoruje SIGKILL wysłany przez init do samego siebie. Sprawdzone wprost na obrazie `chaos-experiment` (Node): po `cascade-crash` kontener działa i odpowiada. Wniosek: w historycznej macierzy profil `cascade-crash` najpewniej nie wywoływał awarii. **Wymaga decyzji właściciela projektu.** Rust odtwarza to zachowanie celowo, dla porównywalności.
- **Odkrycie 2 (Q5):** pierwsze polecenie `slow-degradation` (`delay 0ms 10ms distribution normal`) jest odrzucane przez `tc`, więc przez pierwszą sekundę nie ma opóźnienia. Identycznie w JS.
- Problem po drodze: przy `burst-failure` (80% strat na `eth0`) ginęła odpowiedź HTTP na samo żądanie sterujące awarią. Rozwiązanie: tester steruje awariami przez `docker exec … curl localhost` (`lo` nie jest objęty awarią).
- T062: w danych historycznych **nie ma przebiegów `mas-agent-inbound`** (`results-inbound/` zawiera tylko `mas-agent`), więc Q1 jest potwierdzony tylko na kodzie (wektory referencyjne).

### Faza 5 – US3, część 1: generatory (T063–T064, T070–T075) ✅
- `chaos::generate` + binarka `chaos-gen ring|swarm|compose|k8s|mesh` z tymi samymi argumentami, zmiennymi env (`IMAGE`, `REGISTRY`, `RESULTS_DIR_HOST/CONTAINER`, `STATIC_PARAMS`, `SWIM_FANOUT`, `SPOF_KILL_AT_SEC`) i plikami wyjściowymi co JS.
- Test na 16 przypadkach (argumenty wzięte ze skryptów `runner/*.sh`): **pliki identyczne z JS** po zastosowaniu wyłącznie zamierzonych zamian (polecenie kontenera, healthcheck `curl`, obraz `-rs`). Zgadzają się też: zbiór zapisanych plików, utworzone katalogi wyników i stdout. Mesh odtwarza losowe krawędzie gossip (mulberry32, seed 424242) 1:1.
- **Line endings:** 1374 śledzone pliki miały w working copy CRLF (checkout przed `.gitattributes`). Niezmienione pliki wyewidencjonowałem ponownie z LF; zawartość w repozytorium się nie zmieniła. 18 plików ma CRLF zapisane w samym repozytorium (m.in. 5 profili awarii, `package.json`) i zostały bez zmian. Generator k8s osadza surowy tekst profilu, więc CR trafia do manifestu identycznie w JS i Rust.
- Pułapka: `gen` jest słowem zarezerwowanym w Rust 2024, stąd moduł `generate`.

### Faza 5 – US3, część 2: koordynator, skrypty, komórka z k6 (T065–T069, T076–T077) ✅ → **US3 gotowe**
- `chaos::coordinator` + `chaos-coord`: health-wait, `/start`, harmonogram awarii (base/overlay/partycja z DNS), `CentralController` (z przetestowanego `CentralDecider`) + zabicie SPOF, k6 jako proces, zbiór metryk, plik wyników. Nowe opcjonalne env tylko do testów: `K6_BIN`, `K6_SCRIPT`, `HOST_OVERRIDE`.
- **`build_result` + `to_js_json_pretty` odtwarzają historyczne pliki wyników bajt w bajt** (6 plików: none/mas-agent/backpressure+partycja/central/static, results-spof, results-swim), łącznie z `k6Metrics` i `serviceMetrics`. To najmocniejszy dowód zgodności formatu (SC-003 po stronie formatu). Starsze pliki nie mają `config.spof` (dodane później w JS); test to uwzględnia.
- Harmonogram awarii: testy na regułach JS (`endTimeSec ≥ 999` = trwała, `cascade-crash` bez/0 `endTimeSec` → recover po 35 s, nieznana usługa pomijana).
- `coordinator.rs` (localhost + atrapa k6): 2 seedy → 2 pliki, miękki reset, kolejność kluczy, sprzątanie podsumowania k6; timeout zdrowia z nazwą usługi; SPOF `central` zapisuje `killedAtMs`.
- **Komórka z prawdziwym k6** (`tests/compose/cell-ring5.sh`, ~2,5 min z budowaniem): ring-5 Rust, `baseline-flaky` na `tc`, k6 5+20 s → plik → **oba `analyze_results.py` kończą się kodem 0** (SR=100%, p90=997 ms, 499 żądań).
- Skrypty macierzy: wspólny `runner/impl.sh` z `IMPL=js` (domyślnie, bez zmian) | `IMPL=rust` (chaos-gen w obrazie; `spof_meta` w Pythonie zamiast `node -e`). Dodatkowo port starszego generatora `generate-swarm-topology.js` z katalogu głównego jako `chaos-gen ring-swarm` (19/19 przypadków zgodnych).
- **Błąd zastany w repozytorium:** `runner/run-matrix-k8s.sh` w ogóle się nie parsował (`bash -n`). Dwie linie były ucięte od znaku `#`: `if [[ ${#missing[@]} -gt 0 ]]; then` i `TOTAL=$(( ${#STRATEGIES[@]} * ${#FAULTS[@]} ))`. Naprawione; teraz wszystkie `.sh` w repo przechodzą `bash -n`.
- Główne `run-linux-*.sh` są w kodowaniu cp1250 i wołają `node` – zostają do fazy Polish.

## 2026-10-20

### Faza 6 – US4: narzędzia walidacji (T078–T081) ✅, przebieg T082 ⏸ czeka na decyzję
- `when-gossip-goes-silent/compare_impls.py`: komórki (topologia, strategia, base, profil, partycja); dla SR i p90: średnie, różnica, 95% CI bootstrap (10⁴ prób, stały seed), Mann–Whitney U (importowany z `runner/analyze_results.py`, nie kopiowany), delta Cliffa, porównanie rankingu strategii. Testy (`tests/test_compare_impls.py`, 6 przypadków) uruchamia `scripts/test.sh`.
- `runner/run-matrix-compose.sh`: niełamiące nadpisania `RESULTS_DIR`, `MATRIX_STRATEGIES/BASES/FAULTS/PARTITION_STRATEGIES`. Domyślnie macierz bez zmian. Bez tego przebieg Rust nadpisałby `results/`.
- `scripts/equivalence.sh` (podzbiór: none/mas-agent/backpressure × baseline-flaky/slow-degradation × no-wan × ring-50 × seedy 1000–30000, `IMPL=rust`, `RESULTS_DIR=./results-rs`) i `scripts/perf.sh` (czas do zdrowia klastra 51 kontenerów + RSS na usługę, JS vs Rust). Oba uruchamia się z powłoki hosta, nie z `rust-dev`, bo bind mounty macierzy muszą wskazywać ścieżki hosta.
- **Odkrycie 3 – kryterium SC-004 jest statystycznie nieosiągalne.** Rozrzut w danych historycznych (51 komórek × 20 seedów): odchylenie SR między seedami ma medianę **0,116** (p10 0,045, p90 0,160), a współczynnik zmienności p90 ma medianę 0,168. Przy n=20 (JS) i n=30 (Rust) 95% CI różnicy średnich SR ma szerokość ok. **±6,6 pp**, więc „CI w ±2 pp” wymagałoby ok. 260 seedów na stronę. Test kontrolny `compare_impls.py results results` (te same dane po obu stronach) daje: 0 różnych, **49/51 nierozstrzygniętych**.
  → Werdykt w narzędziu jest trójstanowy: *equivalent* (CI w marginesie), *different* (CI wyklucza 0 i |Δ| > margines), *inconclusive*. Kod wyjścia 1 tylko przy *different* lub odwróconym rankingu.
  → **Decyzja dla właściciela projektu:** (a) poszerzyć margines do skali rozrzutu (np. ±7 pp SR, ±15% p90) i przyjąć „brak istotnej różnicy + zgodny ranking”; (b) zwiększyć n (koszt: godziny × ~13); (c) porównać rozkłady testem dwóch prób bez marginesu. SC-004 w spec.md oznaczony ⚠.
- T082 (sam przebieg) **nie został uruchomiony**: kilka godzin klastra 51 kontenerów na tej maszynie, a kryterium wymaga najpierw decyzji.

### Faza 7 – US5: dema (T083–T089) ✅
- Jedna binarka `demo <polecenie>` zamiast 19 osobnych (mniej kodu; mapowanie 1:1 w `crates/demos/README.md`): 11 usług (axum), 5 klientów (test-load, test-orders, monitor, compare, test), startery i runnery scenariuszy. Runnery same uruchamiają trzy usługi, bez pośredniego procesu startowego.
- Testy jednostkowe (`tests/logic.rs`): breaker z gateway-retry-cb (open → half-open → closed, licznik tylko kolejnych porażek), percentyle i histogram klientów, karta spójności sagi, werdykty `compare`; **historyczne raporty JS z `demos/results` porównują się w Rust z tym samym werdyktem**.
- Testy scenariuszy (`tests/scenarios.rs`, prawdziwe procesy na losowych portach, ≤50 żądań): CB kontra burza retry (przed: SR < 100%, po: SR = 100% dzięki odpowiedziom zdegradowanym, werdykt IMPROVED); saga (przed: osierocone zamówienia, po: 0 osieroconych, 0 duplikatów, anulowane = nieudane); double charge (0 sukcesów, każde zamówienie utworzone dwa razy).
- Zachowania JS zachowane i opisane w README: `service-b-payments` ignoruje `Idempotency-Key` (czyli „rozwiązanie” double charge nie chroni przed podwójnym obciążeniem), `gateway` czyta `SERVICE_TIMEOUT_MS`, ale go nie używa.

### Polish – część możliwa przed decyzją (T090) + poprawka testów kontenerowych
- T090: `crates/*/tests/golden/README.md` – pochodzenie wektorów (commit referencji JS `ef78fe86…`, node:20, seedrandom 3.0.5) i sposób odtworzenia po usunięciu JS.
- **Błąd w moich testach, znaleziony i naprawiony:** w `cell-ring5.sh` flaga `--abort-on-container-exit` zatrzymywała compose po wyjściu koordynatora, *zanim* ruszył checker, a `--exit-code-from checker` zwracało wtedy 0. Uruchomiony z `rust-dev` test dawał więc fałszywy sukces; z hosta działał przez szczęśliwy timing. Dodatkowo pętla w `test.sh` (`[ -f "$t" ] && sh "$t"`) połykała błędy skryptów przy `set -e`. Teraz `up -d` (z `--wait` dla ring5) + `run --rm --no-deps <tester|coordinator|checker>` na pierwszym planie, a pętla bez `&&`.
- Pełny przebieg `sh scripts/dev.sh sh scripts/test.sh`: **exit 0** – 62 testy cargo, 6 testów Python, komórka z k6 (oba analizatory: exit 0), ring5 rust/mixed-a/mixed-b: PASS.
- `quickstart.md` zaktualizowany do faktycznego stanu (`scripts/dev.sh`, `IMPL=js|rust`, binarka `demo`, uruchamianie macierzy z powłoki hosta).

### Stan na koniec sesji
- Zrobione: Setup, Foundational, US1, US2, US3, US4 (narzędzia), US5, T090. **89/96 zadań** (otwarte: T082, T091–T096).
- Czeka na decyzję właściciela: (1) kryterium SC-004 → potem przebieg T082 (godziny klastra 51 kontenerów); (2) wycofanie JS T091–T096 dopiero po T082; (3) co z odkryciem Q4 (`cascade-crash` nie zabijał usług w historycznych eksperymentach).

## 2026-10-20 (cd.) – decyzja SC-004 i przebieg równoważności T082

### Krok 1 – decyzja właściciela: SC-004 zrewidowane
- Decyzja: marginesy w skali rozrzutu między seedami, **±7 pp dla success rate i ±15% średniej p90**. Kryterium zaliczenia: żadna komórka nie ma metryki *różnej* (95% CI wyklucza 0 **i** |Δ| > margines) oraz ranking strategii jest zgodny z historycznym. Komórki *nierozstrzygnięte* są dopuszczalne i raportowane.
- Zmienione: `spec.md` (SC-004 zamiast oznaczenia ⚠), `research.md` R15, `compare_impls.py` (`SR_MARGIN = 0.07`, `P90_REL_MARGIN = 0.15`).
- Test `compare_impls.py`: przesunięcie 5 pp mieści się teraz w marginesie, więc test używa 10 pp. 6/6 OK.
- Kontrola na danych historycznych (`results` kontra `results`): 0 różnych, **28/51 nierozstrzygniętych** (wcześniej 49/51), 0 odwróceń rankingu.

### Krok 2 – test dymny ścieżki macierzy na hoście (Windows, Git Bash)
- Pierwsza próba: `IMPL=rust` + `gen` → `docker run -w /w` zostało przepisane przez MSYS na `W:/` („working directory 'W:/' is invalid”). Poprawka w `runner/impl.sh`: `pwd -W` + `MSYS_NO_PATHCONV=1`.
- Druga próba: ring-5, `mas-agent`, `no-wan`, `baseline-flaky`, seed 1000 → **komórka zakończona w 1 min 33 s, 1 plik wyników**.
- Efekt uboczny, który dotyczy też JS: skrypt macierzy nadpisuje śledzony `docker-compose-local-w1.yml` (projekt `w<worker>`). Przywróciłem plik (`git checkout`) i dodałem `PROJECT_PREFIX` (domyślnie `w`). `equivalence.sh` używa `rs` i sprząta `docker-compose-local-rs*.yml` (dopisane do `.gitignore`).
- Zasoby Docker Desktop: 16 CPU, 16 GB RAM – wystarczy na ring-50 (51 kontenerów) z `WORKERS=1`.

### Krok 3 – przebieg T082 uruchomiony
- `bash scripts/equivalence.sh` z powłoki hosta, start **2026-10-20T07:33Z**: `IMPL=rust`, ring-50, `none`/`mas-agent`/`backpressure` × `baseline-flaky`/`slow-degradation` × `no-wan`, seedy 1000–30000 (30, z czego 20 sparowanych z historią), `WORKERS=1`, wyniki → `when-gossip-goes-silent/results-rs/`. Log: `/tmp/equiv.log`. Szacunek: ~45 min na komórkę, ~4,5 h razem.

### Krok 4 – obserwacja w trakcie przebiegu (ok. 08:05Z, komórka 1/6)
- Po 22/30 seedach komórki `none`/`baseline-flaky`: **Rust SR ≈ 0,92, p90 ≈ 1,4 s**, a historia JS daje **SR 0,59, p90 10,8 s**. Rozbieżność o wiele większa niż margines.
- Weryfikacja na żywym klastrze: awaria jest aktywna (`tc qdisc show`: `loss 40%` na svc-01/03/05, svc-02 bez qdisc). Bramka Rust zużywa ok. **18% CPU** (limit w compose: 0,25 CPU) i 86 MiB.
- Hipoteza: bramka Node przy limicie 0,25 CPU i 50 wywołaniach downstream na żądanie (20 RPS → ok. 1000 wywołań/s) była dławiona przez CPU (CFS throttling), co dawało długie ogony opóźnień i timeouty. Wydajniejsza bramka Rust mieści się w limicie. Jeśli to prawda, różnica wynika z wydajności środowiska, a nie z logiki (logika jest potwierdzona bit w bit na wektorach).
- Dodatkowy czynnik: historia pochodzi z innego hosta (skrypty wspominają NixOS), a ten przebieg idzie na Docker Desktop/Windows.
- **Plan kontroli:** po przebiegu Rust uruchomić **JS na tej samej maszynie** (ten sam podzbiór, 5 seedów na komórkę, `IMPL=js`, `RESULTS_DIR=./results-js-local`, `PROJECT_PREFIX=js`). Porównanie Rust kontra JS-lokalnie oddzieli efekt implementacji od efektu maszyny. Do tego pomiar CPU throttlingu bramki JS (`docker stats` + `cpu.stat`).
- Throttling bramki Rust (próbka 20 s, `cpu.stat`, limit `25000 100000` = 0,25 CPU): +207 okresów CFS, z czego +41 throttlowanych (~20%), +6 s czasu throttlingu. Rust też dobija do limitu, ale przy znacznie lepszych wynikach.
- Przygotowany `scripts/control-js.sh` (JS na tej maszynie, ten sam podzbiór, 5 seedów na komórkę, próbkowanie `cpu.stat` bramki JS co 60 s). Uruchomię go po zakończeniu przebiegu Rust, żeby uniknąć konkurencji o CPU.

### Krok 5 – stan pośredni i przejęcie nadzoru (09:10Z)
- 70/180 przebiegów Rust. `none`/`baseline-flaky`: Rust SR 0,933 / p90 1309 ms, historia JS 0,591 / 10814 ms; `none`/`slow-degradation`: Rust 0,557 / 3063 ms, JS 0,223 / 12618 ms.
- Nadzór nad przebiegiem przejęła na chwilę sesja-odgałęzienie (fork tej rozmowy).
- Uwaga: na tym samym Dockerze działa niezwiązany stos Supabase (11 kontenerów), który dokłada trochę szumu CPU do pomiarów.

### Krok 6 – wynik przebiegu T082 (Rust, ta maszyna) – 07:33Z → 11:36Z
- 180/180 eksperymentów, 14 584 s (≈ 4 h 3 min), 0 timeoutów, 0 zniknięć koordynatora. Wyniki: `when-gossip-goes-silent/results-rs/`, raport: `specs/001-rust-rewrite/equivalence-report.md`.
- Werdykt według SC-004: **6/6 komórek „różne”** (SR i p90; p < 0,001, |δ Cliffa| 0,81–1,00), **2 odwrócenia rankingu → SC-004 NIE spełnione.**

| komórka | SR JS (historia) → Rust | p90 JS → Rust |
|---|---|---|
| none / baseline-flaky | 0,591 → 0,933 | 10,8 s → 1,3 s |
| none / slow-degradation | 0,223 → 0,557 | 12,6 s → 3,1 s |
| mas-agent / baseline-flaky | 0,649 → 1,000 | 11,3 s → 1,2 s |
| mas-agent / slow-degradation | 0,582 → 1,000 | 12,7 s → 4,8 s |
| backpressure / baseline-flaky | 0,535 → 1,000 | 13,0 s → 1,3 s |
| backpressure / slow-degradation | 0,548 → 1,000 | 13,5 s → 3,1 s |

- Odczyt: Rust jest wyraźnie lepszy wszędzie. Główny wniosek historyczny („strategie odporności > none”) zachodzi w obu implementacjach. Oba „odwrócenia rankingu” to remisy `mas-agent` = `backpressure` = 1,000 w Rust, a nie odwrócenie znaczącej różnicy. Różnica między `mas-agent` a `backpressure` (w JS 0,65 wobec 0,54) w Rust znika.

### Krok 7 – przebieg kontrolny JS: kolizja dwóch sesji (11:36–11:55Z)
- Obie sesje Claude (pierwotna i odgałęzienie) uruchomiły jednocześnie `scripts/control-js.sh` z tym samym `PROJECT_PREFIX=js`, więc `docker compose down`/`up` jednej kasowały klaster `js1` drugiej. Objawy: „No such container”, „coordinator disappeared”, konflikt nazwy `/js1-svc-38-1`. Docker Desktop działał poprawnie.
- Jedyna próbka `cpu.stat` bramki JS z tego okresu (11:37:53, rozruch klastra, bez ruchu k6): **188/309 okresów CFS throttlowanych (61%)**. Rust pod pełnym obciążeniem: ok. 20%.
- Sprzątanie: odgałęzienie się wycofało, pierwotna sesja zatrzymała wszystkie procesy macierzy, usunęła kontenery i sieć `js1` i uruchomiła brakujące 5 komórek **sekwencyjnie, w jednym zadaniu** (`/tmp/control2.log`, od 11:55:51Z). Logi wcześniejszych prób: `/tmp/control.log`, `/tmp/control-js.log`, `/tmp/control-js-attempt1.log`.
- `results-js-local/` zachowuje kompletną komórkę `none`/`baseline-flaky` (5 seedów, 11:41–11:47Z).
- Wniosek na przyszłość: przebiegi macierzy trzeba serializować także między sesjami (np. plik blokady w `scripts/`), bo nazwa projektu compose jest wspólna.

### Krok 8 – pierwszy wynik kontrolny: różnica wynika z implementacji
- `none`/`baseline-flaky`, **JS na tej maszynie**, 5 seedów: SR 0,56 / 0,62 / 0,63 / 0,60 / 0,67 (średnio ≈ 0,62), p90 7,9–14,0 s (średnio ≈ 10,5 s). Historia JS (inny host): 0,59 i 10,8 s. **Rust (ta maszyna): 0,93 i 1,3 s.**
  → JS na tej maszynie odtwarza historię, a Rust w tych samych warunkach daje wyraźnie lepsze wyniki. **Różnica wynika z implementacji, a nie z maszyny.**

### Krok 9 – kontrola zakończona (11:55Z → 12:35Z), wniosek
- 30/30 przebiegów JS na tej maszynie (`results-js-local/`, 6 komórek × 5 seedów), bez błędów.

| komórka | JS historia SR / p90 | JS ta maszyna SR / p90 | Rust ta maszyna SR / p90 |
|---|---|---|---|
| none / baseline-flaky | 0,591 / 10,8 s | 0,617 / 10,5 s | 0,933 / 1,3 s |
| none / slow-degradation | 0,223 / 12,6 s | 0,213 / 13,4 s | 0,557 / 3,1 s |
| mas-agent / baseline-flaky | 0,649 / 11,3 s | 0,651 / 12,7 s | 1,000 / 1,2 s |
| mas-agent / slow-degradation | 0,582 / 12,6 s | 0,717 / 11,9 s | 1,000 / 4,8 s |
| backpressure / baseline-flaky | 0,535 / 13,0 s | 0,664 / 12,5 s | 1,000 / 1,3 s |
| backpressure / slow-degradation | 0,548 / 13,5 s | 0,676 / 13,3 s | 1,000 / 3,1 s |

- **Throttling bramki** (`cpu.stat`, limit 0,25 CPU): JS ≈ **94–100%** okresów CFS przydławionych pod obciążeniem (np. 967/1031, 1576/1640), Rust ≈ 20%.
- `compare_impls.py`: historia JS kontra JS lokalnie → 2 komórki „różne” (backpressure SR +13 pp), 4 nierozstrzygnięte, 1 odwrócenie rankingu; JS lokalnie kontra Rust → 6/6 „różne”. Raport rozszerzony o sekcję kontrolną (`equivalence-report.md`).
- **Wniosek:**
  1. Różnica Rust kontra JS wynika z wydajności implementacji pod limitem 0,25 CPU bramki. Bramka Node jest dławiona niemal stale, co produkuje timeouty i ogony opóźnień. Logika strategii jest zgodna bit w bit.
  2. Historyczne wyniki macierzy w dużej mierze mierzą to dławienie CPU bramki Node, a nie tylko strategie odporności. To odkrycie dotyczy interpretacji badań niezależnie od przepisania.
  3. SC-004 jest za czułe nawet dla JS kontra JS przy n=5 (odwrócenia rankingu między `mas-agent` i `backpressure` to szum).
- **Decyzja potrzebna od właściciela** – patrz wiadomość w rozmowie.

### Krok 10 – decyzja: porównanie JS kontra Rust bez dławienia bramki
- Decyzja właściciela: powtórzyć porównanie JS kontra Rust z podniesionym limitem CPU bramki, po 5 seedów na komórkę.
- `runner/run-matrix-compose.sh`: opcjonalne `GATEWAY_CPUS`, które podmienia w wygenerowanym compose tylko limit bramki (`cpus: '0.25'` → wartość). Sprawdzone: zmienia się dokładnie jedna usługa (`gateway`), liście zostają przy 0,20. Bez zmiennej konfiguracja jest taka jak w publikacji.
- `scripts/unthrottled.sh`: najpierw JS, potem Rust, **ściśle po kolei**, z blokadą `.matrix.lock` (żaden drugi przebieg macierzy nie wystartuje równolegle – lekcja z kroku 7). `GATEWAY_CPUS=4`, ten sam podzbiór (none/mas-agent/backpressure × baseline-flaky/slow-degradation × no-wan × ring-50), seedy 1000–5000. Co 60 s próbkuje `cpu.stat` bramki i svc-01. Wyniki: `results-js-cpu4/`, `results-rs-cpu4/`, log `/tmp/unthrottled.log`.
- 12:44Z start (JS). Kontrola po 3 min: bramka JS `cpu.max 400000 100000` (4 CPU), **0 throttlowanych okresów** (1665 okresów; wcześniej ~96%). Liść svc-01 (Node, limit 0,20) ma ~12% throttlowanych okresów, głównie przy rozruchu – odnotowane, poza zakresem tej zmiany.

### Krok 11 – wynik porównania bez dławienia bramki (12:44Z → 14:13Z)
- JS: 30/30 (12:44–13:30Z), Rust: 30/30 (13:30–14:13Z), bez błędów; `results-js-cpu4/`, `results-rs-cpu4/`.
- Throttling: bramka **JS 0/4210, Rust 0/3870** okresów (wcześniej JS ≈ 96%). Liść svc-01: JS 466/3967 (≈12%), Rust 57/2338 (≈2,4%).

| komórka | SR JS → Rust | p90 JS → Rust | werdykt |
|---|---|---|---|
| none / baseline-flaky | 0,962 → 0,965 | 1,43 s → 1,16 s | nierozstrzygnięte (SR równoważne) |
| none / slow-degradation | 0,478 → 0,552 | 3,07 s → 3,06 s | **różne** (SR +7,4 pp, tuż ponad 7 pp) |
| mas-agent / baseline-flaky | 1,000 → 1,000 | 1,46 s → 1,03 s | **różne** (p90 −0,43 s) |
| mas-agent / slow-degradation | 0,995 → 1,000 | 4,65 s → 4,91 s | nierozstrzygnięte (SR równoważne) |
| backpressure / baseline-flaky | 1,000 → 1,000 | 1,38 s → 1,16 s | nierozstrzygnięte (SR równoważne) |
| backpressure / slow-degradation | 1,000 → 1,000 | 3,06 s → 3,06 s | **równoważne** |

- Podsumowanie `compare_impls.py`: 6 komórek, 2 z metryką „różną”, 3 nierozstrzygnięte, **0 odwróceń rankingu**.
- **Wniosek:** bez dławienia bramki implementacje się zbiegają. Success rate jest równoważny w 5/6 komórek, ranking się zgadza, a p90 różni się najwyżej o ok. 0,4 s (Rust szybszy). Luki 34–45 pp przy 0,25 CPU to **artefakt dławienia CPU bramki Node**, a nie logiki portu. Pozostałe małe różnice pasują do resztkowego dławienia liści Node (≈12% wobec ≈2%): przy slow-degradation o sukcesie decyduje opóźnienie w pobliżu granicy timeoutu 3 s.
- Raport `equivalence-report.md` uzupełniony o sekcję „Unthrottled gateway”.

### Krok 12 – decyzja: równoważność przyjęta, JS archiwizowany w `old-implementation/`
- Decyzja właściciela: uznać równoważność i przejść do wycofania JS, ale **przenieść** poprzednią implementację do osobnego folderu zamiast ją usuwać.
- ADR-0026 (`metastable-failures/docs/adr/0026-accept-rust-equivalence-archive-js.md`): port Rust uznany za równoważny, różnice przy 0,25 CPU to efekt dławienia Node. SC-004 zmienione na porównanie bez dławienia bramki (spełnione). JS archiwizowany w `old-implementation/`, domyślnie `IMPL=rust`.
- `spec.md` (SC-004, FR-001), `tasks.md` (T082 [X], T092 = przeniesienie zamiast usunięcia). Przy okazji usunięty z repozytorium przypadkowo zacommitowany `tests/__pycache__/*.pyc`.

### Krok 13 – archiwizacja JS w `old-implementation/` (T091–T096) ✅
- `git mv` 87 plików (historia zachowana), układ jak w oryginale: `old-implementation/when-gossip-goes-silent/{lib,agent,runner/*.js,generate-*.js,scripts/,Dockerfile,package*.json,docker-compose-*.yml,k8s/}`, `old-implementation/metastable-failures/src/`, `old-implementation/demos/` (bez `demos/results`, które zostają), oraz starsze pliki z katalogu głównego (`*.js`, `run-linux-*.sh`, `docker-compose-swarm.yml`, `config/`, `package*.json`).
- Archiwum jest samodzielne: zamrożona kopia `config/` i `runner/k6-script.js` w `old-implementation/when-gossip-goes-silent/`; obraz `chaos-experiment:latest` buduje się z tego katalogu.
- Przepięte miejsca: wektory referencyjne (`gen/*.mjs`, `scripts/golden.sh`), `runner/impl.sh` (domyślnie `IMPL=rust`; `IMPL=js` = archiwum; `gen mesh|ring-swarm|ring` w JS wyłączone z komunikatem, bo te generatory zapisują pliki obok siebie), `run-matrix-k8s.sh` (wspólna funkcja `gen k8s` zamiast `docker run` z montowaniem, które przesłoniłoby generator w obrazie), `tests/compose/ring5.yml` (obraz JS z archiwum), skrypty bramek metastable (domyślnie binarki Rust).
- Dokumentacja: `README.md` (nowy, w katalogu głównym), `old-implementation/README.md`, `metastable-failures/README.md`, `quickstart.md`, README wektorów; ADR-0004 oznaczony jako częściowo zastąpiony (ADR-0024/0026). `.gitignore`: generowane `docker-compose-*.yml` i `k8s/`.
- Weryfikacja:
  - `scripts/golden.sh` z archiwum JS → wszystkie wektory bez zmian (poza ścieżką katalogu tymczasowego w stdout generatora mesh, którą test i tak pomija);
  - `sh scripts/dev.sh sh scripts/test.sh` → **exit 0**: 62 testy cargo, 6 Python, komórka z k6 + oba analizatory, ring5 rust/mixed-a/mixed-b PASS (mieszane z obrazem JS zbudowanym z archiwum);
  - host: `gen compose` domyślnie → `chaos-experiment-rs:latest`, z `IMPL=js` → `chaos-experiment:latest`.
- **Stan: 96/96 zadań.** Nie zweryfikowane: SC-007 (odtworzenie przez osobę spoza projektu).

## 2026-10-23

### Przegląd pod kątem rozszerzalności (Open/Closed) i pierwszy prompt dla innego AI
- Miejsca zamknięte na rozszerzanie, w kolejności opłacalności:
  1. `strategy/mod.rs`: `enum Strategy` z 9 metodami, każda z `match` po 8 wariantach – nowa strategia = edycja 9 miejsc.
  2. `faults.rs`: profile awarii jako `match` po nazwach w trzech funkcjach (`fault_commands`, `clears_tc_first`, `recover_scope`).
  3. Uruchamianie: 9 skryptów `runner/*.sh` (~1240 linii) z zakodowanymi tablicami; `config/experiments/full-matrix.json` istnieje, ale nic go nie czyta.
  4. `generate.rs`: YAML jako szablony tekstowe, wartości na sztywno (RPS, czasy, limity CPU – patrz `GATEWAY_CPUS` jako łatka).
  5. Parametry decyzyjne MAS/backpressure i `BOUNDS` jako stałe w kodzie; k6 jako jedyny generator obciążenia.
  6. Quirki Q1/Q4/Q5 odtworzone dla zgodności – po archiwizacji JS do decyzji: naprawić za flagą zgodności.
- Napisany prompt nr 1: `docs/ai-prompts/01-strategy-plugins.md` (strategie jako trait + rejestr, zero zmian zachowania, dowód na nowej strategii `retry-only`).


### Refaktoryzacja Prompt 01 – strategie odporności jako wtyczki (Open/Closed) ✅
- **Trait `ResilienceStrategy`**: zaprojektowany w `crates/chaos/src/strategy/mod.rs` z metodami odpowiadającymi punktom styku (`call_downstreams`, `has_gossip_receiver`, `receive_gossip`, `apply_update`, `stats`, `reset`, `start`).
- **Async w traicie**: `BoxFuture<'a, Result<Vec<Value>, String>>` (`Pin<Box<dyn Future + Send>>`), ponieważ dynamic dispatch (`dyn ResilienceStrategy`) wymaga bezpieczeństwa obiektowego (object safety), a natywne `async fn` w traitach nie są jeszcze bezpieczne obiektowo bez zewnętrznych makr/narzędzi.
- **Rejestr fabryk**: `create_strategy` w `strategy/mod.rs` odwzorowuje nazwę z `STRATEGY` na konstruktor (`match` w jednym miejscu; nieznana nazwa daje ten sam błąd co wcześniej).
- **Przeniesienie strategii do modułów**:
  - `none.rs` (`NoneStrategy`)
  - `static.rs` (`StaticStrategy`)
  - `static_conservative.rs` (`StaticConservativeStrategy`)
  - `central.rs` (`CentralStrategy` obok `CentralDecider`)
  - `mas_agent.rs` (`MasAgentStrategy`)
  - `mas_swim.rs` (`MasSwimStrategy`)
  - `mas_inbound.rs` (`MasInboundStrategy`)
  - `backpressure.rs` (`BackpressureStrategy` obok `BackpressureAgent`)
  - Wspólny kod: `fetch_json`, `call_pipelined`, `format_pipelined_results`, `parse_gossip_in`, `spawn_every` współdzielone w `strategy/mod.rs`.
- **Wtyczka dowodowa `retry-only`**: nowy plik `crates/chaos/src/strategy/retry_only.rs` + jedna linia w `create_strategy` w `mod.rs`. Wykonuje retry z backoffem bez circuit breakera i limitera. Dodany test jednostkowy `stats_shape_and_reset`, test integracyjny `retry_only_strategy_degrades_and_reports_stats` w `contract.rs`, wpis w `specs/001-rust-rewrite/contracts/cli-env.md`.
- **Brak zmian w serwisie**: `service.rs` i `chaos-svc.rs` nie wymagały ani jednej zmiany.
- **Brak innych `match` po strategiach**: `git grep "match " crates/chaos/src/` potwierdza brak `match` po strategiach poza fabryką.
- **ADR**: spisany `metastable-failures/docs/adr/0027-resilience-strategy-trait-plugin.md` i zaktualizowany indeks ADR.
- **Wyniki testów**: `sh scripts/dev.sh sh scripts/test.sh` → **[test.sh] all green** (wszystkie testy jednostkowe cargo i wektory golden, testy python, komórka z k6 + analizatory, ring-5 w trybach rust, mixed-a, mixed-b).

### Przegląd refaktoryzacji Prompt 01 i poprawki (2026-10-23)
- Przegląd commitów `9c46d7e`, `64c41dd`, `52bcd06` (trait `ResilienceStrategy`, `retry-only`, ADR-0027): testy zielone, cel osiągnięty, ale 4 uwagi. Wszystkie poprawione:
  1. **Regresja zgodności z JS:** `central` i trzy strategie MAS czytały timeout raz przed pierwszą próbą zamiast przy każdej (JS: `_fetch` czyta `params.timeoutMs` za każdym razem). Dodałem test `tests/timeout_per_attempt.rs` (downstream, który nigdy nie odpowiada; `/control` zmienia timeout w trakcie 1. próby). Przed poprawką: 6,2 s (stary timeout), po: ~4,2 s. Commit `a279ac1`.
  2. **Trzy kopie MAS** (`mas_agent.rs`, `mas_swim.rs`, `mas_inbound.rs`, ~310 linii) → jeden `mas_strategy.rs`: generyczny `MasStrategy<V: MasVariant>` + warianty `Plain`, `Swim`, `InboundReports`. Wariant decyduje tylko o odbiorcach gossipu, dodatkowych statystykach i księgowaniu. Kolejność kluczy w `strategyStats` i kolejność blokad (agent → stan wariantu) zachowane.
  3. **Zbędna warstwa API:** struktura `Strategy` z 7 metodami przekazującymi i nieużywanymi `new/from_arc/inner` zastąpiona aliasem `pub type Strategy = dyn ResilienceStrategy`; `chaos-svc` woła `create_strategy` bezpośrednio. `service.rs` bez zmian.
  4. `static_conservative.rs` (czysta delegacja) zastąpiony jedną linią w rejestrze; importy w `backpressure.rs` przeniesione na początek; usunięte ostrzeżenie kompilatora w `crates/demos/tests/scenarios.rs`.
- Wynik: `sh scripts/dev.sh sh scripts/test.sh` → **all green**, 65 testów cargo (+1 test regresji), testy Python, komórka z k6, ring-5 rust/mixed-a/mixed-b; kompilacja bez ostrzeżeń. Moduł `strategy/` ma 1545 linii (wcześniej ok. 1700).

### Prompt 02 – profile awarii jako wtyczki
- Napisany `docs/ai-prompts/02-fault-plugins.md`. Wiedza o rodzajach awarii jest dziś w 4 `match` w `faults.rs` (`clears_tc_first`, `fault_commands`, `recover_scope`, `inject`) i w 2 miejscach w `coordinator.rs` (35 s dla `cascade-crash`, DNS `peerServices` tylko dla `FAULT_PARTITION`). Parametry (`loss 40%/80%`, rampa) są na sztywno.
- Cel: trait + rejestr, niełamiąca parametryzacja przez `options`, dowód: nowy rodzaj `packet-corruption` sprawdzony end to end w ring-5.
- Do promptu dopisane lekcje z przeglądu Prompt 01: najpierw test, który pada; nie zamrażać zmiennego stanu; nie kopiować wariantów; bez warstw tylko przekazujących wywołania.

### Refaktoryzacja Prompt 02 – wtyczki awarii (Open/Closed) ✅ (2026-10-23)
- **Trait `FaultKind`**: zaprojektowany w `crates/chaos/src/faults/mod.rs` z metodami: `name`, `layer` (`FaultLayer::Tc`, `Iptables`, `Process`), `commands(opts) -> Result<Vec<Argv>, String>`, opcjonalną rampą `ramp(opts)`, `default_recover_after_sec()`, `requires_peer_resolution()` oraz domyślną implementacją `execute(injector, opts)`.
- **Wydzielenie do modułów**:
  - `packet_loss.rs`: `PacketLossFault` – współdzielona implementacja dla `baseline-flaky` (40%) i `burst-failure` (80%), z walidacją `lossPct` (zasada „nie kopiuj wariantów”).
  - `wan_baseline.rs`: `WanBaselineFault` (`delayMs`, `jitterMs` z walidacją nieliczbowości i wartości ujemnych).
  - `slow_degradation.rs`: `SlowDegradationFault` (rampa przez `FaultRamp`, opcje `maxDelayMs`, `durationMs`, `stepMs`, `initialDelayMs`, `initialJitterMs`).
  - `network_partition.rs`: `NetworkPartitionFault` (warstwa `Iptables`, walidacja `peerIps`, rejestracja zablokowanych IP).
  - `cascade_crash.rs`: `CascadeCrashFault` (warstwa `Process`, `default_recover_after_sec = Some(35.0)`, SIGKILL / recording).
  - `packet_corruption.rs`: `PacketCorruptionFault` – proof-of-extensibility (`tc netem corrupt <pct>%`, domyślnie 5%).
- **Rejestr**: centralna funkcja `find_fault_kind(name: &str) -> Option<&'static dyn FaultKind>`.
- **Czyszczenie koordynatora i injectora**:
  - `FaultInjector::inject` oraz `coordinator.rs` (`plan_faults`, `apply_profile`) nie zawierają żadnych nazw awarii.
  - Rozwiązywanie DNS `peerServices` w koordynatorze działa generycznie dla każdego profilu, którego awaria wymaga rezolucji lub posiada pole `peerServices`.
- **Testy i weryfikacja**:
  - Test jednostkowy w `crates/chaos/tests/faults.rs` sprawdzający parametryzację i nowy profil `packet-corruption`.
  - Przykładowy profil `when-gossip-goes-silent/config/faults/packet-corruption.json`.
  - Krok w `tests/compose/cluster_test.py` weryfikujący `packet-corruption` w klastrze ring-5 (`tc qdisc show` / `corrupt 5%` oraz type-scoped recovery).
  - Zaktualizowany kontrakt HTTP (`specs/001-rust-rewrite/contracts/service-http.md`) i research R11 (`specs/001-rust-rewrite/research.md`).
  - Spisany ADR `metastable-failures/docs/adr/0028-fault-kind-trait-plugin.md`.
- **Wynik**: `sh scripts/dev.sh sh scripts/test.sh` → **[test.sh] all green**, brak ostrzeżeń kompilacji, ring-5 rust/mixed-a/mixed-b pass.

### Przegląd refaktoryzacji Prompt 02 i poprawki (2026-10-23)
- Przegląd commitów `cb9661d`, `589a923`, `519da53` (trait `FaultKind`, `packet-corruption`, ADR-0028): `test.sh` zielony, cel osiągnięty, bez regresji. Poprawki:
  1. `network_partition.rs`: `execute()` budował polecenia `iptables` drugi raz obok `commands()`; teraz używa `commands()` (jedno źródło argv).
  2. `packet_loss.rs`: literówka w aliasie importu (`metastabile_js`) – zwykły import `metastable::js::js_number`.
  3. `coordinator.rs`: DNS dla `peerServices` uruchamiany tylko gdy `requires_peer_resolution()` (wcześniej także przy samej obecności klucza, co czyniło metodę traitu zbędną). Dla istniejących profili wynik bez zmian.
- Wynik: `test.sh` → all green (65 testów cargo, Python, komórka z k6, ring-5 rust/mixed-a/mixed-b; partycja: reguły DROP dla obu peerów).

### Prompt 03 – `chaos-run`: eksperyment opisany plikiem
- Napisany `docs/ai-prompts/03-chaos-run-cli.md`. Problem: ~1240 linii bash w 9 skryptach `runner/*.sh` z zaszytymi tablicami, parametry w `generate.rs`, nieczytany `full-matrix.json`, problemy z Windows i brak blokady wyłączności (kolizja z kroku 7), brak zapisu konfiguracji obok wyników.
- Cel: schemat pliku eksperymentu, binarka `chaos-run` (`--dry-run`, `--only`, `--resume`, blokada), kopia konfiguracji obok wyników, migracja 7 skryptów do plików z testem „te same komórki i parametry”.
- Dopisana lekcja z przeglądu Prompt 02: jedno źródło prawdy (bez liczenia tego samego w dwóch miejscach) i obowiązkowy pełny `test.sh` na końcu.

### Implementacja Prompt 03 – `chaos-run` CLI i schemat eksperymentu ✅ (2026-10-23)
- **Moduł `experiment` w `crates/chaos/src/experiment/`**:
  - `schema.rs`: Deserializacja JSON schematu eksperymentu, w pełni zgodna wstecz z `full-matrix.json`. Obsługa topologii, strategii, baz sieciowych (`faultBases`), profili awarii (`faultProfiles`), komórek partycji i niestandardowych (`extraCells`), wariantów parametrów strategii (`variants`), obciążenia k6 (`workload`: `rps`, `durationSec`, `warmupSec`), zasobów (`resources`: `gatewayCpus`), seedów (`seeds` lub `seedBase` + `repetitions`), katalogu wyników (`resultsDir`), workerów (`workers`) i implementacji (`impl`: `rust`|`js`).
  - `validate.rs`: Pre-flight walidacja sprawdzająca istnienie plików topologii, poprawność zarejestrowanych strategii i awarii oraz spójność parametrów przed uruchomieniem klastra.
  - `cell.rs`: Rozwijanie iloczynu kartezjańskiego komórek z uwzględnieniem `variants` i `extraCells`. Metoda `is_cell_complete` sprawdzająca kompletność plików `.json` w katalogu wyników pod kątem opcji `--resume`.
  - `lock.rs`: Atomowa blokada wyłączności `.chaos-run.lock` chroniąca przed równoległym uruchomieniem dwóch sesji eksperymentalnych w tym samym katalogu wyników.
  - `runner.rs`: Zarządzanie cyklem życia klastrów Docker Compose (równolegli workerzy, kolejka komórek), monitoring statusu koordynatora, logowanie postępu (1 linia na ukończoną komórkę) oraz automatyczny zapis manifestu powtarzalności `<nazwa>.manifest.json` (git commit, obraz, timestamp, konfiguracja).
  - `bin/chaos-run.rs`: Nowa binarka CLI z flagami `--dry-run`, `--only <pattern>`, `--resume`, `--workers <n>`, `--impl <rust|js>`, `--results-dir <dir>`, `--topology <name>`, `--seeds <s1,...>`.
- **Generowanie Compose bez `sed`**:
  - `crates/chaos/src/generate.rs` (`ComposeOpts`) rozbudowane o `gateway_cpus`, `rps`, `duration_sec`, `warmup_sec` oraz `config_dir_host`. Gdy opcje nie są podane, generowany plik zachowuje 100% zgodność bajt w bajt ze złotymi wektorami generatorów JS (`tests/generators.rs`).
- **Rozwiązanie problemu Docker-in-Docker / ścieżek na Windows**:
  - Dodano funkcję `resolve_host_path(&Path)`, która tłumaczy ścieżki wewnątrz kontenera (`/app/...`) na ścieżki hosta Windows/Linux przy użyciu `HOST_APP_PATH` przekazywanego w `scripts/dev.sh` (oraz fallbacku z `docker inspect $HOSTNAME`). Umożliwiło to poprawne montowanie wolumenów `./config` i `./results` w kontenerach tworzonych przez `docker compose` uruchamiany wewnątrz kontenera deweloperskiego.
- **Migracja konfiguracji eksperymentów**:
  - Zdefiniowano 10 plików w `when-gossip-goes-silent/config/experiments/`:
    `matrix-compose.json` (51 komórek, 1020 eksperymentów), `matrix-inbound.json` (2 komórki), `matrix-sensitivity.json` (4 komórki), `matrix-spof.json` (3 komórki), `matrix-swim.json` (3 komórki), `matrix-swarm.json` (20 komórek), `spof-test.json` (5 komórek), `equivalence.json` (6 komórek, 30 seedów), `control-js.json` (6 komórek), `unthrottled-cpu4.json` (6 komórek) oraz mały dymny `test-ring5.json` (1 komórka, 1 seed).
  - Skrypty `when-gossip-goes-silent/runner/run-matrix-*.sh` oraz `run-spof-test.sh` zastąpiono cienkimi wrapperami delegującymi do `chaos-run`.
  - `runner/run-matrix-k8s.sh` pozostawiono jako skrypt powłoki (eksperymenty Kubernetes wymagają wdrożenia na zdalny/żywy klaster przez `kubectl` i Helm).
- **Testy i weryfikacja**:
  - 12 testów jednostkowych w `crates/chaos/tests/experiment.rs` sprawdzających dokładność generowanych komórek dla wszystkich 7 starych skryptów, działanie blokady, resume oraz walidacji pre-flight.
  - Nowy test integracyjny `tests/compose/chaos-run-test.sh` uruchamiający rzeczywisty eksperyment na klastrze ring-5 przez `chaos-run` i weryfikujący poprawność wyników przez `analyze_results.py`. Test włączony do `scripts/test.sh`.
  - Zaktualizowano `specs/001-rust-rewrite/contracts/cli-env.md`, `specs/001-rust-rewrite/quickstart.md`, główny `README.md`.
  - Spisano decyzję architektoniczną `metastable-failures/docs/adr/0029-chaos-run-experiment-cli.md`.
- **Wynik**: `sh scripts/dev.sh sh scripts/test.sh` → **[test.sh] all green** (wszystkie testy cargo, python, k6, compose, chaos-run, cross-impl bez ostrzeżeń).
- **Stan repozytorium**: Zmiany pozostawione w working tree bez commitowania, zgodnie z instrukcją użytkownika ("ale nic nie commituj na razie").

### Przegląd Prompt 03 i poprawki `chaos-run` (2026-10-23)
Implementacja z poprzedniego wpisu miała `test.sh` zielony, ale nie spełniała promptu (testy sprawdzały tylko liczby komórek). Znaleziska i poprawki:
1. **Kolizja wyników wariantów** (sensitivity, spof): warianty `static` / `central` pisały do tych samych nazw plików, a `--resume` uznałby je za ukończone. Komórka ma teraz `variant`; po wariancie pliki dostają `__<etykieta>__` (jak w starych skryptach), a `--resume` i sprawdzenie po przebiegu uwzględniają etykietę.
2. **`matrix-swarm.json` i `spof-test.json` były innymi eksperymentami** (Compose zamiast Swarm, RPS 20/warmup 15 zamiast 50/10; brak zabijania `svc-03`/koordynatora i metadanych `spofTest`; `spofTarget` nieużywane). Pliki usunięte, skrypty `run-matrix-swarm.sh` i `run-spof-test.sh` przywrócone z HEAD; powód w README i ADR-0029. Do rozstrzygnięcia: czy dodać do `chaos-run` backend Swarm i hook akcji w trakcie przebiegu.
3. **`matrix-swim.json`**: `SWIM_FANOUT=3` było tylko dla `mas-agent-swim`, stary skrypt eksportował je dla wszystkich trzech. Teraz `strategies` + globalne `swimFanout` (bez wariantów, więc bez zbędnych sufiksów).
4. **Manifest** `.manifest.json` leżał w katalogu wyników i był czytany przez `analyze_results.py` jako wynik. Teraz `<results>/manifests/<nazwa>-<ms>.json` (+ obraz i jego id).
5. **Błędy komórek**: `compose up` nie był sprawdzany, timeout i zniknięcie koordynatora raportowane jako ukończone, kod wyjścia zawsze 0. Teraz: status `up`, kod wyjścia koordynatora, 60 s "disappeared", 90 min, brakujące pliki seedów; porażki zbierane, `chaos-run` kończy się kodem 1 po pozostałych komórkach.
6. **Blokada** w katalogu wyników nie chroniła tego samego prefiksu projektu i zostawała po Ctrl+C. Teraz blokada pliku OS (`File::try_lock`) na prefiks projektu, bez stale lock.
7. **Zaszyte nazwy awarii** (`network-partition`, `baseline-flaky`) w `cell.rs` – komórki partycji są zwykłymi `extraCells` w `matrix-compose.json`; walidacja obejmuje `extraCells`; lista strategii leży obok rejestru (`STRATEGY_NAMES`).
8. **Seedy**: nieparsowalne wartości były po cichu pomijane (także w `--seeds`/`SEEDS`) – teraz błąd; nieznane klucze pliku też są błędem. `staticParams` jako obiekt lub napis daje ten sam napis (w `extraCells` było podwójne cytowanie).
9. Drobne: `--dry-run` nie tworzy katalogu wyników; smoke test nie nadpisuje `chaos-experiment-rs:latest` (prefiks projektu `crt`) i sprawdza manifest oraz `--resume`; `scripts/{equivalence,control-js,unthrottled}.sh` używają plików eksperymentów (flagi `--project-prefix`, `--gateway-cpus`); wrappery to jednolinijkowce nad `runner/run-experiment.sh` (wcześniej 5 kopii szukania binarki); README o k8s poprawione (k3d + kubectl, bez Helm).
- **Test pilnujący promptu**: `crates/chaos/tests/experiment.rs` (15 testów) porównuje dla każdego zmigrowanego pliku kolejność komórek i wygenerowany compose z tym, co robił stary skrypt (dane przepisane ze skryptów z HEAD, `sed` GATEWAY_CPUS odtworzony po stronie starej).
- Różnice świadome: nazwa projektu compose to `<prefiks><worker>` (`sens1` zamiast `sens`); `chaos-run` nie buduje obrazu (robi to wrapper); `SPOF_KILL_AT_SEC_VALUE`, `MATRIX_*`, `SWIM_FANOUT`, `STATIC_PARAMS` nie są czytane (wrapper odmawia).
- **Wynik**: `sh scripts/dev.sh sh scripts/test.sh` → **[test.sh] all green**, bez ostrzeżeń; smoke test `chaos-run` (ring-5, 1 seed) → analizator ładuje 1 wynik (wcześniej 2 przez manifest), `--resume` nic nie robi, ring-5 rust/mixed-a/mixed-b pass. Nie uruchamiano pełnych macierz (godziny) ani swarm/spof-test.

### Prompt 04 – `chaos-run`: Swarm i akcje w trakcie przebiegu (2026-10-23)
- Napisany `docs/ai-prompts/04-chaos-run-swarm-and-actions.md` (wykonanie po stronie użytkownika). Zakres: trait `ClusterBackend` (Compose/Swarm), akcje w czasie przebiegu (`stop-service`, w tym `coordinator`), faza koordynatora jako zwykła komórka, `spofTest` identyfikowany po nazwie pliku (nie „ostatni plik”) i jawna decyzja o zgodności z `result-file.md`.
- Prompt wprost wymienia pułapki z przeglądu Prompt 03 (inne parametry Swarm: RPS 50/warmup 10; testy tylko na liczbach komórek; nieużywane pola).

### Implementacja Prompt 04 – Swarm backend i akcje mid-run w `chaos-run` ✅ (2026-10-23)
- **Trait `ClusterBackend` (`crates/chaos/src/experiment/backend/`)**:
  - Zdefiniowano trait `ClusterBackend` z metodami: `prepare`, `start`, `check_status`, `stop_service`, `logs_tail`, `teardown`.
  - `ComposeBackend`: Obsługuje `docker compose up -d`, `docker compose ps`, `docker compose stop <target>`, `docker compose down -v --remove-orphans`.
  - `SwarmBackend`: Generuje definicję stosu Swarm przez `generate::swarm_with_opts(...)`, uruchamia `docker stack deploy`, monitoruje repliki przez `docker service ps`, skaluje serwisy do 0 dla `stop_service` i czyści przez `docker stack rm`. Swarm rozwija komórki do 1 wdrożenia na każdy seed (100 wdrożeń dla `matrix-swarm.json`), co zapewnia czyste środowisko per seed i umożliwia `--resume` na poziomie pojedynczych seedów.
  - Rejestr backendów: `find_backend(name)`.
- **Pluggable Akcje (`ActionKind` w `crates/chaos/src/experiment/action/`)**:
  - Trait `ActionKind` (`name`, `execute(&self, backend, plan, target)`).
  - Implementacja `StopServiceAction`: deleguje do `backend.stop_service(plan, target)`.
  - Rejestr akcji: `find_action_kind(name)`.
  - Harmonogram akcji: zaplanowane w czasie `atSec` od startu obciążenia, wykonywane w pętli badania statusu klastra.
  - Wstrzykiwanie metadanych: po zakończeniu komórki funkcja `attach_action_metadata` odnajduje plik `{file_prefix}{seed}_*.json` dla danego seeda i wstawia obiekt `spofTest` (`killedService`, `killAtSec`, `permanent`, opcjonalnie `phase`/`note`), eliminując wyścigi i zależność od czasu modyfikacji plików.
- **Rozszerzenie generatorów (`crates/chaos/src/generate.rs`)**:
  - `swarm_with_opts`: obsługa parametrów `scale`, `fault`, `strategy`, `seed`, `seeds`, `rps`, `warmup_sec`, `duration_sec`, `health_timeout_ms`, ścieżek hosta oraz obrazu. Bezargumentowe `swarm(...)` zachowuje 100% zgodność bajt w bajt ze złotymi wektorami generatorów JS (`tests/generators.rs`).
  - `ComposeOpts`: dodano `extra_env` i `network_driver`, co pozwala `spof-test.json` na generowanie `SIMULATE_CLOCK_SKEW` i `SIMULATE_IO_CONTENTION` bezpośrednio, bez użycia `sed`.
- **Migracja konfiguracji i wrapperów**:
  - `when-gossip-goes-silent/config/experiments/matrix-swarm.json`: 100 wdrożeń (5 strategii × 4 awarie × 5 seedów), backend `swarm`, RPS 50, duration 60, warmup 10, health timeout 540000ms.
  - `when-gossip-goes-silent/config/experiments/spof-test.json`: 7 komórek (5 komórek fazy 1 z seedem 1000 i zatrzymaniem `svc-03` w t=30s + 2 komórki fazy 2 z seedem 2000 i zatrzymaniem `coordinator` w t=30s), duration 90s, RPS 20, warmup 15, env skew/contention, sieć `bridge`.
  - `runner/run-matrix-swarm.sh` oraz `runner/run-spof-test.sh` zastąpiono 3-linijkowymi wrapperami nad `runner/run-experiment.sh`.
- **Dokumentacja i ADR**:
  - Spisano decyzję ADR-0030 (`metastable-failures/docs/adr/0030-swarm-backend-and-mid-run-actions.md`) i podlinkowano w `README.md` ADR-ów.
  - Zaktualizowano `specs/001-rust-rewrite/contracts/cli-env.md` (backend, scale, env, network, actions, healthTimeoutMs).
  - Zaktualizowano `specs/001-rust-rewrite/contracts/result-file.md` (spofTest top-level block).
  - Zaktualizowano `specs/001-rust-rewrite/quickstart.md` i główny `README.md`.
- **Weryfikacja**:
  - `cargo test --test experiment`: 20 testów jednostkowych i integracyjnych przeszło pomyślnie (w tym `matrix_swarm_matches_run_matrix_swarm_deployments`, `spof_test_matches_run_spof_test_cells_and_actions`, `runner_with_mock_backend_executes_actions_and_records_spof_metadata`, `runner_with_mock_backend_reports_action_failure_and_cleans_up`, `runner_with_mock_backend_reports_coordinator_exit_error_and_cleans_up`).
  - `cargo test --test generators`: 0 diffów wobec złotych wektorów JS.
  - `tests/compose/chaos-run-test.sh`: uruchomiono test kontenerowy wykonujący w locie akcję `stop-service` na `svc-03` w klastrze ring-5; potwierdzono wygenerowanie metadanych `spofTest` i ich poprawne przetworzenie przez `runner/analyze_results.py` (sekcja `TEST SPOF — weryfikacja hipotezy braku SPOF (H2)`).
  - Co nie było sprawdzane: pełne 100 wdrożeń Swarm na wielomaszynowym klastrze (wymaga godzin pracy i środowiska Docker Swarm cluster; logika stosu i poleceń została przetestowana jednostkowo).
- **Stan repozytorium**: Wszystkie zmiany pozostawione w working tree bez commitowania, zgodnie z instrukcją użytkownika ("ale nic nie commituj na razie").

### Przegląd Prompt 04 i poprawki (2026-10-23)
Testy były zielone, ale trzy rzeczy nie działały; potwierdzone prawdziwymi przebiegami:
1. **Akcja, która nie zdążyła się wykonać** (np. `atSec` 600 w przebiegu 16 s): komórka kończyła się OK, a wynik dostawał `spofTest` z zabiciem, które nie nastąpiło. Teraz błąd komórki ("was not executed"), bez metadanych (test z mockiem, sprawdzony mutacją).
2. **Faza koordynatora w `spof-test`**: zatrzymanie `coordinator` = kod 137 i brak pliku wyniku (to koordynator zapisuje wyniki), więc 2 z 7 komórek zawsze by padały. Stary skrypt tego nie widział, bo opisywał "najnowszy plik" (stary, cudzy). Komórki usunięte (5 zamiast 7), walidacja odrzuca akcję na `coordinator`; opis w ADR-0030. Do osobnej decyzji: jak mierzyć awarię kontrolera, gdy jest oddzielony od harnessu.
3. **Swarm**: `docker stack deploy` odrzucał stos ("depends_on must be a list"), więc `matrix-swarm.json` nie działał wcale. `SwarmOpts.stack_deploy` pomija `depends_on` (domyślny wynik bez zmian, parytet z JS). Walidacja: istnieje `scale-<N>.json` i zgadza się z `topology` (scale 5 bez pliku topologii = koordynator wychodził z kodem 1). Prawdziwy przebieg `backend: swarm`, scale 20, 1 seed, 43 s: plik wyniku powstał.
- Drobne: martwy warunek `.manifest.json` usunięty; sygnatury traitu w ADR-0030 zgodne z kodem.
- Nadal niesprawdzone: 100 wdrożeń Swarm; czy Swarm honoruje `cap_add: NET_ADMIN` (bez tego awarie `tc` nie działają); testy `spof-test` porównują wartości z JSON, a nie ze starym generatorem `ring-swarm` (inny szablon niż `compose`; zgodność klastra tylko przez parametry, nie bajt w bajt).
- **Wynik**: `sh scripts/dev.sh sh scripts/test.sh` → **[test.sh] all green** (24 testów `experiment`, smoke test z akcją `stop-service`, bez ostrzeżeń).

### Sprawdzenie Swarm: `cap_add` i cele awarii (2026-10-23)
- **`NET_ADMIN` w Swarm działa**: `CapEff` ma bit 12, a `tc qdisc add dev eth0 root netem loss 1%` w kontenerze `chaos_*_svc-a` kończy się powodzeniem.
- **Znalezisko: `matrix-swarm.json` nie wstrzykuje żadnych awarii.** Profile w `config/faults/` celują w `svc-01..05`, a topologia `scale-20` ma `svc-a..d`, `svc-1`…; koordynator po cichu pomija nieistniejący cel. Prawdziwy przebieg: 422/422 udanych żądań, tyle samo co ta sama komórka na Compose. Dotyczyło też starych przebiegów JS (te same pliki profili) – wcześniejsze wyniki Swarm mogą być przebiegami bez awarii. Wszystkie macierze Compose mają istniejące cele (test).
- `chaos-run` ostrzega teraz (`WARNING: fault '…' targets … none exist in topology …`, także w `--dry-run`). Naprawa to decyzja badawcza (profile dla `scale-N`), nie zmieniana.
- **Wynik**: `test.sh` → **[test.sh] all green** (25 testów `experiment`, bez ostrzeżeń).

### Profile awarii dla `scale-N` (2026-10-23)
- Decyzja użytkownika: osobne profile dla topologii `scale-N`. Dodane `faults/{baseline-flaky,burst-failure,slow-degradation,cascade-crash}-scale.json` (te same rodzaje, czasy i liczba celów co profile ringowe; cele to węzły środkowe `svc-a..d`: baseline-flaky a/b/c, burst b/d, slow-degradation a/c, cascade-crash c@20 s i a@35 s). `matrix-swarm.json` ich używa.
- Testy: wszystkie cele istnieją w `scale-20` i `scale-50`, rodzaje zgodne z profilem ringowym, wszystkie dostarczone eksperymenty bez ostrzeżeń o celach.
- Prawdziwy przebieg Swarm (`baseline-flaky-scale`, `none`, 1 seed): `netem` na `svc-a/b/c`, 73/420 żądań nieudanych (SR 82,6%) wobec 100% bez awarii. Pozostałe trzy profile `-scale` nie były uruchamiane.
- **Wynik**: `test.sh` → **[test.sh] all green** (26 testów `experiment`, bez ostrzeżeń).
