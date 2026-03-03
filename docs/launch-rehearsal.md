# Launch Rehearsal Runner

This guide covers the end-to-end launch rehearsal command.

## Purpose

`run_launch_rehearsal.sh` orchestrates:

1. readiness execution
2. evidence bundle generation
3. checklist validation
4. rehearsal-level summary output

This reduces manual sequencing errors and gives a single artifact root for each rehearsal.

## Run a Rehearsal

```bash
# Full rehearsal (default mode is full)
./scripts/launch/run_launch_rehearsal.sh

# Faster iteration rehearsal
./scripts/launch/run_launch_rehearsal.sh --mode smoke

# Strict signoff rehearsal (fails unless checklist is fully GO)
./scripts/launch/run_launch_rehearsal.sh --require-go
```

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

`go_ready=true` in `summary.json` is only set when:

- readiness gate passed
- checklist has zero todo items
- checklist has zero invalid statuses
- checklist has zero missing signoff fields

## GitHub Actions

Use workflow `.github/workflows/launch-rehearsal.yml` (manual dispatch) to run and archive rehearsal artifacts in CI.
For release-binary checksums and metadata, pair rehearsal output with `./scripts/launch/generate_release_manifest.sh`.
