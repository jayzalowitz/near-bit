# Nightly Fuzz 7-Day Health Summary (Launch Window 2026-03-10)

Date (UTC): 2026-03-05
Owner: launch-readiness

## Command

```bash
./scripts/launch/check_nightly_fuzz_health.sh \
  --branch infinitoshi/btc-near-fork-plan \
  --workflow CI \
  --fuzz-job-pattern "Fuzz" \
  --window-days 7 \
  --min-runs 1 \
  --max-runs 50 \
  --allow-in-progress \
  --json-out docs/external-gate-artifacts/mainnet-2026-03-10/nightly-fuzz-health.json
```

## Result

- Status: `passed`
- Runs in window: `50`
- Successful runs: `42`
- Failed runs: `0`
- Cancelled runs: `7` (treated as non-failures in this launch policy)
- In-progress runs: `1` (allowed for rolling CI activity)

## Evidence

- `docs/external-gate-artifacts/mainnet-2026-03-10/nightly-fuzz-health.json`
- `docs/external-gate-artifacts/mainnet-2026-03-10/nightly-fuzz-health.txt`
