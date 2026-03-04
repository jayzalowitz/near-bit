# Bitcoin Infinity Technical Whitepaper

Version: 0.1  
Date: March 4, 2026

## Abstract

Bitcoin Infinity is a Layer 1 protocol that maps Bitcoin address ownership into a smart-contract-capable execution environment built on the NEAR runtime model. The core objective is operational continuity for Bitcoin holders: the same Bitcoin private keys can authorize transactions on Bitcoin Infinity, while the chain offers fast finality, account-based state, and programmable execution.

This whitepaper documents the implemented architecture, launch constraints, and current security/economic controls as represented in the repository.

## 1. Design Goals

1. Preserve Bitcoin holder control continuity.
   - No claim contract.
   - No bridging prerequisite.
   - Account IDs are Bitcoin addresses.
2. Preserve hard-supply invariants.
   - 21M cap model retained in protocol economics.
3. Add smart contract execution and validator economics.
   - Account-based state and contract calls via NEAR runtime foundations.
4. Keep launch process auditable and reproducible.
   - Deterministic genesis checks.
   - Snapshot-to-genesis supply reconciliation checks.
   - Evidence bundle and rehearsal artifacts.

## 2. Architecture Overview

Bitcoin Infinity consists of three principal layers:

1. Runtime and protocol layer (forked NEAR execution model)
   - Transaction verification.
   - State transitions.
   - Validator-driven consensus/finality.
2. Bitcoin compatibility layer
   - Bitcoin-address account identity support.
   - secp256k1 signature validation and recovery path.
   - Bitcoin-like JSON-RPC compatibility surface.
3. Genesis and launch tooling layer
   - UTXO-derived genesis generation.
   - Patoshi-aware account tagging and controls.
   - Readiness, checklist, evidence, and rehearsal orchestration.

For component map details, see `architecture-overview.md`.

## 3. Identity and Signature Model

### 3.1 Account identity

Bitcoin Infinity accounts use Bitcoin address strings directly as account IDs. Supported formats and validation are implemented in the forked account-ID validation path.

### 3.2 Authorization

Authorization uses secp256k1 signatures for user-originated transactions. Signature verification and recovery logic are integrated so transaction origin can be linked back to Bitcoin-address identity.

### 3.3 Validator keys

Validator operation follows the underlying consensus/runtime requirements and remains separate from end-user Bitcoin-style authorization keys.

## 4. Genesis Construction Model

Genesis generation pipeline:

1. Parse a UTXO snapshot (or deterministic synthetic fixture in test mode).
2. Aggregate balances by supported Bitcoin address type.
3. Emit genesis account records and protocol configuration.
4. Run deterministic verification:
   - identical input => identical `genesis.json` hash.
5. Run supply verification:
   - internal declared total supply equals computed total across records.
6. Run snapshot reconciliation:
   - compare genesis satoshi-equivalent supply against `bitcoin-cli gettxoutsetinfo total_amount`.

Launch scripts expose these checks in reproducible form:

- `scripts/launch/check_genesis_determinism.sh`
- `scripts/launch/check_snapshot_supply_reconciliation.sh`

## 5. Patoshi and Foundation Controls

Bitcoin Infinity includes tooling and runtime pathways intended to support Patoshi-related policy controls and accountability. Current launch readiness includes:

- Patoshi-aware data handling in genesis tooling.
- Dedicated verification suites in `bitinfinity-tools`.
- Launch-check integrations that ensure related code paths remain testable and observable.

Full policy/legal signoff for Patoshi governance remains an external launch dependency and is tracked in `launch-readiness-gates.md`.

## 6. Economics and Units

### 6.1 Supply

- Hard cap model: 21,000,000 coin equivalent.
- Genesis allocation derived from Bitcoin UTXO snapshot state.
- Remaining emission distributed through staking schedule controls.

### 6.2 Denomination model

- 1 BIT = 10^24 yoctoBIT.
- 1 satoshi-equivalent unit maps to 10^16 yoctoBIT.

Detailed economics and governance references are maintained in `tokenomics-and-governance.md`.

## 7. Performance and Throughput Positioning

Bitcoin Infinity does not present an unqualified "1M TPS" empirical claim. Published throughput statements are grounded in documented benchmark runs and methodology:

- Measured single-shard pilot values are published in repository docs.
- Multi-shard scaling statements are described as architectural scaling, not already-observed aggregate production metrics.
- Methodology, command lines, and artifacts are documented in `benchmark-methodology.md`.

## 8. Security Model and Assurance

Security posture is based on layered controls:

1. Static and dynamic quality gates in CI:
   - build, test, clippy, format, audit, fuzz smoke.
2. Dedicated launch gates:
   - checklist parser.
   - nightly fuzz health window checks.
   - issue #1 core-goal verification.
   - deterministic genesis and snapshot reconciliation checks.
3. Reproducible launch artifact generation:
   - evidence bundles with checksums.
   - rehearsal summaries and release manifests.

Threat assumptions and mitigations are documented in `security-and-threat-model.md`.

## 9. Launch Control Plane

Launch process is encoded into scripts so signoff can be re-run on specific commits:

- `scripts/launch/run_readiness_gate.sh`
- `scripts/launch/generate_evidence_bundle.sh`
- `scripts/launch/run_launch_rehearsal.sh`
- `scripts/launch/generate_release_manifest.sh`

These scripts support:

- smoke vs full gate modes.
- optional nightly fuzz health enforcement.
- optional snapshot reconciliation enforcement.
- machine-readable evidence outputs.

## 10. Governance and Operational Boundaries

Repository code and docs cover technical launch readiness gates. Several critical gates remain external by design:

- external security audit publication.
- legal signoff for token and Patoshi constraints.
- multisig/foundation governance publication.
- independent validator and public infra readiness.

The separation between repository-verifiable and external dependencies is tracked in `launch-readiness-gates.md`.

## 11. Non-Goals (Current Scope)

The following are explicitly out of current repository launch scope:

- two-way BTC bridge design and launch.
- Lightning-equivalent payment channel deployment.
- broad non-standard Bitcoin script emulation guarantees.
- consumer wallet UX parity beyond documented tested paths.

## 12. Reproducibility Reference Commands

### 12.1 Core launch smoke gate

```bash
./scripts/launch/run_readiness_gate.sh --smoke
```

### 12.2 Deterministic genesis check

```bash
./scripts/launch/check_genesis_determinism.sh --testnet --expected-hash 95f3e2600eec0dcd3ca51bf530f46ac963fa3b5286e18c6401efdcae8066aa5d
```

### 12.3 Snapshot supply reconciliation

```bash
./scripts/launch/check_snapshot_supply_reconciliation.sh --genesis /path/to/genesis.json --txoutsetinfo /path/to/gettxoutsetinfo.json --tolerance-sats 1
```

### 12.4 Evidence and rehearsal generation

```bash
./scripts/launch/generate_evidence_bundle.sh --mode smoke
./scripts/launch/run_launch_rehearsal.sh --mode smoke
```

## 13. Document Control

This whitepaper is a repository-controlled technical baseline document. It should be updated when any of the following change:

- protocol-level identity/verification behavior.
- supply or emission invariants.
- launch gate logic and required evidence artifacts.
- benchmark claim policy or published throughput interpretation.

