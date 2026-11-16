#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Stage the git-visible tree of a checkout for flatpak-builder.
#
# The local manifest's source is `{"type": "dir", "path": ".."}`, and
# flatpak-builder copies every file under that directory, ignored ones
# included. Building the manifest in place therefore made everything left in
# a working checkout build input: `target/`, `mutants.out*/`, the agents'
# `.claude/handoffs/` (once a `debuginfod-cache` there, owned by `nobody` and
# unreadable, aborted `make flatpak-install` before the first compile).
#
# `make flatpak` and `make flatpak-install` instead build a copy of exactly
# what git sees: tracked files as they are on disk (local edits included,
# tracked files deleted on disk left out) plus untracked files that no
# .gitignore rule excludes. The manifest is unchanged; it is built from the
# copy, so its `..` is the staged tree. CI already builds a clean checkout.
#
# Usage:
#   scripts/flatpak-source-tree.sh stage <checkout> <dir>   # print <dir>/tree
#   scripts/flatpak-source-tree.sh --self-test
#
# <dir> is owned by this script: it must be an absolute path outside the
# checkout (and must not contain it), and an existing non-empty <dir> is
# replaced only when it carries this script's stamp. The tree is rebuilt
# whole on every call, so a file removed from the checkout leaves the stage
# too. Modes and symlinks are preserved (tar).

set -euo pipefail

# Forget the caller's repository. Inside a git hook (the pre-commit hook runs
# `make check-policy`, which runs the self-test) git exports GIT_DIR,
# GIT_INDEX_FILE and friends, and they override `git -C`: every call here
# would then read, and the fixture's `init`/`add` would rewrite, the
# caller's repository instead of the one named (it once set core.bare=true in
# the checkout's config and replaced its index).
while IFS= read -r git_var; do
    unset "$git_var"
done < <(git rev-parse --local-env-vars)
unset git_var

SELF="$(realpath -- "${BASH_SOURCE[0]}")"
readonly SELF
readonly STAMP_NAME=".lushtext-flatpak-source"

die() {
    echo "flatpak-source-tree: $*" >&2
    exit 1
}

# Print each git-visible path of <checkout>, NUL-terminated: tracked and
# untracked-not-ignored files that exist on disk (a symlink counts even when
# dangling).
visible_paths() {
    local checkout="$1" path
    git -C "$checkout" ls-files -z --cached --others --exclude-standard --deduplicate |
        while IFS= read -r -d '' path; do
            if [[ -e "$checkout/$path" || -L "$checkout/$path" ]]; then
                printf '%s\0' "$path"
            fi
        done
}

cmd_stage() {
    local checkout_arg="$1" dir="$2" checkout tree next

    [[ "$dir" == /* ]] || die "<dir> must be an absolute path: $dir"
    git -C "$checkout_arg" rev-parse --is-inside-work-tree >/dev/null 2>&1 ||
        die "not a git checkout: $checkout_arg"
    checkout="$(git -C "$checkout_arg" rev-parse --show-toplevel)"
    checkout="$(realpath -- "$checkout")"
    dir="$(realpath -m -- "$dir")"

    case "$dir/" in
        "$checkout"/*) die "<dir> must be outside the checkout: $dir" ;;
    esac
    case "$checkout/" in
        "$dir"/*) die "<dir> must not contain the checkout: $dir" ;;
    esac

    if [[ -e "$dir" ]]; then
        [[ -d "$dir" && ! -L "$dir" ]] || die "<dir> exists and is not a directory: $dir"
        if [[ ! -e "$dir/$STAMP_NAME" ]] && [[ -n "$(ls -A -- "$dir")" ]]; then
            die "refusing to replace $dir: it is not empty and has no $STAMP_NAME stamp"
        fi
    fi
    mkdir -p -- "$dir"
    printf 'Staged by scripts/flatpak-source-tree.sh from %s\n' "$checkout" >"$dir/$STAMP_NAME"

    tree="$dir/tree"
    next="$dir/tree.next"
    if [[ -e "$next" ]]; then
        chmod -R u+w -- "$next" 2>/dev/null || true
        rm -rf -- "$next"
    fi
    mkdir -- "$next"
    visible_paths "$checkout" |
        tar -C "$checkout" --null --no-recursion -T - -cf - |
        tar -C "$next" -xpf -
    if [[ -e "$tree" ]]; then
        chmod -R u+w -- "$tree" 2>/dev/null || true
        rm -rf -- "$tree"
    fi
    mv -- "$next" "$tree"
    printf '%s\n' "$tree"
}

# Self-test helpers: each assertion is a plain command, no eval.
absent() { [[ ! -e "$1" && ! -L "$1" ]]; }
same() { [[ "$1" == "$2" ]]; }
refused() { (cmd_stage "$@") >/dev/null 2>&1 && return 1; return 0; }

self_test() {
    local scratch repo stage tree failures=0
    scratch="$(mktemp -d "${TMPDIR:-/tmp}/lt-flatpak-src-XXXXXX")"
    # shellcheck disable=SC2064 # expand the scratch path now
    trap "chmod -R u+rwx -- '$scratch' 2>/dev/null || true; rm -rf -- '$scratch'" EXIT

    expect() {
        local description="$1"
        shift
        if "$@"; then
            echo "ok - $description"
        else
            echo "FAIL - $description"
            failures=$((failures + 1))
        fi
    }

    repo="$scratch/checkout"
    stage="$scratch/cache/flatpak-source"
    tree="$stage/tree"
    mkdir -p "$repo/bin" "$repo/build-aux" "$repo/ignored/locked" "$repo/.claude/handoffs/x"
    # The fixture's git ignores the user's and the system's configuration and
    # runs no hooks.
    fixture_git() {
        GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -C "$1" \
            -c core.hooksPath=/dev/null -c user.name=fixture \
            -c user.email=fixture@invalid -c commit.gpgsign=false "${@:2}"
    }
    fixture_git "$repo" init -q
    printf '/ignored/\n*.log\n/.claude/\n' >"$repo/.gitignore"
    echo committed >"$repo/a.txt"
    printf '#!/bin/sh\n' >"$repo/bin/run.sh"
    chmod 755 "$repo/bin/run.sh"
    echo '{}' >"$repo/build-aux/app.json"
    echo doomed >"$repo/gone.txt"
    ln -s a.txt "$repo/link"
    fixture_git "$repo" add -A
    fixture_git "$repo" commit -qm fixture
    echo dirty >"$repo/a.txt"
    rm "$repo/gone.txt"
    echo untracked >"$repo/new.txt"
    echo secret >"$repo/ignored/locked/f"
    echo noise >"$repo/x.log"
    echo handoff >"$repo/.claude/handoffs/x/log"
    # Unreadable ignored directories, like the nobody-owned debuginfod cache
    # that aborted an in-place build.
    chmod 000 "$repo/ignored/locked" "$repo/.claude/handoffs/x"

    expect "prints the staged tree" same "$(cmd_stage "$repo" "$stage")" "$tree"
    expect "a dirty tracked edit is staged" grep -qx dirty "$tree/a.txt"
    expect "the executable bit is kept" test -x "$tree/bin/run.sh"
    expect "a symlink stays a symlink" test -L "$tree/link"
    expect "the symlink target is kept" same "$(readlink "$tree/link")" a.txt
    expect "a nested tracked file is staged" test -f "$tree/build-aux/app.json"
    expect "a tracked file deleted on disk is left out" absent "$tree/gone.txt"
    expect "an untracked, not ignored file is staged" test -f "$tree/new.txt"
    expect "an unreadable ignored directory is left out" absent "$tree/ignored"
    expect "an ignored handoff directory is left out" absent "$tree/.claude"
    expect "an ignored file is left out" absent "$tree/x.log"
    expect "the .git directory is left out" absent "$tree/.git"
    expect "the stamp is beside the tree" test -f "$stage/$STAMP_NAME"
    expect "the stamp is not in the tree" absent "$tree/$STAMP_NAME"

    rm "$repo/new.txt"
    cmd_stage "$repo" "$stage" >/dev/null
    expect "a restage drops a file removed since" absent "$tree/new.txt"
    expect "a restage leaves no tree.next" absent "$stage/tree.next"

    expect "a <dir> inside the checkout is refused" refused "$repo" "$repo/inside"
    expect "a refused <dir> inside the checkout is not created" absent "$repo/inside"
    expect "a <dir> containing the checkout is refused" refused "$repo" "$scratch"
    expect "the checkout survives that refusal" test -d "$repo/.git"
    expect "a relative <dir> is refused" refused "$repo" relative/dir
    mkdir -p "$scratch/foreign"
    echo keep >"$scratch/foreign/precious"
    expect "an unstamped non-empty <dir> is refused" refused "$repo" "$scratch/foreign"
    expect "an unstamped <dir> is kept" grep -qx keep "$scratch/foreign/precious"
    expect "a non-git <checkout> is refused" refused "$scratch/foreign" "$scratch/other"

    # Run as a git hook would run it: the caller's GIT_DIR, GIT_INDEX_FILE and
    # GIT_WORK_TREE point at another repository, which must stay untouched.
    local decoy="$scratch/decoy" index_before config_before
    mkdir -p "$decoy"
    fixture_git "$decoy" init -q
    echo decoy >"$decoy/d.txt"
    fixture_git "$decoy" add d.txt
    index_before="$(sha256sum <"$decoy/.git/index")"
    config_before="$(sha256sum <"$decoy/.git/config")"
    GIT_DIR="$decoy/.git" GIT_INDEX_FILE="$decoy/.git/index" GIT_WORK_TREE="$decoy" \
        "$SELF" stage "$repo" "$scratch/cache/hook-stage" >/dev/null
    expect "under a hook's git environment the named checkout is staged" \
        grep -qx dirty "$scratch/cache/hook-stage/tree/a.txt"
    expect "under a hook's git environment the decoy's file is not staged" \
        absent "$scratch/cache/hook-stage/tree/d.txt"
    expect "the caller's index is untouched" same "$(sha256sum <"$decoy/.git/index")" "$index_before"
    expect "the caller's config is untouched" same "$(sha256sum <"$decoy/.git/config")" "$config_before"

    if ((failures > 0)); then
        echo "flatpak-source-tree self-test: $failures failure(s)" >&2
        return 1
    fi
    echo "flatpak-source-tree self-test: all checks passed"
}

case "${1:-}" in
    stage)
        [[ $# -eq 3 ]] || die "usage: $0 stage <checkout> <dir>"
        cmd_stage "$2" "$3"
        ;;
    --self-test)
        self_test
        ;;
    *)
        echo "usage: $0 stage <checkout> <dir> | --self-test" >&2
        exit 2
        ;;
esac
