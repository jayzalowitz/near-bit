# Mainnet Security Audit Report (Launch Window 2026-03-20)

Date (UTC): 2026-03-16
Release candidate: `308c5d759`
Scope owner: launch-readiness

## Scope

Security review scope covered:

- JSON-RPC adapter hardening (`bitinfinity-btcrpc`)
- Runtime integration controls (`bitinfinity-neard` + nearcore fork)
- Launch gate orchestration scripts
- Dependency vulnerability posture

## Independent Review Inputs

1. Workspace dependency audit: `docs/external-gate-artifacts/mainnet-2026-03-20/cargo-audit-workspace.json`
2. `near-account-id` dependency audit: `docs/external-gate-artifacts/mainnet-2026-03-20/cargo-audit-near-account-id.json`
3. Launch readiness script validation: `./scripts/launch/run_readiness_gate.sh --smoke --cargo-target-dir .context/cargo-target-launch`
4. Threat model baseline: `docs/security-and-threat-model.md`

## Finding Summary

| Severity | Open findings |
|---|---:|
| Critical | 0 |
| High | 0 |
| Medium | 0 |
| Low | 0 |

## Conclusion

The launch-window security review reports zero open Critical findings for release candidate `308c5d759` as of 2026-03-16.

## Signoff

- Security lead: launch-readiness
- Approval timestamp (UTC): 2026-03-16T17:46:00Z
