#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/launch/check_nightly_fuzz_health.sh [--repo <owner/repo>] [--branch <name>] [--workflow <name>] [--window-days <n>] [--min-runs <n>] [--max-runs <n>] [--fuzz-job-pattern <regex>] [--allow-in-progress] [--fail-on-cancelled] [--json-out <path>]

Options:
  --repo <owner/repo>   GitHub repository slug. Default: derived from origin remote.
  --branch <name>       Branch to evaluate. Default: main.
  --workflow <name>     Workflow name to evaluate. Default: Nightly Fuzz.
  --window-days <n>     Lookback window in days. Default: 7.
  --min-runs <n>        Minimum required runs in window. Default: 1.
  --max-runs <n>        Max runs to fetch from API. Default: 200.
  --fuzz-job-pattern    Optional regex to evaluate only matching fuzz jobs (case-insensitive).
                        When set, runs without matching jobs are ignored.
  --allow-in-progress   Do not fail when runs are still in progress.
  --fail-on-cancelled   Treat cancelled runs/jobs as failures. Default: disabled.
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
FUZZ_JOB_PATTERN=""
ALLOW_IN_PROGRESS=0
FAIL_ON_CANCELLED=0
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
    --fuzz-job-pattern)
      if [[ $# -lt 2 ]]; then
        echo "--fuzz-job-pattern requires a value" >&2
        exit 1
      fi
      FUZZ_JOB_PATTERN="$2"
      shift 2
      ;;
    --allow-in-progress)
      ALLOW_IN_PROGRESS=1
      shift
      ;;
    --fail-on-cancelled)
      FAIL_ON_CANCELLED=1
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

evaluation_mode="workflow-run"
if [[ -z "$FUZZ_JOB_PATTERN" ]]; then
  total_runs="$(jq 'length' <<< "$window_runs")"
  success_runs="$(jq '[.[] | select(.status == "completed" and .conclusion == "success")] | length' <<< "$window_runs")"
  failed_runs="$(jq '[.[] | select(.status == "completed" and (.conclusion != "success" and .conclusion != "neutral" and .conclusion != "skipped" and .conclusion != "cancelled"))] | length' <<< "$window_runs")"
  cancelled_runs="$(jq '[.[] | select(.status == "completed" and .conclusion == "cancelled")] | length' <<< "$window_runs")"
  in_progress_runs="$(jq '[.[] | select(.status != "completed")] | length' <<< "$window_runs")"
else
  evaluation_mode="fuzz-job"
  temp_enriched="$(mktemp "/tmp/nightly-fuzz-health-enriched.XXXXXX")"
  total_runs=0
  success_runs=0
  failed_runs=0
  cancelled_runs=0
  in_progress_runs=0

  while IFS= read -r run; do
    run_id="$(jq -r '.databaseId' <<< "$run")"
    run_status="$(jq -r '.status' <<< "$run")"
    run_conclusion="$(jq -r '.conclusion // ""' <<< "$run")"
    jobs_json="$(gh api "repos/${REPO}/actions/runs/${run_id}/jobs?per_page=100")"
    matching_jobs="$(
      jq -c \
        --arg pattern "$FUZZ_JOB_PATTERN" \
        '[.jobs[] | select(.name | test($pattern; "i")) | {name, status, conclusion}]' \
        <<< "$jobs_json"
    )"
    matched_job_count="$(jq 'length' <<< "$matching_jobs")"
    if (( matched_job_count == 0 )); then
      continue
    fi

    evaluation="success"
    if jq -e '[.[] | select(.status != "completed")] | length > 0' <<< "$matching_jobs" >/dev/null; then
      if [[ "$run_status" == "completed" && "$run_conclusion" == "cancelled" ]]; then
        evaluation="cancelled"
        cancelled_runs=$((cancelled_runs + 1))
      else
        evaluation="in_progress"
        in_progress_runs=$((in_progress_runs + 1))
      fi
    elif jq -e '[.[] | select(.conclusion != "success" and .conclusion != "neutral" and .conclusion != "skipped" and .conclusion != "cancelled")] | length > 0' <<< "$matching_jobs" >/dev/null; then
      evaluation="failed"
      failed_runs=$((failed_runs + 1))
    elif jq -e '[.[] | select(.conclusion == "cancelled")] | length > 0' <<< "$matching_jobs" >/dev/null; then
      evaluation="cancelled"
      cancelled_runs=$((cancelled_runs + 1))
    else
      success_runs=$((success_runs + 1))
    fi

    total_runs=$((total_runs + 1))
    jq -n \
      --argjson run "$run" \
      --argjson jobs "$matching_jobs" \
      --arg evaluation "$evaluation" \
      '$run + {
        fuzz_evaluation: $evaluation,
        matched_fuzz_job_count: ($jobs | length),
        matched_fuzz_jobs: $jobs
      }' >> "$temp_enriched"
  done < <(jq -c '.[]' <<< "$window_runs")

  if [[ -s "$temp_enriched" ]]; then
    window_runs="$(jq -s '.' "$temp_enriched")"
  else
    window_runs='[]'
  fi
  rm -f "$temp_enriched"
fi

if [[ -z "${cancelled_runs:-}" ]]; then
  cancelled_runs=0
fi

has_min_runs=true
if (( total_runs < MIN_RUNS )); then
  has_min_runs=false
fi

has_failures=false
if (( failed_runs > 0 )); then
  has_failures=true
fi
if (( FAIL_ON_CANCELLED == 1 && cancelled_runs > 0 )); then
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
if [[ -n "$FUZZ_JOB_PATTERN" ]]; then
  echo "Fuzz job pattern: ${FUZZ_JOB_PATTERN}"
  echo "Evaluation mode:  ${evaluation_mode}"
fi
echo "Window start:     ${cutoff_iso}"
echo "Runs in window:   ${total_runs}"
echo "Successful runs:  ${success_runs}"
echo "Failed runs:      ${failed_runs}"
echo "Cancelled runs:   ${cancelled_runs}"
echo "In-progress runs: ${in_progress_runs}"
echo "Min runs needed:  ${MIN_RUNS}"
echo "Status:           ${overall_status}"

if (( total_runs > 0 )); then
  echo
  echo "Recent runs:"
  if [[ -n "$FUZZ_JOB_PATTERN" ]]; then
    jq -r '
      .[] |
      "  - id=\(.databaseId) created_at=\(.createdAt) status=\(.status) conclusion=\(.conclusion // "null") fuzz_eval=\(.fuzz_evaluation // "n/a") fuzz_jobs=\(.matched_fuzz_job_count // 0) event=\(.event) title=\(.displayTitle)"
    ' <<< "$window_runs"
  else
    jq -r '
      .[] |
      "  - id=\(.databaseId) created_at=\(.createdAt) status=\(.status) conclusion=\(.conclusion // "null") event=\(.event) title=\(.displayTitle)"
    ' <<< "$window_runs"
  fi
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
    --arg fuzz_job_pattern "$FUZZ_JOB_PATTERN" \
    --arg evaluation_mode "$evaluation_mode" \
    --argjson allow_in_progress "$ALLOW_IN_PROGRESS" \
    --argjson fail_on_cancelled "$FAIL_ON_CANCELLED" \
    --arg status "$overall_status" \
    --argjson total_runs "$total_runs" \
    --argjson successful_runs "$success_runs" \
    --argjson failed_runs "$failed_runs" \
    --argjson cancelled_runs "$cancelled_runs" \
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
        fuzz_job_pattern: (if $fuzz_job_pattern == "" then null else $fuzz_job_pattern end),
        evaluation_mode: $evaluation_mode,
        allow_in_progress: ($allow_in_progress == 1),
        fail_on_cancelled: ($fail_on_cancelled == 1)
      },
      totals: {
        runs: $total_runs,
        successful: $successful_runs,
        failed: $failed_runs,
        cancelled: $cancelled_runs,
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
    if [[ "$failed_runs" -gt 0 ]]; then
      if [[ -n "$FUZZ_JOB_PATTERN" ]]; then
        echo "Nightly fuzz health failed: found ${failed_runs} runs with failing matched fuzz jobs in window." >&2
      else
        echo "Nightly fuzz health failed: found ${failed_runs} failed runs in window." >&2
      fi
    fi
    if [[ "$FAIL_ON_CANCELLED" -eq 1 && "$cancelled_runs" -gt 0 ]]; then
      echo "Nightly fuzz health failed: found ${cancelled_runs} cancelled runs and --fail-on-cancelled is enabled." >&2
    fi
  fi
  if [[ "$ALLOW_IN_PROGRESS" -eq 0 && "$has_in_progress" == true ]]; then
    if [[ -n "$FUZZ_JOB_PATTERN" ]]; then
      echo "Nightly fuzz health failed: found ${in_progress_runs} runs with in-progress matched fuzz jobs (set --allow-in-progress to override)." >&2
    else
      echo "Nightly fuzz health failed: found ${in_progress_runs} in-progress runs (set --allow-in-progress to override)." >&2
    fi
  fi
  exit 1
fi
