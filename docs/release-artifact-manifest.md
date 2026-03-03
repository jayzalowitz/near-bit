# Release Artifact Manifest

This guide documents how to generate reproducible release binary manifests for launch candidates.

## Purpose

`generate_release_manifest.sh` produces:

1. built release binaries for launch-critical executables
2. SHA256 checksums for each binary
3. machine-readable metadata with commit/toolchain/version context

This gives operations and reviewers a deterministic artifact record for a candidate commit.

## Run Locally

```bash
# Build binaries and generate manifest (default)
./scripts/launch/generate_release_manifest.sh

# Reuse existing binaries from target/release
./scripts/launch/generate_release_manifest.sh --skip-build

# Write to a custom output directory
./scripts/launch/generate_release_manifest.sh --out-dir /tmp/release-manifests
```

## Outputs

Each run writes under:

```text
artifacts/release-manifests/<timestamp>-<shortsha>/
```

Key files:

- `metadata.json`: git/build/toolchain metadata plus per-binary checksums and sizes.
- `SHA256SUMS.txt`: checksum file for direct verification.
- `SUMMARY.md`: human-readable manifest summary.
- `binaries/`: copied release binaries:
  - `bitinfinity-btcrpc`
  - `bitinfinity-tools`
  - `bitinfinity-neard`

## Verify Checksums

Use one of:

```bash
# Linux
sha256sum -c SHA256SUMS.txt

# macOS
while read -r expected file; do
  actual="$(shasum -a 256 "$file" | awk '{print $1}')"
  [[ "$actual" == "$expected" ]] || { echo "checksum mismatch: $file"; exit 1; }
done < SHA256SUMS.txt
```

## GitHub Actions

Use workflow `.github/workflows/release-manifest.yml` (manual dispatch) to generate and archive manifest artifacts in CI.
