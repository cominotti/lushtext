#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"
cd "$repo_root"

app_id="dev.cominotti.lushtext"
local_manifest="build-aux/$app_id.Flatpak.json"
target_dir="${1:-build-aux/cominotti-flatpak}"
public_dir="$target_dir/flatpak"
build_meta_dir="$target_dir/build"
repo_dir="$public_dir/repo"
manifest="$build_meta_dir/$app_id.json"
cargo_sources="$build_meta_dir/cargo-sources.json"

expected_remote="${COMINOTTI_FLATPAK_REMOTE_NAME:-cominotti}"
expected_collection="${COMINOTTI_FLATPAK_COLLECTION_ID:-dev.cominotti.Apps}"
expected_repo_url="${COMINOTTI_FLATPAK_REPO_URL:-https://flatpak.cominotti.dev/repo/}"
expected_base_url="${COMINOTTI_FLATPAK_BASE_URL:-https://flatpak.cominotti.dev}"
expected_base_url="${expected_base_url%/}"
expected_runtime_repo="${COMINOTTI_FLATPAK_RUNTIME_REPO:-https://dl.flathub.org/repo/flathub.flatpakrepo}"
expected_branch="${COMINOTTI_FLATPAK_BRANCH:-stable}"
expected_source_url="${LUSHTEXT_FLATPAK_SOURCE_URL:-https://github.com/cominotti/lushtext.git}"
expected_flatpakrepo="$public_dir/$expected_remote.flatpakrepo"
expected_flatpakref="$public_dir/lushtext.flatpakref"

fail() {
    echo "error: $*" >&2
    exit 1
}

ini_value() {
    local file="$1"
    local key="$2"

    awk -F= -v key="$key" '$1 == key { sub(/^[^=]*=/, ""); print; exit }' "$file"
}

json_equal() {
    local expr="$1"
    local left right

    left="$(jq -S "$expr" "$local_manifest")"
    right="$(jq -S "$expr" "$manifest")"
    [[ "$left" == "$right" ]]
}

expect_value() {
    local label="$1"
    local expected="$2"
    local actual="$3"

    [[ "$actual" == "$expected" ]] || fail "$label is '$actual', expected '$expected'"
}

ensure_no_no_verify() {
    local file="$1"

    if grep -Fq -- '--no-gpg-verify' "$file"; then
        fail "$file contains --no-gpg-verify; public Cominotti install metadata must keep GPG verification enabled"
    fi
}

ensure_trailing_slash() {
    local value="$1"

    if [[ "$value" == */ ]]; then
        printf '%s\n' "$value"
    else
        printf '%s/\n' "$value"
    fi
}

repo_url="$(ensure_trailing_slash "$expected_repo_url")"

command -v jq >/dev/null 2>&1 || fail "jq is required"
[[ -f "$local_manifest" ]] || fail "local manifest missing: $local_manifest"
[[ -f "$manifest" ]] || fail "Cominotti release manifest missing: $manifest"
[[ -f "$expected_flatpakrepo" ]] || fail "repository descriptor missing: $expected_flatpakrepo"
[[ -f "$expected_flatpakref" ]] || fail "LushText flatpakref missing: $expected_flatpakref"
[[ -f "$cargo_sources" ]] || fail "vendored Cargo sources missing beside manifest: $cargo_sources"

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

expect_value "manifest branch" "$expected_branch" "$(jq -er '.branch' "$manifest")"
[[ "$(jq -er '."build-options".env.CARGO_NET_OFFLINE' "$manifest")" == "true" ]] ||
    fail "Cominotti manifest must set CARGO_NET_OFFLINE=true"

jq -e '[.. | objects | select(.type? == "dir")] | length == 0' "$manifest" >/dev/null ||
    fail "Cominotti release manifest must not contain local type=dir sources"
jq -e --arg source_url "$expected_source_url" '
  .modules[]
  | select(.name == "lushtext")
  | .sources
  | any(type == "object" and .type == "git" and .url == $source_url and (.tag | test("^v[0-9]+\\.[0-9]+\\.[0-9]+")) and (.commit | test("^[0-9a-fA-F]{7,40}$")))
' "$manifest" >/dev/null || fail "Cominotti manifest does not reference the expected public Git tag and commit"
jq -e '
  .modules[]
  | select(.name == "lushtext")
  | .sources
  | any(. == "cargo-sources.json")
' "$manifest" >/dev/null || fail "Cominotti manifest does not include cargo-sources.json"

ensure_no_no_verify "$expected_flatpakrepo"
ensure_no_no_verify "$expected_flatpakref"

expect_value ".flatpakrepo Url" "$repo_url" "$(ini_value "$expected_flatpakrepo" Url)"
expect_value ".flatpakrepo DeployCollectionID" "$expected_collection" "$(ini_value "$expected_flatpakrepo" DeployCollectionID)"
[[ -n "$(ini_value "$expected_flatpakrepo" Title)" ]] || fail ".flatpakrepo Title is missing"
[[ -n "$(ini_value "$expected_flatpakrepo" Description)" ]] || fail ".flatpakrepo Description is missing"

repo_gpg_key="$(ini_value "$expected_flatpakrepo" GPGKey)"
ref_gpg_key="$(ini_value "$expected_flatpakref" GPGKey)"
[[ -n "$repo_gpg_key" ]] || fail ".flatpakrepo GPGKey is missing"
[[ -n "$ref_gpg_key" ]] || fail ".flatpakref GPGKey is missing"
expect_value ".flatpakref GPGKey" "$repo_gpg_key" "$ref_gpg_key"

if [[ -n "${COMINOTTI_FLATPAK_PUBLIC_KEY:-}" ]]; then
    command -v base64 >/dev/null 2>&1 || fail "base64 is required"
    [[ -s "$COMINOTTI_FLATPAK_PUBLIC_KEY" ]] || fail "public key file is missing or empty: $COMINOTTI_FLATPAK_PUBLIC_KEY"
    expected_gpg_key="$(base64 < "$COMINOTTI_FLATPAK_PUBLIC_KEY" | tr -d '\n')"
    expect_value "generated GPGKey" "$expected_gpg_key" "$repo_gpg_key"
fi

expect_value ".flatpakref Name" "$app_id" "$(ini_value "$expected_flatpakref" Name)"
expect_value ".flatpakref Branch" "$expected_branch" "$(ini_value "$expected_flatpakref" Branch)"
expect_value ".flatpakref Url" "$repo_url" "$(ini_value "$expected_flatpakref" Url)"
expect_value ".flatpakref SuggestRemoteName" "$expected_remote" "$(ini_value "$expected_flatpakref" SuggestRemoteName)"
expect_value ".flatpakref RuntimeRepo" "$expected_runtime_repo" "$(ini_value "$expected_flatpakref" RuntimeRepo)"
expect_value ".flatpakref IsRuntime" "false" "$(ini_value "$expected_flatpakref" IsRuntime)"

if [[ -f "$repo_dir/summary" ]]; then
    if command -v flatpak >/dev/null 2>&1; then
        repo_abs="$(cd "$repo_dir" && pwd)"
        if ! flatpak remote-ls "file://$repo_abs" --app --columns=application 2>/dev/null | grep -qx "$app_id"; then
            fail "flatpak remote-ls did not report $app_id from $repo_dir"
        fi
    elif command -v ostree >/dev/null 2>&1; then
        ostree refs --repo="$repo_dir" --list | grep -Eq "^app/$app_id/[^/]+/$expected_branch$" ||
            fail "OSTree refs do not include app/$app_id/*/$expected_branch"
    else
        fail "flatpak or ostree is required to verify app availability from $repo_dir"
    fi
elif [[ "${COMINOTTI_FLATPAK_VERIFY_INSTALL:-0}" == "1" ]]; then
    fail "repository summary is missing from $repo_dir"
else
    echo "warning: repository summary is missing; metadata-only verification completed" >&2
fi

echo "Cominotti Flatpak artifacts are valid: $target_dir"
