#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: ./scripts/launch/generate_release_manifest.sh [--skip-build] [--allow-dirty] [--cargo-target-dir <path>] [--out-dir <path>]

Options:
  --skip-build     Skip `cargo build --release` and use existing binaries.
  --allow-dirty    Allow running on a dirty worktree (default: fail).
  --cargo-target-dir <path> Cargo target directory for release binaries.
                            Default: .context/cargo-target locally, target in CI.
  --out-dir <path> Output root for manifests. Default: artifacts/release-manifests
  -h, --help       Show this help text.
EOF
}

SKIP_BUILD=0
ALLOW_DIRTY=0
CARGO_TARGET_DIR_OVERRIDE=""
OUT_ROOT="artifacts/release-manifests"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build)
      SKIP_BUILD=1
      shift
      ;;
    --allow-dirty)
      ALLOW_DIRTY=1
      shift
      ;;
    --cargo-target-dir)
      if [[ $# -lt 2 ]]; then
        echo "--cargo-target-dir requires a path value" >&2
        exit 1
      fi
      CARGO_TARGET_DIR_OVERRIDE="$2"
      shift 2
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

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

resolve_cargo_target_dir() {
  if [[ -n "$CARGO_TARGET_DIR_OVERRIDE" ]]; then
    echo "$CARGO_TARGET_DIR_OVERRIDE"
    return
  fi
  if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    echo "$CARGO_TARGET_DIR"
    return
  fi
  if [[ "${CI:-}" == "true" ]]; then
    echo "$ROOT_DIR/target"
    return
  fi
  echo "$ROOT_DIR/.context/cargo-target"
}

CARGO_TARGET_DIR="$(resolve_cargo_target_dir)"
export CARGO_TARGET_DIR
mkdir -p "$CARGO_TARGET_DIR"

require_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Required command not found: $cmd" >&2
    exit 1
  fi
}

sha256_file() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  else
    echo "Missing SHA256 tool: install sha256sum or shasum" >&2
    exit 1
  fi
}

file_size_bytes() {
  local file="$1"
  if stat -f '%z' "$file" >/dev/null 2>&1; then
    stat -f '%z' "$file"
  else
    stat -c '%s' "$file"
  fi
}

require_cmd git
require_cmd cargo
require_cmd jq
require_cmd awk
require_cmd stat

if [[ "$ALLOW_DIRTY" -eq 0 ]] && [[ -n "$(git status --porcelain)" ]]; then
  echo "Refusing to generate release manifest on a dirty worktree." >&2
  echo "Commit or stash local changes first, or pass --allow-dirty for local iteration." >&2
  exit 1
fi

timestamp="$(date -u +"%Y%m%dT%H%M%SZ")"
iso_timestamp="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
commit_sha="$(git rev-parse HEAD)"
short_sha="$(git rev-parse --short HEAD)"
branch_name="$(git rev-parse --abbrev-ref HEAD)"

manifest_dir="${OUT_ROOT}/${timestamp}-${short_sha}"
binaries_dir="${manifest_dir}/binaries"
metadata_json="${manifest_dir}/metadata.json"
summary_md="${manifest_dir}/SUMMARY.md"
sha256sums_file="${manifest_dir}/SHA256SUMS.txt"

mkdir -p "$binaries_dir"

if [[ "$SKIP_BUILD" -eq 0 ]]; then
  echo "Building release binaries..."
  cargo build --release -p bitinfinity-btcrpc -p bitinfinity-tools -p bitinfinity-neard
else
  echo "Skipping release build (--skip-build)."
fi

declare -a binaries=(
  "bitinfinity-btcrpc"
  "bitinfinity-tools"
  "bitinfinity-neard"
)

binaries_json='[]'
: > "$sha256sums_file"

for bin in "${binaries[@]}"; do
  src_path="${CARGO_TARGET_DIR}/release/${bin}"
  dst_path="${binaries_dir}/${bin}"

  if [[ ! -f "$src_path" ]]; then
    echo "Missing release binary: $src_path" >&2
    echo "Run without --skip-build or build the binary first." >&2
    exit 1
  fi

  cp "$src_path" "$dst_path"
  checksum="$(sha256_file "$dst_path")"
  size_bytes="$(file_size_bytes "$dst_path")"
  version_line="$("$src_path" --version 2>/dev/null | head -n 1 || true)"
  if [[ -z "$version_line" ]]; then
    version_line="unknown"
  fi

  printf '%s  %s\n' "$checksum" "$bin" >> "$sha256sums_file"
  binaries_json="$(
    jq \
      --arg name "$bin" \
      --arg path "binaries/${bin}" \
      --arg sha256 "$checksum" \
      --arg version "$version_line" \
      --argjson size_bytes "$size_bytes" \
      '. + [{name: $name, path: $path, sha256: $sha256, size_bytes: $size_bytes, version: $version}]' \
      <<< "$binaries_json"
  )"
done

workspace_version="$(awk -F'"' '
  /^\[workspace.package\]/ { in_workspace = 1; next }
  /^\[/ { if (in_workspace) exit }
  in_workspace && /^version[[:space:]]*=/ { print $2; exit }
' Cargo.toml)"
if [[ -z "$workspace_version" ]]; then
  workspace_version="unknown"
fi

rustc_version="$(rustc --version 2>/dev/null || echo "unknown")"
cargo_version="$(cargo --version 2>/dev/null || echo "unknown")"
active_toolchain="$(rustup show active-toolchain 2>/dev/null | awk '{print $1}' || true)"
if [[ -z "$active_toolchain" ]]; then
  active_toolchain="unknown"
fi

jq -n \
  --arg generated_at "$iso_timestamp" \
  --arg manifest_dir "$manifest_dir" \
  --arg commit_sha "$commit_sha" \
  --arg short_sha "$short_sha" \
  --arg branch "$branch_name" \
  --arg workspace_version "$workspace_version" \
  --arg cargo_target_dir "$CARGO_TARGET_DIR" \
  --arg rustc_version "$rustc_version" \
  --arg cargo_version "$cargo_version" \
  --arg active_toolchain "$active_toolchain" \
  --argjson skip_build "$SKIP_BUILD" \
  --argjson allow_dirty "$ALLOW_DIRTY" \
  --argjson binaries "$binaries_json" \
  '{
    generated_at: $generated_at,
    manifest_dir: $manifest_dir,
    git: {
      commit_sha: $commit_sha,
      short_sha: $short_sha,
      branch: $branch
    },
    build: {
      workspace_version: $workspace_version,
      cargo_target_dir: $cargo_target_dir,
      skip_build: $skip_build,
      allow_dirty: $allow_dirty
    },
    toolchain: {
      rustc_version: $rustc_version,
      cargo_version: $cargo_version,
      active_toolchain: $active_toolchain
    },
    binaries: $binaries
  }' > "$metadata_json"

cat > "$summary_md" <<EOF
# Release Artifact Manifest

- generated_at: ${iso_timestamp}
- manifest_dir: ${manifest_dir}
- commit: ${commit_sha}
- branch: ${branch_name}
- workspace_version: ${workspace_version}
- cargo_target_dir: ${CARGO_TARGET_DIR}
- rustc_version: ${rustc_version}
- cargo_version: ${cargo_version}
- active_toolchain: ${active_toolchain}
- skip_build: ${SKIP_BUILD}
- allow_dirty: ${ALLOW_DIRTY}

## Files

- metadata.json
- SHA256SUMS.txt
- binaries/bitinfinity-btcrpc
- binaries/bitinfinity-tools
- binaries/bitinfinity-neard
EOF

echo
echo "Release artifact manifest complete: ${manifest_dir}"
echo "Summary: ${summary_md}"
