#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Gate evidence keyed by tree (.agents/rules/parallel-agent-work.md, "Merging").
#
#   scripts/gate-trees.sh record <worktree> --gate '<gate>' [--change C] [--log-ref PATH] -- <command...>
#   scripts/gate-trees.sh lookup <worktree> --gate '<gate>' [--gate '<gate>'...] [--change C]
#   scripts/gate-trees.sh --self-test
#
# `record` reads the worktree's `git status --porcelain` and `HEAD^{tree}`, runs
# the command as its child (normally `scripts/heavy.sh ... -- make <target>`),
# reads both again, and appends one line to <handoffs>/<change>/gate-trees.log
# only when the command exited 0, the status was empty before and after, and
# the tree did not change in between:
#
#   <date -Is>\ttree=<hash>\tresult=pass\tlog=<PATH or ->\tgate=<gate>
#
# It always returns the command's exit status. A dirty or changing worktree
# records nothing and says why on stderr.
#
# `lookup` prints the newest line whose tree is the worktree's current
# `HEAD^{tree}` and whose gate string equals one of the --gate values exactly
# (exit 0), or exits 1. It refuses (exit 1) while the worktree is dirty, since
# then no tree hash describes what is on disk. Pass several --gate values when
# a broader gate covers the one owed, for example
# `--gate 'make test-widget-shard WIDGET_SHARD=x' --gate 'make test'`.
#
# <gate> is the make target with its variables, without the heavy.sh wrapper.
# <change> defaults to the worktree's branch name. The handoff directory is
# the main checkout's .claude/handoffs (from git's common dir);
# LUSHTEXT_HANDOFF_DIR overrides it.

set -euo pipefail
export LC_ALL=C

# A hook exports GIT_DIR and its siblings, which would override `git -C`
# (.agents/rules/testing.md, "Test Isolation From Real App Data").
scrub_git_env() {
    local name
    while IFS= read -r name; do
        [ -n "$name" ] && unset "$name"
    done < <(git rev-parse --local-env-vars 2>/dev/null || true)
}
scrub_git_env

die() { echo "gate-trees: $*" >&2; exit 2; }

handoff_dir() {
    local wt=$1
    if [ -n "${LUSHTEXT_HANDOFF_DIR:-}" ]; then
        printf '%s\n' "$LUSHTEXT_HANDOFF_DIR"
        return
    fi
    local common
    common=$(git -C "$wt" rev-parse --path-format=absolute --git-common-dir) || die "$wt is not a git worktree"
    printf '%s/.claude/handoffs\n' "$(dirname "$common")"
}

change_of() {
    local wt=$1 given=$2 branch
    if [ -n "$given" ]; then
        printf '%s\n' "$given"
        return
    fi
    branch=$(git -C "$wt" branch --show-current) || die "cannot read the branch of $wt"
    [ -n "$branch" ] || die "$wt has a detached HEAD; pass --change"
    printf '%s\n' "$branch"
}

tree_of() { git -C "$1" rev-parse 'HEAD^{tree}'; }
status_of() { git -C "$1" status --porcelain; }

cmd_record() {
    local wt=${1:-}
    [ -n "$wt" ] && [ -d "$wt" ] || die "record needs an existing worktree path"
    shift
    local gate='' change='' log_ref='-'
    while [ $# -gt 0 ]; do
        case "$1" in
            --gate) gate=${2:?--gate needs a value}; shift 2 ;;
            --change) change=${2:?--change needs a value}; shift 2 ;;
            --log-ref) log_ref=${2:?--log-ref needs a path}; shift 2 ;;
            --) shift; break ;;
            *) die "unknown record option $1" ;;
        esac
    done
    [ -n "$gate" ] || die "record needs --gate"
    [ $# -gt 0 ] || die "record needs a command after --"
    case "$gate" in *$'\t'*|*$'\n'*) die "--gate must be one line without tabs" ;; esac
    local h logfile status_before tree_before status_after tree_after rc=0
    h=$(handoff_dir "$wt")
    change=$(change_of "$wt" "$change")
    logfile="$h/$change/gate-trees.log"
    status_before=$(status_of "$wt") || die "git status failed in $wt"
    tree_before=$(tree_of "$wt") || die "cannot read HEAD^{tree} in $wt"
    "$@" || rc=$?
    status_after=$(status_of "$wt") || die "git status failed in $wt"
    tree_after=$(tree_of "$wt") || die "cannot read HEAD^{tree} in $wt"
    if [ "$rc" -ne 0 ]; then
        echo "gate-trees: '$gate' exited $rc; nothing recorded" >&2
    elif [ -n "$status_before" ]; then
        echo "gate-trees: the worktree was dirty before '$gate'; nothing recorded" >&2
    elif [ -n "$status_after" ]; then
        echo "gate-trees: the worktree was dirty after '$gate'; nothing recorded" >&2
    elif [ "$tree_before" != "$tree_after" ]; then
        echo "gate-trees: HEAD^{tree} changed during '$gate'; nothing recorded" >&2
    else
        mkdir -p "$(dirname "$logfile")"
        printf '%s\ttree=%s\tresult=pass\tlog=%s\tgate=%s\n' \
            "$(date -Is)" "$tree_after" "$log_ref" "$gate" >>"$logfile"
        echo "gate-trees: recorded '$gate' on tree $tree_after"
    fi
    return "$rc"
}

cmd_lookup() {
    local wt=${1:-}
    [ -n "$wt" ] && [ -d "$wt" ] || die "lookup needs an existing worktree path"
    shift
    local change='' gates=()
    while [ $# -gt 0 ]; do
        case "$1" in
            --gate) gates+=("${2:?--gate needs a value}"); shift 2 ;;
            --change) change=${2:?--change needs a value}; shift 2 ;;
            *) die "unknown lookup option $1" ;;
        esac
    done
    [ "${#gates[@]}" -gt 0 ] || die "lookup needs --gate"
    local h logfile tree status
    h=$(handoff_dir "$wt")
    change=$(change_of "$wt" "$change")
    logfile="$h/$change/gate-trees.log"
    status=$(status_of "$wt") || die "git status failed in $wt"
    if [ -n "$status" ]; then
        echo "gate-trees: $wt is dirty; no recorded tree describes it" >&2
        return 1
    fi
    tree=$(tree_of "$wt") || die "cannot read HEAD^{tree} in $wt"
    [ -f "$logfile" ] || return 1
    local line found='' g
    while IFS= read -r line; do
        case "$line" in *$'\t'"tree=$tree"$'\t'"result=pass"$'\t'*) ;; *) continue ;; esac
        for g in "${gates[@]}"; do
            if [ "${line##*$'\t'gate=}" = "$g" ]; then
                found=$line
            fi
        done
    done <"$logfile"
    [ -n "$found" ] || return 1
    printf '%s\n' "$found"
}

self_test_fail() {
    echo "gate-trees self-test FAILED: $*" >&2
    exit 1
}

self_test() {
    SELF_TEST_TMP=$(mktemp -d)
    trap 'rm -rf "$SELF_TEST_TMP"' EXIT INT TERM
    local tmp=$SELF_TEST_TMP
    export LUSHTEXT_HANDOFF_DIR="$tmp/h"
    export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
    local wt="$tmp/repo" log rc out
    git init -q -b fixture "$wt"
    git -C "$wt" -c core.hooksPath=/dev/null -c user.name=t -c user.email=t@example.invalid \
        -c commit.gpgsign=false commit -q --allow-empty -m init
    log="$LUSHTEXT_HANDOFF_DIR/fixture/gate-trees.log"
    count() { [ -f "$log" ] && wc -l <"$log" || echo 0; }

    cmd_record "$wt" --gate 'make check' -- true >/dev/null 2>&1 || self_test_fail "a clean pass returned non-zero"
    [ "$(count)" -eq 1 ] || self_test_fail "a clean pass recorded $(count) lines"
    out=$(cmd_lookup "$wt" --gate 'make check') || self_test_fail "lookup missed the recorded gate"
    case "$out" in *"tree=$(tree_of "$wt")"*"gate=make check") ;; *) self_test_fail "lookup printed '$out'" ;; esac
    rc=0; cmd_lookup "$wt" --gate 'make check-policy' >/dev/null 2>&1 || rc=$?
    [ "$rc" = 1 ] || self_test_fail "lookup matched another gate string"
    rc=0; cmd_lookup "$wt" --gate 'make' >/dev/null 2>&1 || rc=$?
    [ "$rc" = 1 ] || self_test_fail "lookup matched a gate prefix"
    cmd_lookup "$wt" --gate 'make test-widget-shard WIDGET_SHARD=x' --gate 'make check' >/dev/null \
        || self_test_fail "lookup with a covering gate missed"

    rc=0; cmd_record "$wt" --gate 'make test' -- sh -c 'exit 3' >/dev/null 2>&1 || rc=$?
    [ "$rc" = 3 ] || self_test_fail "a failing command returned $rc, expected 3"
    [ "$(count)" -eq 1 ] || self_test_fail "a failing command recorded a line"

    : >"$wt/dirty"
    cmd_record "$wt" --gate 'make test' -- true >/dev/null 2>&1 || self_test_fail "dirty-before pass returned non-zero"
    [ "$(count)" -eq 1 ] || self_test_fail "a dirty-before run recorded a line"
    rc=0; cmd_lookup "$wt" --gate 'make check' >/dev/null 2>&1 || rc=$?
    [ "$rc" = 1 ] || self_test_fail "lookup answered for a dirty worktree"
    rm -f "$wt/dirty"

    cmd_record "$wt" --gate 'make test' -- touch "$wt/made" >/dev/null 2>&1 || self_test_fail "dirty-after pass returned non-zero"
    [ "$(count)" -eq 1 ] || self_test_fail "a dirty-after run recorded a line"
    rm -f "$wt/made"

    # shellcheck disable=SC2016 # the inner sh expands its own $1
    cmd_record "$wt" --gate 'make test' -- sh -c '
        echo x >"$1/f" && git -C "$1" add f &&
        git -C "$1" -c core.hooksPath=/dev/null -c user.name=t -c user.email=t@example.invalid \
            -c commit.gpgsign=false commit -q -m change' sh "$wt" >/dev/null 2>&1 \
        || self_test_fail "tree-changing pass returned non-zero"
    [ "$(count)" -eq 1 ] || self_test_fail "a tree-changing run recorded a line"
    rc=0; cmd_lookup "$wt" --gate 'make check' >/dev/null 2>&1 || rc=$?
    [ "$rc" = 1 ] || self_test_fail "lookup matched a record of another tree"

    cmd_record "$wt" --gate 'make test' --log-ref /x/test.log -- true >/dev/null 2>&1
    out=$(cmd_lookup "$wt" --gate 'make test') || self_test_fail "lookup missed the new tree's record"
    case "$out" in *"log=/x/test.log"*) ;; *) self_test_fail "log reference not recorded: '$out'" ;; esac

    # A decoy repository's GIT_DIR, as a commit hook exports it, must not redirect the helper.
    local decoy="$tmp/decoy"
    git init -q "$decoy"
    out=$(GIT_DIR="$decoy/.git" GIT_WORK_TREE="$decoy" GIT_INDEX_FILE="$decoy/.git/index" \
        "$0" lookup "$wt" --gate 'make test') || self_test_fail "lookup under a decoy GIT_DIR missed"
    echo "gate-trees self-test: ok (clean pass recorded; failing, dirty-before, dirty-after and tree-changing runs record nothing; exact gate match, covering gates, dirty lookup refused; decoy GIT_DIR ignored)"
}

case "${1:-}" in
    record) shift; cmd_record "$@" ;;
    lookup) shift; cmd_lookup "$@" ;;
    --self-test) self_test ;;
    *) echo "usage: gate-trees.sh record <worktree> --gate G [--change C] [--log-ref P] -- <cmd...> | lookup <worktree> --gate G... [--change C] | --self-test" >&2; exit 2 ;;
esac
