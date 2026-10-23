# Feature Specification: Przepisanie platformy chaos-engineering na Rust

**Feature Branch**: `when-gossip-goes-silent` (gałąź robocza przepisania)
**Created**: 2026-10-19
**Status**: Draft
**Input**: "Utworzyłem nowy branch, na którym chcę przepisać cały system na platformę języka Rust. Zastanów się, czy jest to możliwe. Wykonaj dokładny plan działania."

> Uwaga: docelowa platforma (Rust) jest *przedmiotem* tej funkcji, więc jest jedyną dopuszczalną wzmianką technologiczną. Pozostałe wybory (biblioteki, struktura modułów) należą do `/speckit-plan`.

## Ocena wykonalności (podsumowanie)

**Werdykt: wykonalne, w ~4 fazach.** Kod źródłowy to ok. 6–7 tys. linii logiki (reszta repozytorium to wygenerowane topologie, manifesty i 1 220 plików wyników). Nie ma w nim niczego, czego platforma docelowa nie potrafi:

| Obszar | Rozmiar | Ryzyko | Uwagi |
|---|---|---|---|
| `metastable-failures/` (symulator DES + detektor CSD) | ~580 linii, zero zależności | **Niskie** | Czysto obliczeniowy, deterministyczny (mulberry32). Idealny pilot – da się uzyskać wynik identyczny bit w bit. |
| `when-gossip-goes-silent/lib` + `agent` (usługa, 8 strategii, CB, retry, limiter, metryki, gossip) | ~1 900 linii | **Średnie** | Serwer HTTP + timery + współbieżne wywołania. Kluczowe: zgodność protokołu HTTP/JSON i zachowania strategii. |
| Wstrzykiwanie awarii (`tc`, `iptables`, SIGKILL) | ~170 linii | Niskie | To wywołania poleceń systemowych – przenoszą się 1:1. |
| `runner/coordinator.js`, generatory topologii/compose/k8s | ~1 300 linii | Niskie | Logika orkiestracji i generowanie plików. |
| PRNG `seedrandom` (ARC4) | ~30 linii | **Wysokie (dla porównywalności)** | Bez odtworzenia algorytmu ARC4 `seedrandom` nowe przebiegi nie będą miały tych samych sekwencji losowych co 1 020 istniejących wyników. |
| `demos/` | ~1 500 linii | Niskie | Niezależne mikrousługi demonstracyjne, używają niedeterministycznej losowości. |
| Skrypty powłoki, analizator wyników, generator obciążenia (k6) | — | — | Narzędzia zewnętrzne/analityczne; patrz Zakres. |

**Główne ryzyko nie jest techniczne, tylko naukowe:** zmiana środowiska uruchomieniowego zmienia charakterystykę czasową (pętla zdarzeń vs. natywna współbieżność, narzut GC). Przebiegi eksperymentu gossip zależą od zegara ściennego, więc **nie mogą** być identyczne bit w bit z wynikami historycznymi – można (i trzeba) wykazać równoważność statystyczną.

## Clarifications

### Session 2026-10-19

- Q: Czy zakres "cały system" obejmuje też analizator wyników (Python), skrypty powłoki macierzy eksperymentów i dema? → A: Przepisany zostaje cały kod JavaScript (symulator metastabilności, usługi i strategie, koordynator, generatory, dema). Analizator wyników (Python), skrypty powłoki macierzy i generator obciążenia (k6) pozostają bez zmian i muszą współpracować z nową implementacją.
- Q: Czy każdy etap ma dostarczać testy? → A: Tak – każdy etap (US1–US5) dostarcza testy jednostkowe i integracyjne; etap nie jest ukończony, dopóki oba rodzaje nie przechodzą.
- Q: Gdzie mają się uruchamiać testy integracyjne, w tym z prawdziwymi awariami sieci? → A: Lokalnie na komputerze (w kontenerach, jednym poleceniem) – to środowisko rozstrzyga o ukończeniu etapu. Dodatkowo CI na GitHubie przy każdym pushu uruchamia testy jednostkowe i lekkie testy integracyjne (mała topologia); ciężkie przebiegi (topologie 50 usług, raport równoważności US4) tylko lokalnie.

## User Scenarios & Testing *(mandatory)*

Aktorzy: **badacz** (uruchamia eksperymenty i analizuje wyniki), **recenzent** (weryfikuje odtwarzalność), **operator klastra** (uruchamia topologie w kontenerach / k8s).

### User Story 1 – Pilot: symulator metastabilności (Priority: P1)

Badacz uruchamia symulator retry-storm i detektor wczesnego ostrzegania w nowej implementacji i dostaje dokładnie te same wyniki bramek A i B, co w implementacji referencyjnej.

**Why this priority**: Najmniejszy, czysto obliczeniowy moduł – weryfikuje toolchain, budowanie w kontenerze i dyscyplinę determinizmu przed dotknięciem systemu rozproszonego.

**Independent Test**: Uruchomienie bramki A, przebiegu M2 i sweepu histerezy dla tych samych seedów w obu implementacjach i porównanie wyjść.

**Acceptance Scenarios**:

1. **Given** te same parametry i seed, **When** badacz uruchamia pojedynczą próbę, **Then** liczniki zdarzeń i serie czasowe są identyczne z implementacją referencyjną.
2. **Given** skrypt bramki A, **When** zostanie uruchomiony, **Then** wykazuje histerezę (obciążenie wejścia > obciążenie wyjścia) z tymi samymi wartościami co w `results/m1-gate-a.md`.
3. **Given** przebieg M2, **When** zostanie uruchomiony, **Then** alarm wyprzedza kolaps (lead time > 0) z tą samą wartością, a przebieg stabilny nie daje fałszywego alarmu.

---

### User Story 2 – Usługa eksperymentalna jako zamiennik "drop-in" (Priority: P2)

Operator buduje obraz kontenera nowej usługi i podmienia go w istniejących plikach compose/k8s bez zmiany ich treści (poza nazwą obrazu i poleceniem healthcheck, które dziś wywołuje środowisko uruchomieniowe referencji). Koordynator, generator obciążenia i analizator działają bez zmian.

**Why this priority**: Rdzeń badań. Zgodność protokołu pozwala migrować usługa po usłudze i uruchamiać klastry mieszane, więc każdy krok jest weryfikowalny.

**Independent Test**: Uruchomienie topologii `ring-5` z koordynatorem referencyjnym i nowymi usługami; wynikowy plik JSON przechodzi przez analizator bez błędów.

**Acceptance Scenarios**:

1. **Given** te same zmienne środowiskowe (`SERVICE_ID`, `TOPOLOGY`, `FAULT_PROFILE`, `STRATEGY`, `SEED`, `BASE_PORT`, `STATIC_PARAMS`, `SWIM_FANOUT`), **When** usługa startuje, **Then** wybiera tę samą konfigurację co referencja, a przy błędnym `SERVICE_ID` kończy się błędem z listą dostępnych identyfikatorów.
2. **Given** działająca usługa, **When** klient woła którykolwiek z punktów końcowych (`/data`, `/health`, `/metrics`, `/gossip`, `/control`, `/start`, `/experiment/reset`, `/faults/:profile`, `/faults/recover`, `/faults/recover/:type`), **Then** kształt odpowiedzi i kody statusu są zgodne z referencją.
3. **Given** klaster mieszany (część usług referencyjnych, część nowych), **When** trwa eksperyment, **Then** gossip i wywołania downstream działają w obu kierunkach.
4. **Given** każda z 8 strategii (`none`, `static`, `static-conservative`, `central`, `mas-agent`, `mas-agent-swim`, `mas-agent-inbound`, `backpressure`), **When** zostaną podane identyczne obserwacje, **Then** decyzje (parametry retry, CB, limitu współbieżności, timeoutu) są identyczne z referencją.
5. **Given** profil awarii (`baseline-flaky`, `burst-failure`, `slow-degradation`, `cascade-crash`, `wan-baseline`, `network-partition`), **When** zostanie wstrzyknięty i odwołany, **Then** wykonywane są te same polecenia systemowe, a odwołanie po typie usuwa tylko wskazaną warstwę.

---

### User Story 3 – Orkiestracja i generatory (Priority: P3)

Badacz uruchamia koordynator komórki eksperymentu i generatory topologii/compose/k8s w nowej implementacji.

**Why this priority**: Zależy od P2 (protokół). Pozwala usunąć ostatnią zależność od starego środowiska uruchomieniowego w ścieżce eksperymentu.

**Independent Test**: Generatory dla tych samych argumentów produkują pliki identyczne (po normalizacji białych znaków) z referencyjnymi; koordynator produkuje plik wyników zgodny ze schematem.

**Acceptance Scenarios**:

1. **Given** te same argumenty, **When** uruchomione zostaną generatory ring/swarm/mesh/compose/k8s, **Then** wyjście jest semantycznie identyczne z referencyjnym.
2. **Given** komórka z listą seedów, **When** koordynator ją wykonuje, **Then** czeka na zdrowie usług, rozgłasza start, wykonuje miękki reset między seedami, zbiera metryki i zapisuje plik z kluczami `config`, `experimentStartTime`, `summary`, `k6Metrics`, `serviceMetrics`, `timestamp`.
3. **Given** usługa nie staje się zdrowa w limicie czasu, **When** koordynator czeka, **Then** kończy się błędem z listą niegotowych usług.

---

### User Story 4 – Walidacja naukowa (Priority: P3)

Recenzent porównuje wyniki nowej implementacji z historyczną macierzą 1 020 eksperymentów i dostaje raport równoważności.

**Why this priority**: Bez tego przepisanie unieważnia dotychczasowe wnioski. Wymaga P2+P3.

**Independent Test**: Podzbiór macierzy (np. 3 strategie × 2 profile × 20 seedów) uruchomiony w obu implementacjach; raport porównuje rozkłady success rate i p50/p90/p99.

**Acceptance Scenarios**:

1. **Given** te same komórki i seedy, **When** zostanie wygenerowany raport, **Then** dla każdej komórki podaje różnicę średnich, przedział ufności i test istotności.
2. **Given** raport, **When** różnica w komórce przekracza próg równoważności, **Then** komórka jest oznaczona i wymaga wyjaśnienia przed wycofaniem referencji.

---

### User Story 5 – Dema (Priority: P4)

Dema (gateway, usługi A/B, scenariusze CB i saga, klienci testowi) działają w nowej implementacji z tymi samymi scenariuszami.

**Why this priority**: Materiał poglądowy, niezależny od wyników badań – w zakresie (Q1: A), ale realizowany na końcu.

**Independent Test**: Każdy scenariusz `run_*` daje wynik jakościowo zgodny z `demos/results/*.json` (np. CB skraca czas odpowiedzi, saga zapobiega podwójnemu obciążeniu).

**Acceptance Scenarios**:

1. **Given** scenariusz porównania CB, **When** zostanie uruchomiony, **Then** wariant "po" ma niższe opóźnienia niż "przed".
2. **Given** scenariusz sagi, **When** zostanie uruchomiony, **Then** rozwiązanie nie powoduje podwójnego obciążenia.

### Edge Cases

- Seed jako napis vs liczba: generator losowy musi traktować seed jako tekst (`"${seed}_${id}_delay"`) dokładnie jak referencja.
- `uniform(min,max)` jest **włączny** z obu stron i zaokrągla w dół; `normalRandom` (Box–Muller) przy `u1 = 0` – zachowanie musi być zdefiniowane identycznie.
- Polecenie `tc qdisc del` przy braku qdisc kończy się błędem, który ma być ignorowany.
- `cascade-crash` zabija proces natychmiast – bez sprzątania i bez odpowiedzi HTTP.
- `slow-degradation` modyfikuje opóźnienie co sekundę przez 60 s; reset/odwołanie musi zatrzymać ten proces.
- Gossip do niedostępnego sąsiada: limit 1 s, błąd połykany, brak wpływu na obsługę żądań.
- Dane sąsiada starsze niż 3× interwał gossip są ignorowane.
- Miękki reset w trakcie trwających żądań: nie może gubić odpowiedzi ani zostawiać aktywnych reguł sieciowych.
- Nieprawidłowy JSON w `STATIC_PARAMS`: ostrzeżenie i wartości domyślne, nie awaria.
- Usługa liść (bez downstreamów) nie ma strategii, ale wysyła gossip do sąsiadów.

## Requirements *(mandatory)*

### Functional Requirements

**Zgodność i migracja**

- **FR-001**: Nowa implementacja MUSI powstawać obok referencyjnej; referencja pozostaje uruchamialna do czasu spełnienia SC-004. Po spełnieniu referencja jest archiwizowana w `old-implementation/` (ADR-0026), nie usuwana.
- **FR-002**: Protokół sieciowy usługi (ścieżki, metody, pola żądań i odpowiedzi, kody statusu) MUSI być zgodny z referencją, by umożliwić klastry mieszane.
- **FR-003**: Format plików konfiguracyjnych (topologie, profile awarii, macierz eksperymentu) i plików wyników MUSI pozostać bez zmian; istniejące pliki są czytane bez konwersji.
- **FR-004**: Konfiguracja usług i koordynatora przez zmienne środowiskowe MUSI zachować nazwy i wartości domyślne referencji.

**Determinizm**

- **FR-005**: Generator liczb pseudolosowych usług MUSI dla danego seeda tekstowego produkować tę samą sekwencję co referencja (algorytm ARC4 z biblioteki `seedrandom`), wraz z rozkładami bernoulli/uniform/normal/exponential.
- **FR-006**: Symulator metastabilności MUSI produkować wyniki identyczne bit w bit z referencją dla tych samych parametrów i seeda.

**Funkcje usługi**

- **FR-007**: Usługa MUSI obsługiwać 8 strategii odporności o identycznej logice decyzyjnej, granicach parametrów i interwałach (decyzja 2 s, gossip 3 s, wygaśnięcie 9 s).
- **FR-008**: Usługa MUSI zapewniać circuit breaker, retry z backoffem, limiter współbieżności i kolektor metryk o semantyce referencji (okna, percentyle, podsumowanie zdrowia).
- **FR-009**: Usługa MUSI wstrzykiwać i odwoływać wszystkie profile awarii referencji, w tym odwołanie po typie warstwy.
- **FR-010**: Miękki reset MUSI przeseedować generator, wyczyścić metryki, odwołać awarie i zresetować stan strategii.
- **FR-011**: Obraz kontenera MUSI zawierać narzędzia sieciowe wymagane przez wstrzykiwanie awarii i generator obciążenia.

**Orkiestracja i narzędzia**

- **FR-012**: Koordynator MUSI odtworzyć cykl komórki: oczekiwanie na zdrowie → start → (obciążenie → zbiór metryk → reset) dla każdego seeda → zapis wyników.
- **FR-013**: Generatory topologii i manifestów MUSZĄ produkować semantycznie identyczne pliki dla tych samych argumentów.
- **FR-014**: Przepisaniu podlega cały kod JavaScript, łącznie z demami. Analizator wyników, skrypty powłoki macierzy i generator obciążenia pozostają bez zmian; skrypty macierzy MUSZĄ uruchamiać nową implementację wyłącznie przez zmianę nazwy obrazu/polecenia.

**Weryfikacja**

- **FR-015**: Każdy przeniesiony moduł logiki (PRNG, CB, retry, limiter, metryki, każda strategia, symulator, detektor) MUSI mieć test porównawczy z referencją na zapisanych wektorach wejście/wyjście.
- **FR-016**: MUSI istnieć raport równoważności statystycznej dla podzbioru macierzy (US4).
- **FR-017**: Każdy etap (US1–US5) MUSI dostarczyć razem z kodem:
  - **testy jednostkowe** – każda przeniesiona jednostka logiki w izolacji (bez sieci, bez zegara ściennego), w tym przypadki brzegowe z sekcji Edge Cases;
  - **testy integracyjne** – scenariusze akceptacyjne danego etapu uruchamiane na działających komponentach (procesy/kontenery komunikujące się przez protokół z FR-002).
  Etap jest ukończony dopiero, gdy wszystkie jego testy oraz testy etapów wcześniejszych przechodzą (brak regresji).
- **FR-018**: Pełny zestaw testów (jednostkowe + integracyjne, łącznie z wstrzykiwaniem awarii sieci) MUSI dać się uruchomić lokalnie jednym poleceniem, wyłącznie w kontenerach, bez instalowania toolchainu na hoście – na Windows i Linux. Ten przebieg rozstrzyga o ukończeniu etapu.
- **FR-019**: Przy każdym pushu CI MUSI uruchamiać testy jednostkowe i lekkie testy integracyjne (topologia ≤ 5 usług, w tym co najmniej jeden test awarii sieci). Testy ciężkie (topologie ≥ 20 usług, raport równoważności) są oznaczone i uruchamiane tylko lokalnie.

### Key Entities

- **Topologia**: zbiór usług (id, port, downstreamy, sąsiedzi gossip, rozkład opóźnienia przetwarzania) + punkt wejścia.
- **Profil awarii**: lista awarii przypisanych do usług docelowych (typ, opcje, np. adresy partycji).
- **Strategia**: polityka odporności z parametrami w granicach (retry, backoff, próg i timeout CB, limit współbieżności, timeout).
- **Komunikat gossip**: nadawca, errorRate, avgLatency, load, znacznik czasu.
- **Wynik eksperymentu**: config, experimentStartTime, summary, k6Metrics, serviceMetrics, timestamp.
- **Próba symulacji**: konfiguracja obciążenia/surge, seed, liczniki i serie czasowe, sygnały CSD, alarm.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: 100% prób symulatora metastabilności dla co najmniej 30 seedów daje wyniki identyczne z referencją; bramki A i B przechodzą z tymi samymi wartościami.
- **SC-002**: 100% wektorów testowych generatora losowego (≥ 10 000 losowań na ≥ 20 seedów) jest identyczne z referencją.
- **SC-003**: Analizator wyników przetwarza 100% plików wyników nowej implementacji bez zmian w jego kodzie.
- **SC-004** *(zmienione 2026-10-20, ADR-0026)*: Równoważność oceniana **bez dławienia CPU bramki** (`GATEWAY_CPUS`), JS i Rust na tej samej maszynie, ten sam podzbiór macierzy: success rate w marginesie ±7 pp, p90 w ±15% w większości komórek i zgodny ranking strategii. Pozostałe małe różnice są dokumentowane, ale nie blokują. Wynik: SR równoważny w 5/6 komórek, 0 odwróceń rankingu – **spełnione**. Przy opublikowanym limicie 0,25 CPU różnice wynikają z dławienia bramki Node (equivalence-report.md).
- **SC-005**: Klaster 50 usług startuje i osiąga stan zdrowy co najmniej tak szybko jak referencja; zużycie pamięci na usługę nie jest wyższe niż w referencji.
- **SC-006**: Pełna komórka eksperymentu (20 seedów) kończy się w czasie nie dłuższym niż w referencji.
- **SC-007**: Osoba spoza projektu odtwarza bramki A/B i jedną komórkę macierzy, korzystając wyłącznie z README, bez instalowania narzędzi na hoście (tylko kontenery).
- **SC-008**: Każdy etap kończy się z 100% przechodzących testów jednostkowych i integracyjnych (swoich i wcześniejszych etapów) w lokalnym przebiegu; każdy scenariusz akceptacyjny w US1–US5 ma co najmniej jeden test integracyjny, a każda jednostka logiki z FR-015 – co najmniej jeden test jednostkowy.
- **SC-009**: Przebieg CI (testy jednostkowe + lekkie integracyjne) kończy się w czasie poniżej 15 minut, a lokalny pełny przebieg testów bez raportu równoważności – poniżej 30 minut.

## Assumptions

- Plan migracji jest fazowy (P1 → P4), moduł po module, z klastrami mieszanymi jako siatką bezpieczeństwa; nie ma "wielkiego przełączenia".
- Generator obciążenia (k6) i narzędzia systemowe (`tc`, `iptables`) pozostają narzędziami zewnętrznymi – nie są przepisywane.
- Wyniki historyczne (1 220 plików JSON, ADR-y, notatki) pozostają nietknięte i są punktem odniesienia.
- Równoważność eksperymentu gossip jest oceniana statystycznie (SC-004), nie bit w bit – zależy od zegara ściennego i sieci.
- Funkcje matematyczne (log, cos, sqrt) mogą różnić się w ostatnim bicie między środowiskami; jeśli to łamie FR-006, symulator używa przenośnych implementacji tych funkcji.
- Dema używają niedeterministycznej losowości – oceniane są jakościowo.
- Zduplikowane pliki (np. `backpressure-agent.js` i `generate-*-topology.js` w katalogu głównym vs `when-gossip-goes-silent/`, dwa `analyze_results.py`) są konsolidowane do jednej wersji – źródłem prawdy jest wersja w podprojekcie.
- Docelowo build i uruchomienie odbywa się w kontenerach (jak obecnie), bez toolchainu na hoście.
