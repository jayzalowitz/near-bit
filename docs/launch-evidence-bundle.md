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
```

Output is written under:

```text
artifacts/launch-readiness/<timestamp>-<shortsha>/
```

The script fails on a dirty worktree by default so signoff artifacts always map to a committed state.

## Bundle Contents

- `metadata.json`: machine-readable run metadata (commit, branch, toolchain, gate status).
- `SUMMARY.md`: human-readable summary for release review.
- `readiness-gate.log`: full readiness gate output (unless `--skip-gate` is used).
- `go-no-go-checklist-report.txt`: parsed checklist status report (and strict-go failure details when enabled).
- `SHA256SUMS.txt`: checksums for captured policy/workflow/script snapshots.
- launch-critical snapshots:
  - `launch-readiness-gates.md`
  - `mainnet-go-no-go-checklist.md`
  - `incident-communication-templates.md`
  - `ci.yml`
  - `nightly-fuzz.yml`
  - `run_readiness_gate.sh`
  - `check_go_no_go_checklist.sh`

## Validate Go/No-Go Checklist

```bash
# Parse and summarize checklist status
./scripts/launch/check_go_no_go_checklist.sh

# Enforce GO criteria (all gates done + signoff populated)
./scripts/launch/check_go_no_go_checklist.sh --require-go
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
```

Use `--skip-gate` only for documentation snapshots, not for launch signoff evidence.

## Generate in GitHub Actions

Use workflow `.github/workflows/launch-evidence.yml` via manual dispatch:

1. choose `mode` (`smoke` or `full`)
2. optionally set `include_fuzz=true`
3. set `require_go=true` for final signoff runs (will fail until checklist is fully complete)
4. download the uploaded `launch-evidence-*` artifact for signoff records
