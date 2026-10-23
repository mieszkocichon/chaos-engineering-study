# Quickstart: budowanie, testy i walidacja wersji Rust

Wymagania: Docker (Desktop na Windows, uruchomiony – demon startuje około minuty) i powłoka `sh`/`bash` (Git Bash na Windows). Na hoście nie instalujemy toolchainu (ADR-0004). Wszystkie polecenia wykonujemy z katalogu głównego repozytorium.

## 0. Obraz deweloperski (jednorazowo)

```bash
docker build -t rust-dev -f Dockerfile.dev .
```

`scripts/dev.sh <polecenie>` uruchamia polecenie w `rust-dev` z repozytorium pod `/app`, cache cargo w wolumenach i socketem Dockera. Działa w Git Bash i na Linuksie.

## 1. Testy – jedno polecenie (FR-018)

```bash
sh scripts/dev.sh sh scripts/test.sh
```

Kolejność: `cargo test --workspace` (testy jednostkowe na wektorach z JS oraz integracyjne na localhost) → `python3 -m unittest` (`compare_impls.py`) → `tests/compose/*.sh`:
- `ring5.sh` – klaster ring-5 z NET_ADMIN w trzech trybach: Rust (prawdziwe `tc`/`iptables` dla każdego profilu), mieszany a (gateway Rust + liście JS), mieszany b (gateway JS + liście Rust);
- `cell-ring5.sh` – jedna komórka eksperymentu: `chaos-coord` z prawdziwym k6, a plik wyników przechodzi przez oba `analyze_results.py`.
- `chaos-run-test.sh` – przebieg eksperymentu przez `chaos-run`, w tym weryfikacja akcji `stop-service` i generowania metadanych `spofTest`.

**Oczekiwany wynik:** `[test.sh] all green` (pierwszy przebieg z budowaniem obrazów trwa około 6–8 min, kolejne około 4 min). Ten sam zestaw (bez testów ciężkich) chodzi w CI (`.github/workflows/ci.yml`).

## 2. Wektory referencyjne (tylko gdy zmienia się zakres testów)

```bash
docker run --rm -v "${PWD}:/app" -w /app node:20-bookworm sh scripts/golden.sh
```

Pochodzenie i odtwarzanie wektorów: `crates/*/tests/golden/README.md`.

## 3. US1 – bramki metastabilności (SC-001)

```bash
sh scripts/dev.sh cargo build --release -p metastable
sh scripts/dev.sh sh -c 'cd metastable-failures && M1=/app/target/release/run-m1 sh scripts/gate-a.sh'
sh scripts/dev.sh cargo run -q --release -p metastable --bin run-m2
```

Oczekiwane: wyjście identyczne bajt w bajt z wersją JS (sprawdza to `crates/metastable/tests/{cli,gates}.rs`).

## 4. US2/US3 – usługa, koordynator, generatory i runner

- Obraz runtime: `docker build -t chaos-experiment-rs:latest .` (zawiera `chaos-svc`, `chaos-coord`, `chaos-gen`, `chaos-run`, k6, `/app/config`).
- Eksperyment to plik w `when-gossip-goes-silent/config/experiments/` (ADR-0029, ADR-0030, schemat w `contracts/cli-env.md`):

```bash
cd when-gossip-goes-silent
sh ../scripts/dev.sh sh -c 'cd when-gossip-goes-silent && cargo run -q -p chaos --bin chaos-run -- config/experiments/matrix-compose.json --dry-run'
SEEDS=1000 bash runner/run-matrix-compose.sh ring-5      # wrapper: zmienne env -> flagi, sam znajduje chaos-run
chaos-run config/experiments/matrix-sensitivity.json --resume   # gdy binarka jest w PATH
chaos-run config/experiments/matrix-swarm.json --dry-run        # 100 wdrożeń Swarm
chaos-run config/experiments/spof-test.json                     # test SPOF z akcjami stop-service
```

Flagi: `--dry-run`, `--resume`, `--only <tekst>`, `--workers`, `--impl`, `--results-dir`, `--seeds`, `--topology`. `IMPL=js` uruchamia zarchiwizowaną implementację z `old-implementation/` (ADR-0026).
`chaos-run` nie buduje obrazów; wrapper `runner/run-experiment.sh` buduje obraz raz, jeśli go brak. Macierze uruchamiamy z hosta albo przez `scripts/dev.sh` (ścieżki bind-mountów tłumaczy `HOST_APP_PATH`).
Zarówno Swarm (`matrix-swarm.json`), jak i SPOF test (`spof-test.json`) są zmigrowane do `chaos-run` (ADR-0030). Jedynie k8s (`run-matrix-k8s.sh`) pozostaje skryptem do czasu implementacji backendu k3d.

## 5. US4 – równoważność i wydajność (tylko lokalnie, godziny)
