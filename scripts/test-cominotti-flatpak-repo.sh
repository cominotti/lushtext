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

assert_success() {
    local label="$1"
    shift

    if ! "$@" > "$tmpdir/assert-success.log" 2>&1; then
        printf 'FAIL: %s\n' "$label" >&2
        cat "$tmpdir/assert-success.log" >&2
        exit 1
    fi
}

assert_failure() {
    local label="$1"
    shift

    if "$@" > "$tmpdir/assert-failure.log" 2>&1; then
        printf 'FAIL: %s succeeded unexpectedly\n' "$label" >&2
        exit 1
    fi
}

fake_public_key="$tmpdir/cominotti-public.gpg"
printf 'metadata-only test public key\n' > "$fake_public_key"

out_dir="$tmpdir/cominotti-flatpak"
assert_success "generate metadata-only Cominotti artifacts" env \
    COMINOTTI_FLATPAK_SKIP_BUILD=1 \
    COMINOTTI_FLATPAK_PUBLIC_KEY="$fake_public_key" \
    "$repo_root/scripts/generate-cominotti-flatpak-repo.sh" \
    v1.2.3 deadbeef "$out_dir"

assert_success "verify generated Cominotti artifacts" env \
    COMINOTTI_FLATPAK_PUBLIC_KEY="$fake_public_key" \
    "$repo_root/scripts/verify-cominotti-flatpak-repo.sh" "$out_dir"

assert_success "verify Cloudflare Pages limits for generated metadata" \
    "$repo_root/scripts/verify-cominotti-pages-limits.sh" "$out_dir/flatpak"

jq -e '
  .branch == "stable"
  and ."build-options".env.CARGO_NET_OFFLINE == "true"
  and (.modules[] | select(.name == "lushtext") | .sources | any(type == "object" and .type == "git" and .tag == "v1.2.3" and .commit == "deadbeef"))
' "$out_dir/build/dev.cominotti.lushtext.json" >/dev/null

grep -q '^Url=https://flatpak.cominotti.dev/repo/$' "$out_dir/flatpak/cominotti.flatpakrepo"
grep -q '^Url=https://flatpak.cominotti.dev/repo/$' "$out_dir/flatpak/lushtext.flatpakref"
grep -q '^DeployCollectionID=dev.cominotti.Apps$' "$out_dir/flatpak/cominotti.flatpakrepo"
grep -q '^SuggestRemoteName=cominotti$' "$out_dir/flatpak/lushtext.flatpakref"
grep -q '^RuntimeRepo=https://dl.flathub.org/repo/flathub.flatpakrepo$' "$out_dir/flatpak/lushtext.flatpakref"

assert_failure "generator rejects missing public key" env \
    COMINOTTI_FLATPAK_SKIP_BUILD=1 \
    "$repo_root/scripts/generate-cominotti-flatpak-repo.sh" \
    v1.2.3 deadbeef "$tmpdir/no-key"

bad_collection="$tmpdir/bad-collection"
cp -R "$out_dir" "$bad_collection"
sed -i 's/^DeployCollectionID=.*/DeployCollectionID=dev.example.Bad/' "$bad_collection/flatpak/cominotti.flatpakrepo"
assert_failure "verifier rejects wrong collection ID" \
    "$repo_root/scripts/verify-cominotti-flatpak-repo.sh" "$bad_collection"

bad_app="$tmpdir/bad-app"
cp -R "$out_dir" "$bad_app"
sed -i 's/^Name=.*/Name=dev.example.Bad/' "$bad_app/flatpak/lushtext.flatpakref"
assert_failure "verifier rejects wrong app ID" \
    "$repo_root/scripts/verify-cominotti-flatpak-repo.sh" "$bad_app"

bad_runtime="$tmpdir/bad-runtime"
cp -R "$out_dir" "$bad_runtime"
sed -i 's#^RuntimeRepo=.*#RuntimeRepo=https://example.com/runtime.flatpakrepo#' "$bad_runtime/flatpak/lushtext.flatpakref"
assert_failure "verifier rejects wrong runtime repo" \
    "$repo_root/scripts/verify-cominotti-flatpak-repo.sh" "$bad_runtime"

bad_no_verify="$tmpdir/bad-no-verify"
cp -R "$out_dir" "$bad_no_verify"
printf '\n# flatpak remote-add --no-gpg-verify cominotti\n' >> "$bad_no_verify/flatpak/lushtext.flatpakref"
assert_failure "verifier rejects no-gpg-verify instructions" \
    "$repo_root/scripts/verify-cominotti-flatpak-repo.sh" "$bad_no_verify"

pages_fixture="$tmpdir/pages-fixture"
mkdir -p "$pages_fixture"
printf 'small\n' > "$pages_fixture/small.txt"
assert_success "pages limit check accepts small assets" env \
    COMINOTTI_FLATPAK_PAGES_MAX_FILE_BYTES=16 \
    COMINOTTI_FLATPAK_PAGES_MAX_FILES=2 \
    "$repo_root/scripts/verify-cominotti-pages-limits.sh" "$pages_fixture"

printf 'this file is intentionally too large\n' > "$pages_fixture/too-large.txt"
assert_failure "pages limit check rejects oversized assets" env \
    COMINOTTI_FLATPAK_PAGES_MAX_FILE_BYTES=8 \
    COMINOTTI_FLATPAK_PAGES_MAX_FILES=3 \
    "$repo_root/scripts/verify-cominotti-pages-limits.sh" "$pages_fixture"

assert_failure "pages limit check rejects excessive file count" env \
    COMINOTTI_FLATPAK_PAGES_MAX_FILE_BYTES=64 \
    COMINOTTI_FLATPAK_PAGES_MAX_FILES=1 \
    "$repo_root/scripts/verify-cominotti-pages-limits.sh" "$pages_fixture"

echo "Cominotti Flatpak repository tests passed"
