# Incident Launch Pack

This guide explains how to prefill incident communication templates for the launch window (go/no-go gate #13).

## Purpose

`generate_incident_launch_pack.sh` creates a launch-window-specific communication pack so operator response messages already include:

1. release version/commit
2. launch window timestamps
3. status-page URL
4. validator coordination channel
5. consistent incident ID prefix

This reduces response latency during launch incidents and avoids ad-hoc formatting errors.

## Generate Pack

```bash
./scripts/launch/generate_incident_launch_pack.sh \
  --release-version <tag-or-commit> \
  --launch-window-start <YYYY-MM-DDTHH:MM:SSZ> \
  --launch-window-end <YYYY-MM-DDTHH:MM:SSZ> \
  --status-page-url https://status.bitcoininfinity.io \
  --coordination-channel <channel-url-or-name>
```

Optional overrides:

```bash
# Custom incident ID prefix and output path
./scripts/launch/generate_incident_launch_pack.sh \
  --release-version v1.0.0-rc1 \
  --launch-window-start 2026-03-10T18:00:00Z \
  --launch-window-end 2026-03-10T22:00:00Z \
  --status-page-url https://status.bitcoininfinity.io \
  --coordination-channel "#validators-bridge" \
  --incident-id-prefix MAINNET \
  --out-file artifacts/incident-launch-packs/mainnet-rc1.md
```

Default output path:

```text
artifacts/incident-launch-packs/<timestamp>-<shortsha>/incident-communication-pack.md
```

## How To Use During Launch

1. Generate the pack before the launch rehearsal or launch window.
2. Attach the generated file as gate #13 evidence in `docs/mainnet-go-no-go-checklist.md`.
3. During incidents, copy the matching section and replace remaining `<...>` placeholders with incident-specific values.
4. Keep incident IDs sequential (`<prefix>-001`, `-002`, etc.) for traceability.

## Validation

- `--launch-window-start` and `--launch-window-end` must be RFC3339 UTC (`YYYY-MM-DDTHH:MM:SSZ`).
- Required inputs: release version, launch window start/end, status page URL, coordination channel.
