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

## Development and Testing

- [Local Development and Testing](local-dev-and-testing.md): build, lint, test, fuzz, benchmark dry-run.
- [Troubleshooting Guide](troubleshooting.md): common errors and recovery steps.

## Economics, Security, and Governance

- [Tokenomics and Governance](tokenomics-and-governance.md): 21M cap, emission, Patoshi floor, governance controls.
- [Security and Threat Model](security-and-threat-model.md): key risks, assumptions, and mitigations.

## Operations

- [Validator Operations Runbook](validator-operations-runbook.md): node lifecycle, health checks, and incident response.
- [Incident Communication Templates](incident-communication-templates.md): copy-ready templates for investigating, security halt, resolution, and emergency upgrade notices.
- [Launch Communications Plan](communications-launch-plan.md): publication order, ownership, and launch-window messaging controls.
- [TPS Benchmark Methodology](benchmark-methodology.md): measurement standards and published benchmark profiles.

## Communications and Narrative

- [Blog Draft - What Bitcoin Infinity Is](blog-what-is-bitcoin-infinity.md): high-level technical narrative for launch audiences.
- [Blog Draft - UTXO to Genesis Deep Dive](blog-utxo-to-genesis-deep-dive.md): deterministic genesis and reconciliation walkthrough.
- [Blog Draft - Patoshi Balance Floor Explainer](blog-patoshi-balance-floor-explainer.md): technical versus external policy boundaries.

## Release Documentation

- [Launch Readiness Gates](launch-readiness-gates.md): tracked launch blockers split into repository-verifiable and external dependencies.
- [Mainnet Go / No-Go Checklist](mainnet-go-no-go-checklist.md): 16 required launch decision gates with owner/evidence fields.
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
6. `security-and-threat-model.md`
7. `communications-launch-plan.md`
8. `blog-what-is-bitcoin-infinity.md`
9. `blog-utxo-to-genesis-deep-dive.md`
10. `blog-patoshi-balance-floor-explainer.md`
11. `validator-operations-runbook.md`
12. `incident-communication-templates.md`
13. `benchmark-methodology.md`
14. `launch-readiness-gates.md`
15. `mainnet-go-no-go-checklist.md`
16. `launch-evidence-bundle.md`
17. `launch-rehearsal.md`
18. `nightly-fuzz-health-check.md`
19. `issue1-core-goal-check.md`
20. `genesis-determinism-check.md`
21. `snapshot-supply-reconciliation.md`
22. `release-artifact-manifest.md`
