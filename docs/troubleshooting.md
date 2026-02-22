# Troubleshooting Guide

This guide helps diagnose the most common local and CI-facing issues.

## Build and Toolchain

### `cargo fuzz` fails with nightly-only sanitizer errors

Cause: running fuzz on stable toolchain.

Fix:

```bash
cargo +nightly fuzz run <target> -- -max_total_time=30 -timeout=5
```

### Clippy failures on strict mode

Cause: workspace CI uses `-D warnings`.

Fix:

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --manifest-path near-account-id/Cargo.toml --all-targets -- -D warnings
```

## Audit

### `cargo audit` manifest-path argument errors

Cause: modern `cargo-audit` expects lockfile path.

Fix:

```bash
cargo audit --file near-account-id/Cargo.lock
```

## RPC and Wallet Errors

### `-18` wallet not loaded

Fix:

- load/create wallet context before wallet-scoped calls
- verify alias and lifecycle ordering

### `-13` wallet locked

Fix:

- unlock wallet before signing/sending operations
- verify unlock window has not expired

### `-22` decode/format errors

Fix:

- validate PSBT/base64/raw tx formatting
- verify JSON numeric/string typing

For broader error mappings: [RPC Error Codes](rpc-error-codes.md).

## Fuzz Corpus Noise in Working Tree

Symptoms:

- large untracked file sets under `**/fuzz/corpus/**` and `**/fuzz/artifacts/**`

Fix:

- ensure `.gitignore` includes fuzz runtime outputs
- keep only intentional seed corpus files under version control

## Benchmark Runner

### Script syntax failures

Fix:

```bash
bash -n scripts/benchmark/run_tps_profiles.sh
```

### Dry-run behavior mismatches

Fix:

```bash
./scripts/benchmark/run_tps_profiles.sh --dry-run --skip-build --profile all --metrics-interval 1
```

## CI Job Starts but Immediately Fails

If jobs fail before steps run, confirm external factors first:

- GitHub Actions billing/spending limits
- repository action permissions
- branch protection requirements

These platform-level failures are independent from repository code health.
