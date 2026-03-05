# Bitcoin Infinity Documentation Hub

This hub links all major documentation for building, integrating, operating, and auditing Bitcoin Infinity.

## Start Here

- [Project README](../README.md): high-level overview, repository layout, and CI parity commands.
- [Quick Start Guide](../QUICKSTART.md): stand up a local network and send the first transactions.
- [Technical Whitepaper](technical-whitepaper.md): architecture, economics, launch model, and security assumptions.
- [Architecture Overview](architecture-overview.md): component map and cross-layer data flow.

## Integration and API

- [RPC Integration Playbook](rpc-integration-playbook.md): endpoint setup, wallet behavior, and request/response patterns.
- [Bitcoin RPC Compatibility Matrix](rpc-compatibility-matrix.md): method-by-method compatibility and adaptation status.
- [RPC Error Codes](rpc-error-codes.md): operational mapping of JSON-RPC error codes.
- [Sparrow-Compatible Testnet Walkthrough](sparrow-testnet-walkthrough.md): end-to-end PSBT, signing, send, and receive validation evidence.

## Development and Testing

- [Local Development and Testing](local-dev-and-testing.md): build, lint, test, fuzz, benchmark dry-run.
- [Troubleshooting Guide](troubleshooting.md): common errors and recovery steps.

## Economics, Security, and Governance

- [Tokenomics and Governance](tokenomics-and-governance.md): 21M cap, emission, Patoshi floor, governance controls.
- [Security and Threat Model](security-and-threat-model.md): key risks, assumptions, and mitigations.

## Operations

- [Validator Operations Runbook](validator-operations-runbook.md): node lifecycle, health checks, and incident response.
- [Incident Communication Templates](incident-communication-templates.md): copy-ready templates for investigating, security halt, resolution, and emergency upgrade notices.
- [Incident Launch Pack](incident-launch-pack.md): prefill launch-window incident templates for go/no-go gate #13 evidence.
- [Launch Communications Plan](communications-launch-plan.md): publication order, ownership, and launch-window messaging controls.
- [TPS Benchmark Methodology](benchmark-methodology.md): measurement standards and published benchmark profiles.

## Communications and Narrative

- [Launch Updates Signup](index.html#launch-updates): mailing-list subscription path plus GitHub and whitepaper links for prelaunch updates.
- [Blog Draft - What Bitcoin Infinity Is](blog-what-is-bitcoin-infinity.md): high-level technical narrative for launch audiences.
- [Blog Draft - UTXO to Genesis Deep Dive](blog-utxo-to-genesis-deep-dive.md): deterministic genesis and reconciliation walkthrough.
- [Blog Draft - Patoshi Balance Floor Explainer](blog-patoshi-balance-floor-explainer.md): technical versus external policy boundaries.

## Release Documentation

- [Launch Readiness Gates](launch-readiness-gates.md): tracked launch blockers split into repository-verifiable and external dependencies.
- [Mainnet Go / No-Go Checklist](mainnet-go-no-go-checklist.md): 16 required launch decision gates with owner/evidence fields.
- [Go/No-Go Gate Update Helper](go-no-go-gate-update.md): safely mark individual checklist gates `done`/`todo` with metadata validation.
- [Go/No-Go Signoff Prefill](go-no-go-signoff-prefill.md): one-command signoff block prefill with format-safe launch metadata.
- [External Gate Packet](external-gate-packet.md): generate a structured signoff packet for external launch blockers.
- [Launch Evidence Bundle](launch-evidence-bundle.md): reproducible artifact packaging for launch rehearsals and signoff.
- [Launch Rehearsal Runner](launch-rehearsal.md): single-command orchestration for readiness, evidence, and checklist reporting.
- [Nightly Fuzz Health Check](nightly-fuzz-health-check.md): enforce 7-day nightly fuzz stability gate for launch signoff.
- [Issue #1 Core-Goal Check](issue1-core-goal-check.md): targeted verification for Bitcoin account-ID and Patoshi/genesis toolchain guarantees.
- [Genesis Determinism Check](genesis-determinism-check.md): enforce same-input genesis hash stability and capture supply metadata.
- [Snapshot Supply Reconciliation](snapshot-supply-reconciliation.md): compare genesis supply against `bitcoin-cli gettxoutsetinfo` snapshot totals.
- [Release Artifact Manifest](release-artifact-manifest.md): checksummed release binaries plus machine-readable metadata for candidate commits.
- [Issue #11 Execution Report](issue-11-execution-report.md): launch-plan execution details and completion artifacts.

## Recommended Reading Order

1. `README.md`
2. `QUICKSTART.md`
3. `technical-whitepaper.md`
4. `architecture-overview.md`
5. `rpc-integration-playbook.md`
6. `sparrow-testnet-walkthrough.md`
7. `security-and-threat-model.md`
8. `communications-launch-plan.md`
9. `blog-what-is-bitcoin-infinity.md`
10. `blog-utxo-to-genesis-deep-dive.md`
11. `blog-patoshi-balance-floor-explainer.md`
12. `index.html#launch-updates`
13. `validator-operations-runbook.md`
14. `incident-communication-templates.md`
15. `incident-launch-pack.md`
16. `benchmark-methodology.md`
17. `launch-readiness-gates.md`
18. `mainnet-go-no-go-checklist.md`
19. `go-no-go-gate-update.md`
20. `go-no-go-signoff-prefill.md`
21. `external-gate-packet.md`
22. `launch-evidence-bundle.md`
23. `launch-rehearsal.md`
24. `nightly-fuzz-health-check.md`
25. `issue1-core-goal-check.md`
26. `genesis-determinism-check.md`
27. `snapshot-supply-reconciliation.md`
28. `release-artifact-manifest.md`
