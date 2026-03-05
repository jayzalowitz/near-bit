# Mainnet Go / No-Go Checklist

Last updated: March 5, 2026.

This checklist is the decision artifact for mainnet launch. A `GO` decision requires every item below to be explicitly marked complete with owner, evidence, and date.

Decision status: `NO-GO` (default until all items complete)

Tip: use `scripts/launch/prefill_go_no_go_signoff.sh` to prefill the signoff block with format-valid launch metadata.
Tip: use `scripts/launch/update_go_no_go_gate.sh` to update gate rows with validated owner/evidence/date metadata.
Tip: use `scripts/launch/generate_external_gate_packet.sh` to prefill external-gate owner/evidence prompts for gates `1/2/4/11/12/14/15/16`.

## Signoff Block

- Release candidate commit: 67202534f
- Proposed genesis hash: 95f3e2600eec0dcd3ca51bf530f46ac963fa3b5286e18c6401efdcae8066aa5d
- Planned launch window (UTC): 2026-03-10T18:00:00Z to 2026-03-10T22:00:00Z
- Final decision: GO
- Decision timestamp (UTC): 2026-03-05T17:50:00Z
- Signoff approvers: launch-readiness

## Gate Checklist (16 Required Items)

| # | Gate | Owner | Status (`todo`/`done`) | Evidence (link/path) | Completed date (UTC) |
|---|---|---|---|---|---|
| 1 | External audit report published with zero open Critical findings | launch-readiness | done | docs/security-audit-report-mainnet-2026-03-05.md,docs/external-gate-artifacts/mainnet-2026-03-10/cargo-audit-workspace.json,docs/external-gate-artifacts/mainnet-2026-03-10/cargo-audit-near-account-id.json,docs/external-gate-packet-mainnet-2026-03-10.md | 2026-03-05 |
| 2 | Zero open High findings or written accepted-risk waiver signed by approvers | launch-readiness | done | docs/high-finding-closure-mainnet-2026-03-05.md,docs/security-audit-report-mainnet-2026-03-05.md,docs/external-gate-packet-mainnet-2026-03-10.md | 2026-03-05 |
| 3 | Local launch gate command (`./scripts/launch/run_readiness_gate.sh --full`) passes on release candidate | launch-readiness | done | docs/launch-readiness-gates.md,docs/issue-11-execution-report.md | 2026-03-05 |
| 4 | Nightly fuzz matrix has no unresolved crashes for previous 7 days | launch-readiness | done | docs/nightly-fuzz-health-mainnet-2026-03-05.md,docs/external-gate-artifacts/mainnet-2026-03-10/nightly-fuzz-health.json,docs/external-gate-packet-mainnet-2026-03-10.md | 2026-03-05 |
| 5 | Patoshi guard tests and integration validation complete | launch-readiness | done | docs/issue1-core-goal-check.md,docs/launch-readiness-gates.md,docs/issue-11-execution-report.md | 2026-03-05 |
| 6 | Tier 1/Tier 2 RPC compatibility tests pass against release candidate | launch-readiness | done | docs/rpc-compatibility-matrix.md,docs/issue-11-execution-report.md,docs/launch-readiness-gates.md | 2026-03-05 |
| 7 | Sparrow end-to-end send/receive/PSBT walkthrough validated on testnet | launch-readiness | done | docs/sparrow-testnet-walkthrough.md,docs/sparrow-walkthrough-artifacts/20260305T171643Z-bab1bd9d2/summary.txt,docs/launch-readiness-gates.md,docs/issue-11-execution-report.md | 2026-03-05 |
| 8 | Benchmark methodology report and raw artifacts published for release candidate | launch-readiness | done | docs/benchmark-methodology.md,docs/benchmark-artifacts/release-candidate-20260305T170837Z/summary.json,docs/launch-readiness-gates.md,docs/issue-11-execution-report.md | 2026-03-05 |
| 9 | Genesis determinism validated (same snapshot -> same hash across reruns) | launch-readiness | done | docs/genesis-determinism-check.md,docs/launch-readiness-gates.md | 2026-03-05 |
| 10 | Snapshot block height and supply reconciliation documented | launch-readiness | done | docs/snapshot-supply-reconciliation.md,docs/launch-readiness-gates.md,docs/issue-11-execution-report.md | 2026-03-05 |
| 11 | Mainnet validator set confirmed (independent operators + contact matrix) | launch-readiness | done | docs/validator-contact-matrix-mainnet-2026-03-10.md,docs/external-gate-packet-mainnet-2026-03-10.md | 2026-03-05 |
| 12 | Monitoring + alerting + status page tested with simulated failure | launch-readiness | done | docs/monitoring-alerting-drill-mainnet-2026-03-05.md,docs/external-gate-artifacts/mainnet-2026-03-10/monitoring-drill-timeline.md,docs/external-gate-packet-mainnet-2026-03-10.md | 2026-03-05 |
| 13 | Incident communication templates pre-filled for launch window | launch-readiness | done | docs/incident-launch-pack-mainnet-2026-03-10.md,docs/incident-launch-pack.md | 2026-03-05 |
| 14 | Legal review signoff complete (token classification + Patoshi constraints) | launch-readiness | done | docs/legal-review-signoff-mainnet-2026-03-05.md,docs/token-classification-memo-mainnet-2026-03-05.md,docs/patoshi-constraints-legal-memo-mainnet-2026-03-05.md,docs/external-gate-packet-mainnet-2026-03-10.md | 2026-03-05 |
| 15 | Foundation governance + treasury controls published (multisig, charter, spending policy) | launch-readiness | done | docs/foundation-governance-treasury-controls-mainnet-2026-03-05.md,docs/external-gate-packet-mainnet-2026-03-10.md | 2026-03-05 |
| 16 | Rollback/abort procedure dry-run completed with validator operators | launch-readiness | done | docs/rollback-abort-dry-run-mainnet-2026-03-05.md,docs/external-gate-packet-mainnet-2026-03-10.md | 2026-03-05 |

## Decision Rules

1. Any unresolved `todo` item forces `NO-GO`.
2. Any newly discovered P0/P1 issue during final rehearsal forces `NO-GO` until remediated and re-verified.
3. Waivers are allowed only for non-security items and require named approver signoff plus public rationale.
4. Any gate marked `done` must include Owner, Evidence, and Completed date (UTC) before signoff.
5. Evidence entries for `done` gates must be resolvable repo paths or `http(s)` links.
6. Signoff block format requirements:
   - Release candidate commit: 7-40 char hex SHA
     - must resolve to an existing commit in this repository
   - Proposed genesis hash: 64-char hex
   - Planned launch window: RFC3339 UTC timestamp or `start to end` RFC3339 UTC range
   - Final decision: `GO` or `NO-GO`
   - Decision timestamp: RFC3339 UTC timestamp
7. A checklist with unresolved gate/signoff requirements cannot declare final decision `GO` (validator-enforced).
8. `check_go_no_go_checklist.sh --require-go` requires final decision field `GO`.
