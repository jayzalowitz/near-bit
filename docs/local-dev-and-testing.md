# Local Development and Testing

This guide provides a stable local workflow that mirrors the CI pipeline.

## Prerequisites

- Rust toolchains: stable and nightly
- `cargo-audit`
- `cargo-fuzz`
- standard Unix tooling used by benchmark scripts (`bash`, `jq`, `perl`)

## Build

```bash
# Optional: keep local build outputs out of tracked target/ artifacts
export CARGO_TARGET_DIR="$(pwd)/.context/cargo-target"

cargo build --release -p bitinfinity-btcrpc -p bitinfinity-tools -p bitinfinity-neard
```

## Test

```bash
cargo test --workspace
cargo test --manifest-path near-account-id/Cargo.toml
```

## Lint and Format (CI parity)

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --manifest-path near-account-id/Cargo.toml --all-targets -- -D warnings
cargo fmt --all -- --check
cargo fmt --manifest-path near-account-id/Cargo.toml --all -- --check
```

## Security Audit (CI parity)

```bash
cargo audit
cargo audit --file near-account-id/Cargo.lock
```

## Auth Coverage Matrix Check

```bash
./scripts/check_auth_coverage.sh
```

## Benchmark Runner Smoke (CI parity)

```bash
bash -n scripts/benchmark/run_tps_profiles.sh
./scripts/benchmark/run_tps_profiles.sh --dry-run --skip-build --profile all --metrics-interval 1
```

## Fuzzing

### CI smoke-equivalent local run

```bash
cd near-account-id
cargo +nightly fuzz run fuzz_account_id_parse -- -max_total_time=30 -timeout=5

cd ../bitinfinity-btcrpc
cargo +nightly fuzz run fuzz_rpc_parse -- -max_total_time=30 -timeout=5
cargo +nightly fuzz run fuzz_tx_hex -- -max_total_time=30 -timeout=5
cargo +nightly fuzz run fuzz_tx_translator -- -max_total_time=30 -timeout=5
cargo +nightly fuzz run fuzz_amount_math -- -max_total_time=30 -timeout=5

cd ../bitinfinity-tools
cargo +nightly fuzz run fuzz_patoshi_csv -- -max_total_time=30 -timeout=5

cd ../nearcore/core/crypto
cargo +nightly fuzz run fuzz_secp256k1_recover -- -max_total_time=30 -timeout=5
```

### Nightly-style longer run

Use `-max_total_time` values in hours to match `nightly-fuzz.yml` strategy.

## Recommended Local Gate Before Push

Run this sequence in order:

1. `cargo fmt` checks
2. strict clippy checks
3. tests
4. audit
5. auth coverage
6. benchmark dry-run
7. fuzz smoke

This order catches low-cost failures first and limits wasted execution time.

## Common Pitfalls

- Running `cargo fuzz` on stable toolchain will fail due to sanitizer flags.
- Using `cargo audit --manifest-path` will fail on modern `cargo-audit`; use `--file`.
- Fuzz corpus/artifacts can grow quickly; do not commit generated runtime outputs.
