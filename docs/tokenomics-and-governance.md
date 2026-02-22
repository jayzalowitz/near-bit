# Tokenomics and Governance

This document summarizes supply constraints, emission strategy, and governance-sensitive controls.

## Supply Invariant

Bitcoin Infinity targets the Bitcoin-style hard cap:

- `21,000,000` total coin equivalent
- denomination pipeline supports satoshi and sub-satoshi precision mapping (`finney` / yocto units)

## Emission Model

- halving-oriented issuance schedule is preserved conceptually
- PoS execution requires wall-clock-aware adaptation rather than PoW block-count assumptions
- staking rewards replace miner rewards in security economics

## Patoshi Floor Policy

Patoshi-associated balances are handled with explicit constraints.

Key principles:

- genesis-aligned floor behavior is policy-protected
- allowed movements are constrained by runtime rules
- supply/accounting semantics avoid accidental or policy-bypassing dilution

## Why This Matters

- preserves Bitcoin scarcity narrative
- keeps issuance predictable for validators and integrators
- maintains explicit policy around historically sensitive balances

## Governance Surfaces

Governance-sensitive controls include:

- security posture controls (for example, quantum-readiness activation paths)
- policy transitions requiring validator consensus coordination
- runtime-level toggles that must be documented and auditable

## Operational Recommendations

- tie any governance-sensitive change to a documented proposal and rollout plan
- record expected state transition and rollback path before activation
- publish post-change validation artifacts (tests, metrics, and run logs)

## Related Documents

- [Security and Threat Model](security-and-threat-model.md)
- [Benchmark Methodology](benchmark-methodology.md)
- [Issue #11 Execution Report](issue-11-execution-report.md)
