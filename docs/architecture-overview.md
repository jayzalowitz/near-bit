# Bitcoin Infinity Architecture Overview

Bitcoin Infinity preserves Bitcoin keys and address semantics while running execution on a NEAR-based PoS runtime.

## Design Goals

- Keep Bitcoin address/key UX intact.
- Provide low-latency finality and high throughput.
- Preserve a hard 21M supply model.
- Expose Bitcoin Core-compatible RPC surfaces for existing wallet workflows.

## System Components

- `bitinfinity-btcrpc`: JSON-RPC adapter that accepts Bitcoin Core-style methods and translates to NEAR-backed behavior.
- `bitinfinity-neard`: node binary and chain runtime entrypoint.
- `bitinfinity-tools`: genesis generation, UTXO snapshot processing, key tooling, and Patoshi handling.
- `near-account-id` (fork): account validation layer extended to accept Bitcoin address formats.
- `nearcore` (fork): consensus/runtime base with Bitcoin-compatible account and signature behavior.

## Data Flow

1. Wallet sends Bitcoin-style RPC request to `bitinfinity-btcrpc`.
2. RPC adapter validates parameters and wallet state.
3. Adapter maps request to NEAR-compatible transaction or state query.
4. Runtime verifies signatures and address/account ownership.
5. Transaction executes under PoS finality.
6. Adapter returns Bitcoin-compatible response shape to caller.

## Account Model

Bitcoin Infinity supports Bitcoin address formats as account identifiers:

- P2PKH (`1...`)
- P2SH (`3...`)
- P2WPKH (`bc1q...`)
- P2WSH (`bc1q...` script hash variants)
- P2TR (`bc1p...`)

The account validation path is centralized in the `near-account-id` fork and runtime checks.

## Signature Model

- Signatures remain secp256k1-based.
- Runtime paths recover/verify key material against Bitcoin-form account identifiers.
- Wallet-facing signing flows are exposed through adapted Bitcoin RPC methods.

## Supply and Emission

- Hard cap target remains 21,000,000 BIT equivalent.
- Emission follows halving-oriented schedule adapted to wall-clock time semantics in PoS operation.
- Patoshi-associated balances are handled under explicit floor and policy constraints.

See [Tokenomics and Governance](tokenomics-and-governance.md) for details.

## Compatibility Boundaries

Bitcoin Infinity intentionally adapts or stubs some Bitcoin Core semantics due to architecture differences.

- For method status: [RPC Compatibility Matrix](rpc-compatibility-matrix.md)
- For code mapping and operational behavior: [RPC Error Codes](rpc-error-codes.md)

## Operational Observability

Operational confidence is enforced through:

- unit/integration tests
- strict clippy/fmt checks
- fuzzing (CI smoke + nightly long runs)
- benchmark-runner dry-run checks and full methodology profiles

See [Local Development and Testing](local-dev-and-testing.md) and [TPS Benchmark Methodology](benchmark-methodology.md).
