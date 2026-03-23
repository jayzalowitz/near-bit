#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: ./scripts/launch/check_snapshot_supply_reconciliation.sh --genesis <path> --txoutsetinfo <path> [--tolerance-sats <n>] [--json-out <path>]

Purpose:
  Validate launch gate #10 against Bitcoin snapshot metadata by comparing
  genesis total supply with `bitcoin-cli gettxoutsetinfo` total_amount.

Options:
  --genesis <path>       Path to genesis.json
  --txoutsetinfo <path>  Path to JSON output from `bitcoin-cli gettxoutsetinfo`
  --tolerance-sats <n>   Allowed absolute difference in satoshis. Default: 1
  --json-out <path>      Write machine-readable summary JSON
  -h, --help             Show this help text
USAGE
}

GENESIS_FILE=""
TXOUTSETINFO_FILE=""
TOLERANCE_SATS=1
JSON_OUT=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --genesis)
      if [[ $# -lt 2 ]]; then
        echo "--genesis requires a path value" >&2
        exit 1
      fi
      GENESIS_FILE="$2"
      shift 2
      ;;
    --txoutsetinfo)
      if [[ $# -lt 2 ]]; then
        echo "--txoutsetinfo requires a path value" >&2
        exit 1
      fi
      TXOUTSETINFO_FILE="$2"
      shift 2
      ;;
    --tolerance-sats)
      if [[ $# -lt 2 ]]; then
        echo "--tolerance-sats requires a numeric value" >&2
        exit 1
      fi
      TOLERANCE_SATS="$2"
      shift 2
      ;;
    --json-out)
      if [[ $# -lt 2 ]]; then
        echo "--json-out requires a path value" >&2
        exit 1
      fi
      JSON_OUT="$2"
      shift 2
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

if [[ -z "$GENESIS_FILE" || -z "$TXOUTSETINFO_FILE" ]]; then
  echo "--genesis and --txoutsetinfo are required." >&2
  usage
  exit 1
fi

if [[ ! -f "$GENESIS_FILE" ]]; then
  echo "Genesis file not found: $GENESIS_FILE" >&2
  exit 1
fi

if [[ ! -f "$TXOUTSETINFO_FILE" ]]; then
  echo "txoutsetinfo file not found: $TXOUTSETINFO_FILE" >&2
  exit 1
fi

if [[ ! "$TOLERANCE_SATS" =~ ^[0-9]+$ ]]; then
  echo "--tolerance-sats must be a non-negative integer: $TOLERANCE_SATS" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

if ! command -v cargo >/dev/null 2>&1; then
  echo "Required command not found: cargo" >&2
  exit 1
fi

declare -a cmd=(
  cargo run -q -p bitinfinity-tools -- verify-snapshot-supply
  --genesis "$GENESIS_FILE"
  --txoutsetinfo "$TXOUTSETINFO_FILE"
  --tolerance-sats "$TOLERANCE_SATS"
)
if [[ -n "$JSON_OUT" ]]; then
  cmd+=(--json-out "$JSON_OUT")
fi

echo "Running: ${cmd[*]}"
"${cmd[@]}"
