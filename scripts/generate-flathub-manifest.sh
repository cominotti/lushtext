#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Generate a Flathub-facing manifest update from the local checkout manifest.

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"
cd "$repo_root"

app_id="dev.cominotti.lushtext"
local_manifest="build-aux/$app_id.Flatpak.json"
cargo_sources="build-aux/cargo-sources.json"

usage() {
    cat >&2 <<EOF
Usage: $0 <version-tag> <commit-sha> [output-dir]

Example:
  $0 v0.2.0 \$(git rev-parse HEAD) build-aux/flathub
EOF
    exit 2
}

version="${1:-}"
commit="${2:-}"
out_dir="${3:-build-aux/flathub}"
repo_url="${LUSHTEXT_FLATHUB_SOURCE_URL:-https://github.com/cominotti/lushtext.git}"

[[ -n "$version" && -n "$commit" ]] || usage
[[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9]+(\.[A-Za-z0-9]+)*)?$ ]] ||
    { echo "error: invalid version tag '$version'" >&2; exit 1; }
[[ "$commit" =~ ^[0-9a-fA-F]{7,40}$ ]] ||
    { echo "error: commit must be a 7-40 character hex SHA" >&2; exit 1; }
command -v jq >/dev/null 2>&1 ||
    { echo "error: jq is required" >&2; exit 1; }
[[ -f "$local_manifest" ]] ||
    { echo "error: local manifest not found: $local_manifest" >&2; exit 1; }
[[ -f "$cargo_sources" ]] ||
    { echo "error: cargo sources not found: $cargo_sources" >&2; exit 1; }

mkdir -p "$out_dir"

jq \
    --arg version "$version" \
    --arg commit "$commit" \
    --arg repo_url "$repo_url" \
    '
    ."build-options".env.CARGO_NET_OFFLINE = "true"
    | .modules = (.modules | map(
        if .name == "lushtext" then
          .sources = [
            {
              "type": "git",
              "url": $repo_url,
              "tag": $version,
              "commit": $commit
            },
            "cargo-sources.json"
          ]
        else
          .
        end
      ))
    ' \
    "$local_manifest" > "$out_dir/$app_id.json"

cp "$cargo_sources" "$out_dir/cargo-sources.json"

echo "Generated Flathub manifest: $out_dir/$app_id.json"
echo "Copied Cargo sources: $out_dir/cargo-sources.json"
