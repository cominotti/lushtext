#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Fixture test for scripts/flatpak-install-origin.sh against REAL flatpak, in a
# private FLATPAK_USER_DIR under this run's own TMPDIR (never the real user
# installation). It pins the flatpak behaviour `make flatpak-install` relies on
# (origin naming, reuse by textual URL, the silent skip a disabled origin with a
# matching URL causes, pruning of the previous origin) and the guard built on
# it. Every install goes through the production `install` subcommand, with
# `flatpak build-export` (or `false`, for a failed build) in place of
# flatpak-builder.
#
# The fixture is the dependency-free runtime ref org.test.lushtext: its last id
# component is `lushtext`, so the remote names are the real ones
# (lushtext-origin, lushtext1-origin). Run it as `make
# test-flatpak-install-origin`, which wraps it in scripts/with-temp-scope.sh.

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"
origin_script="$repo_root/scripts/flatpak-install-origin.sh"

for tool in flatpak ostree flock timeout realpath; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "SKIP: $tool is not installed; the install-origin fixture test needs real flatpak, ostree and flock."
        exit 0
    fi
done

base_tmp="$(realpath -m -- "${TMPDIR:-/tmp}")"
scratch="$(mktemp -d "$base_tmp/lt-flatpak-origin-XXXXXX")"
scratch="$(realpath -- "$scratch")"
lockfd=""

cleanup() {
    if [[ -n "$lockfd" ]]; then
        exec {lockfd}>&-
    fi
    chmod -R u+w -- "$scratch" 2>/dev/null || true
    rm -rf -- "$scratch"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

export HOME="$scratch/home"
export FLATPAK_USER_DIR="$scratch/flatpak-user"
export XDG_DATA_HOME="$scratch/data"
export XDG_CACHE_HOME="$scratch/cache"
export XDG_CONFIG_HOME="$scratch/config"
export LUSHTEXT_FLATPAK_APP_ID=org.test.lushtext
export LUSHTEXT_FLATPAK_INSTALL_REPO="$XDG_DATA_HOME/lushtext-flatpak-install/repo"

# Isolation: every location flatpak or the script can write lies under this
# run's TMPDIR. Checked before the first flatpak call.
case "$scratch" in
    "$base_tmp"/*) ;;
    *) echo "refusing: $scratch is not under TMPDIR $base_tmp" >&2; exit 2 ;;
esac
for var in HOME FLATPAK_USER_DIR XDG_DATA_HOME XDG_CACHE_HOME XDG_CONFIG_HOME LUSHTEXT_FLATPAK_INSTALL_REPO; do
    case "${!var}" in
        "$scratch"/*) ;;
        *) echo "refusing: $var=${!var} is not under $scratch" >&2; exit 2 ;;
    esac
done
mkdir -p "$HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME" "$XDG_CONFIG_HOME"

arch="$(flatpak --default-arch)"
ref="org.test.lushtext/$arch/master"
typed_ref="runtime/$ref"
tree="$scratch/tree"
shared_repo="$LUSHTEXT_FLATPAK_INSTALL_REPO"
shared_uri="file://$shared_repo"
case_name="setup"

fail() {
    echo "FAIL [$case_name]: $*" >&2
    exit 1
}

pass() {
    echo "ok   [$case_name]"
}

# A runtime tree whose content differs per version, so every export is a new commit.
make_tree() {
    rm -rf -- "$tree"
    mkdir -p "$tree/usr" "$tree/files" "$tree/var"
    printf '[Runtime]\nname=org.test.lushtext\n' > "$tree/metadata"
    printf '%s\n' "$1" > "$tree/usr/version"
}

export_to() {
    flatpak build-export --runtime "$1" "$tree" master >/dev/null
}

bare_install() {
    flatpak install --user -y --noninteractive --reinstall "$1" "$ref" > "$scratch/bare-install.log" 2>&1
}

origin_install() {
    "$origin_script" install "$1" "$ref" -- "${@:2}"
}

remote_field() {
    flatpak remotes --user --show-disabled --columns=name,url,options |
        awk -F'\t' -v n="$1" -v f="$2" '$1 == n { print $f }'
}

origin_remotes() {
    flatpak remotes --user --show-disabled --columns=name |
        grep -E '^lushtext[0-9]*-origin$' || true
}

remote_ls_ok() {
    flatpak remote-ls --user --updates > /dev/null 2> "$scratch/remote-ls.err"
}

repo_head() {
    ostree --repo="$1" rev-parse "$typed_ref"
}

deployed_commit() {
    flatpak info --user -c "$ref"
}

# Run a command that is expected to fail; its combined output lands in $2.
expect_failure() {
    local log="$1"
    shift
    if "$@" > "$log" 2>&1; then
        cat "$log" >&2
        fail "expected failure: $*"
    fi
}

# The maintainer's state: lushtext-origin installed from a checkout's build
# cache that was then deleted.
make_tree one
export_to "$scratch/deleted-checkout-cache"
bare_install "$scratch/deleted-checkout-cache" || fail "fixture install failed: $(cat "$scratch/bare-install.log")"
rm -rf -- "$scratch/deleted-checkout-cache"

# --- 1. dangling enabled origin ---------------------------------------------
case_name="1 dangling enabled origin"
if remote_ls_ok; then
    fail "baseline: remote-ls --updates passed with a dangling origin"
fi
grep -q "Remote listing for lushtext-origin not available; server has no summary file" "$scratch/remote-ls.err" ||
    fail "baseline: unexpected remote-ls error: $(cat "$scratch/remote-ls.err")"
echo "ok   [$case_name] baseline: remote-ls --updates reports the missing summary"
[[ -x "$origin_script" ]] || fail "scripts/flatpak-install-origin.sh is missing (the install-origin guard)"
expect_failure "$scratch/check1.log" "$origin_script" check
grep -q "lushtext-origin" "$scratch/check1.log" || fail "check does not name the remote"
grep -q "file://$scratch/deleted-checkout-cache" "$scratch/check1.log" || fail "check does not name the URL"
grep -q "enabled" "$scratch/check1.log" || fail "check does not name the enabled state"
grep -q "server has no summary file" "$scratch/check1.log" || fail "check does not name the symptom"
grep -q "make flatpak-install" "$scratch/check1.log" || fail "check does not print the fix"
pass

# --- 2. the same origin disabled (the 2026-11-14 machine) -------------------
case_name="2 dangling disabled origin"
flatpak remote-modify --user --disable lushtext-origin
remote_ls_ok || fail "remote-ls --updates fails with the origin disabled: $(cat "$scratch/remote-ls.err")"
expect_failure "$scratch/check2.log" "$origin_script" check
grep -q "lushtext-origin" "$scratch/check2.log" || fail "check does not name the remote"
grep -q "disabled" "$scratch/check2.log" || fail "check does not name the disabled state"
grep -q "can no longer be updated" "$scratch/check2.log" || fail "check does not name the stranded build"
grep -q "make flatpak-install" "$scratch/check2.log" || fail "check does not print the fix"
pass

# --- path: canonical and spelling-independent -------------------------------
case_name="path"
ln -s "$XDG_DATA_HOME" "$scratch/data-link"
[[ "$(LUSHTEXT_FLATPAK_INSTALL_REPO='' "$origin_script" path)" == "$XDG_DATA_HOME/lushtext-flatpak-install/repo" ]] ||
    fail "default path is not \$XDG_DATA_HOME/lushtext-flatpak-install/repo"
[[ "$(LUSHTEXT_FLATPAK_INSTALL_REPO='' XDG_DATA_HOME="$scratch/data-link" "$origin_script" path)" == "$XDG_DATA_HOME/lushtext-flatpak-install/repo" ]] ||
    fail "a symlinked XDG_DATA_HOME spelling yields another path"
[[ "$("$origin_script" path)" == "$shared_repo" ]] || fail "LUSHTEXT_FLATPAK_INSTALL_REPO is not honoured"
pass

# --- 3. a failed build touches no remote ------------------------------------
case_name="3 failed build"
url_before="$(remote_field lushtext-origin 2)"
options_before="$(remote_field lushtext-origin 3)"
expect_failure "$scratch/install3.log" origin_install "$shared_repo" false
[[ "$(remote_field lushtext-origin 2)" == "$url_before" ]] || fail "the URL changed after a failed build"
[[ "$(remote_field lushtext-origin 3)" == "$options_before" ]] || fail "the options changed after a failed build"
[[ ",$options_before," == *,disabled,* ]] || fail "fixture: the origin was not disabled"
remote_ls_ok || fail "remote-ls --updates fails after a failed build: $(cat "$scratch/remote-ls.err")"
pass

# --- 4. repair ---------------------------------------------------------------
case_name="4 repair"
# An earlier build's commit in the shared repository, so the prune has work.
make_tree zero
mkdir -p "$(dirname -- "$shared_repo")"
export_to "$shared_repo"
make_tree two
origin_install "$shared_repo" flatpak build-export --runtime "$shared_repo" "$tree" master > "$scratch/install4.log" 2>&1 ||
    { cat "$scratch/install4.log" >&2; fail "install failed"; }
grep -q "repointed lushtext-origin from file://$scratch/deleted-checkout-cache" "$scratch/install4.log" ||
    fail "install did not report the repair"
[[ "$(origin_remotes)" == "lushtext-origin" ]] || fail "origin remotes are: $(origin_remotes | tr '\n' ' ')"
[[ "$(remote_field lushtext-origin 2)" == "$shared_uri" ]] || fail "URL is $(remote_field lushtext-origin 2)"
[[ ",$(remote_field lushtext-origin 3)," != *,disabled,* ]] || fail "the origin is still disabled"
[[ "$(flatpak info --user -o "$ref")" == "lushtext-origin" ]] || fail "installed from $(flatpak info --user -o "$ref")"
head="$(repo_head "$shared_repo")"
[[ "$(deployed_commit)" == "$head" ]] || fail "deployed $(deployed_commit), head $head"
[[ "$(flatpak remote-info --user -c lushtext-origin "$ref")" == "$head" ]] || fail "the remote does not serve the head"
commits="$(ostree --repo="$shared_repo" log "$typed_ref" | grep -c '^commit')"
[[ "$commits" == 1 ]] || fail "the repository holds $commits commits after the prune"
remote_ls_ok || fail "remote-ls --updates fails after the repair: $(cat "$scratch/remote-ls.err")"
"$origin_script" check > "$scratch/check4.log" 2>&1 || { cat "$scratch/check4.log" >&2; fail "check fails after the repair"; }
if grep -qi "warning" "$scratch/check4.log"; then
    fail "check warns about the shared repository: $(cat "$scratch/check4.log")"
fi
pass

# --- 5. case e pinned: a disabled matching origin silently skips the update --
case_name="5 silent skip"
flatpak remote-modify --user --disable lushtext-origin
make_tree three
export_to "$shared_repo"
new_head="$(repo_head "$shared_repo")"
bare_install "$shared_repo" || fail "bare install exited non-zero: $(cat "$scratch/bare-install.log")"
stale="$(deployed_commit)"
[[ "$stale" != "$new_head" ]] || fail "flatpak no longer skips the update for a disabled origin (case e changed)"
expect_failure "$scratch/check5.log" "$origin_script" check --installed "$shared_repo" "$ref"
grep -q "$stale" "$scratch/check5.log" || fail "check --installed does not name the deployed commit"
grep -q "$new_head" "$scratch/check5.log" || fail "check --installed does not name the repository head"
make_tree four
origin_install "$shared_repo" flatpak build-export --runtime "$shared_repo" "$tree" master > "$scratch/install5.log" 2>&1 ||
    { cat "$scratch/install5.log" >&2; fail "install after the skip failed"; }
[[ "$(deployed_commit)" == "$(repo_head "$shared_repo")" ]] || fail "install did not deploy the new head"
[[ ",$(remote_field lushtext-origin 3)," != *,disabled,* ]] || fail "install left the origin disabled"
pass

# --- 5b. the other half: deploying from another remote fails the post-check --
case_name="5b other origin"
ln -s "$(dirname -- "$shared_repo")" "$scratch/install-link"
bare_install "$scratch/install-link/repo" || fail "bare install through a symlink failed: $(cat "$scratch/bare-install.log")"
[[ "$(flatpak info --user -o "$ref")" == "lushtext1-origin" ]] ||
    fail "flatpak no longer creates lushtext1-origin for another spelling (case h changed): $(flatpak info --user -o "$ref")"
expect_failure "$scratch/check5b.log" "$origin_script" check --installed "$shared_repo" "$ref"
grep -q "lushtext1-origin" "$scratch/check5b.log" || fail "check --installed does not name the actual origin"
grep -q "expected lushtext-origin" "$scratch/check5b.log" || fail "check --installed does not name the expected origin"
make_tree five
origin_install "$shared_repo" flatpak build-export --runtime "$shared_repo" "$tree" master > "$scratch/install5b.log" 2>&1 ||
    { cat "$scratch/install5b.log" >&2; fail "install after the other origin failed"; }
[[ "$(flatpak info --user -o "$ref")" == "lushtext-origin" ]] || fail "install did not move back to lushtext-origin"
[[ "$(origin_remotes)" == "lushtext-origin" ]] || fail "leftover origins: $(origin_remotes | tr '\n' ' ')"
pass

# --- 5c. steady state: every later install, with the origin already valid ---
case_name="5c steady state"
make_tree seven
origin_install "$shared_repo" flatpak build-export --runtime "$shared_repo" "$tree" master > "$scratch/install5c.log" 2>&1 ||
    { cat "$scratch/install5c.log" >&2; fail "steady-state install failed"; }
grep -q "lushtext-origin already points at $shared_uri" "$scratch/install5c.log" || fail "install modified a valid origin"
head="$(repo_head "$shared_repo")"
[[ "$(deployed_commit)" == "$head" ]] || fail "deployed $(deployed_commit), head $head"
commits="$(ostree --repo="$shared_repo" log "$typed_ref" | grep -c '^commit')"
[[ "$commits" == 1 ]] || fail "the repository holds $commits commits after the prune"
[[ "$(origin_remotes)" == "lushtext-origin" ]] || fail "origin remotes are: $(origin_remotes | tr '\n' ' ')"
remote_ls_ok || fail "remote-ls --updates fails: $(cat "$scratch/remote-ls.err")"
pass

# --- 6. installs serialize on the lock beside the repository ----------------
case_name="6 lock"
exec {lockfd}> "$shared_repo.lock"
flock "$lockfd"
rc=0
timeout 2 "$origin_script" install "$shared_repo" "$ref" -- touch "$scratch/marker6" > "$scratch/install6.log" 2>&1 || rc=$?
exec {lockfd}>&-
lockfd=""
[[ "$rc" == 124 ]] || { cat "$scratch/install6.log" >&2; fail "install did not wait on the held lock (exit $rc)"; }
[[ ! -e "$scratch/marker6" ]] || fail "the export ran while another install held the lock"
pass

# --- 7. a repository path outside the URI-safe character set is refused -----
case_name="7 character set"
bad_repo="$(LUSHTEXT_FLATPAK_INSTALL_REPO="$scratch/with space/repo" "$origin_script" path)"
expect_failure "$scratch/install7.log" origin_install "$bad_repo" touch "$scratch/marker7"
[[ ! -e "$scratch/marker7" ]] || fail "the export ran for a refused path"
[[ ! -e "$scratch/with space" ]] || fail "the refused path was created"
grep -q "LUSHTEXT_FLATPAK_INSTALL_REPO" "$scratch/install7.log" || fail "the refusal does not name the override"
pass

# --- 8. a valid origin that is not the shared repository only warns ---------
case_name="8 non-shared origin warns"
make_tree six
export_to "$scratch/other-checkout-repo"
flatpak remote-modify --user --url="file://$scratch/other-checkout-repo" lushtext-origin
"$origin_script" check > "$scratch/check8.log" 2>&1 || { cat "$scratch/check8.log" >&2; fail "check fails for a valid origin"; }
grep -qi "warning" "$scratch/check8.log" || fail "check does not warn about a non-shared origin"
grep -q "$shared_uri" "$scratch/check8.log" || fail "the warning does not name the shared repository"
pass

echo "flatpak install-origin fixture tests passed"
