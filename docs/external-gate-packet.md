# External Gate Packet

This guide covers generating a structured evidence packet for launch checklist gates that require non-repository signoff.

Target gates:

- `1` external audit report
- `2` high-finding closure/waiver
- `4` 7-day nightly fuzz stability
- `11` validator set + contact matrix
- `12` monitoring/alerting/status-page drill
- `14` legal signoff
- `15` governance + treasury publication
- `16` rollback/abort dry-run with operators

## Command

```bash
./scripts/launch/generate_external_gate_packet.sh \
  --release-version <tag-or-commit> \
  --launch-window-start <YYYY-MM-DDTHH:MM:SSZ> \
  --launch-window-end <YYYY-MM-DDTHH:MM:SSZ> \
  --status-page-url https://status.bitcoininfinity.io \
  --coordination-channel "#validators-bridge" \
  --out-file docs/external-gate-packet-mainnet-<date>.md
```

## Example

```bash
./scripts/launch/generate_external_gate_packet.sh \
  --release-version fb54b14a0 \
  --launch-window-start 2026-03-10T18:00:00Z \
  --launch-window-end 2026-03-10T22:00:00Z \
  --status-page-url https://status.bitcoininfinity.io \
  --coordination-channel "#validators-bridge" \
  --out-file docs/external-gate-packet-mainnet-2026-03-10.md
```

## How To Use During Launch

1. Generate packet for the target launch window.
2. Assign owners for each external gate section.
3. Collect links to final evidence artifacts and signoff records.
4. Once evidence is complete, update `docs/mainnet-go-no-go-checklist.md` gate rows with `scripts/launch/update_go_no_go_gate.sh`.
5. Re-run checklist validation:

```bash
./scripts/launch/check_go_no_go_checklist.sh --file docs/mainnet-go-no-go-checklist.md
```
