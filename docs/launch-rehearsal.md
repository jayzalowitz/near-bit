# Launch Rehearsal Runner

This guide covers the end-to-end launch rehearsal command.

## Purpose

`run_launch_rehearsal.sh` orchestrates:

1. readiness execution
2. evidence bundle generation
3. checklist validation
4. optional release artifact manifest generation
5. rehearsal-level summary output

This reduces manual sequencing errors and gives a single artifact root for each rehearsal.

## Run a Rehearsal

```bash
# Full rehearsal (default mode is full; includes release manifest)
./scripts/launch/run_launch_rehearsal.sh

# Faster iteration rehearsal
./scripts/launch/run_launch_rehearsal.sh --mode smoke

# Smoke rehearsal + release manifest from existing binaries
./scripts/launch/run_launch_rehearsal.sh --mode smoke --include-release-manifest --release-manifest-skip-build

# Strict signoff rehearsal (fails unless checklist is fully GO)
./scripts/launch/run_launch_rehearsal.sh --require-go

# Set explicit operator/signoff owner in rehearsal metadata
./scripts/launch/run_launch_rehearsal.sh --operator "launch-operator"

# Enforce nightly fuzz 7-day health gate during rehearsal readiness checks
./scripts/launch/run_launch_rehearsal.sh --check-nightly-fuzz-health

# Enforce nightly fuzz health with explicit workflow/window criteria
./scripts/launch/run_launch_rehearsal.sh --check-nightly-fuzz-health --nightly-fuzz-branch main --nightly-fuzz-workflow "Nightly Fuzz" --nightly-fuzz-window-days 7 --nightly-fuzz-min-runs 1 --nightly-fuzz-max-runs 200

# Allow in-progress nightly runs during maintenance windows
./scripts/launch/run_launch_rehearsal.sh --check-nightly-fuzz-health --nightly-fuzz-allow-in-progress

# Skip Issue #1 target suites for quick local iteration (not for signoff rehearsals)
./scripts/launch/run_launch_rehearsal.sh --skip-issue1-goal-checks
```

Release-manifest behavior defaults:

- `--mode full`: manifest generation enabled
- `--mode smoke`: manifest generation disabled

Override with `--include-release-manifest` or `--skip-release-manifest`.

## Outputs

Each run writes under:

```text
artifacts/launch-rehearsals/<timestamp>-<shortsha>/
```

Key files:

- `SUMMARY.md`: human-readable rehearsal result.
- `summary.json`: machine-readable rehearsal result.
- `rehearsal.log`: full command output.
- `evidence/`: nested launch evidence bundle artifacts.
- `release-manifests/`: nested release artifact manifest bundle (when enabled).
- `release-manifest.log`: release manifest command output (when enabled).

Operator metadata:

- `--operator <name>` records ownership/signoff context in `SUMMARY.md` and `summary.json`.
- If omitted, the script uses `git config user.name`, then `$USER`, then `unknown`.

`go_ready=true` in `summary.json` is only set when:

- readiness gate passed
- checklist has zero todo items
- checklist has zero invalid statuses
- checklist has zero missing signoff fields

## GitHub Actions

Use workflow `.github/workflows/launch-rehearsal.yml` (manual dispatch) to run and archive rehearsal artifacts in CI.
The workflow exposes `release_manifest` (`auto|include|skip`) and `release_manifest_skip_build` inputs to control manifest behavior explicitly.
CI rehearsals automatically set `--operator` to `${{ github.actor }}` for attribution.
The workflow also exposes `check_nightly_fuzz_health`, `nightly_fuzz_branch`, `nightly_fuzz_workflow`, `nightly_fuzz_window_days`, `nightly_fuzz_min_runs`, `nightly_fuzz_max_runs`, and `nightly_fuzz_allow_in_progress` inputs for gate #4 enforcement, plus `skip_issue1_goal_checks` for fast iteration runs.
