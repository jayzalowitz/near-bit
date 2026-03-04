# Snapshot Supply Reconciliation

This guide documents launch gate #10 verification against real Bitcoin snapshot metadata:

- compare `genesis.json` total supply with `bitcoin-cli gettxoutsetinfo` `total_amount`
- enforce satoshi-level tolerance (default: 1 satoshi)

## Prepare Snapshot Metadata

```bash
bitcoin-cli gettxoutsetinfo > /tmp/gettxoutsetinfo.json
```

## Run Reconciliation Check

```bash
# Default tolerance (1 satoshi)
./scripts/launch/check_snapshot_supply_reconciliation.sh \
  --genesis /path/to/genesis.json \
  --txoutsetinfo /tmp/gettxoutsetinfo.json

# Strict exact match
./scripts/launch/check_snapshot_supply_reconciliation.sh \
  --genesis /path/to/genesis.json \
  --txoutsetinfo /tmp/gettxoutsetinfo.json \
  --tolerance-sats 0

# Machine-readable output
./scripts/launch/check_snapshot_supply_reconciliation.sh \
  --genesis /path/to/genesis.json \
  --txoutsetinfo /tmp/gettxoutsetinfo.json \
  --json-out /tmp/snapshot-supply-check.json
```

The command fails if the absolute difference in satoshis exceeds the configured tolerance.

## Direct Tool Command

```bash
bitinfinity-tools verify-snapshot-supply \
  --genesis /path/to/genesis.json \
  --txoutsetinfo /tmp/gettxoutsetinfo.json \
  --tolerance-sats 1 \
  --json-out /tmp/snapshot-supply-check.json
```

For launch signoff, attach:

- `gettxoutsetinfo` JSON used for the check
- reconciliation command output
- optional JSON summary (`--json-out`)
