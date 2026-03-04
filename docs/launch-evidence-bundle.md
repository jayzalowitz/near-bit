# Launch Evidence Bundle

This guide documents how to generate a reproducible launch-evidence bundle for a specific commit.

## Purpose

The bundle is designed to support launch rehearsal and go/no-go review by capturing:

1. exact git commit/branch and worktree state
2. readiness gate execution result and logs
3. checksummed snapshots of launch-critical docs and workflows

## Generate a Bundle

```bash
# Full bundle with full readiness gate
./scripts/launch/generate_evidence_bundle.sh --mode full

# Faster bundle for iteration
./scripts/launch/generate_evidence_bundle.sh --mode smoke

# Include fuzz smoke in readiness gate execution
./scripts/launch/generate_evidence_bundle.sh --mode full --include-fuzz

# Enforce nightly fuzz 7-day health gate in readiness execution
./scripts/launch/generate_evidence_bundle.sh --mode full --check-nightly-fuzz-health

# Enforce nightly fuzz health gate with explicit criteria
./scripts/launch/generate_evidence_bundle.sh --mode full --check-nightly-fuzz-health --nightly-fuzz-branch main --nightly-fuzz-workflow "Nightly Fuzz" --nightly-fuzz-window-days 7 --nightly-fuzz-min-runs 1 --nightly-fuzz-max-runs 200
```

Output is written under:

```text
artifacts/launch-readiness/<timestamp>-<shortsha>/
```

The script fails on a dirty worktree by default so signoff artifacts always map to a committed state.
It executes readiness checks with checklist parsing disabled internally, then runs checklist validation once to emit both text and JSON reports.

## Bundle Contents

- `metadata.json`: machine-readable run metadata (commit, branch, toolchain, gate status).
- `SUMMARY.md`: human-readable summary for release review.
- `readiness-gate.log`: full readiness gate output (unless `--skip-gate` is used).
- `go-no-go-checklist-report.txt`: parsed checklist status report (and strict-go failure details when enabled).
- `go-no-go-checklist-report.json`: machine-readable checklist summary for automation/reporting.
- `SHA256SUMS.txt`: checksums for captured policy/workflow/script snapshots.
- launch-critical snapshots:
  - `launch-readiness-gates.md`
  - `mainnet-go-no-go-checklist.md`
  - `incident-communication-templates.md`
  - `ci.yml`
  - `nightly-fuzz.yml`
  - `launch-evidence.yml`
  - `launch-rehearsal.yml`
  - `release-manifest.yml`
  - `run_readiness_gate.sh`
  - `check_go_no_go_checklist.sh`
  - `check_nightly_fuzz_health.sh`
  - `check_issue1_core_goals.sh`
  - `run_launch_rehearsal.sh`
  - `generate_release_manifest.sh`

## Validate Go/No-Go Checklist

```bash
# Parse and summarize checklist status
./scripts/launch/check_go_no_go_checklist.sh

# Enforce GO criteria (all gates done + signoff populated)
./scripts/launch/check_go_no_go_checklist.sh --require-go

# Validate expected checklist structure (default is 16 gates)
./scripts/launch/check_go_no_go_checklist.sh --expected-gates 16
```

## Optional Modes

```bash
# Snapshot metadata/docs without running readiness gate
./scripts/launch/generate_evidence_bundle.sh --skip-gate

# Write bundle to custom root directory
./scripts/launch/generate_evidence_bundle.sh --out-dir /tmp/launch-evidence

# Allow dirty worktree for local drafting only (not for signoff evidence)
./scripts/launch/generate_evidence_bundle.sh --allow-dirty

# Enforce strict GO criteria from checklist
./scripts/launch/generate_evidence_bundle.sh --require-go

# Override nightly fuzz health branch target
./scripts/launch/generate_evidence_bundle.sh --check-nightly-fuzz-health --nightly-fuzz-branch main

# Override nightly fuzz workflow/window criteria
./scripts/launch/generate_evidence_bundle.sh --check-nightly-fuzz-health --nightly-fuzz-workflow "Nightly Fuzz" --nightly-fuzz-window-days 14 --nightly-fuzz-min-runs 10 --nightly-fuzz-max-runs 500

# Permit in-progress nightly runs during active maintenance windows
./scripts/launch/generate_evidence_bundle.sh --check-nightly-fuzz-health --nightly-fuzz-allow-in-progress

# Skip Issue #1 target suites for quick local iteration (not for signoff evidence)
./scripts/launch/generate_evidence_bundle.sh --skip-issue1-goal-checks
```

Use `--skip-gate` only for documentation snapshots, not for launch signoff evidence.
For full end-to-end orchestration with a rehearsal summary wrapper, use `./scripts/launch/run_launch_rehearsal.sh`.
For release binary checksum records, generate a separate artifact manifest via `./scripts/launch/generate_release_manifest.sh`.

## Generate in GitHub Actions

Use workflow `.github/workflows/launch-evidence.yml` via manual dispatch:

1. choose `mode` (`smoke` or `full`)
2. optionally set `include_fuzz=true`
3. optionally set `check_nightly_fuzz_health=true` and tune:
   `nightly_fuzz_branch`, `nightly_fuzz_workflow`, `nightly_fuzz_window_days`, `nightly_fuzz_min_runs`, `nightly_fuzz_max_runs`, `nightly_fuzz_allow_in_progress`
4. optionally set `skip_issue1_goal_checks=true` only for fast iteration runs
5. set `require_go=true` for final signoff runs (will fail until checklist is fully complete)
6. download the uploaded `launch-evidence-*` artifact for signoff records
