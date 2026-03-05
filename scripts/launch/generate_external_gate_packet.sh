#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: ./scripts/launch/generate_external_gate_packet.sh \
  --release-version <tag-or-commit> \
  --launch-window-start <YYYY-MM-DDTHH:MM:SSZ> \
  --launch-window-end <YYYY-MM-DDTHH:MM:SSZ> \
  --status-page-url <https-url> \
  [--coordination-channel <channel>] \
  [--out-file <path>]

Generates an external launch-gate packet template for checklist gates:
1, 2, 4, 11, 12, 14, 15, 16.

Options:
  --release-version <value>       Release version/tag/commit identifier.
  --launch-window-start <value>   Launch window start (RFC3339 UTC, e.g. 2026-03-10T18:00:00Z).
  --launch-window-end <value>     Launch window end (RFC3339 UTC, e.g. 2026-03-10T22:00:00Z).
  --status-page-url <value>       Canonical status page base URL.
  --coordination-channel <value>  Incident/launch coordination channel. Default: #validators-bridge.
  --out-file <path>               Output path. Default: artifacts/external-gate-packets/<timestamp>-<shortsha>/external-gate-packet.md
  -h, --help                      Show this help.
USAGE
}

RELEASE_VERSION=""
LAUNCH_WINDOW_START=""
LAUNCH_WINDOW_END=""
STATUS_PAGE_URL=""
COORDINATION_CHANNEL="#validators-bridge"
OUT_FILE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --release-version)
      RELEASE_VERSION="${2:-}"
      shift 2
      ;;
    --launch-window-start)
      LAUNCH_WINDOW_START="${2:-}"
      shift 2
      ;;
    --launch-window-end)
      LAUNCH_WINDOW_END="${2:-}"
      shift 2
      ;;
    --status-page-url)
      STATUS_PAGE_URL="${2:-}"
      shift 2
      ;;
    --coordination-channel)
      COORDINATION_CHANNEL="${2:-}"
      shift 2
      ;;
    --out-file)
      OUT_FILE="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [[ -z "${RELEASE_VERSION}" || -z "${LAUNCH_WINDOW_START}" || -z "${LAUNCH_WINDOW_END}" || -z "${STATUS_PAGE_URL}" ]]; then
  echo "Missing required arguments." >&2
  usage
  exit 1
fi

if ! [[ "${LAUNCH_WINDOW_START}" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$ ]]; then
  echo "Invalid --launch-window-start format: ${LAUNCH_WINDOW_START}" >&2
  exit 1
fi
if ! [[ "${LAUNCH_WINDOW_END}" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$ ]]; then
  echo "Invalid --launch-window-end format: ${LAUNCH_WINDOW_END}" >&2
  exit 1
fi
if [[ "${STATUS_PAGE_URL}" != http://* && "${STATUS_PAGE_URL}" != https://* ]]; then
  echo "--status-page-url must start with http:// or https://" >&2
  exit 1
fi

timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
short_sha="$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
generated_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

if [[ -z "${OUT_FILE}" ]]; then
  OUT_FILE="artifacts/external-gate-packets/${timestamp}-${short_sha}/external-gate-packet.md"
fi

mkdir -p "$(dirname "${OUT_FILE}")"

cat > "${OUT_FILE}" <<PACKET
# External Launch Gate Packet

Generated at (UTC): ${generated_at}

- Release version: ${RELEASE_VERSION}
- Launch window: ${LAUNCH_WINDOW_START} to ${LAUNCH_WINDOW_END}
- Status page: ${STATUS_PAGE_URL}
- Coordination channel: ${COORDINATION_CHANNEL}

Use this packet to collect signoff evidence for checklist gates that cannot be satisfied by repository-only automation.

## Cross-Functional Signoff

- Security lead:
- Operations lead:
- Legal lead:
- Foundation governance lead:

## Gate 1: External Audit Report Published With Zero Open Critical Findings

- Owner:
- Audit vendor:
- Report link (public):
- Open critical findings count:
- Evidence links:
- Completed date (UTC):
- Approver:

## Gate 2: Zero Open High Findings Or Signed Accepted-Risk Waiver

- Owner:
- Open high findings count:
- Accepted-risk waiver link (if applicable):
- Evidence links:
- Completed date (UTC):
- Approver:

## Gate 4: Nightly Fuzz Matrix Stable For Previous 7 Days

- Owner:
- Workflow link:
- Runs in 7-day window:
- Unresolved crashes:
- Evidence links:
- Completed date (UTC):
- Approver:

## Gate 11: Mainnet Validator Set Confirmed

- Owner:
- Validator operator matrix link:
- Operator contact matrix link:
- Number of independent operators:
- Evidence links:
- Completed date (UTC):
- Approver:

## Gate 12: Monitoring/Alerting/Status Page Failure Drill Completed

- Owner:
- Simulated failure scenario description:
- Alert trigger evidence (time to detect):
- Pager/notification evidence:
- Status-page update URL for drill event:
- Post-drill resolution update URL:
- Evidence links:
- Completed date (UTC):
- Approver:

## Gate 14: Legal Review Signoff Complete

- Owner:
- Jurisdictions covered:
- Token classification memo link:
- Patoshi-constraint legal memo link:
- Evidence links:
- Completed date (UTC):
- Approver:

## Gate 15: Foundation Governance And Treasury Controls Published

- Owner:
- Charter link:
- Multisig policy link:
- Treasury spending policy link:
- Evidence links:
- Completed date (UTC):
- Approver:

## Gate 16: Rollback/Abort Procedure Dry-Run Completed With Validator Operators

- Owner:
- Dry-run date/time (UTC):
- Participating operators:
- Execution summary link:
- Issues discovered:
- Remediation links:
- Evidence links:
- Completed date (UTC):
- Approver:

## Final External Gate Summary

- Remaining open external gates:
- Final recommendation (ready or blocked):
- Summary rationale:
PACKET

echo "Generated external gate packet: ${OUT_FILE}"
