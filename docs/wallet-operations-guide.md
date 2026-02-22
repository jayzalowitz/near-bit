# Wallet Operations Guide

This guide summarizes wallet behavior and operational flows when using Bitcoin Core-style RPC against Bitcoin Infinity.

## Core Principle

Your Bitcoin key material remains the identity anchor. Wallet-facing RPC behavior is adapted to Bitcoin Infinity runtime semantics while preserving compatibility where practical.

## Lifecycle States

Wallet context can be in one of these effective states:

- not loaded
- loaded and unlocked
- loaded and locked (for encrypted wallet mode)

Many signing and send paths require unlocked state.

## Common Lifecycle Flow

1. Load or create wallet context.
2. Unlock wallet (when encryption is enabled).
3. Perform signing/sending operations.
4. Optionally lock wallet.
5. Unload wallet when done.

## Operational Guardrails

- `-18` signals missing wallet context.
- `-13` signals locked wallet for operations requiring key use.
- `-14` signals passphrase mismatch.
- `-15` signals encryption/lock state conflict.

For full mapping see [RPC Error Codes](rpc-error-codes.md).

## Transaction Flows

### Direct send flows

Use direct send methods for simple value movement and straightforward wallet UX.

### PSBT flows

Use PSBT flows for hardware-signing, multi-step reviews, or advanced transaction assembly:

- `createpsbt`
- `walletprocesspsbt`
- `finalizepsbt`

## Key Hygiene

- never log raw private key material
- avoid passphrase persistence in shell history
- keep wallet unlock windows minimal
- isolate automation credentials by environment

## Operational Checks Before Production Use

- validate wallet load/unload behavior under restarts
- verify lock/unlock behavior across all signing methods in use
- run negative tests for malformed parameters and invalid amounts
- run smoke tests for mempool visibility of submitted transactions

## Recovery Readiness

Prepare for operator mistakes and restarts by documenting:

- wallet alias naming conventions
- unlock procedures
- process ownership and secret storage paths
- rollback plan for deployment config changes
