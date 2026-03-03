#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/launch/run_readiness_gate.sh [--smoke|--full] [--include-fuzz]

Modes:
  --smoke         Fast readiness checks (docs + script + benchmark/auth smoke).
  --full          Full local gate (build, test, lint, fmt, audit + smoke checks).

Options:
  --include-fuzz  Add nightly-toolchain fuzz smoke runs (slow).
  -h, --help      Show this help text.
EOF
}

MODE="full"
INCLUDE_FUZZ=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --smoke)
      MODE="smoke"
      shift
      ;;
    --full)
      MODE="full"
      shift
      ;;
    --include-fuzz)
      INCLUDE_FUZZ=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

require_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Required command not found: $cmd" >&2
    exit 1
  fi
}

run_cmd() {
  local title="$1"
  shift
  echo
  echo "==> $title"
  "$@"
}

run_subshell_cmd() {
  local title="$1"
  local cmd="$2"
  echo
  echo "==> $title"
  bash -lc "$cmd"
}

check_required_docs() {
  local required_files=(
    "docs/launch-readiness-gates.md"
    "docs/incident-communication-templates.md"
    "docs/security-and-threat-model.md"
    "docs/validator-operations-runbook.md"
    "docs/benchmark-methodology.md"
    "docs/rpc-compatibility-matrix.md"
    "docs/rpc-error-codes.md"
  )

  for file in "${required_files[@]}"; do
    if [[ ! -f "$file" ]]; then
      echo "Missing required launch document: $file" >&2
      exit 1
    fi
  done

  if rg -n "link TBD|\\bTBD\\b|TODO" docs/launch-readiness-gates.md docs/incident-communication-templates.md >/dev/null; then
    echo "Launch docs still contain placeholder text (TBD/TODO)." >&2
    echo "Resolve placeholders before marking readiness gates complete." >&2
    exit 1
  fi
}

run_fuzz_smoke() {
  if ! cargo fuzz --help >/dev/null 2>&1; then
    echo "cargo-fuzz is required for --include-fuzz (install: cargo install cargo-fuzz --locked)." >&2
    exit 1
  fi

  run_subshell_cmd \
    "Fuzz account-id parser (runs=100)" \
    "cd near-account-id && cargo +nightly fuzz run fuzz_account_id_parse -- -runs=100"

  run_subshell_cmd \
    "Fuzz RPC JSON parser (runs=100)" \
    "cd bitinfinity-btcrpc && cargo +nightly fuzz run fuzz_rpc_parse -- -runs=100"

  run_subshell_cmd \
    "Fuzz tx hex parser (runs=100)" \
    "cd bitinfinity-btcrpc && cargo +nightly fuzz run fuzz_tx_hex -- -runs=100"

  run_subshell_cmd \
    "Fuzz tx translator (runs=100)" \
    "cd bitinfinity-btcrpc && cargo +nightly fuzz run fuzz_tx_translator -- -runs=100"

  run_subshell_cmd \
    "Fuzz amount arithmetic (runs=100)" \
    "cd bitinfinity-btcrpc && cargo +nightly fuzz run fuzz_amount_math -- -runs=100"

  run_subshell_cmd \
    "Fuzz Patoshi CSV parser (runs=100)" \
    "cd bitinfinity-tools && cargo +nightly fuzz run fuzz_patoshi_csv -- -runs=100"

  run_subshell_cmd \
    "Fuzz secp256k1 recover path (runs=100)" \
    "cd nearcore/core/crypto && cargo +nightly fuzz run fuzz_secp256k1_recover -- -runs=100"
}

require_cmd bash
require_cmd cargo
require_cmd rg
require_cmd jq

run_cmd "Validate launch required docs and placeholder-free state" check_required_docs
run_cmd "Auth coverage matrix" ./scripts/check_auth_coverage.sh
run_cmd "Benchmark runner script syntax" bash -n scripts/benchmark/run_tps_profiles.sh
run_cmd "Launch gate script syntax" bash -n scripts/launch/run_readiness_gate.sh
run_cmd "Benchmark runner dry-run smoke" ./scripts/benchmark/run_tps_profiles.sh --dry-run --skip-build --profile all --metrics-interval 1

if [[ "$MODE" == "full" ]]; then
  run_cmd "Build release binaries" cargo build --release -p bitinfinity-btcrpc -p bitinfinity-tools -p bitinfinity-neard
  run_cmd "Run tests (workspace)" cargo test --workspace
  run_cmd "Run tests (near-account-id)" cargo test --manifest-path near-account-id/Cargo.toml
  run_cmd "Clippy (workspace)" cargo clippy --workspace --all-targets -- -D warnings
  run_cmd "Clippy (near-account-id)" cargo clippy --manifest-path near-account-id/Cargo.toml --all-targets -- -D warnings
  run_cmd "Check formatting (workspace)" cargo fmt --all -- --check
  run_cmd "Check formatting (near-account-id)" cargo fmt --manifest-path near-account-id/Cargo.toml --all -- --check
  run_cmd "Audit workspace dependencies" cargo audit
  run_cmd "Audit near-account-id dependencies" cargo audit --file near-account-id/Cargo.lock
fi

if [[ "$INCLUDE_FUZZ" -eq 1 ]]; then
  run_fuzz_smoke
fi

echo
echo "Launch readiness gate passed: mode=${MODE}, include_fuzz=${INCLUDE_FUZZ}, at=$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
