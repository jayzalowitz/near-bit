# Bitcoin Infinity Documentation Hub

This hub links all major documentation for building, integrating, operating, and auditing Bitcoin Infinity.

## Start Here

- [Project README](../README.md): high-level overview, repository layout, and CI parity commands.
- [Quick Start Guide](../QUICKSTART.md): stand up a local network and send the first transactions.
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
- [TPS Benchmark Methodology](benchmark-methodology.md): measurement standards and published benchmark profiles.

## Release Documentation

- [Launch Readiness Gates](launch-readiness-gates.md): tracked launch blockers split into repository-verifiable and external dependencies.
- [Mainnet Go / No-Go Checklist](mainnet-go-no-go-checklist.md): 16 required launch decision gates with owner/evidence fields.
- [Launch Evidence Bundle](launch-evidence-bundle.md): reproducible artifact packaging for launch rehearsals and signoff.
- [Launch Rehearsal Runner](launch-rehearsal.md): single-command orchestration for readiness, evidence, and checklist reporting.
- [Nightly Fuzz Health Check](nightly-fuzz-health-check.md): enforce 7-day nightly fuzz stability gate for launch signoff.
- [Release Artifact Manifest](release-artifact-manifest.md): checksummed release binaries plus machine-readable metadata for candidate commits.
- [Issue #11 Execution Report](issue-11-execution-report.md): launch-plan execution details and completion artifacts.

## Recommended Reading Order

1. `README.md`
2. `QUICKSTART.md`
3. `architecture-overview.md`
4. `rpc-integration-playbook.md`
5. `security-and-threat-model.md`
6. `validator-operations-runbook.md`
7. `incident-communication-templates.md`
8. `benchmark-methodology.md`
9. `launch-readiness-gates.md`
10. `mainnet-go-no-go-checklist.md`
11. `launch-evidence-bundle.md`
12. `launch-rehearsal.md`
13. `nightly-fuzz-health-check.md`
14. `release-artifact-manifest.md`
