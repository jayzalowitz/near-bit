# Sydney (BTC-NEAR Fork)

Bitcoin Infinity — Layer 1 combining Bitcoin's address space with NEAR Protocol's execution engine.

## Repository structure

```
bitinfinity-btcrpc/     Bitcoin Core-compatible JSON-RPC proxy (204 methods)
bitinfinity-tools/      UTXO snapshot parser, genesis builder, Patoshi detector
bitinfinity-neard/      Custom nearcore binary with Bitcoin Infinity genesis
near-account-id/        Forked near-account-id: accepts Bitcoin addresses as account IDs
nearcore/               Forked nearcore: secp256k1 signature verification, BTC accounts
docs/                   GitHub Pages site
genesis-testnet/        Testnet genesis.json and config
```

## Build

```bash
cargo build --release
cargo build --release -p bitinfinity-btcrpc
cargo build --release -p bitinfinity-tools
cargo build --release -p bitinfinity-neard
```

## Test

```bash
cargo test --workspace
cargo test --manifest-path near-account-id/Cargo.toml
```

## Lint

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --manifest-path near-account-id/Cargo.toml --all-targets -- -D warnings
cargo fmt --all -- --check
cargo fmt --manifest-path near-account-id/Cargo.toml --all -- --check
```

## Security

- Cookie auth file and wallet.json are excluded from git via .gitignore
- Wallet encryption uses ChaCha20-Poly1305 (AEAD) with 100k-round SHA256 KDF
- Auth credential comparison is constant-time (XOR-based)
- Sensitive files written with 0o600 permissions on Unix
- Found a vulnerability? Email security@bitcoininfinity.io

## Key crates

- `near-account-id` is a standalone crate (separate Cargo.lock) — test and lint it separately
- `bitinfinity-btcrpc` contains the RPC server (`src/main.rs`), keystore (`src/keystore.rs`), transaction translation (`src/tx_translator.rs`), and NEAR client (`src/near_client.rs`)

## gstack

Use /browse from gstack for all web browsing. Never use mcp__claude-in-chrome__* tools.

Available skills: /office-hours, /plan-ceo-review, /plan-eng-review, /plan-design-review,
/design-consultation, /review, /ship, /land-and-deploy, /canary, /benchmark, /browse,
/qa, /qa-only, /design-review, /setup-browser-cookies, /setup-deploy, /retro,
/investigate, /document-release, /codex, /cso, /autoplan, /careful, /freeze, /guard,
/unfreeze, /gstack-upgrade.

If gstack skills aren't working, run `cd .claude/skills/gstack && ./setup` to build the binary and register skills.
