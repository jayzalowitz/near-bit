#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/launch/check_nightly_fuzz_health.sh [--repo <owner/repo>] [--branch <name>] [--workflow <name>] [--window-days <n>] [--min-runs <n>] [--max-runs <n>] [--allow-in-progress] [--json-out <path>]

Options:
  --repo <owner/repo>   GitHub repository slug. Default: derived from origin remote.
  --branch <name>       Branch to evaluate. Default: main.
  --workflow <name>     Workflow name to evaluate. Default: Nightly Fuzz.
  --window-days <n>     Lookback window in days. Default: 7.
  --min-runs <n>        Minimum required runs in window. Default: 1.
  --max-runs <n>        Max runs to fetch from API. Default: 200.
  --allow-in-progress   Do not fail when runs are still in progress.
  --json-out <path>     Write machine-readable summary JSON.
  -h, --help            Show this help text.
EOF
}

REPO=""
BRANCH="main"
WORKFLOW_NAME="Nightly Fuzz"
WINDOW_DAYS=7
MIN_RUNS=1
MAX_RUNS=200
ALLOW_IN_PROGRESS=0
JSON_OUT=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo)
      if [[ $# -lt 2 ]]; then
        echo "--repo requires a value" >&2
        exit 1
      fi
      REPO="$2"
      shift 2
      ;;
    --branch)
      if [[ $# -lt 2 ]]; then
        echo "--branch requires a value" >&2
        exit 1
      fi
      BRANCH="$2"
      shift 2
      ;;
    --workflow)
      if [[ $# -lt 2 ]]; then
        echo "--workflow requires a value" >&2
        exit 1
      fi
      WORKFLOW_NAME="$2"
      shift 2
      ;;
    --window-days)
      if [[ $# -lt 2 ]]; then
        echo "--window-days requires a numeric value" >&2
        exit 1
      fi
      WINDOW_DAYS="$2"
      shift 2
      ;;
    --min-runs)
      if [[ $# -lt 2 ]]; then
        echo "--min-runs requires a numeric value" >&2
        exit 1
      fi
      MIN_RUNS="$2"
      shift 2
      ;;
    --max-runs)
      if [[ $# -lt 2 ]]; then
        echo "--max-runs requires a numeric value" >&2
        exit 1
      fi
      MAX_RUNS="$2"
      shift 2
      ;;
    --allow-in-progress)
      ALLOW_IN_PROGRESS=1
      shift
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

for num in "$WINDOW_DAYS" "$MIN_RUNS" "$MAX_RUNS"; do
  if [[ ! "$num" =~ ^[0-9]+$ ]]; then
    echo "Numeric option values must be non-negative integers." >&2
    exit 1
  fi
done

require_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Required command not found: $cmd" >&2
    exit 1
  fi
}

require_cmd git
require_cmd gh
require_cmd jq
require_cmd date

if [[ -z "$REPO" ]]; then
  origin_url="$(git config --get remote.origin.url || true)"
  if [[ -z "$origin_url" ]]; then
    echo "Could not resolve --repo: remote.origin.url is missing." >&2
    exit 1
  fi
  if [[ "$origin_url" =~ github\.com[:/]([^/]+)/([^/.]+)(\.git)?$ ]]; then
    REPO="${BASH_REMATCH[1]}/${BASH_REMATCH[2]}"
  else
    echo "Could not parse GitHub repo from origin URL: $origin_url" >&2
    echo "Pass --repo owner/repo explicitly." >&2
    exit 1
  fi
fi

cutoff_iso=""
if cutoff_iso="$(date -u -v-"${WINDOW_DAYS}"d +"%Y-%m-%dT%H:%M:%SZ" 2>/dev/null)"; then
  :
elif cutoff_iso="$(date -u -d "${WINDOW_DAYS} days ago" +"%Y-%m-%dT%H:%M:%SZ" 2>/dev/null)"; then
  :
else
  echo "Could not compute cutoff timestamp for --window-days=${WINDOW_DAYS}" >&2
  exit 1
fi

runs_json="$(gh run list \
  --repo "$REPO" \
  --branch "$BRANCH" \
  --limit "$MAX_RUNS" \
  --json databaseId,createdAt,status,conclusion,workflowName,event,displayTitle,url)"

window_runs="$(
  jq -c \
    --arg workflow "$WORKFLOW_NAME" \
    --arg cutoff "$cutoff_iso" \
    '
      map(select(.workflowName == $workflow and .createdAt >= $cutoff))
      | sort_by(.createdAt)
      | reverse
    ' \
    <<< "$runs_json"
)"

total_runs="$(jq 'length' <<< "$window_runs")"
success_runs="$(jq '[.[] | select(.status == "completed" and .conclusion == "success")] | length' <<< "$window_runs")"
failed_runs="$(jq '[.[] | select(.status == "completed" and (.conclusion != "success" and .conclusion != "neutral" and .conclusion != "skipped"))] | length' <<< "$window_runs")"
in_progress_runs="$(jq '[.[] | select(.status != "completed")] | length' <<< "$window_runs")"

has_min_runs=true
if (( total_runs < MIN_RUNS )); then
  has_min_runs=false
fi

has_failures=false
if (( failed_runs > 0 )); then
  has_failures=true
fi

has_in_progress=false
if (( in_progress_runs > 0 )); then
  has_in_progress=true
fi

overall_status="passed"
if [[ "$has_min_runs" == false || "$has_failures" == true ]]; then
  overall_status="failed"
fi
if [[ "$ALLOW_IN_PROGRESS" -eq 0 && "$has_in_progress" == true ]]; then
  overall_status="failed"
fi

echo "Nightly fuzz health summary"
echo "Repo:             ${REPO}"
echo "Branch:           ${BRANCH}"
echo "Workflow:         ${WORKFLOW_NAME}"
echo "Window start:     ${cutoff_iso}"
echo "Runs in window:   ${total_runs}"
echo "Successful runs:  ${success_runs}"
echo "Failed runs:      ${failed_runs}"
echo "In-progress runs: ${in_progress_runs}"
echo "Min runs needed:  ${MIN_RUNS}"
echo "Status:           ${overall_status}"

if (( total_runs > 0 )); then
  echo
  echo "Recent runs:"
  jq -r '
    .[] |
    "  - id=\(.databaseId) created_at=\(.createdAt) status=\(.status) conclusion=\(.conclusion // "null") event=\(.event) title=\(.displayTitle)"
  ' <<< "$window_runs"
fi

if [[ -n "$JSON_OUT" ]]; then
  jq -n \
    --arg repo "$REPO" \
    --arg branch "$BRANCH" \
    --arg workflow "$WORKFLOW_NAME" \
    --arg window_start "$cutoff_iso" \
    --argjson window_days "$WINDOW_DAYS" \
    --argjson min_runs "$MIN_RUNS" \
    --argjson max_runs "$MAX_RUNS" \
    --argjson allow_in_progress "$ALLOW_IN_PROGRESS" \
    --arg status "$overall_status" \
    --argjson total_runs "$total_runs" \
    --argjson successful_runs "$success_runs" \
    --argjson failed_runs "$failed_runs" \
    --argjson in_progress_runs "$in_progress_runs" \
    --argjson runs "$window_runs" \
    '{
      repo: $repo,
      branch: $branch,
      workflow: $workflow,
      window: {
        days: $window_days,
        start_iso: $window_start
      },
      criteria: {
        min_runs: $min_runs,
        allow_in_progress: ($allow_in_progress == 1)
      },
      totals: {
        runs: $total_runs,
        successful: $successful_runs,
        failed: $failed_runs,
        in_progress: $in_progress_runs
      },
      status: $status,
      runs: $runs
    }' > "$JSON_OUT"
fi

if [[ "$overall_status" != "passed" ]]; then
  echo
  if [[ "$has_min_runs" == false ]]; then
    echo "Nightly fuzz health failed: required at least ${MIN_RUNS} runs, found ${total_runs}." >&2
  fi
  if [[ "$has_failures" == true ]]; then
    echo "Nightly fuzz health failed: found ${failed_runs} failed/cancelled runs in window." >&2
  fi
  if [[ "$ALLOW_IN_PROGRESS" -eq 0 && "$has_in_progress" == true ]]; then
    echo "Nightly fuzz health failed: found ${in_progress_runs} in-progress runs (set --allow-in-progress to override)." >&2
  fi
  exit 1
fi
