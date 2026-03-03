# Launch Readiness Gates

Last updated: March 3, 2026.

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
| Launch evidence bundle generator | complete | `scripts/launch/generate_evidence_bundle.sh`, `docs/launch-evidence-bundle.md` |
| Documentation hub and technical guides | complete | `docs/documentation-hub.md` and linked docs |
| Launch execution report for implementation phases | complete | `docs/issue-11-execution-report.md` |

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

1. `./scripts/launch/run_readiness_gate.sh --full` passes on the target commit.
2. Latest benchmark artifact and summary are attached to the rehearsal record.
3. Incident communication templates are pre-filled for current version/epoch window.
4. A launch evidence bundle is generated for the target commit and attached to the rehearsal record.
5. This document is updated with rehearsal date, commit SHA, and operator signoff.
