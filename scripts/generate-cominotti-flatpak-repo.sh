#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Generate Cominotti-owned Flatpak publication artifacts for LushText.

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

Required environment for public artifacts:
  COMINOTTI_FLATPAK_PUBLIC_KEY=/path/to/public-key.gpg

Required environment unless COMINOTTI_FLATPAK_SKIP_BUILD=1:
  COMINOTTI_FLATPAK_GPG_KEY=<gpg-key-id>

Example:
  COMINOTTI_FLATPAK_PUBLIC_KEY=public.gpg \\
  COMINOTTI_FLATPAK_GPG_KEY=ABCDEF1234567890 \\
    $0 v0.2.0 \$(git rev-parse HEAD) build-aux/cominotti-flatpak
EOF
    exit 2
}

fail() {
    echo "error: $*" >&2
    exit 1
}

ensure_trailing_slash() {
    local value="$1"

    if [[ "$value" == */ ]]; then
        printf '%s\n' "$value"
    else
        printf '%s/\n' "$value"
    fi
}

version="${1:-}"
commit="${2:-}"
out_dir="${3:-build-aux/cominotti-flatpak}"

remote_name="${COMINOTTI_FLATPAK_REMOTE_NAME:-cominotti}"
collection_id="${COMINOTTI_FLATPAK_COLLECTION_ID:-dev.cominotti.Apps}"
repo_url="$(ensure_trailing_slash "${COMINOTTI_FLATPAK_REPO_URL:-https://flatpak.cominotti.dev/repo/}")"
base_url="${COMINOTTI_FLATPAK_BASE_URL:-https://flatpak.cominotti.dev}"
base_url="${base_url%/}"
homepage="${COMINOTTI_FLATPAK_HOMEPAGE:-https://cominotti.dev/}"
runtime_repo="${COMINOTTI_FLATPAK_RUNTIME_REPO:-https://dl.flathub.org/repo/flathub.flatpakrepo}"
branch="${COMINOTTI_FLATPAK_BRANCH:-stable}"
source_url="${LUSHTEXT_FLATPAK_SOURCE_URL:-https://github.com/cominotti/lushtext.git}"
repo_title="${COMINOTTI_FLATPAK_TITLE:-Cominotti Apps}"
repo_comment="${COMINOTTI_FLATPAK_COMMENT:-Official Cominotti applications}"
repo_description="${COMINOTTI_FLATPAK_DESCRIPTION:-Official Flatpak repository for applications published by Cominotti.}"
app_title="${COMINOTTI_FLATPAK_LUSHTEXT_TITLE:-LushText}"
public_key_file="${COMINOTTI_FLATPAK_PUBLIC_KEY:-}"
signing_key="${COMINOTTI_FLATPAK_GPG_KEY:-}"
gpg_homedir="${COMINOTTI_FLATPAK_GPG_HOMEDIR:-}"
skip_build="${COMINOTTI_FLATPAK_SKIP_BUILD:-0}"
prune_depth="${COMINOTTI_FLATPAK_PRUNE_DEPTH:-20}"

[[ -n "$version" && -n "$commit" ]] || usage
[[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9]+(\.[A-Za-z0-9]+)*)?$ ]] ||
    fail "invalid version tag '$version'"
[[ "$commit" =~ ^[0-9a-fA-F]{7,40}$ ]] ||
    fail "commit must be a 7-40 character hex SHA"
command -v jq >/dev/null 2>&1 || fail "jq is required"
command -v base64 >/dev/null 2>&1 || fail "base64 is required"
[[ -f "$local_manifest" ]] || fail "local manifest not found: $local_manifest"
[[ -f "$cargo_sources" ]] || fail "cargo sources not found: $cargo_sources"
[[ -n "$public_key_file" ]] || fail "COMINOTTI_FLATPAK_PUBLIC_KEY is required"
[[ -s "$public_key_file" ]] || fail "public GPG key file is missing or empty: $public_key_file"

if [[ "$skip_build" != "1" ]]; then
    command -v flatpak-builder >/dev/null 2>&1 || fail "flatpak-builder is required"
    command -v flatpak >/dev/null 2>&1 || fail "flatpak is required"
    [[ -n "$signing_key" ]] || fail "COMINOTTI_FLATPAK_GPG_KEY is required unless COMINOTTI_FLATPAK_SKIP_BUILD=1"
fi

public_dir="$out_dir/flatpak"
repo_dir="$public_dir/repo"
build_meta_dir="$out_dir/build"
build_dir="$out_dir/build-dir"
manifest="$build_meta_dir/$app_id.json"
flatpakrepo="$public_dir/$remote_name.flatpakrepo"
flatpakref="$public_dir/lushtext.flatpakref"
public_key_b64="$(base64 < "$public_key_file" | tr -d '\n')"

mkdir -p "$repo_dir" "$build_meta_dir"

jq \
    --arg branch "$branch" \
    --arg version "$version" \
    --arg commit "$commit" \
    --arg source_url "$source_url" \
    '
    .branch = $branch
    | ."build-options".env.CARGO_NET_OFFLINE = "true"
    | .modules = (.modules | map(
        if .name == "lushtext" then
          .sources = [
            {
              "type": "git",
              "url": $source_url,
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
    "$local_manifest" > "$manifest"
cp "$cargo_sources" "$build_meta_dir/cargo-sources.json"

cat > "$flatpakrepo" <<EOF
[Flatpak Repo]
Title=$repo_title
Url=$repo_url
Homepage=$homepage
Comment=$repo_comment
Description=$repo_description
GPGKey=$public_key_b64
DeployCollectionID=$collection_id
EOF

cat > "$flatpakref" <<EOF
[Flatpak Ref]
Title=$app_title
Name=$app_id
Branch=$branch
Url=$repo_url
SuggestRemoteName=$remote_name
RuntimeRepo=$runtime_repo
IsRuntime=false
GPGKey=$public_key_b64
EOF

if [[ "$skip_build" == "1" ]]; then
    echo "Generated Cominotti Flatpak metadata only: $public_dir"
    echo "Generated release manifest: $manifest"
    exit 0
fi

builder_args=(
    --disable-rofiles-fuse
    --force-clean
    --user
    --install-deps-from="${FLATPAK_REMOTE:-flathub}"
    --repo="$repo_dir"
    --collection-id="$collection_id"
    --gpg-sign="$signing_key"
)
if [[ -n "$gpg_homedir" ]]; then
    builder_args+=(--gpg-homedir="$gpg_homedir")
fi

flatpak-builder "${builder_args[@]}" "$build_dir" "$manifest"

update_args=(
    --title="$repo_title"
    --comment="$repo_comment"
    --description="$repo_description"
    --homepage="$homepage"
    --default-branch="$branch"
    --gpg-import="$public_key_file"
    --collection-id="$collection_id"
    --deploy-collection-id
    --gpg-sign="$signing_key"
    --generate-static-deltas
    --prune
    --prune-depth="$prune_depth"
)
if [[ -n "$gpg_homedir" ]]; then
    update_args+=(--gpg-homedir="$gpg_homedir")
fi

flatpak build-update-repo "${update_args[@]}" "$repo_dir"

echo "Generated Cominotti Flatpak repository: $repo_dir"
echo "Generated repository descriptor: $flatpakrepo"
echo "Generated LushText installer reference: $flatpakref"
