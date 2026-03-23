#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: ./scripts/launch/generate_incident_launch_pack.sh \
  --release-version <tag-or-commit> \
  --launch-window-start <YYYY-MM-DDTHH:MM:SSZ> \
  --launch-window-end <YYYY-MM-DDTHH:MM:SSZ> \
  --status-page-url <url> \
  --coordination-channel <channel-url-or-name> \
  [--incident-id-prefix <prefix>] \
  [--out-file <path>]

Options:
  --release-version <value>      Required. Release tag/commit for launch candidate.
  --launch-window-start <value>  Required. Launch window start (UTC timestamp).
  --launch-window-end <value>    Required. Launch window end (UTC timestamp).
  --status-page-url <value>      Required. Canonical public status-page URL.
  --coordination-channel <value> Required. Validator coordination channel.
  --incident-id-prefix <value>   Incident ID prefix. Default: LAUNCH.
  --out-file <path>              Output file path. Default: artifacts/incident-launch-packs/<timestamp>-<shortsha>/incident-communication-pack.md
  -h, --help                     Show this help text.
USAGE
}

RELEASE_VERSION=""
LAUNCH_WINDOW_START=""
LAUNCH_WINDOW_END=""
STATUS_PAGE_URL=""
COORDINATION_CHANNEL=""
INCIDENT_ID_PREFIX="LAUNCH"
OUT_FILE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --release-version)
      if [[ $# -lt 2 ]]; then
        echo "--release-version requires a value" >&2
        exit 1
      fi
      RELEASE_VERSION="$2"
      shift 2
      ;;
    --launch-window-start)
      if [[ $# -lt 2 ]]; then
        echo "--launch-window-start requires a value" >&2
        exit 1
      fi
      LAUNCH_WINDOW_START="$2"
      shift 2
      ;;
    --launch-window-end)
      if [[ $# -lt 2 ]]; then
        echo "--launch-window-end requires a value" >&2
        exit 1
      fi
      LAUNCH_WINDOW_END="$2"
      shift 2
      ;;
    --status-page-url)
      if [[ $# -lt 2 ]]; then
        echo "--status-page-url requires a value" >&2
        exit 1
      fi
      STATUS_PAGE_URL="$2"
      shift 2
      ;;
    --coordination-channel)
      if [[ $# -lt 2 ]]; then
        echo "--coordination-channel requires a value" >&2
        exit 1
      fi
      COORDINATION_CHANNEL="$2"
      shift 2
      ;;
    --incident-id-prefix)
      if [[ $# -lt 2 ]]; then
        echo "--incident-id-prefix requires a value" >&2
        exit 1
      fi
      INCIDENT_ID_PREFIX="$2"
      shift 2
      ;;
    --out-file)
      if [[ $# -lt 2 ]]; then
        echo "--out-file requires a path value" >&2
        exit 1
      fi
      OUT_FILE="$2"
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

require_value() {
  local value="$1"
  local flag="$2"
  if [[ -z "${value// }" ]]; then
    echo "$flag is required." >&2
    exit 1
  fi
}

require_value "$RELEASE_VERSION" "--release-version"
require_value "$LAUNCH_WINDOW_START" "--launch-window-start"
require_value "$LAUNCH_WINDOW_END" "--launch-window-end"
require_value "$STATUS_PAGE_URL" "--status-page-url"
require_value "$COORDINATION_CHANNEL" "--coordination-channel"

if [[ ! "$LAUNCH_WINDOW_START" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$ ]]; then
  echo "--launch-window-start must be UTC RFC3339 format (YYYY-MM-DDTHH:MM:SSZ)." >&2
  exit 1
fi

if [[ ! "$LAUNCH_WINDOW_END" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$ ]]; then
  echo "--launch-window-end must be UTC RFC3339 format (YYYY-MM-DDTHH:MM:SSZ)." >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

if [[ -z "$OUT_FILE" ]]; then
  timestamp="$(date -u +"%Y%m%dT%H%M%SZ")"
  short_sha="$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")"
  OUT_FILE="artifacts/incident-launch-packs/${timestamp}-${short_sha}/incident-communication-pack.md"
fi

mkdir -p "$(dirname "$OUT_FILE")"

generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

cat > "$OUT_FILE" <<PACK
# Incident Communication Pack (Launch Window Prefill)

Generated at (UTC): ${generated_at}
Release candidate: ${RELEASE_VERSION}
Launch window (UTC): ${LAUNCH_WINDOW_START} to ${LAUNCH_WINDOW_END}
Status page: ${STATUS_PAGE_URL}
Coordination channel: ${COORDINATION_CHANNEL}
Incident ID prefix: ${INCIDENT_ID_PREFIX}

Use this pack to satisfy go/no-go gate #13 by pre-filling launch-window constants before incidents happen.
Replace remaining angle-bracket placeholders with incident-specific values when sending an update.

## 1) Investigating Issue

Title: Investigating issue affecting <component>

Timestamp (UTC): ${LAUNCH_WINDOW_START}
Severity: <P0|P1|P2|P3>
Affected component(s): <rpc|validator|wallet flow|explorer|other>
Current impact: <one-sentence impact statement>

We are investigating an issue affecting <component>. Users may experience <symptom>.
Current mitigation status: <none|partial mitigation active>.
Next update ETA: <minutes>.

Tracking:
- Status page: ${STATUS_PAGE_URL}
- Incident ID: ${INCIDENT_ID_PREFIX}-001
- Build/version: ${RELEASE_VERSION}

## 2) Security Halt / Do Not Transact

Title: Security advisory: pause transactions while emergency response is active

Timestamp (UTC): ${LAUNCH_WINDOW_START}
Severity: P0
Scope: <mainnet|testnet|specific subsystem>

A security issue has been identified in <component>. We are coordinating a fix with validator operators.
Do not submit new transactions until further notice.

Immediate actions in progress:
1. Validator coordination and containment.
2. Patch validation on isolated environment.
3. Public follow-up with remediation steps.

Next update ETA: <minutes>.
Status page: ${STATUS_PAGE_URL}
Incident ID: ${INCIDENT_ID_PREFIX}-002

## 3) Resolution / Recovery Complete

Title: Incident resolved: normal operations restored

Timestamp (UTC): ${LAUNCH_WINDOW_END}
Severity: <P0|P1|P2|P3>
Affected component(s): <component list>

The incident affecting <component> has been resolved. Normal operations have resumed.

What happened:
- <short factual summary>

What we changed:
- <fix 1>
- <fix 2>

Verification completed:
- <health check 1>
- <health check 2>

Post-incident report ETA: <date/time>.
Status page: ${STATUS_PAGE_URL}
Incident ID: ${INCIDENT_ID_PREFIX}-003

## 4) Emergency Upgrade Request (Validators)

Title: Emergency upgrade required: version ${RELEASE_VERSION}

Timestamp (UTC): ${LAUNCH_WINDOW_START}
Severity: <P0|P1>
Upgrade deadline (UTC): ${LAUNCH_WINDOW_END}

Validators: upgrade to ${RELEASE_VERSION} by the deadline above.
Reason: <one-sentence reason>.

Upgrade instructions:
1. Pull release/tag: ${RELEASE_VERSION}.
2. Verify artifact checksum/signature: <checksum instructions>.
3. Restart node using standard rollout (canary first).
4. Confirm post-upgrade health: block progression, RPC availability, validator status.

Compatibility notes:
- Minimum compatible version: <version>
- Unsafe versions: <version list>

Coordination channel: ${COORDINATION_CHANNEL}
Status page: ${STATUS_PAGE_URL}
Incident ID: ${INCIDENT_ID_PREFIX}-004
PACK

echo "Incident launch pack generated: ${OUT_FILE}"
