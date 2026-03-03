# Bitcoin Infinity

**Bitcoin Infinity** is a Layer 1 blockchain that combines Bitcoin's address space and 21M token supply with NEAR Protocol's execution engine — smart contracts, sharding, and sub-second finality.

Your existing Bitcoin private key is your Bitcoin Infinity private key. No migration. No claiming. No new wallet software required.

```
Bitcoin address:  1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa
Bitcoin Infinity: same address, same key, now with smart contracts
```

## What's different

| | Bitcoin | Bitcoin Infinity |
|--|---------|-----------------|
| Addresses | Bitcoin (P2PKH, P2WPKH, P2TR) | Same |
| Keys | secp256k1 | Same |
| Supply | 21,000,000 BTC | 21,000,000 BIT |
| Smallest unit | 1 satoshi | 1 finney (10⁻⁸ satoshi) |
| Consensus | Proof of Work | Proof of Stake (NEAR BFT) |
| Finality | ~60 min | ~1 second |
| Throughput | ~7 TPS | ~858 avg / ~1,036 peak TPS observed (single-shard 60s pilot at 1k target; scales with shards) |
| Smart contracts | No | Yes (NEAR VM, Rust/JS) |
| Satoshi's coins | Spendable | Staking-only, floor-enforced |

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

## Quick start

See [QUICKSTART.md](QUICKSTART.md) for step-by-step instructions to:
- Generate a Bitcoin Infinity keypair
- Boot a local testnet node
- Connect Sparrow Wallet
- Send your first transaction

## Connect your Bitcoin wallet

Bitcoin Infinity exposes a Bitcoin Core-compatible RPC endpoint. Any wallet that supports a custom Bitcoin Core RPC server works out of the box.

```bash
# Start the RPC proxy (needs a running nearcore node)
cargo run -p bitinfinity-btcrpc

# In Sparrow Wallet: File → Preferences → Server → Bitcoin Core
# Host: 127.0.0.1  Port: 8332  Use SSL: No
```

## Build

```bash
# All three binaries
cargo build --release

# Individual
cargo build --release -p bitinfinity-btcrpc
cargo build --release -p bitinfinity-tools
cargo build --release -p bitinfinity-neard
```

## Test

```bash
cargo test --workspace
cargo test --manifest-path near-account-id/Cargo.toml
```

## Run CI checks locally

```bash
# Lint / format parity with CI
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --manifest-path near-account-id/Cargo.toml --all-targets -- -D warnings
cargo fmt --all -- --check
cargo fmt --manifest-path near-account-id/Cargo.toml --all -- --check

# Security audit parity with CI
cargo audit
cargo audit --file near-account-id/Cargo.lock

# Extra CI smoke checks
./scripts/check_auth_coverage.sh
bash -n scripts/benchmark/run_tps_profiles.sh
./scripts/benchmark/run_tps_profiles.sh --dry-run --skip-build --profile all --metrics-interval 1
```

## Run launch-readiness gates locally

```bash
# Fast readiness checks
./scripts/launch/run_readiness_gate.sh --smoke

# Full launch gate (build/test/lint/fmt/audit + smoke checks)
./scripts/launch/run_readiness_gate.sh --full

# Add nightly fuzz smoke runs
./scripts/launch/run_readiness_gate.sh --full --include-fuzz
```

## Generate launch evidence bundle

```bash
# Full evidence bundle for go/no-go reviews
./scripts/launch/generate_evidence_bundle.sh --mode full

# Faster evidence bundle for iteration
./scripts/launch/generate_evidence_bundle.sh --mode smoke
```

See `docs/launch-readiness-gates.md` for gate status, `docs/mainnet-go-no-go-checklist.md` for decision signoff, `docs/incident-communication-templates.md` for incident messaging templates, and `docs/launch-evidence-bundle.md` for evidence packaging details.

## Fuzzing

```bash
# One-time setup
cargo install cargo-fuzz --locked
rustup toolchain install nightly

# near-account-id parser fuzz smoke
cd near-account-id
cargo +nightly fuzz run fuzz_account_id_parse -- -runs=100

# btcrpc fuzz smoke
cd ../bitinfinity-btcrpc
cargo +nightly fuzz run fuzz_rpc_parse -- -runs=100
cargo +nightly fuzz run fuzz_tx_hex -- -runs=100
cargo +nightly fuzz run fuzz_tx_translator -- -runs=100
```

CI runs short fuzz smoke in `.github/workflows/ci.yml` and scheduled matrix fuzz in `.github/workflows/nightly-fuzz.yml`.
CI also runs a benchmark-runner dry-run smoke check in `.github/workflows/ci.yml`.

## TPS Benchmarking

Use the profile runner to execute methodology-defined benchmarks and capture reproducible artifacts.

```bash
# Dry-run command plan (no workload execution)
./scripts/benchmark/run_tps_profiles.sh --dry-run --skip-build --profile baseline

# Baseline only
./scripts/benchmark/run_tps_profiles.sh --profile baseline

# Full profile set (baseline + stress + peak)
./scripts/benchmark/run_tps_profiles.sh --profile all
```

Artifacts are written under `artifacts/benchmarks/<timestamp>/` with per-profile logs and metrics plus aggregate `summary.json`, `summary.csv`, and `summary.md`.
See `docs/benchmark-methodology.md` for required publication format and reporting rules.
The runner exits non-zero if any profile exits non-zero; add `--allow-nonzero-run-status` for exploratory runs that should keep diagnostics but not fail the command.
The runner uses controller-enabled schedules by default and records both raw `run_status` and normalized `effective_run_status` for benchmark pass/fail accounting.

Latest measured pilot results (February 20, 2026):
- Single-shard baseline pilot (`target 1000 TPS`, `60s`): `avg 857.586 TPS`, `peak 1035.614 TPS`, `final_success_metric 58856`, `final_failed_metric 0`
- Native-TPS multi-profile pilot (`duration 20s each`):
  - baseline (`1000 TPS` target): `avg 620.265`, `peak 886.428`
  - stress (`10000 TPS` target): `avg 6143.507`, `peak 8844.634`
  - peak (`50000 TPS` target): `avg 8935.701`, `peak 12648.842`
- Pre-mitigation 60s high-load pilots (historical, self-stop path):
  - stress (`10000 TPS` target): `avg 8551.340`, `peak 10328.730`, `run_status 139`, `signal_11_from_log 1`
  - peak (`50000 TPS` target): `avg 12353.138`, `peak 14985.768`, `run_status 139`, `signal_11_from_log 1`
- Post-mitigation 60s high-load pilots (controller-mode + external graceful stop):
  - stress (`10000 TPS` target): `avg 8331.414`, `peak 11059.288`, `run_status 143`, `effective_run_status 0`, `signal_11_from_log 0`
  - peak (`50000 TPS` target): `avg 8798.048`, `peak 11576.004`, `run_status 143`, `effective_run_status 0`, `signal_11_from_log 0`
- Raw artifacts:
  - `artifacts/benchmarks/pilot-baseline-1000-20260220T180109Z/summary.json`
  - `artifacts/benchmarks/pilot-all-native-tps-20260220T180236Z/summary.json`
  - `artifacts/benchmarks/pilot-stress-10000-60s-20260220T181011Z/summary.json`
  - `artifacts/benchmarks/pilot-peak-50000-60s-20260220T181137Z/summary.json`
  - `artifacts/benchmarks/post-fix-stress60-20260220T183159Z/summary.json`
  - `artifacts/benchmarks/post-fix2-peak60-20260220T183819Z/summary.json`

## Key design decisions

- **Bitcoin addresses as NEAR account IDs**: `near-account-id` accepts P2PKH, P2SH, P2WPKH, P2WSH, and P2TR addresses natively
- **secp256k1 in nearcore**: transactions signed with Bitcoin keys are verified via pubkey recovery + address derivation before any action executes
- **Patoshi balance floor**: locked Patoshi accounts are staking-only plus foundation-only transfers; the genesis balance is a permanent floor enforced in runtime
- **21M hard cap**: `MAX_SUPPLY = 21_000_000 * 10^24 yoctoBIT`; remaining emission distributed as staking rewards on the Bitcoin halving schedule, time-based not block-count-based
- **The finney**: 1 satoshi = 10⁸ finneys = 10¹⁶ yoctoBIT; named after Hal Finney

## Security

Found a vulnerability? Email security@bitcoininfinity.io. Do not open a public GitHub issue for security findings.
Launch and disclosure process tracking is documented in `docs/launch-readiness-gates.md`.

## Related issues

- [#2 — Quantum Resistance](https://github.com/jayzalowitz/near-bit/issues/2)
- [#10 — Patoshi Balance Floor](https://github.com/jayzalowitz/near-bit/issues/10)
- [#11 — Launch Plan](https://github.com/jayzalowitz/near-bit/issues/11)
- [Issue #11 Execution Report](docs/issue-11-execution-report.md)
- [TPS Benchmark Methodology](docs/benchmark-methodology.md)

## License

MIT — see [LICENSE](LICENSE) or each crate's `Cargo.toml`.
The `nearcore/` subtree is Apache 2.0 per upstream NEAR Protocol.
