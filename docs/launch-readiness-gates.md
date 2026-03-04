# Launch Readiness Gates

Last updated: March 4, 2026.

This document tracks launch-readiness progress for items in [issue #11](https://github.com/infinitoshi/near-bit/issues/11), with a strict split between:

- repository-verifiable gates (provable from source + local commands)
- external gates (audit/legal/infra/community items that require out-of-repo execution)

## Local Gate Runner

Use one command path for repeatable local verification:

```bash
# Fast iteration gate
./scripts/launch/run_readiness_gate.sh --smoke

# Full engineering gate used before launch rehearsals
./scripts/launch/run_readiness_gate.sh --full

# Full gate + optional fuzz smoke
./scripts/launch/run_readiness_gate.sh --full --include-fuzz

# Full gate + enforced 7-day nightly fuzz health
./scripts/launch/run_readiness_gate.sh --full --check-nightly-fuzz-health

# Full gate + explicit nightly fuzz criteria
./scripts/launch/run_readiness_gate.sh --full --check-nightly-fuzz-health --nightly-fuzz-branch main --nightly-fuzz-workflow "Nightly Fuzz" --nightly-fuzz-window-days 7 --nightly-fuzz-min-runs 1 --nightly-fuzz-max-runs 200

# Optional for faster iteration only: skip Issue #1 target test suites
./scripts/launch/run_readiness_gate.sh --smoke --skip-issue1-goal-checks

# Direct gate #9 deterministic genesis hash verification
./scripts/launch/check_genesis_determinism.sh --testnet
```

## Repository-Verifiable Gates

| Gate | Status | Evidence |
|---|---|---|
| Build/test/lint/format automation | complete | `.github/workflows/ci.yml`, local gate script |
| Dependency vulnerability audit automation | complete | `.github/workflows/ci.yml` audit job + `cargo audit` parity commands |
| Fuzz targets + nightly matrix workflow | complete (workflow), execution depends on Actions billing | `.github/workflows/nightly-fuzz.yml`, crate fuzz targets |
| RPC auth-coverage guard | complete | `scripts/check_auth_coverage.sh` + CI/test integration |
| Benchmark methodology and artifact schema | complete | `docs/benchmark-methodology.md`, `scripts/benchmark/run_tps_profiles.sh` |
| Operations runbook | complete | `docs/validator-operations-runbook.md` |
| Incident communication templates | complete | `docs/incident-communication-templates.md` |
| Mainnet go/no-go decision checklist template | complete | `docs/mainnet-go-no-go-checklist.md` |
| Go/no-go checklist validator script | complete | `scripts/launch/check_go_no_go_checklist.sh` |
| Nightly fuzz 7-day health verifier script | complete | `scripts/launch/check_nightly_fuzz_health.sh`, `docs/nightly-fuzz-health-check.md` |
| Issue #1 core-goal verification script | complete | `scripts/launch/check_issue1_core_goals.sh`, `docs/issue1-core-goal-check.md` |
| Genesis determinism verifier script (gate #9) | complete | `scripts/launch/check_genesis_determinism.sh`, `docs/genesis-determinism-check.md` |
| Launch evidence bundle generator | complete | `scripts/launch/generate_evidence_bundle.sh`, `docs/launch-evidence-bundle.md` |
| Launch rehearsal orchestration runner | complete | `scripts/launch/run_launch_rehearsal.sh`, `docs/launch-rehearsal.md` |
| Release artifact checksum manifest generator | complete | `scripts/launch/generate_release_manifest.sh`, `docs/release-artifact-manifest.md` |
| Documentation hub and technical guides | complete | `docs/documentation-hub.md` and linked docs |
| Launch execution report for implementation phases | complete | `docs/issue-11-execution-report.md` |

## Latest Verification Snapshot

- `2026-03-04`: `./scripts/launch/run_readiness_gate.sh --full --include-fuzz --skip-checklist` passed locally on commit `e01fe7a32`.
- `2026-03-04`: `./scripts/launch/run_launch_rehearsal.sh --mode smoke --include-release-manifest --release-manifest-skip-build --operator "launch-readiness"` passed locally on commit `3cdce7b63` in clean-worktree mode (no `--allow-dirty`).
- `2026-03-04`: `scripts/launch/run_launch_rehearsal.sh` fixed to stage evidence/manifest generation in a temporary directory before writing to `artifacts/`, preventing false dirty-worktree failures during strict rehearsal execution.
- `2026-03-04`: `./scripts/launch/run_launch_rehearsal.sh --mode full --include-fuzz --release-manifest-skip-build --operator "launch-readiness"` passed locally on commit `783a956b1` in strict mode (no `--allow-dirty`).
- `2026-03-04`: `scripts/launch/run_launch_rehearsal.sh` updated to restore generated `target/.rustc_info.json` before strict release-manifest execution so full-gate build metadata does not block manifest generation.
- `2026-03-04`: `./scripts/launch/check_genesis_determinism.sh --testnet --num-accounts 32 --json-out /tmp/genesis-determinism.json` passed locally on commit `5437c6dc7`.
- `2026-03-04`: `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist` passed locally on commit `5437c6dc7` with gate #9 deterministic-genesis verification enabled by default.
- Rehearsal metadata now includes operator attribution via `--operator` (or workflow actor in CI).
- `2026-03-04`: CI run `22652391057` (commit `d1fd2c22d`) completed success across Build/Test/Clippy/Fuzz (smoke)/Security Audit/Format/Launch Readiness.
- `2026-03-04`: `./scripts/launch/check_issue1_core_goals.sh` passed locally (`near-account-id`: `10 passed`; `bitinfinity-tools`: `22 passed`, `1 ignored`).
- `2026-03-04`: `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist --check-nightly-fuzz-health --nightly-fuzz-workflow CI --nightly-fuzz-branch infinitoshi/btc-near-fork-plan --nightly-fuzz-window-days 0 --nightly-fuzz-min-runs 0 --nightly-fuzz-max-runs 50 --nightly-fuzz-allow-in-progress` passed locally.
- `2026-03-04`: `./scripts/launch/run_launch_rehearsal.sh --mode smoke --check-nightly-fuzz-health --nightly-fuzz-workflow CI --nightly-fuzz-branch infinitoshi/btc-near-fork-plan --nightly-fuzz-window-days 0 --nightly-fuzz-min-runs 0 --nightly-fuzz-max-runs 50 --nightly-fuzz-allow-in-progress --skip-release-manifest --allow-dirty` passed locally.

## External Gates (Not Solvable by Repository Changes Alone)

These remain launch blockers until completed by operations/legal/security workstreams:

1. External security audit engagement, fixes, and public report publication.
2. Bug bounty platform launch and triage policy publication.
3. Legal opinions (US/EU/Singapore) for Patoshi restrictions and token classification.
4. Foundation charter/multisig governance publication.
5. Public testnet infra gates (independent validators, monitoring, status page, faucet, explorer, snapshots).
6. Mainnet go/no-go signoff with named approvers and dated checklist artifacts.

## Suggested Launch-Rehearsal Exit Criteria

Before each launch rehearsal, require all of:

1. `./scripts/launch/run_readiness_gate.sh --full --check-nightly-fuzz-health` passes on the target commit.
2. Latest benchmark artifact and summary are attached to the rehearsal record.
3. Incident communication templates are pre-filled for current version/epoch window.
4. A launch evidence bundle is generated for the target commit and attached to the rehearsal record.
5. A release artifact manifest is generated and attached (`SHA256SUMS.txt` + `metadata.json`).
6. A launch rehearsal summary is generated and attached (`SUMMARY.md` + `summary.json`).
7. This document is updated with rehearsal date, commit SHA, and operator signoff.
