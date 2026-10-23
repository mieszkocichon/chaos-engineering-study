# demos (Rust)

Rust port of `demos/` (circuit breaker, retry storm, saga). One multi-call binary:

```sh
sh scripts/dev.sh cargo build --release -p demos      # → target/release/demo
demo run-cb-comparison                                # same flow as node scenarios/run_cb_comparison.js
```

## JS → `demo` mapping

| JS file | `demo` command |
|---|---|
| `gateway/index.js` | `gateway` |
| `service-a/index.js`, `service-b/index.js` | `service-a`, `service-b` |
| `chaos/gateway-retry.js` | `gateway-retry` |
| `chaos/gateway-retry-cb.js` | `gateway-retry-cb` |
| `chaos/gateway-orders.js` | `gateway-orders` (`RETRY`, `TIMEOUT_MS`) |
| `chaos/gateway-orders-saga.js` | `gateway-orders-saga` (`RETRY`, `TIMEOUT_MS`) |
| `chaos/service-a-orders.js` | `service-a-orders` |
| `chaos/service-a-slow.js` | `service-a-slow` |
| `chaos/service-b-flaky.js` | `service-b-flaky` |
| `chaos/service-b-payments.js` | `service-b-payments` (`SLOW_MODE`) |
| `client/test-load.js` | `test-load [--requests N] [--save LABEL]` |
| `client/test-orders.js` | `test-orders [--orders N] [--save LABEL]` |
| `client/monitor.js` | `monitor [--rps --duration --endpoint --method --body --save]` |
| `client/compare.js` | `compare <before.json> <after.json>` |
| `client/test.js` | `test` |
| `starter.js` | `starter` |
| `scenarios/circuit_breaker_timeout.js` | `scenario circuit-breaker-timeout` |
| `scenarios/circuit_breaker_cascade.js` | `scenario circuit-breaker-cascade` |
| `scenarios/circuit_breaker_cascade_solution.js` | `scenario circuit-breaker-cascade-solution` |
| `scenarios/saga_lost_payment.js` | `scenario saga-lost-payment` |
| `scenarios/saga_lost_payment_solution.js` | `scenario saga-lost-payment-solution` |
| `scenarios/saga_double_charge.js` | `scenario saga-double-charge` |
| `scenarios/saga_double_charge_solution.js` | `scenario saga-double-charge-solution` |
| `scenarios/run_all_chaos.js` | `run-all-chaos` |
| `scenarios/run_cb_comparison.js` | `run-cb-comparison` |
| `scenarios/run_saga_comparison.js` | `run-saga-comparison` |

Ports, paths, flags and report files (`demos/results/<label>.json`) are the same as in JS.
New optional env vars (tests): `DEMO_A_URL`, `DEMO_B_URL` (downstream bases, default
`http://localhost:3001` / `:3002`), `DEMO_GATEWAY` (clients' target, default
`http://localhost:3000`), `DEMO_RESULTS_DIR` (default `demos/results` when run from the repo root).

Differences from JS: runners start the three services directly instead of spawning a
starter script that spawns them; randomness is unseeded (`rand`) like `Math.random()`.

## Kept JS behaviour worth knowing

- `service-b-payments` ignores the `Idempotency-Key` header the saga gateway sends, so the
  "double charge solution" still double-charges once slow payments complete (as in JS).
- `gateway` reads `SERVICE_TIMEOUT_MS` but never applies it (as in JS).
