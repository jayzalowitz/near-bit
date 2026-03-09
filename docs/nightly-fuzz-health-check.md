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
./scripts/launch/check_nightly_fuzz_health.sh --branch jayzalowitz/btc-near-fork-plan

# Check a different workflow name
./scripts/launch/check_nightly_fuzz_health.sh --workflow CI

# Evaluate only fuzz jobs inside a broader workflow (case-insensitive regex)
./scripts/launch/check_nightly_fuzz_health.sh --workflow CI --fuzz-job-pattern "Fuzz"

# Use fuzz-job filtering through readiness orchestration
./scripts/launch/run_readiness_gate.sh --full --check-nightly-fuzz-health --nightly-fuzz-workflow CI --nightly-fuzz-job-pattern "Fuzz"

# Tighten or relax lookback criteria
./scripts/launch/check_nightly_fuzz_health.sh --window-days 14 --min-runs 10 --max-runs 500

# Require at least 7 runs in the window
./scripts/launch/check_nightly_fuzz_health.sh --min-runs 7

# Export machine-readable output
./scripts/launch/check_nightly_fuzz_health.sh --json-out /tmp/nightly-fuzz-health.json

# Allow in-progress runs during active triage windows
./scripts/launch/check_nightly_fuzz_health.sh --allow-in-progress

# Treat cancelled runs/jobs as failures (strict mode)
./scripts/launch/check_nightly_fuzz_health.sh --fail-on-cancelled
```

## Integrate with Launch Gates

Use strict readiness mode when preparing final signoff evidence:

```bash
./scripts/launch/run_readiness_gate.sh --full --check-nightly-fuzz-health
./scripts/launch/run_readiness_gate.sh --full --check-nightly-fuzz-health --nightly-fuzz-branch main --nightly-fuzz-workflow "Nightly Fuzz" --nightly-fuzz-window-days 7 --nightly-fuzz-min-runs 1 --nightly-fuzz-max-runs 200
./scripts/launch/run_readiness_gate.sh --full --check-nightly-fuzz-health --nightly-fuzz-branch main --nightly-fuzz-workflow CI --nightly-fuzz-job-pattern "Fuzz" --nightly-fuzz-window-days 7 --nightly-fuzz-min-runs 1 --nightly-fuzz-max-runs 200 --nightly-fuzz-fail-on-cancelled
```

For rehearsal/evidence flows, pass the same check through orchestration:

```bash
./scripts/launch/generate_evidence_bundle.sh --mode full --check-nightly-fuzz-health
./scripts/launch/generate_evidence_bundle.sh --mode full --check-nightly-fuzz-health --nightly-fuzz-workflow CI --nightly-fuzz-job-pattern "Fuzz"
./scripts/launch/run_launch_rehearsal.sh --check-nightly-fuzz-health
./scripts/launch/run_launch_rehearsal.sh --check-nightly-fuzz-health --nightly-fuzz-workflow CI --nightly-fuzz-job-pattern "Fuzz"
```

## Failure Semantics

The check exits non-zero when any of these are true:

1. fewer than `--min-runs` runs exist in the lookback window
2. any evaluated run (or matched fuzz job when `--fuzz-job-pattern` is set) completed with a failing conclusion (`failure`, `timed_out`, etc.)
3. any evaluated run has in-progress status (unless `--allow-in-progress` is set)
4. any evaluated run is cancelled when `--fail-on-cancelled` is set

Notes:

- `cancelled` runs are tracked separately and do not fail by default.
- When `--fuzz-job-pattern` is set, runs without matching jobs are ignored.
