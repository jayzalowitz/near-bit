# RPC Integration Playbook

This playbook is for wallet and service developers integrating with `bitinfinity-btcrpc`.

## Endpoint Model

`bitinfinity-btcrpc` exposes a Bitcoin Core-compatible JSON-RPC endpoint.

Typical local defaults:

- host: `127.0.0.1`
- port: `8332` (adapter-side, configurable)

## Transport and Envelope

- JSON-RPC 2.0 request/response structure.
- Method and params are validated in the adapter.
- Error codes are compatibility-oriented and documented in [RPC Error Codes](rpc-error-codes.md).

## Method Compatibility Strategy

Methods fall into three buckets:

- Core-like: close behavior to Bitcoin Core semantics.
- Adapted: behavior mapped to account-based/PoS internals.
- Intentional stubs: unsupported by architecture (for example, PoW-only flows).

See [RPC Compatibility Matrix](rpc-compatibility-matrix.md).

## Wallet Lifecycle Expectations

Wallet-scoped calls require a loaded wallet context.

Typical flow:

1. `createwallet` or `loadwallet`
2. unlock where required (`walletpassphrase`)
3. sign/send methods
4. optional `walletlock`
5. optional `unloadwallet`

When wallet context is absent, wallet-scoped calls return `-18`.

## PSBT Workflow

Supported PSBT method set is adapted for Bitcoin Infinity execution.

Canonical flow:

1. `createpsbt`
2. `walletprocesspsbt`
3. `finalizepsbt`
4. `sendrawtransaction` or equivalent send path

Operational notes:

- malformed base64/PSBT structures map to decode/format errors
- locked wallet flows return lock-state errors
- mismatch guards are implemented for combine/join paths

## Mempool and State Observability

Mempool APIs are adapter-backed and reflect pending state model semantics.

- `getrawmempool`
- `getmempoolentry`
- `getmempoolancestors`
- `getmempooldescendants`

Use these for operational monitoring, but treat semantics as adapted rather than byte-for-byte Bitcoin Core internals.

## Recommended Client-Side Hardening

- retry idempotent read methods with bounded backoff
- classify errors by code (`-28`, `-22`, `-18`, `-13`, etc.)
- avoid silent fallback across wallet contexts
- enforce strict JSON types for numeric fields

## Integration Test Checklist

- valid/invalid JSON-RPC envelope handling
- method-not-found behavior
- wallet loaded/unloaded transitions
- lock-state handling for signing flows
- PSBT malformed payload rejection
- mempool request behavior for missing txids

## Incident Diagnostics

If integration fails in production-like environments:

1. capture full request/response JSON (with sensitive values redacted)
2. verify adapter version/commit
3. check backend readiness (`-28` patterns)
4. validate wallet state transitions before sign/send calls
5. compare behavior against matrix and error-code docs
