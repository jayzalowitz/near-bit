# Blog Draft: From Bitcoin UTXO Snapshot to Deterministic Genesis

Status: Draft for technical review

## Why This Matters

A chain that claims Bitcoin-address continuity must prove two things:

1. it can transform UTXO state into genesis state deterministically
2. its declared supply is auditable against source inputs

This post summarizes the implemented pipeline and the verification controls around it.

## Input Model

The genesis pipeline consumes Bitcoin UTXO snapshot data and converts balances into account records.

At launch-readiness level, three checks are required:

1. deterministic genesis hash stability
2. internal supply reconciliation in `genesis.json`
3. reconciliation against `bitcoin-cli gettxoutsetinfo total_amount`

## Generation Flow

High-level flow:

1. parse snapshot input and normalize address-bearing entries
2. aggregate balances into account-compatible records
3. emit `genesis.json` and associated metadata
4. run deterministic rerun and compare output hash
5. run supply verification commands

## Determinism Check

Determinism is tested by running generation twice with identical inputs and comparing SHA256 of the generated `genesis.json`.

Reference command:

```bash
./scripts/launch/check_genesis_determinism.sh --testnet --expected-hash 95f3e2600eec0dcd3ca51bf530f46ac963fa3b5286e18c6401efdcae8066aa5d
```

This script also validates internal supply reconciliation for each generated file.

## Internal Supply Reconciliation

Internal reconciliation validates:

1. declared total supply in genesis metadata
2. computed total from account records

Reference command:

```bash
cargo run -q -p bitinfinity-tools -- verify-genesis --genesis /path/to/genesis.json --json-out /tmp/genesis-verify.json
```

## Snapshot-versus-Genesis Reconciliation

To tie genesis state back to Bitcoin snapshot totals, launch flow includes a satoshi-level comparison against `gettxoutsetinfo`.

Reference command:

```bash
./scripts/launch/check_snapshot_supply_reconciliation.sh --genesis /path/to/genesis.json --txoutsetinfo /path/to/gettxoutsetinfo.json --tolerance-sats 1
```

This emits machine-readable output and fails if the difference exceeds configured tolerance.

## Why Tolerance Exists

The check is satoshi-based and expects exact conversion in normal conditions. A tolerance parameter exists to make policy explicit and prevent hidden assumptions:

1. tolerance defaults are encoded in scripts
2. non-default tolerance is visible in command args and evidence metadata

## Launch Control Integration

These checks are not optional ad hoc commands. They are wired into:

1. readiness gate execution
2. evidence bundle generation
3. launch rehearsal orchestration

This means each launch rehearsal carries deterministic and reconciliation evidence tied to commit SHA.

## Operator Guidance

For signoff-grade runs:

1. run on clean commit
2. archive command output and JSON summaries
3. include snapshot input hashes in evidence artifacts
4. keep exact command lines in launch records

## Next Reads

1. `genesis-determinism-check.md`
2. `snapshot-supply-reconciliation.md`
3. `launch-evidence-bundle.md`
4. `launch-rehearsal.md`
