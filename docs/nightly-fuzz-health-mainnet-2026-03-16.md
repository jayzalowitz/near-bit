# Nightly Fuzz 7-Day Health Summary (Launch Window 2026-03-20)

Date (UTC): 2026-03-16
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
  --json-out docs/external-gate-artifacts/mainnet-2026-03-20/nightly-fuzz-health.json
```

## Result

- Status: `passed`
- Runs in window: `4`
- Successful runs: `4`
- Failed runs: `0`
- Cancelled runs: `0`
- In-progress runs: `0`

## Policy Decision

Gate `4` launch signoff policy remains fuzz-job scoped and treats cancelled runs as non-failing by default unless strict mode (`--fail-on-cancelled`) is explicitly required.

## Evidence

- `docs/external-gate-artifacts/mainnet-2026-03-20/nightly-fuzz-health.json`
- `docs/external-gate-artifacts/mainnet-2026-03-20/nightly-fuzz-health.txt`
