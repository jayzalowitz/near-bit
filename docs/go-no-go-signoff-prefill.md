# Go/No-Go Signoff Prefill

This guide covers pre-filling the signoff block in `docs/mainnet-go-no-go-checklist.md` with correctly formatted launch metadata.

## Purpose

`prefill_go_no_go_signoff.sh` updates these fields in one command:

1. Release candidate commit
2. Proposed genesis hash
3. Planned launch window (UTC)
4. Final decision (`GO` or `NO-GO`)
5. Decision timestamp (UTC)
6. Signoff approvers

It prevents manual formatting drift and keeps output compatible with `check_go_no_go_checklist.sh` strict validation.

## Usage

```bash
./scripts/launch/prefill_go_no_go_signoff.sh \
  --release-commit <7-40 char hex sha> \
  --genesis-hash <64-char hex hash> \
  --launch-window-start <YYYY-MM-DDTHH:MM:SSZ> \
  --launch-window-end <YYYY-MM-DDTHH:MM:SSZ> \
  --final-decision <GO|NO-GO> \
  --approvers "<name1>, <name2>, <name3>"
```

Optional parameters:

```bash
# Override checklist file path
./scripts/launch/prefill_go_no_go_signoff.sh ... --file /tmp/mainnet-go-no-go-checklist.md

# Override decision timestamp (default is current UTC time)
./scripts/launch/prefill_go_no_go_signoff.sh ... --decision-timestamp 2026-03-10T17:55:00Z
```

## Validation Rules

The script enforces:

- release commit: `^[0-9a-fA-F]{7,40}$`
- genesis hash: `^[0-9a-fA-F]{64}$`
- launch window timestamps: RFC3339 UTC (`YYYY-MM-DDTHH:MM:SSZ`)
- final decision: `GO` or `NO-GO`
- decision timestamp: RFC3339 UTC

After prefill, validate with:

```bash
./scripts/launch/check_go_no_go_checklist.sh --json-out /tmp/go-no-go-summary.json
```
