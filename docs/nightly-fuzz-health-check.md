# Nightly Fuzz Health Check

This guide documents how to verify launch gate #4:

> Nightly fuzz matrix has no unresolved crashes for previous 7 days.

## Command

```bash
./scripts/launch/check_nightly_fuzz_health.sh
```

Defaults:

- repository: derived from `origin` remote
- branch: `main`
- workflow: `Nightly Fuzz`
- lookback window: `7` days
- minimum runs required: `1`
- in-progress runs: treated as unresolved (fails)

## Useful Variants

```bash
# Check another branch
./scripts/launch/check_nightly_fuzz_health.sh --branch infinitoshi/btc-near-fork-plan

# Require at least 7 runs in the window
./scripts/launch/check_nightly_fuzz_health.sh --min-runs 7

# Export machine-readable output
./scripts/launch/check_nightly_fuzz_health.sh --json-out /tmp/nightly-fuzz-health.json

# Allow in-progress runs during active triage windows
./scripts/launch/check_nightly_fuzz_health.sh --allow-in-progress
```

## Integrate with Launch Gates

Use strict readiness mode when preparing final signoff evidence:

```bash
./scripts/launch/run_readiness_gate.sh --full --check-nightly-fuzz-health
```

For rehearsal/evidence flows, pass the same check through orchestration:

```bash
./scripts/launch/generate_evidence_bundle.sh --mode full --check-nightly-fuzz-health
./scripts/launch/run_launch_rehearsal.sh --check-nightly-fuzz-health
```

## Failure Semantics

The check exits non-zero when any of these are true:

1. fewer than `--min-runs` runs exist in the lookback window
2. any run completed with a non-success conclusion (`failure`, `cancelled`, `timed_out`, etc.)
3. any run is still in progress (unless `--allow-in-progress` is set)
