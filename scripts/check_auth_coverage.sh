#!/usr/bin/env bash
set -euo pipefail

SCRIPT_PATH="${1:-scripts/e2e_testnet.sh}"

if [[ ! -f "$SCRIPT_PATH" ]]; then
  echo "E2E script not found: $SCRIPT_PATH" >&2
  exit 1
fi

if ! command -v rg >/dev/null 2>&1; then
  echo "Required command not found: rg" >&2
  exit 1
fi

extract_methods() {
  rg -o '\\\"method\\\":\\\"[^\\\"]+\\\"|\"method\":\"[^\"]+\"' \
    | sed -E 's/^\\\"method\\\":\\\"([^\\\"]+)\\\"$/\1/; s/^\"method\":\"([^\"]+)\"$/\1/' \
    | sort -u
}

all_methods="$(extract_methods <"$SCRIPT_PATH")"

auth_block="$(awk '/\[auth\] Verifying Bitcoin RPC auth behavior\.\.\./{flag=1} flag {print}' "$SCRIPT_PATH")"
if [[ -z "$auth_block" ]]; then
  echo "Could not locate auth verification block in $SCRIPT_PATH" >&2
  exit 1
fi
auth_methods="$(printf '%s\n' "$auth_block" | extract_methods)"

ignore_methods=$(
  cat <<'EOF'
query
EOF
)

missing_methods="$(
  comm -23 \
    <(printf '%s\n' "$all_methods") \
    <(printf '%s\n%s\n' "$auth_methods" "$ignore_methods" | sort -u)
)"

if [[ -n "${missing_methods//$'\n'/}" ]]; then
  echo "Missing auth-coverage methods detected in $SCRIPT_PATH:" >&2
  printf '  - %s\n' $missing_methods >&2
  exit 1
fi

echo "Auth coverage check passed for $SCRIPT_PATH"
echo "Total methods: $(printf '%s\n' "$all_methods" | wc -l | tr -d ' ')"
echo "Auth methods:  $(printf '%s\n' "$auth_methods" | wc -l | tr -d ' ')"
