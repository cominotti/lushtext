#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"
cd "$repo_root"

app_id="dev.cominotti.lushtext"
local_manifest="build-aux/$app_id.Flatpak.json"
manifest="${1:-build-aux/flathub/$app_id.json}"

fail() {
    echo "error: $*" >&2
    exit 1
}

jq_value() {
    local file="$1"
    local expr="$2"

    jq -er "$expr" "$file"
}

json_equal() {
    local expr="$1"
    local left right

    left="$(jq -S "$expr" "$local_manifest")"
    right="$(jq -S "$expr" "$manifest")"
    [[ "$left" == "$right" ]]
}

command -v jq >/dev/null 2>&1 || fail "jq is required"
[[ -f "$local_manifest" ]] || fail "local manifest missing: $local_manifest"
[[ -f "$manifest" ]] || fail "Flathub manifest missing: $manifest"

for expr in \
    '.id' \
    '.command' \
    '.runtime' \
    '."runtime-version"' \
    '.sdk' \
    '."sdk-extensions"' \
    '."finish-args"' \
    '.cleanup' \
    '.modules[] | select(.name == "lushtext") | .buildsystem' \
    '.modules[] | select(.name == "lushtext") | ."config-opts"'; do
    json_equal "$expr" || fail "manifest invariant differs for jq expression: $expr"
done

if jq -e '[.. | objects | select(.type? == "dir")] | length == 0' "$manifest" >/dev/null; then
    :
else
    fail "Flathub manifest must not contain local type=dir sources"
fi

jq -e '
  .modules[]
  | select(.name == "lushtext")
  | .sources
  | any(type == "object" and .type == "git" and (.url | test("^https://github.com/cominotti/lushtext\\.git$")) and (.tag | test("^v[0-9]+\\.[0-9]+\\.[0-9]+")) and (.commit | test("^[0-9a-fA-F]{7,40}$")))
' "$manifest" >/dev/null || fail "Flathub manifest does not reference a public Git tag and commit"

jq -e '
  .modules[]
  | select(.name == "lushtext")
  | .sources
  | any(. == "cargo-sources.json")
' "$manifest" >/dev/null || fail "Flathub manifest does not include cargo-sources.json"

[[ "$(jq_value "$manifest" '."build-options".env.CARGO_NET_OFFLINE')" == "true" ]] ||
    fail "Flathub manifest must set CARGO_NET_OFFLINE=true"

echo "Flathub manifest is valid: $manifest"
