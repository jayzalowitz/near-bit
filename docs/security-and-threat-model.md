# Security and Threat Model

This document describes security assumptions, primary risks, and mitigation strategy for Bitcoin Infinity.

## Security Scope

Covers:

- RPC boundary and adapter behavior
- signature/account verification paths
- wallet operation semantics
- supply and governance-sensitive runtime rules
- CI and fuzzing assurance posture

## Trust Boundaries

- Client boundary: wallets and automation calling JSON-RPC.
- Adapter boundary: `bitinfinity-btcrpc` request validation and translation.
- Runtime boundary: nearcore fork execution and consensus.
- Operator boundary: key/passphrase handling and deployment configuration.

## Primary Threats

- malformed JSON-RPC payloads targeting parser/decoder paths
- amount and arithmetic edge-case exploitation
- wallet state confusion (loaded/locked transitions)
- signature/account mismatch bypass attempts
- supply/floor policy bypass attempts
- operational misconfiguration and stale process state

## Mitigations

### Input and protocol hardening

- strict parameter validation and shape checks
- explicit error-code handling paths for malformed inputs
- decode guards for raw tx and PSBT payloads

### Signature and account checks

- secp256k1 verification and account/address correlation checks in runtime paths
- explicit rejection for address/signature mismatch scenarios

### Wallet-state enforcement

- wallet-scoped error signaling (`-18`, `-13`, `-14`, `-15`)
- lock-state checks before key-using operations

### Arithmetic safety

- checked arithmetic in high-risk amount aggregation paths
- sub-satoshi and invalid amount rejection

### Policy protection

- hard-cap and floor-oriented constraints for supply-sensitive balances
- explicit runtime-level restrictions around Patoshi floor behavior

## Assurance Pipeline

- strict clippy and format checks in CI
- workspace and fork-crate test suites
- auth coverage script checks for e2e pathways
- fuzz smoke in CI and long-run nightly fuzz matrix
- benchmark-runner smoke for scripting and orchestration integrity

## Residual Risk Areas

- compatibility drift against upstream Bitcoin Core behavior over time
- complex adapter semantics where account-based behavior diverges from UTXO-native expectations
- operational secret handling by downstream integrators

## Operator Recommendations

- run CI-equivalent checks locally before push
- pin toolchain versions for deterministic results
- enforce secret management and shell history hygiene
- track and triage nightly fuzz artifacts continuously

## Vulnerability Reporting

See repository security guidance in `README.md`.
