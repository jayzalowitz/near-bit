#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/launch/generate_evidence_bundle.sh [--mode smoke|full] [--include-fuzz] [--skip-gate] [--require-go] [--checklist-file <path>] [--out-dir <path>]

Options:
  --mode <smoke|full>  Readiness gate mode to run. Default: full.
  --include-fuzz       Pass --include-fuzz to readiness gate.
  --skip-gate          Skip executing readiness gate and only snapshot metadata/docs.
  --require-go         Enforce GO criteria in checklist validation.
  --checklist-file     Checklist file path. Default: docs/mainnet-go-no-go-checklist.md
  --allow-dirty        Allow running on a dirty worktree (default: fail if dirty).
  --out-dir <path>     Output root for bundles. Default: artifacts/launch-readiness.
  -h, --help           Show this help text.
EOF
}

MODE="full"
INCLUDE_FUZZ=0
SKIP_GATE=0
REQUIRE_GO=0
ALLOW_DIRTY=0
OUT_ROOT="artifacts/launch-readiness"
CHECKLIST_FILE="docs/mainnet-go-no-go-checklist.md"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      if [[ $# -lt 2 ]]; then
        echo "--mode requires a value (smoke|full)" >&2
        exit 1
      fi
      MODE="$2"
      shift 2
      ;;
    --include-fuzz)
      INCLUDE_FUZZ=1
      shift
      ;;
    --skip-gate)
      SKIP_GATE=1
      shift
      ;;
    --require-go)
      REQUIRE_GO=1
      shift
      ;;
    --checklist-file)
      if [[ $# -lt 2 ]]; then
        echo "--checklist-file requires a path value" >&2
        exit 1
      fi
      CHECKLIST_FILE="$2"
      shift 2
      ;;
    --allow-dirty)
      ALLOW_DIRTY=1
      shift
      ;;
    --out-dir)
      if [[ $# -lt 2 ]]; then
        echo "--out-dir requires a path value" >&2
        exit 1
      fi
      OUT_ROOT="$2"
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

if [[ "$MODE" != "smoke" && "$MODE" != "full" ]]; then
  echo "Invalid --mode value: $MODE (expected smoke or full)" >&2
  exit 1
fi

if [[ ! -f "$CHECKLIST_FILE" ]]; then
  echo "Checklist file not found: $CHECKLIST_FILE" >&2
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

require_cmd git
require_cmd jq
require_cmd shasum
require_cmd rustc
require_cmd cargo
require_cmd bash

timestamp="$(date -u +"%Y%m%dT%H%M%SZ")"
iso_timestamp="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
commit_sha="$(git rev-parse HEAD)"
short_sha="$(git rev-parse --short HEAD)"
branch_name="$(git rev-parse --abbrev-ref HEAD)"
worktree_status="$(git status --porcelain)"

if [[ -n "$worktree_status" ]]; then
  worktree_dirty=true
else
  worktree_dirty=false
fi

if [[ "$worktree_dirty" == true && "$ALLOW_DIRTY" -eq 0 ]]; then
  echo "Refusing to generate launch evidence on a dirty worktree." >&2
  echo "Commit/stash changes or pass --allow-dirty for non-signoff runs." >&2
  exit 1
fi

bundle_dir="${OUT_ROOT}/${timestamp}-${short_sha}"
mkdir -p "$bundle_dir"

echo "Writing launch evidence bundle to: $bundle_dir"

jq -n \
  --arg generated_at "$iso_timestamp" \
  --arg timestamp "$timestamp" \
  --arg commit_sha "$commit_sha" \
  --arg short_sha "$short_sha" \
  --arg branch "$branch_name" \
  --arg mode "$MODE" \
  --argjson include_fuzz "$INCLUDE_FUZZ" \
  --argjson skip_gate "$SKIP_GATE" \
  --argjson require_go "$REQUIRE_GO" \
  --argjson allow_dirty "$ALLOW_DIRTY" \
  --argjson worktree_dirty "$worktree_dirty" \
  --arg checklist_file "$CHECKLIST_FILE" \
  --arg rustc_version "$(rustc --version)" \
  --arg cargo_version "$(cargo --version)" \
  '{
    generated_at: $generated_at,
    timestamp: $timestamp,
    git: {
      commit_sha: $commit_sha,
      short_sha: $short_sha,
      branch: $branch,
      worktree_dirty: $worktree_dirty
    },
    readiness_gate: {
      mode: $mode,
      include_fuzz: $include_fuzz,
      skipped: $skip_gate
    },
    checklist: {
      file: $checklist_file,
      require_go: $require_go
    },
    execution: {
      allow_dirty: $allow_dirty
    },
    toolchain: {
      rustc_version: $rustc_version,
      cargo_version: $cargo_version
    }
  }' > "${bundle_dir}/metadata.json"

{
  echo "$worktree_status"
} > "${bundle_dir}/git-status.txt"

git show --no-patch --pretty=fuller "$commit_sha" > "${bundle_dir}/git-commit.txt"

copy_and_checksum() {
  local src="$1"
  local dst="$2"
  cp "$src" "$dst"
  shasum -a 256 "$dst" >> "${bundle_dir}/SHA256SUMS.txt"
}

copy_and_checksum "docs/launch-readiness-gates.md" "${bundle_dir}/launch-readiness-gates.md"
checklist_basename="$(basename "$CHECKLIST_FILE")"
copy_and_checksum "$CHECKLIST_FILE" "${bundle_dir}/${checklist_basename}"
copy_and_checksum "docs/incident-communication-templates.md" "${bundle_dir}/incident-communication-templates.md"
copy_and_checksum ".github/workflows/ci.yml" "${bundle_dir}/ci.yml"
copy_and_checksum ".github/workflows/nightly-fuzz.yml" "${bundle_dir}/nightly-fuzz.yml"
copy_and_checksum "scripts/launch/run_readiness_gate.sh" "${bundle_dir}/run_readiness_gate.sh"
copy_and_checksum "scripts/launch/check_go_no_go_checklist.sh" "${bundle_dir}/check_go_no_go_checklist.sh"
copy_and_checksum "scripts/launch/run_launch_rehearsal.sh" "${bundle_dir}/run_launch_rehearsal.sh"

gate_status="skipped"
gate_exit_code=0
if [[ "$SKIP_GATE" -eq 0 ]]; then
  gate_log="${bundle_dir}/readiness-gate.log"
  gate_cmd=(./scripts/launch/run_readiness_gate.sh "--${MODE}")
  gate_cmd+=(--skip-checklist)
  if [[ "$REQUIRE_GO" -eq 1 ]]; then
    gate_cmd+=(--require-go)
  fi
  if [[ "$INCLUDE_FUZZ" -eq 1 ]]; then
    gate_cmd+=(--include-fuzz)
  fi

  echo "Running readiness gate: ${gate_cmd[*]}"
  set +e
  "${gate_cmd[@]}" 2>&1 | tee "$gate_log"
  gate_exit_code=${PIPESTATUS[0]}
  set -e
  if [[ "$gate_exit_code" -eq 0 ]]; then
    gate_status="passed"
  else
    gate_status="failed"
  fi
else
  echo "Skipping readiness gate execution (--skip-gate set)."
fi

checklist_status="passed"
checklist_exit_code=0
checklist_log="${bundle_dir}/go-no-go-checklist-report.txt"
checklist_json="${bundle_dir}/go-no-go-checklist-report.json"
checklist_cmd=(bash ./scripts/launch/check_go_no_go_checklist.sh --file "$CHECKLIST_FILE" --json-out "$checklist_json")
if [[ "$REQUIRE_GO" -eq 1 ]]; then
  checklist_cmd+=(--require-go)
fi

echo "Running checklist validation: ${checklist_cmd[*]}"
set +e
"${checklist_cmd[@]}" 2>&1 | tee "$checklist_log"
checklist_exit_code=${PIPESTATUS[0]}
set -e
if [[ "$checklist_exit_code" -ne 0 ]]; then
  checklist_status="failed"
fi

jq \
  --arg gate_status "$gate_status" \
  --argjson gate_exit_code "$gate_exit_code" \
  --arg checklist_status "$checklist_status" \
  --argjson checklist_exit_code "$checklist_exit_code" \
  '.readiness_gate.status = $gate_status
   | .readiness_gate.exit_code = $gate_exit_code
   | .checklist.status = $checklist_status
   | .checklist.exit_code = $checklist_exit_code' \
  "${bundle_dir}/metadata.json" > "${bundle_dir}/metadata.tmp.json"
mv "${bundle_dir}/metadata.tmp.json" "${bundle_dir}/metadata.json"

cat > "${bundle_dir}/SUMMARY.md" <<EOF
# Launch Evidence Bundle Summary

- generated_at: ${iso_timestamp}
- bundle_dir: ${bundle_dir}
- commit: ${commit_sha}
- branch: ${branch_name}
- worktree_dirty: ${worktree_dirty}
- readiness_gate_mode: ${MODE}
- readiness_gate_include_fuzz: ${INCLUDE_FUZZ}
- readiness_gate_status: ${gate_status}
- readiness_gate_exit_code: ${gate_exit_code}
- checklist_file: ${CHECKLIST_FILE}
- checklist_require_go: ${REQUIRE_GO}
- checklist_status: ${checklist_status}
- checklist_exit_code: ${checklist_exit_code}

## Files

- metadata.json
- git-status.txt
- git-commit.txt
- SHA256SUMS.txt
- launch-readiness-gates.md
- ${checklist_basename}
- incident-communication-templates.md
- ci.yml
- nightly-fuzz.yml
- run_readiness_gate.sh
- check_go_no_go_checklist.sh
- run_launch_rehearsal.sh
- go-no-go-checklist-report.txt
- go-no-go-checklist-report.json
EOF

if [[ "$SKIP_GATE" -eq 0 ]]; then
  echo "- readiness-gate.log" >> "${bundle_dir}/SUMMARY.md"
fi

echo
echo "Launch evidence bundle complete: ${bundle_dir}"

if [[ "$SKIP_GATE" -eq 0 && "$gate_exit_code" -ne 0 ]]; then
  echo "Readiness gate failed during evidence generation. See ${bundle_dir}/readiness-gate.log" >&2
  exit "$gate_exit_code"
fi

if [[ "$checklist_exit_code" -ne 0 ]]; then
  echo "Checklist validation failed during evidence generation. See ${bundle_dir}/go-no-go-checklist-report.txt" >&2
  exit "$checklist_exit_code"
fi
