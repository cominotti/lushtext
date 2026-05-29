#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"
tmpdir="$(mktemp -d)"

cleanup() {
    rm -rf "$tmpdir"
}
trap cleanup EXIT

out_dir="$tmpdir/flathub"
manifest="$out_dir/dev.cominotti.lushtext.json"

"$repo_root/scripts/generate-flathub-manifest.sh" v1.2.3 deadbeef "$out_dir" >/dev/null
"$repo_root/scripts/verify-flathub-manifest.sh" "$manifest" >/dev/null

jq -e '
  .modules[]
  | select(.name == "lushtext")
  | .sources
  | any(type == "object" and .type == "git" and .tag == "v1.2.3" and .commit == "deadbeef")
' "$manifest" >/dev/null

jq -e '[.. | objects | select(.type? == "dir")] | length == 0' "$manifest" >/dev/null
cmp -s "$repo_root/build-aux/cargo-sources.json" "$out_dir/cargo-sources.json"

bad_manifest="$tmpdir/bad.json"
jq '.modules[0].sources[0] = {"type":"dir","path":".."}' "$manifest" > "$bad_manifest"
if "$repo_root/scripts/verify-flathub-manifest.sh" "$bad_manifest" >/dev/null 2>&1; then
    echo "expected verifier to reject a Flathub manifest with type=dir" >&2
    exit 1
fi

echo "Flathub manifest tests passed"
