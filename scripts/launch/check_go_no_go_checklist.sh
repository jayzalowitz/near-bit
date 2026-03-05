#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/launch/check_go_no_go_checklist.sh [--file <path>] [--require-go] [--expected-gates <n>] [--json-out <path>]

Options:
  --file <path>  Checklist file path. Default: docs/mainnet-go-no-go-checklist.md
  --require-go   Exit non-zero unless every gate status is "done" and signoff fields are populated.
  --expected-gates <n>  Expected number of gate rows. Default: 16.
  --json-out     Write machine-readable summary JSON to the specified file path.
                 Gates marked "done" must include evidence and completed date.
                 Completed date format: YYYY-MM-DD or YYYY-MM-DDTHH:MM:SSZ.
                 Evidence entries must be resolvable repo paths or http(s) URLs.
  -h, --help     Show this help text.
EOF
}

CHECKLIST_FILE="docs/mainnet-go-no-go-checklist.md"
REQUIRE_GO=0
EXPECTED_GATES=16
JSON_OUT=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --file)
      if [[ $# -lt 2 ]]; then
        echo "--file requires a path value" >&2
        exit 1
      fi
      CHECKLIST_FILE="$2"
      shift 2
      ;;
    --require-go)
      REQUIRE_GO=1
      shift
      ;;
    --expected-gates)
      if [[ $# -lt 2 ]]; then
        echo "--expected-gates requires a numeric value" >&2
        exit 1
      fi
      EXPECTED_GATES="$2"
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

if [[ ! -f "$CHECKLIST_FILE" ]]; then
  echo "Checklist file not found: $CHECKLIST_FILE" >&2
  exit 1
fi

if [[ ! "$EXPECTED_GATES" =~ ^[0-9]+$ ]]; then
  echo "--expected-gates must be a non-negative integer: $EXPECTED_GATES" >&2
  exit 1
fi

if ! command -v awk >/dev/null 2>&1; then
  echo "Required command not found: awk" >&2
  exit 1
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "Required command not found: jq" >&2
  exit 1
fi

total_gates=0
done_gates=0
todo_gates=0
invalid_status=0
done_missing_evidence=0
done_missing_completed_date=0
done_invalid_completed_date=0
done_invalid_evidence_refs=0

declare -a pending_lines
declare -a invalid_lines
declare -a done_missing_evidence_lines
declare -a done_missing_completed_date_lines
declare -a done_invalid_completed_date_lines
declare -a done_invalid_evidence_ref_lines

trim() {
  local value="$1"
  echo "$value" | xargs
}

normalize_evidence_ref() {
  local value="$1"
  local normalized="$value"

  if [[ "$normalized" == \[*\]\(*\) ]]; then
    local link_target="${normalized#*](}"
    if [[ "$link_target" != "$normalized" && "$link_target" == *")" ]]; then
      normalized="${link_target%)}"
    fi
  fi
  if [[ "$normalized" == \`*\` ]]; then
    normalized="${normalized#\`}"
    normalized="${normalized%\`}"
  fi
  echo "$normalized"
}

resolve_local_evidence_candidate() {
  local value="$1"
  local candidate="$value"
  if [[ "$candidate" =~ ^(.+)#L[0-9]+(C[0-9]+)?$ ]]; then
    candidate="${BASH_REMATCH[1]}"
  fi
  if [[ "$candidate" =~ ^(.+):[0-9]+(:[0-9]+)?$ ]]; then
    candidate="${BASH_REMATCH[1]}"
  fi
  echo "$candidate"
}

is_valid_evidence_ref() {
  local ref="$1"
  local normalized
  local candidate

  normalized="$(normalize_evidence_ref "$ref")"
  if [[ "$normalized" =~ ^https?:// ]]; then
    return 0
  fi
  candidate="$(resolve_local_evidence_candidate "$normalized")"
  if [[ -f "$candidate" ]]; then
    return 0
  fi
  return 1
}

while IFS=$'\t' read -r gate_id gate_name gate_status gate_evidence gate_completed_date; do
  total_gates=$((total_gates + 1))
  normalized_status="$(echo "$gate_status" | tr '[:upper:]' '[:lower:]' | xargs)"
  evidence_value="$(trim "$gate_evidence")"
  completed_value="$(trim "$gate_completed_date")"
  if [[ "$normalized_status" == "done" ]]; then
    done_gates=$((done_gates + 1))
    if [[ -z "${evidence_value// }" ]]; then
      done_missing_evidence=$((done_missing_evidence + 1))
      done_missing_evidence_lines+=("${gate_id}: ${gate_name}")
    else
      evidence_entries="${evidence_value//;/,}"
      IFS=',' read -r -a evidence_refs <<< "$evidence_entries"
      for evidence_ref in "${evidence_refs[@]}"; do
        evidence_ref="$(trim "$evidence_ref")"
        if [[ -z "$evidence_ref" ]]; then
          continue
        fi
        if ! is_valid_evidence_ref "$evidence_ref"; then
          done_invalid_evidence_refs=$((done_invalid_evidence_refs + 1))
          done_invalid_evidence_ref_lines+=("${gate_id}: ${gate_name} [evidence=${evidence_ref}]")
        fi
      done
    fi
    if [[ -z "${completed_value// }" ]]; then
      done_missing_completed_date=$((done_missing_completed_date + 1))
      done_missing_completed_date_lines+=("${gate_id}: ${gate_name}")
    elif [[ ! "$completed_value" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}([T][0-9]{2}:[0-9]{2}:[0-9]{2}Z)?$ ]]; then
      done_invalid_completed_date=$((done_invalid_completed_date + 1))
      done_invalid_completed_date_lines+=("${gate_id}: ${gate_name} [completed=${completed_value}]")
    fi
  elif [[ "$normalized_status" == "todo" ]]; then
    todo_gates=$((todo_gates + 1))
    pending_lines+=("${gate_id}: ${gate_name}")
  else
    invalid_status=$((invalid_status + 1))
    invalid_lines+=("${gate_id}: ${gate_name} [status=${gate_status}]")
  fi
done < <(
  awk -F'|' '
    $0 ~ /^\|[[:space:]]*[0-9]+[[:space:]]*\|/ {
      id=$2; gate=$3; status=$5; evidence=$6; completed=$7;
      gsub(/^[ \t]+|[ \t]+$/, "", id);
      gsub(/^[ \t]+|[ \t]+$/, "", gate);
      gsub(/^[ \t]+|[ \t]+$/, "", status);
      gsub(/^[ \t]+|[ \t]+$/, "", evidence);
      gsub(/^[ \t]+|[ \t]+$/, "", completed);
      printf "%s\t%s\t%s\t%s\t%s\n", id, gate, status, evidence, completed;
    }
  ' "$CHECKLIST_FILE"
)

if [[ "$total_gates" -eq 0 ]]; then
  echo "No gate rows found in checklist: $CHECKLIST_FILE" >&2
  exit 1
fi

gate_count_mismatch=0
if [[ "$EXPECTED_GATES" -gt 0 && "$total_gates" -ne "$EXPECTED_GATES" ]]; then
  gate_count_mismatch=1
fi

required_signoff_fields=(
  "Release candidate commit:"
  "Proposed genesis hash:"
  "Planned launch window (UTC):"
  "Final decision:"
  "Decision timestamp (UTC):"
  "Signoff approvers:"
)

missing_signoff=0
declare -a missing_signoff_lines
for field in "${required_signoff_fields[@]}"; do
  raw_line="$(
    awk -v needle="$field" '
      {
        line = $0
        sub(/^[[:space:]]*-[[:space:]]*/, "", line)
        if (index(line, needle) == 1) {
          print line
          exit
        }
      }
    ' "$CHECKLIST_FILE"
  )"
  value="${raw_line#"$field"}"
  value="$(echo "$value" | xargs)"
  if [[ -z "$raw_line" || -z "${value// }" ]]; then
    missing_signoff=$((missing_signoff + 1))
    missing_signoff_lines+=("$field")
  fi
done

echo "Checklist summary: file=${CHECKLIST_FILE}"
echo "Expected gates: ${EXPECTED_GATES}"
echo "Total gates:   ${total_gates}"
echo "Done gates:    ${done_gates}"
echo "Todo gates:    ${todo_gates}"
echo "Invalid gates: ${invalid_status}"
echo "Done missing evidence: ${done_missing_evidence}"
echo "Done missing completed date: ${done_missing_completed_date}"
echo "Done invalid completed date: ${done_invalid_completed_date}"
echo "Done invalid evidence refs: ${done_invalid_evidence_refs}"
echo "Missing signoff fields: ${missing_signoff}"

if [[ "$gate_count_mismatch" -eq 1 ]]; then
  echo "Gate count mismatch: expected ${EXPECTED_GATES}, found ${total_gates}" >&2
fi

if [[ "$todo_gates" -gt 0 ]]; then
  echo
  echo "Pending gates:"
  for line in "${pending_lines[@]-}"; do
    if [[ -z "$line" ]]; then
      continue
    fi
    echo "  - ${line}"
  done
fi

if [[ "$invalid_status" -gt 0 ]]; then
  echo
  echo "Invalid gate statuses:"
  for line in "${invalid_lines[@]-}"; do
    if [[ -z "$line" ]]; then
      continue
    fi
    echo "  - ${line}"
  done
fi

if [[ "$missing_signoff" -gt 0 ]]; then
  echo
  echo "Missing signoff fields:"
  for line in "${missing_signoff_lines[@]-}"; do
    if [[ -z "$line" ]]; then
      continue
    fi
    echo "  - ${line}"
  done
fi

if [[ "$done_missing_evidence" -gt 0 ]]; then
  echo
  echo "Done gates missing evidence:"
  for line in "${done_missing_evidence_lines[@]-}"; do
    if [[ -z "$line" ]]; then
      continue
    fi
    echo "  - ${line}"
  done
fi

if [[ "$done_missing_completed_date" -gt 0 ]]; then
  echo
  echo "Done gates missing completed date:"
  for line in "${done_missing_completed_date_lines[@]-}"; do
    if [[ -z "$line" ]]; then
      continue
    fi
    echo "  - ${line}"
  done
fi

if [[ "$done_invalid_completed_date" -gt 0 ]]; then
  echo
  echo "Done gates with invalid completed date format:"
  for line in "${done_invalid_completed_date_lines[@]-}"; do
    if [[ -z "$line" ]]; then
      continue
    fi
    echo "  - ${line}"
  done
fi

if [[ "$done_invalid_evidence_refs" -gt 0 ]]; then
  echo
  echo "Done gates with invalid evidence refs:"
  for line in "${done_invalid_evidence_ref_lines[@]-}"; do
    if [[ -z "$line" ]]; then
      continue
    fi
    echo "  - ${line}"
  done
fi

if [[ -n "$JSON_OUT" ]]; then
  pending_json="$(printf '%s\n' "${pending_lines[@]-}" | jq -R -s 'split("\n") | map(select(length > 0))')"
  invalid_json="$(printf '%s\n' "${invalid_lines[@]-}" | jq -R -s 'split("\n") | map(select(length > 0))')"
  missing_json="$(printf '%s\n' "${missing_signoff_lines[@]-}" | jq -R -s 'split("\n") | map(select(length > 0))')"
  done_missing_evidence_json="$(printf '%s\n' "${done_missing_evidence_lines[@]-}" | jq -R -s 'split("\n") | map(select(length > 0))')"
  done_missing_completed_json="$(printf '%s\n' "${done_missing_completed_date_lines[@]-}" | jq -R -s 'split("\n") | map(select(length > 0))')"
  done_invalid_completed_json="$(printf '%s\n' "${done_invalid_completed_date_lines[@]-}" | jq -R -s 'split("\n") | map(select(length > 0))')"
  done_invalid_evidence_json="$(printf '%s\n' "${done_invalid_evidence_ref_lines[@]-}" | jq -R -s 'split("\n") | map(select(length > 0))')"

  jq -n \
    --arg file "$CHECKLIST_FILE" \
    --argjson require_go "$REQUIRE_GO" \
    --argjson expected_gates "$EXPECTED_GATES" \
    --argjson total_gates "$total_gates" \
    --argjson done_gates "$done_gates" \
    --argjson todo_gates "$todo_gates" \
    --argjson invalid_gates "$invalid_status" \
    --argjson done_missing_evidence "$done_missing_evidence" \
    --argjson done_missing_completed_date "$done_missing_completed_date" \
    --argjson done_invalid_completed_date "$done_invalid_completed_date" \
    --argjson done_invalid_evidence_refs "$done_invalid_evidence_refs" \
    --argjson missing_signoff_fields "$missing_signoff" \
    --argjson gate_count_mismatch "$gate_count_mismatch" \
    --argjson pending "$pending_json" \
    --argjson invalid "$invalid_json" \
    --argjson missing "$missing_json" \
    --argjson done_missing_evidence_rows "$done_missing_evidence_json" \
    --argjson done_missing_completed_rows "$done_missing_completed_json" \
    --argjson done_invalid_completed_rows "$done_invalid_completed_json" \
    --argjson done_invalid_evidence_rows "$done_invalid_evidence_json" \
    '{
      checklist_file: $file,
      require_go: $require_go,
      totals: {
        expected: $expected_gates,
        gates: $total_gates,
        done: $done_gates,
        todo: $todo_gates,
        invalid: $invalid_gates,
        done_missing_evidence: $done_missing_evidence,
        done_missing_completed_date: $done_missing_completed_date,
        done_invalid_completed_date: $done_invalid_completed_date,
        done_invalid_evidence_refs: $done_invalid_evidence_refs,
        missing_signoff_fields: $missing_signoff_fields
      },
      gate_count_mismatch: ($gate_count_mismatch == 1),
      pending_gates: ($pending // []),
      invalid_gates: ($invalid // []),
      missing_signoff: ($missing // []),
      done_missing_evidence: ($done_missing_evidence_rows // []),
      done_missing_completed_date: ($done_missing_completed_rows // []),
      done_invalid_completed_date: ($done_invalid_completed_rows // []),
      done_invalid_evidence_refs: ($done_invalid_evidence_rows // [])
    }' > "$JSON_OUT"
fi

if [[ "$gate_count_mismatch" -eq 1 ]]; then
  echo
  echo "Checklist structure invalid: expected ${EXPECTED_GATES} gates, found ${total_gates}." >&2
  exit 1
fi

if [[ "$REQUIRE_GO" -eq 1 ]]; then
  if [[ "$todo_gates" -gt 0 || "$invalid_status" -gt 0 || "$missing_signoff" -gt 0 || "$done_missing_evidence" -gt 0 || "$done_missing_completed_date" -gt 0 || "$done_invalid_completed_date" -gt 0 || "$done_invalid_evidence_refs" -gt 0 ]]; then
    echo
    echo "GO criteria not met (--require-go)." >&2
    exit 1
  fi
fi
