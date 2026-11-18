#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The repository's build cache root, and the ONLY place its location is
# computed (design D1 of openspec/changes/archive/2026-11-15-reduce-build-disk-writes; the
# "Build Cache Root" section of .agents/rules/build.md):
#
#   scripts/build-cache.sh root
#   scripts/build-cache.sh checkout [<path>]       # print and create
#   scripts/build-cache.sh checkout-path [<path>]  # print only, create nothing
#   scripts/build-cache.sh prune [--dry-run]
#   scripts/build-cache.sh audit [<repo>]
#   scripts/build-cache.sh --self-test
#
# The root is ${LUSHTEXT_BUILD_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/lushtext-build}.
# It is outside the snapshotted checkout, so build trees kept there (mutation
# slots, the proof-strength worktree, flatpak-builder state) are neither
# snapshotted nor copied by cargo-mutants.
#
# Each checkout gets <root>/checkouts/<basename>-<sha256(realpath)[:16]>, which
# `checkout` creates with a CACHEDIR.TAG and a `checkout-path` file holding the
# checkout's real path. <path> defaults to the checkout holding this script.
# `checkout-path` prints the same directory without creating anything, for
# callers that resolve the location before they know they will write there
# (the Makefile's variables, proof-strength.py's import).
#
# `prune` removes the checkout directories whose recorded path is no longer a
# directory, and nothing else: the mutation slots (<root>/mutants) and live
# checkouts' directories are kept.
#
# `audit` (in `make check-build-cache-helpers`) fails when the Makefile,
# scripts/ or .githooks/:
#   1. export CARGO_INCREMENTAL=1 anywhere (a lane that keeps incremental
#      leaves the variable unset, design D3);
#   2. set CARGO_INCREMENTAL in the widget, test, Clippy or dev-build targets
#      or the scripts they run: those share the checkout's build dir, where a
#      toggle recompiles every workspace member;
#   3. name a build-cache location under the home directory anywhere but here:
#      the cache root's name, or XDG_CACHE_HOME / ~/.cache on a line that sets
#      a build path. clear-lushtext-xdg.sh's app-data cache is not a build
#      cache and is allowed by name.

set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

usage() {
    echo "usage: $0 root | checkout [<path>] | checkout-path [<path>] | prune [--dry-run] | --self-test" >&2
    exit 2
}

cache_root() {
    printf '%s\n' "${LUSHTEXT_BUILD_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/lushtext-build}"
}

# The checkout's real path: the git top level of <path> (default: this
# script's checkout), or <path> itself outside git.
checkout_realpath() {
    local path=${1:-$here/..} top
    [ -d "$path" ] || { echo "build-cache: not a directory: $path" >&2; return 1; }
    if top=$(git -C "$path" rev-parse --show-toplevel 2>/dev/null); then
        realpath "$top"
    else
        realpath "$path"
    fi
}

checkout_dir() {
    local real name hash
    real=$(checkout_realpath "${1:-}")
    name=$(basename "$real")
    hash=$(printf '%s' "$real" | sha256sum | cut -c1-16)
    printf '%s/checkouts/%s-%s\n' "$(cache_root)" "$name" "$hash"
}

cachedir_tag() {
    [ -e "$1/CACHEDIR.TAG" ] && return
    printf '%s\n' 'Signature: 8a477f597d28d172789f06886806bc55' \
        '# This directory holds LushText build state (scripts/build-cache.sh).' \
        '# It can be regenerated; backup and snapshot tools may skip it.' >"$1/CACHEDIR.TAG"
}

checkout_create() {
    local dir real
    real=$(checkout_realpath "${1:-}")
    dir=$(checkout_dir "$real")
    mkdir -p "$dir"
    cachedir_tag "$(cache_root)"
    cachedir_tag "$dir"
    if [ "$(cat "$dir/checkout-path" 2>/dev/null)" != "$real" ]; then
        printf '%s\n' "$real" >"$dir/checkout-path"
    fi
    printf '%s\n' "$dir"
}

prune() {
    local dry=0 dir recorded removed=0
    [ "${1:-}" = --dry-run ] && dry=1
    for dir in "$(cache_root)"/checkouts/*/; do
        dir=${dir%/}
        [ -f "$dir/checkout-path" ] || continue
        recorded=$(cat "$dir/checkout-path")
        [ -n "$recorded" ] && [ -d "$recorded" ] && continue
        if [ "$dry" = 1 ]; then
            echo "would remove $dir (checkout $recorded is gone)"
        else
            echo "removing $dir (checkout $recorded is gone)"
            rm -rf -- "$dir"
        fi
        removed=$((removed + 1))
    done
    if [ "$removed" -gt 0 ] && [ "$dry" = 0 ]; then
        echo "A proof-strength worktree inside a removed directory stays listed by 'git worktree list'; 'git worktree prune' (or the next proof-strength run) drops it."
    fi
    [ "$removed" -gt 0 ] || echo "nothing to prune under $(cache_root)/checkouts"
}


# Makefile targets that build in the checkout's shared build dir (audit 2),
# and the scripts they run.
SHARED_BUILD_TARGETS='build|build-debug|run|test|test-unit|test-int|test-prop|test-prop-deep|test-widget|test-widget-headless|test-widget-shard|check-clippy|check|pre-commit|pre-commit-docs'
SHARED_BUILD_SCRIPTS='scripts/run-widget-tests.sh scripts/widget-shards.py'
BUILD_PATH_WORDS='CARGO_TARGET_DIR|CARGO_BUILD_BUILD_DIR|build-dir|target-dir|--state-dir|state-dir|flatpak-builder|mutants|proof-strength'

audit() {
    local repo=${1:-$here/..} found=0 files f inc1 cache_name
    repo=$(cd "$repo" && pwd)
    files=$(cd "$repo" && { [ -f Makefile ] && echo Makefile; find scripts .githooks -type f 2>/dev/null; } | grep -v -e '^scripts/build-cache\.sh$' -e '__pycache__' | sort)
    inc1='CARGO_INCREMENTAL''=1'
    cache_name='lushtext''-build'
    for f in $files; do
        if grep -n -F "$inc1" "$repo/$f" | sed "s|^|$f:|" | grep .; then
            echo "build-cache audit: $f exports $inc1; a lane that keeps incremental leaves it unset" >&2
            found=1
        fi
        case $f in scripts/clear-lushtext-xdg.sh) continue ;; esac
        # The root's own name, not a longer one (lushtext-build-support).
        if grep -n -E "${cache_name}([^-[:alnum:]_]|\$)" "$repo/$f" | sed "s|^|$f:|" | grep .; then
            echo "build-cache audit: $f names the build cache root; resolve it with scripts/build-cache.sh" >&2
            found=1
        fi
        if grep -n -E '(XDG_CACHE_HOME|HOME/\.cache|~/\.cache)' "$repo/$f" | grep -E "$BUILD_PATH_WORDS" | sed "s|^|$f:|" | grep .; then
            echo "build-cache audit: $f puts a build path under the home cache; use scripts/build-cache.sh" >&2
            found=1
        fi
    done
    if [ -f "$repo/Makefile" ] && awk -v targets="^($SHARED_BUILD_TARGETS):" '
            $0 ~ targets { on = 1; print FILENAME ":" NR ": " $0; next }
            on && /^\t/ { print FILENAME ":" NR ": " $0; next }
            { on = 0 }' "$repo/Makefile" | grep 'CARGO_INCREMENTAL'; then
        echo "build-cache audit: a shared-build-dir Makefile target sets CARGO_INCREMENTAL" >&2
        found=1
    fi
    for f in $SHARED_BUILD_SCRIPTS; do
        if [ -f "$repo/$f" ] && grep -n 'CARGO_INCREMENTAL' "$repo/$f" | sed "s|^|$f:|" | grep .; then
            echo "build-cache audit: $f (a shared-build-dir lane) sets CARGO_INCREMENTAL" >&2
            found=1
        fi
    done
    [ "$found" = 0 ] || return 1
    echo "build-cache audit: ok"
}

fail() {
    echo "build-cache self-test: $*" >&2
    return 1
}

self_test() {
    local me=$here/build-cache.sh dir out a b da db
    dir=$(mktemp -d "${TMPDIR:-/tmp}/build-cache-self-test.XXXXXX")
    # shellcheck disable=SC2064 # expand now: the trap must remove this dir
    trap "rm -rf '$dir'" EXIT
    # A hook-run self-test inherits the commit's git environment; the fixture
    # repositories below must not read it.
    unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_PREFIX GIT_OBJECT_DIRECTORY GIT_COMMON_DIR

    # Root precedence: LUSHTEXT_BUILD_CACHE, then XDG_CACHE_HOME, then HOME.
    out=$(env -u LUSHTEXT_BUILD_CACHE -u XDG_CACHE_HOME HOME="$dir/home" "$me" root)
    [ "$out" = "$dir/home/.cache/lushtext-build" ] || fail "HOME root was '$out'" || return
    out=$(env -u LUSHTEXT_BUILD_CACHE XDG_CACHE_HOME="$dir/xdg" "$me" root)
    [ "$out" = "$dir/xdg/lushtext-build" ] || fail "XDG root was '$out'" || return
    out=$(LUSHTEXT_BUILD_CACHE="$dir/explicit" XDG_CACHE_HOME="$dir/xdg" "$me" root)
    [ "$out" = "$dir/explicit" ] || fail "explicit root was '$out'" || return

    export LUSHTEXT_BUILD_CACHE=$dir/root
    git init -q "$dir/repo"
    git -C "$dir/repo" -c user.name=t -c user.email=t@example.invalid -c commit.gpgsign=false \
        commit -q --allow-empty -m fixture
    git -C "$dir/repo" worktree add -q --detach "$dir/wt-a"
    git -C "$dir/repo" worktree add -q --detach "$dir/wt-b"

    # checkout-path prints the directory and creates nothing under the root.
    a=$("$me" checkout-path "$dir/wt-a")
    [ ! -e "$LUSHTEXT_BUILD_CACHE" ] || fail "checkout-path created $LUSHTEXT_BUILD_CACHE" || return
    case $a in
        "$LUSHTEXT_BUILD_CACHE"/checkouts/wt-a-????????????????) ;;
        *) fail "checkout-path printed '$a'" || return ;;
    esac

    # checkout prints the same directory, creating it with its records.
    da=$("$me" checkout "$dir/wt-a")
    [ "$da" = "$a" ] || fail "checkout printed '$da', checkout-path '$a'" || return
    [ "$(cat "$da/checkout-path")" = "$(realpath "$dir/wt-a")" ] || fail "checkout-path file is wrong" || return
    grep -q '^Signature: 8a477f597d28d172789f06886806bc55' "$da/CACHEDIR.TAG" || fail "no CACHEDIR.TAG in $da" || return
    [ -f "$LUSHTEXT_BUILD_CACHE/CACHEDIR.TAG" ] || fail "no CACHEDIR.TAG at the root" || return
    # A subdirectory resolves to its checkout.
    mkdir -p "$dir/wt-a/sub"
    [ "$("$me" checkout-path "$dir/wt-a/sub")" = "$da" ] || fail "a subdirectory got another directory" || return

    # Two worktrees get distinct directories, each recording its own path.
    db=$("$me" checkout "$dir/wt-b")
    [ "$db" != "$da" ] || fail "two worktrees share $da" || return
    [ "$(cat "$db/checkout-path")" = "$(realpath "$dir/wt-b")" ] || fail "wt-b records the wrong path" || return

    # Prune removes only the directory whose checkout is gone.
    mkdir -p "$LUSHTEXT_BUILD_CACHE/mutants/slot-1/build"
    git -C "$dir/repo" worktree remove --force "$dir/wt-b"
    out=$("$me" prune --dry-run)
    [ -d "$db" ] || fail "prune --dry-run removed $db" || return
    case $out in *"would remove $db"*) ;; *) fail "dry run did not name $db: $out" || return ;; esac
    "$me" prune >/dev/null
    [ ! -e "$db" ] || fail "prune kept the removed worktree's $db" || return
    [ -d "$da" ] || fail "prune removed the live checkout's $da" || return
    [ -d "$LUSHTEXT_BUILD_CACHE/mutants/slot-1/build" ] || fail "prune removed a mutation slot" || return
    b=$("$me" checkout-path "$dir/repo")
    [ "$b" != "$da" ] || fail "the main checkout shares wt-a's directory" || return

    # The audit: each rule fails on its fixture, a clean tree and this one pass.
    local fx=$dir/audit
    mkdir -p "$fx/scripts" "$fx/.githooks"
    printf 'test:\n\tcargo test\nkani:\n\tCARGO_INCREMENTAL=0 cargo kani\n' >"$fx/Makefile"
    # shellcheck disable=SC2016 # the fixtures are literal script text
    printf '#!/bin/sh\nXDG_CACHE_HOME=$dir/cache ./app\n' >"$fx/scripts/smoke.sh"
    "$me" audit "$fx" >/dev/null 2>&1 || fail "the clean audit fixture failed" || return
    printf 'mutants:\n\tCARGO_INCREMENTAL=%s cargo mutants\n' 1 >>"$fx/Makefile"
    ! "$me" audit "$fx" >/dev/null 2>&1 || fail "an exported incremental=1 passed the audit" || return
    printf 'test:\n\tcargo test\ncheck-clippy:\n\tCARGO_INCREMENTAL=0 cargo clippy\n' >"$fx/Makefile"
    ! "$me" audit "$fx" >/dev/null 2>&1 || fail "CARGO_INCREMENTAL in check-clippy passed the audit" || return
    printf 'test:\n\tcargo test\n' >"$fx/Makefile"
    printf 'export CARGO_INCREMENTAL=0\n' >"$fx/scripts/run-widget-tests.sh"
    ! "$me" audit "$fx" >/dev/null 2>&1 || fail "CARGO_INCREMENTAL in run-widget-tests.sh passed the audit" || return
    rm "$fx/scripts/run-widget-tests.sh"
    # shellcheck disable=SC2016
    printf 'CARGO_BUILD_BUILD_DIR=${XDG_CACHE_HOME:-$HOME/.cache}/x cargo build\n' >"$fx/scripts/b.sh"
    ! "$me" audit "$fx" >/dev/null 2>&1 || fail "a build dir under XDG_CACHE_HOME passed the audit" || return
    # shellcheck disable=SC2016
    printf 'root=$HOME/.cache/%s\n' "lushtext""-build" >"$fx/scripts/b.sh"
    ! "$me" audit "$fx" >/dev/null 2>&1 || fail "the cache root's name outside build-cache.sh passed the audit" || return
    rm "$fx/scripts/b.sh"
    "$me" audit >/dev/null || fail "this checkout fails the audit" || return

    echo "build-cache self-test: ok"
}

case "${1:-}" in
    root) [ $# -eq 1 ] || usage; cache_root ;;
    checkout) [ $# -le 2 ] || usage; checkout_create "${2:-}" ;;
    checkout-path) [ $# -le 2 ] || usage; checkout_dir "${2:-}" ;;
    prune) [ $# -le 2 ] || usage; prune "${2:-}" ;;
    audit) [ $# -le 2 ] || usage; audit "${2:-}" ;;
    --self-test) self_test ;;
    *) usage ;;
esac
