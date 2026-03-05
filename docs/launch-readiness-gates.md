# Launch Readiness Gates

Last updated: March 5, 2026.

This document tracks launch-readiness progress for items in [issue #11](https://github.com/jayzalowitz/near-bit/issues/11), with a strict split between:

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
./scripts/launch/check_genesis_determinism.sh --testnet --expected-hash 95f3e2600eec0dcd3ca51bf530f46ac963fa3b5286e18c6401efdcae8066aa5d

# Optional override when intentionally rotating fixture hash
GENESIS_FIXTURE_EXPECTED_HASH=<new_sha256> ./scripts/launch/run_readiness_gate.sh --smoke

# Direct gate #10 snapshot-vs-genesis supply reconciliation
./scripts/launch/check_snapshot_supply_reconciliation.sh --genesis /path/to/genesis.json --txoutsetinfo /path/to/gettxoutsetinfo.json --tolerance-sats 1

# Readiness gate with enforced gate #10 snapshot reconciliation
./scripts/launch/run_readiness_gate.sh --smoke --check-snapshot-supply --snapshot-genesis /path/to/genesis.json --snapshot-txoutsetinfo /path/to/gettxoutsetinfo.json --snapshot-tolerance-sats 1 --snapshot-json-out /tmp/snapshot-supply-check.json

# Optional: isolate build/test artifacts in a custom Cargo target directory
./scripts/launch/run_readiness_gate.sh --full --cargo-target-dir /tmp/bitinfinity-cargo-target

# Gate #13 preparation helper: prefill launch-window incident pack
./scripts/launch/generate_incident_launch_pack.sh --release-version <tag-or-commit> --launch-window-start <YYYY-MM-DDTHH:MM:SSZ> --launch-window-end <YYYY-MM-DDTHH:MM:SSZ> --status-page-url https://status.bitcoininfinity.io --coordination-channel <channel-url-or-name>

# Prefill checklist signoff block with validated launch metadata
./scripts/launch/prefill_go_no_go_signoff.sh --release-commit <sha> --genesis-hash <sha256> --launch-window-start <YYYY-MM-DDTHH:MM:SSZ> --launch-window-end <YYYY-MM-DDTHH:MM:SSZ> --final-decision NO-GO --approvers "<name1>, <name2>"
```

By default, local launch-gate commands write Cargo artifacts to `.context/cargo-target` to avoid mutating tracked `target/` files. In CI, default behavior remains `target/`. Override with `--cargo-target-dir` when needed.

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
| Incident launch-pack generator (gate #13 prefill helper) | complete | `scripts/launch/generate_incident_launch_pack.sh`, `docs/incident-launch-pack.md` |
| Mainnet go/no-go decision checklist template | complete | `docs/mainnet-go-no-go-checklist.md` |
| Go/no-go checklist validator script | complete | `scripts/launch/check_go_no_go_checklist.sh` |
| Go/no-go checklist gate-row update helper | complete | `scripts/launch/update_go_no_go_gate.sh`, `docs/go-no-go-gate-update.md` |
| Go/no-go checklist signoff prefill helper | complete | `scripts/launch/prefill_go_no_go_signoff.sh`, `docs/go-no-go-signoff-prefill.md` |
| Nightly fuzz 7-day health verifier script | complete | `scripts/launch/check_nightly_fuzz_health.sh`, `docs/nightly-fuzz-health-check.md` |
| Issue #1 core-goal verification script | complete | `scripts/launch/check_issue1_core_goals.sh`, `docs/issue1-core-goal-check.md` |
| Genesis determinism verifier script (gate #9) | complete | `scripts/launch/check_genesis_determinism.sh`, `docs/genesis-determinism-check.md` |
| Genesis supply reconciliation verifier (gate #10 primitive) | complete | `bitinfinity-tools verify-genesis`, `docs/genesis-determinism-check.md` |
| Snapshot supply reconciliation verifier (gate #10 signoff) | complete | `scripts/launch/check_snapshot_supply_reconciliation.sh`, `bitinfinity-tools verify-snapshot-supply`, `docs/snapshot-supply-reconciliation.md` |
| Launch evidence bundle generator | complete | `scripts/launch/generate_evidence_bundle.sh`, `docs/launch-evidence-bundle.md` |
| Launch rehearsal orchestration runner | complete | `scripts/launch/run_launch_rehearsal.sh`, `docs/launch-rehearsal.md` |
| Release artifact checksum manifest generator | complete | `scripts/launch/generate_release_manifest.sh`, `docs/release-artifact-manifest.md` |
| Technical whitepaper baseline document | complete | `docs/technical-whitepaper.md` |
| Launch website update channels (mailing list + GitHub + whitepaper links) | complete | `docs/index.html#launch-updates`, `scripts/launch/run_readiness_gate.sh` |
| Launch communications package (plan + technical draft posts) | complete | `docs/communications-launch-plan.md`, `docs/blog-what-is-bitcoin-infinity.md`, `docs/blog-utxo-to-genesis-deep-dive.md`, `docs/blog-patoshi-balance-floor-explainer.md` |
| Documentation hub and technical guides | complete | `docs/documentation-hub.md` and linked docs |
| Launch execution report for implementation phases | complete | `docs/issue-11-execution-report.md` |

## Latest Verification Snapshot

- `2026-03-04`: `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist` passed locally on commit `fb113bc50` with snapshot-reconciliation doc/syntax checks integrated.
- `2026-03-04`: `./scripts/launch/check_snapshot_supply_reconciliation.sh --genesis <generated genesis> --txoutsetinfo <fixture gettxoutsetinfo.json> --tolerance-sats 0 --json-out <summary>` passed locally on commit `fb113bc50` (`difference_satoshis=0`, `within_tolerance=true`).
- `2026-03-04`: `./scripts/launch/run_readiness_gate.sh --full --include-fuzz --skip-checklist` passed locally on commit `947011618`.
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
- `2026-03-04`: `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist --check-nightly-fuzz-health --nightly-fuzz-workflow CI --nightly-fuzz-branch jayzalowitz/btc-near-fork-plan --nightly-fuzz-window-days 0 --nightly-fuzz-min-runs 0 --nightly-fuzz-max-runs 50 --nightly-fuzz-allow-in-progress` passed locally.
- `2026-03-04`: `./scripts/launch/run_launch_rehearsal.sh --mode smoke --check-nightly-fuzz-health --nightly-fuzz-workflow CI --nightly-fuzz-branch jayzalowitz/btc-near-fork-plan --nightly-fuzz-window-days 0 --nightly-fuzz-min-runs 0 --nightly-fuzz-max-runs 50 --nightly-fuzz-allow-in-progress --skip-release-manifest --allow-dirty` passed locally.
- `2026-03-04`: `./scripts/launch/check_genesis_determinism.sh --testnet --num-accounts 100 --chain-id bitinfinity-mainnet --genesis-time 2026-01-01T00:00:00Z --expected-hash 95f3e2600eec0dcd3ca51bf530f46ac963fa3b5286e18c6401efdcae8066aa5d` passed locally; readiness now enforces this pinned fixture hash.
- `2026-03-04`: `./scripts/launch/check_snapshot_supply_reconciliation.sh --genesis <generated genesis> --txoutsetinfo <fixture gettxoutsetinfo.json> --tolerance-sats 0 --json-out <summary>` passed locally (`difference_satoshis=0`, `within_tolerance=true`).
- `2026-03-04`: `scripts/launch/run_launch_rehearsal.sh` strict-mode pre-manifest cleanup generalized to restore all tracked `target/` file diffs (not only `target/.rustc_info.json`), preventing smoke/full rehearsal test artifacts from blocking manifest generation.
- `2026-03-04`: `./scripts/launch/run_launch_rehearsal.sh --mode smoke --include-release-manifest --release-manifest-skip-build --operator "launch-readiness"` passed locally on commit `196b0f43d` in strict mode after tracked-`target/` restore generalization.
- `2026-03-04`: `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist --check-snapshot-supply --snapshot-genesis <generated genesis> --snapshot-txoutsetinfo <generated gettxoutsetinfo.json> --snapshot-tolerance-sats 0 --snapshot-json-out <summary>` passed locally (`difference_satoshis=0`, `within_tolerance=true`).
- `2026-03-04`: `./scripts/launch/generate_evidence_bundle.sh --mode smoke --allow-dirty --check-snapshot-supply --snapshot-genesis <generated genesis> --snapshot-txoutsetinfo <generated gettxoutsetinfo.json> --snapshot-tolerance-sats 0 --snapshot-json-out <summary> --out-dir <tmp>` passed locally; evidence bundle captured `snapshot-inputs.txt`, `snapshot-gettxoutsetinfo.json`, and `snapshot-supply-check.json`.
- `2026-03-04`: `./scripts/launch/run_launch_rehearsal.sh --mode smoke --allow-dirty --skip-release-manifest --check-snapshot-supply --snapshot-genesis <generated genesis> --snapshot-txoutsetinfo <generated gettxoutsetinfo.json> --snapshot-tolerance-sats 0 --snapshot-json-out <summary> --out-dir <tmp>` passed locally, confirming snapshot pass-through from rehearsal -> evidence -> readiness.
- `2026-03-04`: Added `docs/technical-whitepaper.md` and wired it into launch required-doc checks (`run_readiness_gate.sh`) plus evidence snapshots (`generate_evidence_bundle.sh`).
- `2026-03-04`: `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist` passed locally after whitepaper gating was added.
- `2026-03-04`: `./scripts/launch/generate_evidence_bundle.sh --mode smoke --skip-gate --allow-dirty --out-dir <tmp>` passed locally after whitepaper gating was added; bundle includes `technical-whitepaper.md`.
- `2026-03-05`: Added launch communications pack docs: `communications-launch-plan.md`, `blog-what-is-bitcoin-infinity.md`, `blog-utxo-to-genesis-deep-dive.md`, and `blog-patoshi-balance-floor-explainer.md`.
- `2026-03-05`: `run_readiness_gate.sh` now requires the communications pack docs, and `generate_evidence_bundle.sh` now snapshots them for launch artifact review.
- `2026-03-05`: `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist` passed locally after communications-pack doc gating changes.
- `2026-03-05`: `./scripts/launch/generate_evidence_bundle.sh --mode smoke --skip-gate --allow-dirty --out-dir <tmp>` passed locally and captured `communications-launch-plan.md` plus all three blog drafts.
- `2026-03-05`: Added launch-updates section on `docs/index.html` with mailing-list subscription path plus canonical GitHub and whitepaper links.
- `2026-03-05`: `run_readiness_gate.sh` now enforces launch website channel presence (`Join Launch Updates Mailing List`, `launch-updates@bitcoininfinity.io`, GitHub repo link, and `technical-whitepaper.md`).
- `2026-03-05`: `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist` passed locally after website-channel enforcement was added.
- `2026-03-05`: CI runs `22723211541` (push) and `22723216772` (PR) completed `success` on commit `31ba10e99` across Build/Test/Clippy/Fuzz (smoke)/Security Audit/Format/Launch Readiness.
- `2026-03-05`: `./scripts/launch/run_launch_rehearsal.sh --mode full --include-fuzz --include-release-manifest --release-manifest-skip-build --operator "launch-readiness"` passed locally on commit `31ba10e99` in strict mode (no `--allow-dirty`), producing `artifacts/launch-rehearsals/20260305T145305Z-31ba10e99`.
- `2026-03-05`: Launch scripts (`run_readiness_gate.sh`, `generate_evidence_bundle.sh`, `run_launch_rehearsal.sh`, `generate_release_manifest.sh`) added `--cargo-target-dir` support with local default `.context/cargo-target` and CI default `target/`, preventing tracked `target/` churn during local signoff runs.
- `2026-03-05`: `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist --cargo-target-dir .context/cargo-target-launch` passed locally on commit `4f9bfc7c4`.
- `2026-03-05`: `./scripts/launch/run_launch_rehearsal.sh --mode smoke --skip-release-manifest --skip-issue1-goal-checks --allow-dirty --cargo-target-dir .context/cargo-target-launch --operator "launch-readiness"` passed locally on commit `4f9bfc7c4`.
- `2026-03-05`: `./scripts/launch/generate_release_manifest.sh --skip-build --allow-dirty --cargo-target-dir target --out-dir /tmp/bitinfinity-release-manifests` passed locally on commit `4f9bfc7c4`.
- `2026-03-05`: `check_go_no_go_checklist.sh` now enforces Evidence + Completed-date metadata for any gate marked `done`, including machine-readable reporting (`done_missing_evidence`, `done_missing_completed_date`, `done_invalid_completed_date`).
- `2026-03-05`: `./scripts/launch/check_go_no_go_checklist.sh --json-out /tmp/go-no-go-summary.json` passed locally on commit `1fc0cdace`.
- `2026-03-05`: Manual dispatch workflows now expose optional `cargo_target_dir` inputs and pass-through:
  - `.github/workflows/launch-evidence.yml`
  - `.github/workflows/launch-rehearsal.yml`
  - `.github/workflows/release-manifest.yml`
- `2026-03-05`: Added `scripts/launch/generate_incident_launch_pack.sh` and `docs/incident-launch-pack.md` to prefill launch-window incident communications for gate #13 evidence.
- `2026-03-05`: `bash -n scripts/launch/generate_incident_launch_pack.sh` passed locally and readiness/evidence script syntax checks now include it.
- `2026-03-05`: `./scripts/launch/generate_incident_launch_pack.sh --release-version v1.0.0-rc1 --launch-window-start 2026-03-10T18:00:00Z --launch-window-end 2026-03-10T22:00:00Z --status-page-url https://status.bitcoininfinity.io --coordination-channel '#validators-bridge' --out-file /tmp/incident-launch-pack.md` passed locally.
- `2026-03-05`: `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist --skip-issue1-goal-checks --cargo-target-dir .context/cargo-target-launch` passed locally after incident-launch-pack gating was added.
- `2026-03-05`: `check_go_no_go_checklist.sh` now validates resolvable evidence refs for `done` gates (repo file paths or `http(s)` links), with machine-readable reporting via `done_invalid_evidence_refs`.
- `2026-03-05`: `./scripts/launch/check_go_no_go_checklist.sh --json-out /tmp/go-no-go-summary-evidence-refs.json` passed locally after evidence-ref validation was added.
- `2026-03-05`: `./scripts/launch/generate_evidence_bundle.sh --mode smoke --skip-gate --allow-dirty --cargo-target-dir .context/cargo-target-launch --out-dir /tmp/evidence-refs-check2` passed locally after evidence-ref validation was added.
- `2026-03-05`: `check_go_no_go_checklist.sh` now validates signoff-field formats (`release candidate commit`, `proposed genesis hash`, `planned launch window`, `final decision`, `decision timestamp`) and reports `invalid_signoff_format`.
- `2026-03-05`: `./scripts/launch/check_go_no_go_checklist.sh --json-out /tmp/go-no-go-signoff-format.json` passed locally after signoff-format validation was added.
- `2026-03-05`: `./scripts/launch/generate_evidence_bundle.sh --mode smoke --skip-gate --allow-dirty --cargo-target-dir .context/cargo-target-launch --out-dir /tmp/evidence-signoff-format` passed locally after signoff-format validation was added.
- `2026-03-05`: Added signoff prefill helper `scripts/launch/prefill_go_no_go_signoff.sh` and companion guide `docs/go-no-go-signoff-prefill.md`; readiness/evidence checks now include them.
- `2026-03-05`: `./scripts/launch/prefill_go_no_go_signoff.sh --file /tmp/mainnet-go-no-go-checklist.prefill.md --release-commit 3dcd38186 --genesis-hash 95f3e2600eec0dcd3ca51bf530f46ac963fa3b5286e18c6401efdcae8066aa5d --launch-window-start 2026-03-10T18:00:00Z --launch-window-end 2026-03-10T22:00:00Z --final-decision NO-GO --approvers \"alice,bob\" --decision-timestamp 2026-03-10T17:55:00Z` passed locally.
- `2026-03-05`: `./scripts/launch/run_readiness_gate.sh --smoke --skip-checklist --skip-issue1-goal-checks --cargo-target-dir .context/cargo-target-launch` and `./scripts/launch/generate_evidence_bundle.sh --mode smoke --skip-gate --allow-dirty --cargo-target-dir .context/cargo-target-launch --out-dir /tmp/evidence-signoff-prefill` passed locally with signoff-prefill wiring enabled.
- `2026-03-05`: `run_launch_rehearsal.sh` now computes `go_ready=true` only when all strict checklist quality counters are zero (including `invalid_signoff_format`, `done_missing_evidence`, `done_missing_completed_date`, `done_invalid_completed_date`, and `done_invalid_evidence_refs`), not just todo/invalid/missing-signoff.
- `2026-03-05`: `check_go_no_go_checklist.sh` now enforces Owner metadata for any gate marked `done` (`done_missing_owner`), and `run_launch_rehearsal.sh` includes this counter in strict `go_ready` computation and summary outputs.
- `2026-03-05`: `run_readiness_gate.sh` now enforces placeholder-marker-free content across the entire required launch-doc set (not only a subset), preventing signoff with unresolved placeholders in any required launch artifact.
- `2026-03-05`: Added checklist gate-row helper `scripts/launch/update_go_no_go_gate.sh` and guide `docs/go-no-go-gate-update.md`; readiness/evidence checks now include both.
- `2026-03-05`: `./scripts/launch/update_go_no_go_gate.sh --file /tmp/mainnet-go-no-go-checklist.update.md --gate 13 --status done --owner "ops-lead" --evidence "docs/incident-launch-pack.md" --completed-date 2026-03-05` and `./scripts/launch/check_go_no_go_checklist.sh --file /tmp/mainnet-go-no-go-checklist.update.md --json-out /tmp/go-no-go-gate-update-check.json` passed locally.
- `2026-03-05`: `generate_evidence_bundle.sh` now copies strict checklist totals (todo/invalid/missing-signoff/invalid-signoff-format/done metadata counters) into `metadata.json` and `SUMMARY.md` for machine + human launch reviews.
- `2026-03-05`: `./scripts/launch/generate_evidence_bundle.sh --mode smoke --skip-gate --allow-dirty --cargo-target-dir .context/cargo-target-launch --out-dir /tmp/evidence-checklist-totals` passed locally with checklist totals present in bundle summary/metadata.
- `2026-03-05`: `check_go_no_go_checklist.sh` now fails contradictory decision state when final signoff decision is `GO` but unresolved requirements remain, and `--require-go` now additionally requires final decision field `GO`.
- `2026-03-05`: `./scripts/launch/prefill_go_no_go_signoff.sh --file /tmp/mainnet-go-no-go-checklist.go-invalid.md --release-commit 1a6189961 --genesis-hash 95f3e2600eec0dcd3ca51bf530f46ac963fa3b5286e18c6401efdcae8066aa5d --launch-window-start 2026-03-10T18:00:00Z --launch-window-end 2026-03-10T22:00:00Z --final-decision GO --approvers "alice,bob" --decision-timestamp 2026-03-10T17:55:00Z` followed by `./scripts/launch/check_go_no_go_checklist.sh --file /tmp/mainnet-go-no-go-checklist.go-invalid.md --json-out /tmp/go-no-go-go-invalid.json` correctly failed locally (`totals.inconsistent_go_decision=1`).
- `2026-03-05`: `prefill_go_no_go_signoff.sh` now requires explicit `--allow-go` when setting final decision `GO`, preventing accidental premature GO signoff during checklist prefill.
- `2026-03-05`: `./scripts/launch/prefill_go_no_go_signoff.sh --file /tmp/mainnet-go-no-go-checklist.go-no-allow.md --release-commit 1a6189961 --genesis-hash 95f3e2600eec0dcd3ca51bf530f46ac963fa3b5286e18c6401efdcae8066aa5d --launch-window-start 2026-03-10T18:00:00Z --launch-window-end 2026-03-10T22:00:00Z --final-decision GO --approvers "alice,bob" --decision-timestamp 2026-03-10T17:55:00Z` correctly failed until `--allow-go` was provided.
- `2026-03-05`: Evidence/rehearsal summary pipelines now propagate `inconsistent_go_decision` alongside other strict checklist counters in both machine-readable and human-readable outputs (`generate_evidence_bundle.sh`, `run_launch_rehearsal.sh`).
- `2026-03-05`: `./scripts/launch/generate_evidence_bundle.sh --mode smoke --skip-gate --allow-dirty --cargo-target-dir .context/cargo-target-launch --out-dir /tmp/evidence-checklist-inconsistent-go` and `./scripts/launch/run_launch_rehearsal.sh --mode smoke --skip-release-manifest --allow-dirty --skip-issue1-goal-checks --cargo-target-dir .context/cargo-target-launch --operator "launch-readiness"` passed locally with `checklist_inconsistent_go_decision` recorded in bundle/rehearsal summaries.
- `2026-03-05`: `check_go_no_go_checklist.sh` now validates that the signoff release candidate commit SHA resolves to an actual commit in this repository (not just format-valid hex).
- `2026-03-05`: `./scripts/launch/check_go_no_go_checklist.sh --file /tmp/mainnet-go-no-go-checklist.bad-commit.md --json-out /tmp/go-no-go-bad-commit.json` correctly failed locally after signoff prefill with non-existent commit SHA.
- `2026-03-05`: `.github/workflows/ci.yml` now supports `workflow_dispatch` for on-demand full CI execution against a selected ref, enabling launch-readiness reruns when push-trigger queues are delayed.
- `2026-03-05`: `./scripts/launch/run_readiness_gate.sh --smoke --cargo-target-dir .context/cargo-target-launch` passed locally (`at=2026-03-05T16:35:01Z`) after CI manual-dispatch support was added.

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
