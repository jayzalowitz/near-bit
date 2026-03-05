#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/launch/run_readiness_gate.sh [--smoke|--full] [--include-fuzz] [--check-nightly-fuzz-health] [--nightly-fuzz-branch <name>] [--nightly-fuzz-workflow <name>] [--nightly-fuzz-window-days <n>] [--nightly-fuzz-min-runs <n>] [--nightly-fuzz-max-runs <n>] [--nightly-fuzz-allow-in-progress] [--check-snapshot-supply] [--snapshot-genesis <path>] [--snapshot-txoutsetinfo <path>] [--snapshot-tolerance-sats <n>] [--snapshot-json-out <path>] [--cargo-target-dir <path>] [--skip-issue1-goal-checks] [--require-go] [--skip-checklist]

Modes:
  --smoke         Fast readiness checks (docs + script + benchmark/auth smoke).
  --full          Full local gate (build, test, lint, fmt, audit + smoke checks).

Options:
  --include-fuzz  Add nightly-toolchain fuzz smoke runs (slow).
  --check-nightly-fuzz-health  Enforce 7-day nightly fuzz health check (gate #4).
  --nightly-fuzz-branch <name> Branch used by nightly fuzz health check. Default: main.
  --nightly-fuzz-workflow <name> Workflow name used by nightly fuzz health check. Default: Nightly Fuzz.
  --nightly-fuzz-window-days <n> Lookback window in days for nightly fuzz health. Default: 7.
  --nightly-fuzz-min-runs <n> Minimum required runs in lookback window. Default: 1.
  --nightly-fuzz-max-runs <n> Max runs fetched from GitHub API. Default: 200.
  --nightly-fuzz-allow-in-progress Do not fail when in-progress runs are present.
  --check-snapshot-supply Enforce gate #10 snapshot-vs-genesis reconciliation.
  --snapshot-genesis <path> Path to genesis.json used for snapshot reconciliation.
  --snapshot-txoutsetinfo <path> Path to bitcoin-cli gettxoutsetinfo JSON.
  --snapshot-tolerance-sats <n> Allowed satoshi diff for snapshot check. Default: 1.
  --snapshot-json-out <path> Optional machine-readable snapshot-check output path.
  --cargo-target-dir <path> Cargo target directory for build/test artifacts.
                            Default: .context/cargo-target locally, target in CI.
  --skip-issue1-goal-checks Skip targeted Issue #1 goal validation tests.
  --require-go    Enforce GO criteria during checklist parse.
  --skip-checklist  Skip checklist parse step (used by higher-level orchestration).
  -h, --help      Show this help text.
EOF
}

MODE="full"
INCLUDE_FUZZ=0
CHECK_NIGHTLY_FUZZ_HEALTH=0
NIGHTLY_FUZZ_BRANCH="main"
NIGHTLY_FUZZ_WORKFLOW="Nightly Fuzz"
NIGHTLY_FUZZ_WINDOW_DAYS=7
NIGHTLY_FUZZ_MIN_RUNS=1
NIGHTLY_FUZZ_MAX_RUNS=200
NIGHTLY_FUZZ_ALLOW_IN_PROGRESS=0
CHECK_SNAPSHOT_SUPPLY=0
SNAPSHOT_GENESIS=""
SNAPSHOT_TXOUTSETINFO=""
SNAPSHOT_TOLERANCE_SATS=1
SNAPSHOT_JSON_OUT=""
CARGO_TARGET_DIR_OVERRIDE=""
SKIP_ISSUE1_GOAL_CHECKS=0
REQUIRE_GO=0
SKIP_CHECKLIST=0
HAS_RG=0
GENESIS_FIXTURE_EXPECTED_HASH="${GENESIS_FIXTURE_EXPECTED_HASH:-95f3e2600eec0dcd3ca51bf530f46ac963fa3b5286e18c6401efdcae8066aa5d}"

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
    --check-nightly-fuzz-health)
      CHECK_NIGHTLY_FUZZ_HEALTH=1
      shift
      ;;
    --nightly-fuzz-branch)
      if [[ $# -lt 2 ]]; then
        echo "--nightly-fuzz-branch requires a value" >&2
        exit 1
      fi
      NIGHTLY_FUZZ_BRANCH="$2"
      shift 2
      ;;
    --nightly-fuzz-workflow)
      if [[ $# -lt 2 ]]; then
        echo "--nightly-fuzz-workflow requires a value" >&2
        exit 1
      fi
      NIGHTLY_FUZZ_WORKFLOW="$2"
      shift 2
      ;;
    --nightly-fuzz-window-days)
      if [[ $# -lt 2 ]]; then
        echo "--nightly-fuzz-window-days requires a numeric value" >&2
        exit 1
      fi
      NIGHTLY_FUZZ_WINDOW_DAYS="$2"
      shift 2
      ;;
    --nightly-fuzz-min-runs)
      if [[ $# -lt 2 ]]; then
        echo "--nightly-fuzz-min-runs requires a numeric value" >&2
        exit 1
      fi
      NIGHTLY_FUZZ_MIN_RUNS="$2"
      shift 2
      ;;
    --nightly-fuzz-max-runs)
      if [[ $# -lt 2 ]]; then
        echo "--nightly-fuzz-max-runs requires a numeric value" >&2
        exit 1
      fi
      NIGHTLY_FUZZ_MAX_RUNS="$2"
      shift 2
      ;;
    --nightly-fuzz-allow-in-progress)
      NIGHTLY_FUZZ_ALLOW_IN_PROGRESS=1
      shift
      ;;
    --check-snapshot-supply)
      CHECK_SNAPSHOT_SUPPLY=1
      shift
      ;;
    --snapshot-genesis)
      if [[ $# -lt 2 ]]; then
        echo "--snapshot-genesis requires a path value" >&2
        exit 1
      fi
      SNAPSHOT_GENESIS="$2"
      shift 2
      ;;
    --snapshot-txoutsetinfo)
      if [[ $# -lt 2 ]]; then
        echo "--snapshot-txoutsetinfo requires a path value" >&2
        exit 1
      fi
      SNAPSHOT_TXOUTSETINFO="$2"
      shift 2
      ;;
    --snapshot-tolerance-sats)
      if [[ $# -lt 2 ]]; then
        echo "--snapshot-tolerance-sats requires a numeric value" >&2
        exit 1
      fi
      SNAPSHOT_TOLERANCE_SATS="$2"
      shift 2
      ;;
    --snapshot-json-out)
      if [[ $# -lt 2 ]]; then
        echo "--snapshot-json-out requires a path value" >&2
        exit 1
      fi
      SNAPSHOT_JSON_OUT="$2"
      shift 2
      ;;
    --cargo-target-dir)
      if [[ $# -lt 2 ]]; then
        echo "--cargo-target-dir requires a path value" >&2
        exit 1
      fi
      CARGO_TARGET_DIR_OVERRIDE="$2"
      shift 2
      ;;
    --skip-issue1-goal-checks)
      SKIP_ISSUE1_GOAL_CHECKS=1
      shift
      ;;
    --require-go)
      REQUIRE_GO=1
      shift
      ;;
    --skip-checklist)
      SKIP_CHECKLIST=1
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

for num in "$NIGHTLY_FUZZ_WINDOW_DAYS" "$NIGHTLY_FUZZ_MIN_RUNS" "$NIGHTLY_FUZZ_MAX_RUNS"; do
  if [[ ! "$num" =~ ^[0-9]+$ ]]; then
    echo "Nightly fuzz numeric options must be non-negative integers." >&2
    exit 1
  fi
done

if [[ ! "$SNAPSHOT_TOLERANCE_SATS" =~ ^[0-9]+$ ]]; then
  echo "--snapshot-tolerance-sats must be a non-negative integer." >&2
  exit 1
fi

if [[ "$CHECK_SNAPSHOT_SUPPLY" -eq 1 ]]; then
  if [[ -z "$SNAPSHOT_GENESIS" || -z "$SNAPSHOT_TXOUTSETINFO" ]]; then
    echo "--check-snapshot-supply requires --snapshot-genesis and --snapshot-txoutsetinfo." >&2
    exit 1
  fi
  if [[ ! -f "$SNAPSHOT_GENESIS" ]]; then
    echo "Snapshot-check genesis file not found: $SNAPSHOT_GENESIS" >&2
    exit 1
  fi
  if [[ ! -f "$SNAPSHOT_TXOUTSETINFO" ]]; then
    echo "Snapshot-check txoutsetinfo file not found: $SNAPSHOT_TXOUTSETINFO" >&2
    exit 1
  fi
elif [[ -n "$SNAPSHOT_GENESIS" || -n "$SNAPSHOT_TXOUTSETINFO" || -n "$SNAPSHOT_JSON_OUT" ]]; then
  echo "Snapshot parameters provided without --check-snapshot-supply." >&2
  echo "Use --check-snapshot-supply to enforce gate #10 snapshot reconciliation." >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

resolve_cargo_target_dir() {
  if [[ -n "$CARGO_TARGET_DIR_OVERRIDE" ]]; then
    echo "$CARGO_TARGET_DIR_OVERRIDE"
    return
  fi
  if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    echo "$CARGO_TARGET_DIR"
    return
  fi
  if [[ "${CI:-}" == "true" ]]; then
    echo "$ROOT_DIR/target"
    return
  fi
  echo "$ROOT_DIR/.context/cargo-target"
}

CARGO_TARGET_DIR="$(resolve_cargo_target_dir)"
export CARGO_TARGET_DIR
mkdir -p "$CARGO_TARGET_DIR"

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
    "docs/technical-whitepaper.md"
    "docs/communications-launch-plan.md"
    "docs/blog-what-is-bitcoin-infinity.md"
    "docs/blog-utxo-to-genesis-deep-dive.md"
    "docs/blog-patoshi-balance-floor-explainer.md"
    "docs/mainnet-go-no-go-checklist.md"
    "docs/genesis-determinism-check.md"
    "docs/snapshot-supply-reconciliation.md"
    "docs/launch-evidence-bundle.md"
    "docs/launch-rehearsal.md"
    "docs/release-artifact-manifest.md"
    "docs/incident-communication-templates.md"
    "docs/incident-launch-pack.md"
    "docs/go-no-go-signoff-prefill.md"
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

  if [[ "$HAS_RG" -eq 1 ]]; then
    if rg -n "link TBD|\\bTBD\\b|TODO" docs/launch-readiness-gates.md docs/incident-communication-templates.md >/dev/null; then
      echo "Launch docs still contain placeholder text (TBD/TODO)." >&2
      echo "Resolve placeholders before marking readiness gates complete." >&2
      exit 1
    fi
  elif grep -En "link TBD|\\bTBD\\b|TODO" docs/launch-readiness-gates.md docs/incident-communication-templates.md >/dev/null; then
    echo "Launch docs still contain placeholder text (TBD/TODO)." >&2
    echo "Resolve placeholders before marking readiness gates complete." >&2
    exit 1
  fi
}

check_site_launch_channels() {
  local site_file="docs/index.html"
  local required_patterns=(
    "Join Launch Updates Mailing List"
    "launch-updates@bitcoininfinity.io"
    "https://github.com/jayzalowitz/near-bit"
    "technical-whitepaper.md"
  )

  if [[ ! -f "$site_file" ]]; then
    echo "Missing launch website file: $site_file" >&2
    exit 1
  fi

  for pattern in "${required_patterns[@]}"; do
    if [[ "$HAS_RG" -eq 1 ]]; then
      if ! rg -F "$pattern" "$site_file" >/dev/null; then
        echo "Launch website check failed: missing '$pattern' in $site_file" >&2
        exit 1
      fi
    elif ! grep -F "$pattern" "$site_file" >/dev/null; then
      echo "Launch website check failed: missing '$pattern' in $site_file" >&2
      exit 1
    fi
  done
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

verify_release_versions() {
  local binaries=(
    "${CARGO_TARGET_DIR}/release/bitinfinity-btcrpc"
    "${CARGO_TARGET_DIR}/release/bitinfinity-tools"
    "${CARGO_TARGET_DIR}/release/bitinfinity-neard"
  )
  local version_line

  for bin in "${binaries[@]}"; do
    if [[ ! -x "$bin" ]]; then
      echo "Missing release binary for version check: $bin" >&2
      exit 1
    fi
    version_line="$("$bin" --version 2>/dev/null | head -n 1 || true)"
    if [[ -z "$version_line" ]]; then
      echo "Release binary version check failed (empty --version): $bin" >&2
      exit 1
    fi
    if [[ "$version_line" == "unknown" ]]; then
      echo "Release binary version check failed (unknown --version): $bin" >&2
      exit 1
    fi
    echo "$bin -> $version_line"
  done
}

require_cmd bash
require_cmd cargo
require_cmd jq

if command -v rg >/dev/null 2>&1; then
  HAS_RG=1
elif ! command -v grep >/dev/null 2>&1; then
  echo "Required command not found: need either rg or grep" >&2
  exit 1
fi

run_cmd "Validate launch required docs and placeholder-free state" check_required_docs
run_cmd "Validate launch website channels (mailing list + GitHub + whitepaper)" check_site_launch_channels
run_cmd "Auth coverage matrix" ./scripts/check_auth_coverage.sh
run_cmd "Benchmark runner script syntax" bash -n scripts/benchmark/run_tps_profiles.sh
run_cmd "Launch gate script syntax" bash -n scripts/launch/run_readiness_gate.sh
run_cmd "Launch evidence bundle script syntax" bash -n scripts/launch/generate_evidence_bundle.sh
run_cmd "Launch rehearsal script syntax" bash -n scripts/launch/run_launch_rehearsal.sh
run_cmd "Release artifact manifest script syntax" bash -n scripts/launch/generate_release_manifest.sh
run_cmd "Incident launch-pack script syntax" bash -n scripts/launch/generate_incident_launch_pack.sh
run_cmd "Go/no-go signoff prefill script syntax" bash -n scripts/launch/prefill_go_no_go_signoff.sh
run_cmd "Go/no-go checklist script syntax" bash -n scripts/launch/check_go_no_go_checklist.sh
run_cmd "Nightly fuzz health script syntax" bash -n scripts/launch/check_nightly_fuzz_health.sh
run_cmd "Issue #1 core-goal checker script syntax" bash -n scripts/launch/check_issue1_core_goals.sh
run_cmd "Genesis determinism checker script syntax" bash -n scripts/launch/check_genesis_determinism.sh
run_cmd "Snapshot supply reconciliation checker script syntax" bash -n scripts/launch/check_snapshot_supply_reconciliation.sh
if [[ "$SKIP_ISSUE1_GOAL_CHECKS" -eq 0 ]]; then
  run_cmd "Issue #1 core-goal checks" ./scripts/launch/check_issue1_core_goals.sh
fi
run_cmd \
  "Genesis determinism check (testnet fixture + pinned hash)" \
  ./scripts/launch/check_genesis_determinism.sh \
  --testnet \
  --num-accounts 100 \
  --chain-id bitinfinity-mainnet \
  --genesis-time 2026-01-01T00:00:00Z \
  --expected-hash "$GENESIS_FIXTURE_EXPECTED_HASH"
if [[ "$CHECK_SNAPSHOT_SUPPLY" -eq 1 ]]; then
  snapshot_cmd=(
    ./scripts/launch/check_snapshot_supply_reconciliation.sh
    --genesis "$SNAPSHOT_GENESIS"
    --txoutsetinfo "$SNAPSHOT_TXOUTSETINFO"
    --tolerance-sats "$SNAPSHOT_TOLERANCE_SATS"
  )
  if [[ -n "$SNAPSHOT_JSON_OUT" ]]; then
    snapshot_cmd+=(--json-out "$SNAPSHOT_JSON_OUT")
  fi
  run_cmd \
    "Snapshot supply reconciliation (gate #10 signoff)" \
    "${snapshot_cmd[@]}"
fi
if [[ "$SKIP_CHECKLIST" -eq 0 ]]; then
  checklist_cmd=(./scripts/launch/check_go_no_go_checklist.sh)
  if [[ "$REQUIRE_GO" -eq 1 ]]; then
    checklist_cmd+=(--require-go)
  fi
  run_cmd "Go/no-go checklist parse" "${checklist_cmd[@]}"
fi
if [[ "$CHECK_NIGHTLY_FUZZ_HEALTH" -eq 1 ]]; then
  nightly_fuzz_cmd=(
    ./scripts/launch/check_nightly_fuzz_health.sh
    --branch "$NIGHTLY_FUZZ_BRANCH"
    --workflow "$NIGHTLY_FUZZ_WORKFLOW"
    --window-days "$NIGHTLY_FUZZ_WINDOW_DAYS"
    --min-runs "$NIGHTLY_FUZZ_MIN_RUNS"
    --max-runs "$NIGHTLY_FUZZ_MAX_RUNS"
  )
  if [[ "$NIGHTLY_FUZZ_ALLOW_IN_PROGRESS" -eq 1 ]]; then
    nightly_fuzz_cmd+=(--allow-in-progress)
  fi
  run_cmd \
    "Nightly fuzz health (7d window)" \
    "${nightly_fuzz_cmd[@]}"
fi
run_cmd "Benchmark runner dry-run smoke" ./scripts/benchmark/run_tps_profiles.sh --dry-run --skip-build --profile all --metrics-interval 1

if [[ "$MODE" == "full" ]]; then
  run_cmd "Build release binaries" cargo build --release -p bitinfinity-btcrpc -p bitinfinity-tools -p bitinfinity-neard
  run_cmd "Verify release binary --version metadata" verify_release_versions
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
echo "Launch readiness gate passed: mode=${MODE}, include_fuzz=${INCLUDE_FUZZ}, check_nightly_fuzz_health=${CHECK_NIGHTLY_FUZZ_HEALTH}, nightly_fuzz_branch=${NIGHTLY_FUZZ_BRANCH}, nightly_fuzz_workflow=${NIGHTLY_FUZZ_WORKFLOW}, nightly_fuzz_window_days=${NIGHTLY_FUZZ_WINDOW_DAYS}, nightly_fuzz_min_runs=${NIGHTLY_FUZZ_MIN_RUNS}, nightly_fuzz_max_runs=${NIGHTLY_FUZZ_MAX_RUNS}, nightly_fuzz_allow_in_progress=${NIGHTLY_FUZZ_ALLOW_IN_PROGRESS}, check_snapshot_supply=${CHECK_SNAPSHOT_SUPPLY}, snapshot_tolerance_sats=${SNAPSHOT_TOLERANCE_SATS}, cargo_target_dir=${CARGO_TARGET_DIR}, skip_issue1_goal_checks=${SKIP_ISSUE1_GOAL_CHECKS}, require_go=${REQUIRE_GO}, skip_checklist=${SKIP_CHECKLIST}, at=$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
