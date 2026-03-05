# Mainnet Go / No-Go Checklist

Last updated: March 3, 2026.

This checklist is the decision artifact for mainnet launch. A `GO` decision requires every item below to be explicitly marked complete with owner, evidence, and date.

Decision status: `NO-GO` (default until all items complete)

## Signoff Block

- Release candidate commit:
- Proposed genesis hash:
- Planned launch window (UTC):
- Final decision:
- Decision timestamp (UTC):
- Signoff approvers:

## Gate Checklist (16 Required Items)

| # | Gate | Owner | Status (`todo`/`done`) | Evidence (link/path) | Completed date (UTC) |
|---|---|---|---|---|---|
| 1 | External audit report published with zero open Critical findings |  | todo |  |  |
| 2 | Zero open High findings or written accepted-risk waiver signed by approvers |  | todo |  |  |
| 3 | Local launch gate command (`./scripts/launch/run_readiness_gate.sh --full`) passes on release candidate |  | todo |  |  |
| 4 | Nightly fuzz matrix has no unresolved crashes for previous 7 days |  | todo |  |  |
| 5 | Patoshi guard tests and integration validation complete |  | todo |  |  |
| 6 | Tier 1/Tier 2 RPC compatibility tests pass against release candidate |  | todo |  |  |
| 7 | Sparrow end-to-end send/receive/PSBT walkthrough validated on testnet |  | todo |  |  |
| 8 | Benchmark methodology report and raw artifacts published for release candidate |  | todo |  |  |
| 9 | Genesis determinism validated (same snapshot -> same hash across reruns) |  | todo |  |  |
| 10 | Snapshot block height and supply reconciliation documented |  | todo |  |  |
| 11 | Mainnet validator set confirmed (independent operators + contact matrix) |  | todo |  |  |
| 12 | Monitoring + alerting + status page tested with simulated failure |  | todo |  |  |
| 13 | Incident communication templates pre-filled for launch window |  | todo |  |  |
| 14 | Legal review signoff complete (token classification + Patoshi constraints) |  | todo |  |  |
| 15 | Foundation governance + treasury controls published (multisig, charter, spending policy) |  | todo |  |  |
| 16 | Rollback/abort procedure dry-run completed with validator operators |  | todo |  |  |

## Decision Rules

1. Any unresolved `todo` item forces `NO-GO`.
2. Any newly discovered P0/P1 issue during final rehearsal forces `NO-GO` until remediated and re-verified.
3. Waivers are allowed only for non-security items and require named approver signoff plus public rationale.
4. Any gate marked `done` must include both Evidence and Completed date (UTC) before signoff.
