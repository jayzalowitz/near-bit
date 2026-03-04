#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: ./scripts/launch/check_issue1_core_goals.sh

Runs targeted test suites that directly validate Issue #1 core-goal behavior:
  1) Bitcoin addresses as account IDs (near-account-id)
  2) Patoshi reassignment + genesis tooling integrity (bitinfinity-tools)
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ $# -gt 0 ]]; then
  echo "Unknown argument: $1" >&2
  usage
  exit 1
fi

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

require_cmd cargo

run_cmd "Issue #1: Bitcoin account-ID validation suite" cargo test --manifest-path near-account-id/Cargo.toml
run_cmd "Issue #1: Patoshi/genesis tooling suite" cargo test -p bitinfinity-tools

echo
echo "Issue #1 core-goal checks passed at $(date -u +"%Y-%m-%dT%H:%M:%SZ")"
