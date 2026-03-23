# Go/No-Go Gate Update Helper

This helper updates one gate row in `docs/mainnet-go-no-go-checklist.md` without manual table editing.

## Purpose

`update_go_no_go_gate.sh` reduces formatting mistakes during launch operations by:

1. selecting a gate by numeric id
2. setting status to `todo` or `done`
3. validating required fields for `done`
4. preserving the checklist table structure

## Usage

```bash
# Mark gate 13 done with owner/evidence/date metadata
./scripts/launch/update_go_no_go_gate.sh \
  --gate 13 \
  --status done \
  --owner "ops-lead" \
  --evidence "docs/incident-launch-pack.md, artifacts/launch-rehearsals/20260305T160032Z-9710ac08e/SUMMARY.md" \
  --completed-date 2026-03-05

# Move a gate back to todo and clear owner/evidence/date cells
./scripts/launch/update_go_no_go_gate.sh --gate 13 --status todo

# Update a non-default checklist file path
./scripts/launch/update_go_no_go_gate.sh \
  --file /tmp/mainnet-go-no-go-checklist.md \
  --gate 3 \
  --status done \
  --owner "release-eng" \
  --evidence "artifacts/launch-rehearsals/20260305T160032Z-9710ac08e/SUMMARY.md" \
  --completed-date 2026-03-05T16:00:38Z
```

## Validation Rules

For `--status done`, the script requires:

- `--owner` non-empty
- `--evidence` non-empty
- `--completed-date` in `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM:SSZ` format
- evidence refs that resolve to repository file paths (supports `:line` / `#L...` suffixes) or `http(s)` URLs

For `--status todo`, `--owner`, `--evidence`, and `--completed-date` are rejected and the target row metadata cells are cleared.

## Recommended Follow-Up

After updates, run checklist validation:

```bash
./scripts/launch/check_go_no_go_checklist.sh --json-out /tmp/go-no-go-summary.json
```
