#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Gate evidence keyed by tree (.agents/rules/parallel-agent-work.md, "Merging").
#
#   scripts/gate-trees.sh record <worktree> --gate '<gate>' [--change C] [--log-ref PATH] -- <command...>
#   scripts/gate-trees.sh lookup <worktree> --gate '<gate>' [--gate '<gate>'...] [--change C]
#   scripts/gate-trees.sh lookup <checkout> --tree <hash> --change C --gate '<gate>' [--gate ...]
#   scripts/gate-trees.sh void <checkout> --change C --line <timestamp> --gate '<gate>' --log PATH --reason TEXT
#   scripts/gate-trees.sh --self-test
#
# `record` reads the worktree's `git status --porcelain` and `HEAD^{tree}`, runs
# the command as its child (normally `scripts/heavy.sh ... -- make <target>`),
# reads both again, and appends one line to <handoffs>/<change>/gate-trees.log
# when the status was empty before and after and the tree did not change in
# between: result=pass when the command exited 0, result=interrupted when it
# timed out (124) or was killed by a signal (128 and above), result=fail
# otherwise. A refusal before the gate ran (75 busy, 78 off its CPUs, 126 not
# executable, 127 not found) records nothing.
#
#   <timestamp>\ttree=<hash>\tresult=<pass|fail|interrupted>\tlog=<PATH or ->\tgate=<gate>
#
# An interrupted line is not a result: lookup ignores it but names it on
# stderr, so a hang or an OOM kill that passes on a retry leaves a trace for
# the flake rule (preexisting-blockers.md).
#
# `void` is the coordinator's alone. It never deletes a line: it appends
#
#   <timestamp>\ttree=<hash>\tresult=void\tlog=<PATH>\tvoids=<timestamp>\treason=<text>\tgate=<gate>
#
# naming exactly one fail line (its timestamp and gate) and the log that shows
# that gate never ran (a wrapper's usage refusal, which exits 2 like a failing
# `make`). lookup then ignores that fail line, and only that one. It refuses a
# missing or empty log, an empty reason, and a timestamp and gate that match no
# fail line, more than one, or one already voided.
#
# It always returns the command's exit status. A dirty or changing worktree
# records nothing and says why on stderr. Every field is one line without tabs.
#
# `lookup` prints the newest pass line whose tree is the worktree's current
# `HEAD^{tree}` and whose gate string equals one of the --gate values exactly
# (exit 0), or exits 1. It refuses (exit 1) while the worktree is dirty, since
# then no tree hash describes what is on disk, and when any named gate has a
# fail line on that tree that no void line names: a gate that passed and
# failed on one tree is a flake, a blocker (preexisting-blockers.md), and
# reuse would hide it. A line with any other result also refuses. Pass several
# --gate values when a broader gate covers the one owed, for example
# `--gate 'make test-widget-shard WIDGET_SHARD=x' --gate 'make test'`.
# With --tree, the tree is the given full hash instead of a worktree's HEAD
# (a merged change whose worktree is gone); <checkout> then only locates the
# handoff directory, and --change is required.
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
# Exit statuses of a gate that never ran: heavy.sh's busy (75) and
# CPU-placement (78) refusals, and a command that could not start (126 not
# executable, 127 not found).
never_ran() {
    case "$1" in 75|78|126|127) return 0 ;; *) return 1 ;; esac
}
# A run that started and did not finish: a timeout (124) or a signal (128 and
# above, an OOM kill among them).
interrupted() {
    [ "$1" -eq 124 ] || [ "$1" -ge 128 ]
}
one_line() {
    case "$2" in *$'\t'*|*$'\n'*) die "$1 must be one line without tabs" ;; esac
}
status_of() { git -C "$1" status --porcelain; }
# A line's timestamp, to the nanosecond, so a void names one line even when
# two runs end in the same second (older lines carry `date -Is` seconds).
stamp() { date +%Y-%m-%dT%H:%M:%S.%N%:z; }

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
    one_line --gate "$gate"
    one_line --change "$change"
    one_line --log-ref "$log_ref"
    local h logfile status_before tree_before status_after tree_after rc=0
    h=$(handoff_dir "$wt")
    change=$(change_of "$wt" "$change")
    logfile="$h/$change/gate-trees.log"
    status_before=$(status_of "$wt") || die "git status failed in $wt"
    tree_before=$(tree_of "$wt") || die "cannot read HEAD^{tree} in $wt"
    "$@" || rc=$?
    status_after=$(status_of "$wt") || die "git status failed in $wt"
    tree_after=$(tree_of "$wt") || die "cannot read HEAD^{tree} in $wt"
    local result=pass
    [ "$rc" -eq 0 ] || result=fail
    ! interrupted "$rc" || result=interrupted
    if never_ran "$rc"; then
        echo "gate-trees: '$gate' never ran (exit $rc); nothing recorded" >&2
    elif [ -n "$status_before" ]; then
        echo "gate-trees: the worktree was dirty before '$gate'; nothing recorded" >&2
    elif [ -n "$status_after" ]; then
        echo "gate-trees: the worktree was dirty after '$gate'; nothing recorded" >&2
    elif [ "$tree_before" != "$tree_after" ]; then
        echo "gate-trees: HEAD^{tree} changed during '$gate'; nothing recorded" >&2
    else
        mkdir -p "$(dirname "$logfile")"
        printf '%s\ttree=%s\tresult=%s\tlog=%s\tgate=%s\n' \
            "$(stamp)" "$tree_after" "$result" "$log_ref" "$gate" >>"$logfile"
        echo "gate-trees: recorded '$gate' as $result (exit $rc) on tree $tree_after"
    fi
    return "$rc"
}

field_of() {
    # The value of one tab-separated field (name=value) of a record line.
    local line=$1 name=$2 v
    case "$line" in *$'\t'"$name="*) ;; *) return 1 ;; esac
    v=${line#*$'\t'"$name="}
    printf '%s\n' "${v%%$'\t'*}"
}

cmd_lookup() {
    local wt=${1:-}
    [ -n "$wt" ] && [ -d "$wt" ] || die "lookup needs an existing worktree path"
    shift
    local change='' given_tree='' gates=()
    while [ $# -gt 0 ]; do
        case "$1" in
            --gate) gates+=("${2:?--gate needs a value}"); shift 2 ;;
            --change) change=${2:?--change needs a value}; shift 2 ;;
            --tree) given_tree=${2:?--tree needs a hash}; shift 2 ;;
            *) die "unknown lookup option $1" ;;
        esac
    done
    one_line --change "$change"
    [ "${#gates[@]}" -gt 0 ] || die "lookup needs --gate"
    local h logfile tree status
    h=$(handoff_dir "$wt")
    if [ -n "$given_tree" ]; then
        [[ $given_tree =~ ^[0-9a-f]{40}$ ]] || die "--tree needs a full 40-hex tree hash, not '$given_tree'"
        [ -n "$change" ] || die "lookup --tree needs --change"
        tree=$given_tree
    else
        status=$(status_of "$wt") || die "git status failed in $wt"
        if [ -n "$status" ]; then
            echo "gate-trees: $wt is dirty; no recorded tree describes it" >&2
            return 1
        fi
        tree=$(tree_of "$wt") || die "cannot read HEAD^{tree} in $wt"
    fi
    change=$(change_of "$wt" "$change")
    logfile="$h/$change/gate-trees.log"
    [ -f "$logfile" ] || return 1
    local line found='' failed='' odd='' g ts
    local -A voided=()
    while IFS= read -r line; do
        case "$line" in *$'\t'"tree=$tree"$'\t'*) ;; *) continue ;; esac
        case "$line" in *$'\t'"result=void"$'\t'*) ;; *) continue ;; esac
        ts=$(field_of "$line" voids) || continue
        voided["$ts|${line##*$'\t'gate=}"]=1
    done <"$logfile"
    while IFS= read -r line; do
        case "$line" in *$'\t'"tree=$tree"$'\t'*) ;; *) continue ;; esac
        for g in "${gates[@]}"; do
            [ "${line##*$'\t'gate=}" = "$g" ] || continue
            case "$line" in
                *$'\t'"result=pass"$'\t'*) found=$line ;;
                *$'\t'"result=fail"$'\t'*)
                    if [ -n "${voided["${line%%$'\t'*}|$g"]:-}" ]; then
                        echo "gate-trees: note: a voided fail on this tree is ignored: $line" >&2
                    else
                        failed=$line
                    fi ;;
                *$'\t'"result=interrupted"$'\t'*)
                    echo "gate-trees: note: an interrupted run (a timeout or a signal) is recorded on this tree; a hang or a kill that passed on a retry is a flake to investigate: $line" >&2 ;;
                *$'\t'"result=void"$'\t'*) ;;
                *) odd=$line ;;
            esac
        done
    done <"$logfile"
    if [ -n "$odd" ]; then
        echo "gate-trees: a line with an unknown result names this tree, so no record is reused: $odd" >&2
        return 1
    fi
    if [ -n "$failed" ]; then
        echo "gate-trees: a failed run is recorded on this tree, so no record is reused (a pass and a fail on one tree is a flake): $failed" >&2
        return 1
    fi
    [ -n "$found" ] || return 1
    printf '%s\n' "$found"
}

cmd_void() {
    local wt=${1:-}
    [ -n "$wt" ] && [ -d "$wt" ] || die "void needs an existing checkout path"
    shift
    local change='' at='' gate='' evidence='' reason=''
    while [ $# -gt 0 ]; do
        case "$1" in
            --change) change=${2:?--change needs a value}; shift 2 ;;
            --line) at=${2:?--line needs the fail line timestamp}; shift 2 ;;
            --gate) gate=${2:?--gate needs a value}; shift 2 ;;
            --log) evidence=${2:?--log needs a path}; shift 2 ;;
            --reason) reason=${2:?--reason needs a text}; shift 2 ;;
            *) die "unknown void option $1" ;;
        esac
    done
    [ -n "$change" ] || die "void needs --change"
    [ -n "$at" ] && [ -n "$gate" ] || die "void needs --line and --gate"
    [ -n "$reason" ] || die "void needs --reason"
    [ -n "$evidence" ] && [ -s "$evidence" ] || die "void needs --log naming a non-empty log that shows the gate never ran"
    for v in --change:"$change" --line:"$at" --gate:"$gate" --log:"$evidence" --reason:"$reason"; do
        one_line "${v%%:*}" "${v#*:}"
    done
    local h logfile line matches=0 target=''
    h=$(handoff_dir "$wt")
    logfile="$h/$change/gate-trees.log"
    [ -f "$logfile" ] || die "no gate-trees.log for $change"
    while IFS= read -r line; do
        [ "${line%%$'\t'*}" = "$at" ] || continue
        [ "${line##*$'\t'gate=}" = "$gate" ] || continue
        case "$line" in *$'\t'"result=fail"$'\t'*) matches=$((matches + 1)); target=$line ;; esac
    done <"$logfile"
    [ "$matches" -eq 1 ] || die "the timestamp '$at' and gate '$gate' match $matches fail lines; a void names exactly one"
    local tree
    tree=$(field_of "$target" tree) || die "the fail line has no tree field"
    while IFS= read -r line; do
        case "$line" in *$'\t'"result=void"$'\t'*) ;; *) continue ;; esac
        [ "$(field_of "$line" voids)" = "$at" ] && [ "${line##*$'\t'gate=}" = "$gate" ] \
            && [ "$(field_of "$line" tree)" = "$tree" ] && die "that fail line is already voided: $line"
    done <"$logfile"
    printf '%s\ttree=%s\tresult=void\tlog=%s\tvoids=%s\treason=%s\tgate=%s\n' \
        "$(stamp)" "$tree" "$evidence" "$at" "$reason" "$gate" >>"$logfile"
    echo "gate-trees: voided the fail of '$gate' at $at on tree $tree"
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

    # A failing run on a clean, unchanged tree is recorded as a fail, and a
    # fail of any named gate on that tree voids the reuse: a gate that passed
    # and failed on one tree is a flake (preexisting-blockers.md).
    rc=0; cmd_record "$wt" --gate 'make test' -- sh -c 'exit 3' >/dev/null 2>&1 || rc=$?
    [ "$rc" = 3 ] || self_test_fail "a failing command returned $rc, expected 3"
    [ "$(count)" -eq 2 ] || self_test_fail "a failing run on a clean tree recorded $(count) lines, expected 2"
    case "$(tail -n1 "$log")" in *"result=fail"*"gate=make test") ;; *) self_test_fail "the fail line reads '$(tail -n1 "$log")'" ;; esac
    rc=0; cmd_lookup "$wt" --gate 'make test' >/dev/null 2>&1 || rc=$?
    [ "$rc" = 1 ] || self_test_fail "lookup answered for a gate with only a fail record"
    cmd_lookup "$wt" --gate 'make check' >/dev/null || self_test_fail "a fail of another gate voided 'make check'"
    rc=0; cmd_lookup "$wt" --gate 'make check' --gate 'make test' >/dev/null 2>&1 || rc=$?
    [ "$rc" = 1 ] || self_test_fail "a fail of one named gate did not void the covering lookup"
    # A gate that never ran (busy 75, off its CPUs 78, not executable 126,
    # not found 127) records nothing. One that started and did not finish
    # (timed out 124, a signal 128+) records result=interrupted: lookup
    # ignores it, but names it on stderr, so a hang or a kill leaves a trace.
    local code n
    n=$(count)
    for code in 75 78 126 127; do
        rc=0; cmd_record "$wt" --gate 'make check' -- sh -c "exit $code" >/dev/null 2>&1 || rc=$?
        [ "$rc" = "$code" ] || self_test_fail "a run that never started returned $rc, expected $code"
        [ "$(count)" -eq "$n" ] || self_test_fail "a run that never started (exit $code) recorded a line"
    done
    rc=0; cmd_record "$wt" --gate 'make check' -- "$tmp/no-such-command" >/dev/null 2>&1 || rc=$?
    [ "$rc" = 127 ] || self_test_fail "a missing command returned $rc, expected 127"
    [ "$(count)" -eq "$n" ] || self_test_fail "a command that could not start recorded a line"
    for code in 124 137 143; do
        rc=0; cmd_record "$wt" --gate 'make check' -- sh -c "exit $code" >/dev/null 2>&1 || rc=$?
        [ "$rc" = "$code" ] || self_test_fail "an interrupted run returned $rc, expected $code"
        n=$((n + 1))
        [ "$(count)" -eq "$n" ] || self_test_fail "an interrupted run (exit $code) recorded $(count) lines, expected $n"
        case "$(tail -n1 "$log")" in *$'\t'"result=interrupted"$'\t'*"gate=make check") ;; *) self_test_fail "the interrupted line reads '$(tail -n1 "$log")'" ;; esac
    done
    out=$(cmd_lookup "$wt" --gate 'make check' 2>"$tmp/err") || self_test_fail "an interrupted run voided 'make check'"
    grep -q 'interrupted run' "$tmp/err" || self_test_fail "lookup did not name the interrupted runs on stderr"
    cmd_record "$wt" --gate 'make test' -- true >/dev/null 2>&1 || self_test_fail "a pass after a fail returned non-zero"
    n=$((n + 1))
    [ "$(count)" -eq "$n" ] || self_test_fail "a pass after a fail recorded $(count) lines, expected $n"
    rc=0; cmd_lookup "$wt" --gate 'make test' >/dev/null 2>&1 || rc=$?
    [ "$rc" = 1 ] || self_test_fail "a pass after a fail on the same tree hid the flake"

    # The coordinator's void: append-only, one named fail line, with the log
    # that shows the gate never ran; lookup then ignores that line only.
    local fail_ts evidence="$tmp/refusal.log"
    fail_ts=$(awk -F'\t' '$3 == "result=fail" && $5 == "gate=make test" { print $1 }' "$log")
    [ "$(printf '%s\n' "$fail_ts" | wc -l)" -eq 1 ] || self_test_fail "expected one make test fail line, found '$fail_ts'"
    : >"$evidence"
    rc=0; ( cmd_void "$wt" --change fixture --line "$fail_ts" --gate 'make test' --log "$evidence" --reason 'usage refusal' ) >/dev/null 2>&1 || rc=$?
    [ "$rc" = 2 ] || self_test_fail "void accepted an empty evidence log (exit $rc)"
    echo 'heavy: usage: --mem none is refused for this recipe' >"$evidence"
    rc=0; ( cmd_void "$wt" --change fixture --line "$fail_ts" --gate 'make test' --log "$tmp/missing.log" --reason 'usage refusal' ) >/dev/null 2>&1 || rc=$?
    [ "$rc" = 2 ] || self_test_fail "void accepted a missing evidence log (exit $rc)"
    rc=0; ( cmd_void "$wt" --change fixture --line "$fail_ts" --gate 'make test' --log "$evidence" ) >/dev/null 2>&1 || rc=$?
    [ "$rc" = 2 ] || self_test_fail "void accepted no reason (exit $rc)"
    rc=0; ( cmd_void "$wt" --change fixture --line "$fail_ts" --gate 'make check' --log "$evidence" --reason 'usage refusal' ) >/dev/null 2>&1 || rc=$?
    [ "$rc" = 2 ] || self_test_fail "void named a gate with no fail line at that time (exit $rc)"
    rc=0; ( cmd_void "$wt" --change fixture --line '2000-01-01T00:00:00+00:00' --gate 'make test' --log "$evidence" --reason 'usage refusal' ) >/dev/null 2>&1 || rc=$?
    [ "$rc" = 2 ] || self_test_fail "void named a timestamp with no fail line (exit $rc)"
    [ "$(count)" -eq "$n" ] || self_test_fail "a refused void wrote a line"
    cmd_void "$wt" --change fixture --line "$fail_ts" --gate 'make test' --log "$evidence" --reason 'usage refusal' >/dev/null \
        || self_test_fail "void of the one fail line failed"
    n=$((n + 1))
    [ "$(count)" -eq "$n" ] || self_test_fail "void wrote $(count) lines, expected $n"
    grep -q "result=fail.*gate=make test" "$log" || self_test_fail "void deleted the fail line"
    case "$(tail -n1 "$log")" in *$'\t'"result=void"$'\t'"log=$evidence"$'\t'"voids=$fail_ts"$'\t'"reason=usage refusal"$'\t'"gate=make test") ;;
        *) self_test_fail "the void line reads '$(tail -n1 "$log")'" ;; esac
    cmd_lookup "$wt" --gate 'make test' >/dev/null 2>&1 || self_test_fail "lookup still refused after the fail was voided"
    rc=0; ( cmd_void "$wt" --change fixture --line "$fail_ts" --gate 'make test' --log "$evidence" --reason 'again' ) >/dev/null 2>&1 || rc=$?
    [ "$rc" = 2 ] || self_test_fail "void voided the same fail line twice (exit $rc)"
    # A later fail on the same tree still blocks: the void named one line.
    rc=0; cmd_record "$wt" --gate 'make test' -- false >/dev/null 2>&1 || rc=$?
    n=$((n + 1))
    rc=0; cmd_lookup "$wt" --gate 'make test' >/dev/null 2>&1 || rc=$?
    [ "$rc" = 1 ] || self_test_fail "a fail after a void did not refuse the reuse"
    # A fail line can only be voided when exactly one line matches.
    awk -F'\t' '$3 == "result=fail" { last = $0 } END { print last }' "$log" >>"$log.dup" && cat "$log.dup" >>"$log" && rm -f "$log.dup"
    n=$((n + 1))
    local dup_ts
    dup_ts=$(tail -n1 "$log" | cut -f1)
    rc=0; ( cmd_void "$wt" --change fixture --line "$dup_ts" --gate 'make test' --log "$evidence" --reason 'ambiguous' ) >/dev/null 2>&1 || rc=$?
    [ "$rc" = 2 ] || self_test_fail "void accepted a timestamp matching two fail lines (exit $rc)"

    # lookup --tree reads a given tree, with no worktree HEAD (a merged change).
    local first_tree
    first_tree=$(tree_of "$wt")
    out=$(cmd_lookup "$tmp" --tree "$first_tree" --change fixture --gate 'make check' 2>/dev/null) \
        || self_test_fail "lookup --tree missed a recorded pass"
    case "$out" in *"tree=$first_tree"*"gate=make check") ;; *) self_test_fail "lookup --tree printed '$out'" ;; esac
    rc=0; ( cmd_lookup "$wt" --tree "$first_tree" --gate 'make check' ) >/dev/null 2>&1 || rc=$?
    [ "$rc" = 2 ] || self_test_fail "lookup --tree accepted no --change (exit $rc)"
    rc=0; ( cmd_lookup "$wt" --tree "${first_tree:0:12}" --change fixture --gate 'make check' ) >/dev/null 2>&1 || rc=$?
    [ "$rc" = 2 ] || self_test_fail "lookup --tree accepted an abbreviated hash (exit $rc)"
    rc=0; cmd_lookup "$wt" --tree "$first_tree" --change fixture --gate 'make test' >/dev/null 2>&1 || rc=$?
    [ "$rc" = 1 ] || self_test_fail "lookup --tree hid the recorded fail"

    : >"$wt/dirty"
    cmd_record "$wt" --gate 'make test' -- true >/dev/null 2>&1 || self_test_fail "dirty-before pass returned non-zero"
    rc=0; cmd_record "$wt" --gate 'make test' -- false >/dev/null 2>&1 || rc=$?
    [ "$(count)" -eq "$n" ] || self_test_fail "a dirty-before run recorded a line"
    rc=0; cmd_lookup "$wt" --gate 'make check' >/dev/null 2>&1 || rc=$?
    [ "$rc" = 1 ] || self_test_fail "lookup answered for a dirty worktree"
    cmd_lookup "$wt" --tree "$first_tree" --change fixture --gate 'make check' >/dev/null 2>&1 \
        || self_test_fail "lookup --tree read the checkout's dirty state"
    rm -f "$wt/dirty"

    cmd_record "$wt" --gate 'make test' -- touch "$wt/made" >/dev/null 2>&1 || self_test_fail "dirty-after pass returned non-zero"
    [ "$(count)" -eq "$n" ] || self_test_fail "a dirty-after run recorded a line"
    rm -f "$wt/made"

    # shellcheck disable=SC2016 # the inner sh expands its own $1
    cmd_record "$wt" --gate 'make test' -- sh -c '
        echo x >"$1/f" && git -C "$1" add f &&
        git -C "$1" -c core.hooksPath=/dev/null -c user.name=t -c user.email=t@example.invalid \
            -c commit.gpgsign=false commit -q -m change' sh "$wt" >/dev/null 2>&1 \
        || self_test_fail "tree-changing pass returned non-zero"
    [ "$(count)" -eq "$n" ] || self_test_fail "a tree-changing run recorded a line"
    rc=0; cmd_lookup "$wt" --gate 'make check' >/dev/null 2>&1 || rc=$?
    [ "$rc" = 1 ] || self_test_fail "lookup matched a record of another tree"

    cmd_record "$wt" --gate 'make test' --log-ref /x/test.log -- true >/dev/null 2>&1
    n=$((n + 1))
    out=$(cmd_lookup "$wt" --gate 'make test') || self_test_fail "lookup missed the new tree's record"
    case "$out" in *"log=/x/test.log"*) ;; *) self_test_fail "log reference not recorded: '$out'" ;; esac

    # Every field of a record is one line without tabs.
    rc=0; ( cmd_record "$wt" --log-ref $'/x/a\tb' --gate 'make test' -- true ) >/dev/null 2>&1 || rc=$?
    [ "$rc" = 2 ] || self_test_fail "record accepted --log-ref with a tab (exit $rc)"
    rc=0; ( cmd_record "$wt" --log-ref $'/x/a\nb' --gate 'make test' -- true ) >/dev/null 2>&1 || rc=$?
    [ "$rc" = 2 ] || self_test_fail "record accepted --log-ref with a newline (exit $rc)"
    rc=0; ( cmd_record "$wt" --change $'a\nb' --gate 'make test' -- true ) >/dev/null 2>&1 || rc=$?
    [ "$rc" = 2 ] || self_test_fail "record accepted --change with a newline (exit $rc)"
    rc=0; ( cmd_lookup "$wt" --gate 'make test' --change $'a\tb' ) >/dev/null 2>&1 || rc=$?
    [ "$rc" = 2 ] || self_test_fail "lookup accepted --change with a tab (exit $rc)"
    rc=0; ( cmd_void "$wt" --change fixture --line "$fail_ts" --gate 'make test' --log "$evidence" --reason $'a\tb' ) >/dev/null 2>&1 || rc=$?
    [ "$rc" = 2 ] || self_test_fail "void accepted --reason with a tab (exit $rc)"
    [ "$(count)" -eq "$n" ] || self_test_fail "a refused record wrote a line"

    # A line with a result the helper does not know refuses the reuse.
    printf '%s\ttree=%s\tresult=skipped\tlog=-\tgate=make test\n' "$(date -Is)" "$(tree_of "$wt")" >>"$log"
    rc=0; cmd_lookup "$wt" --gate 'make test' >/dev/null 2>&1 || rc=$?
    [ "$rc" = 1 ] || self_test_fail "lookup reused a tree with an unknown result line"
    sed -i '$d' "$log"

    # A decoy repository's GIT_DIR, as a commit hook exports it, must not redirect the helper.
    local decoy="$tmp/decoy"
    git init -q "$decoy"
    out=$(GIT_DIR="$decoy/.git" GIT_WORK_TREE="$decoy" GIT_INDEX_FILE="$decoy/.git/index" \
        "$0" lookup "$wt" --gate 'make test') || self_test_fail "lookup under a decoy GIT_DIR missed"
    echo "gate-trees self-test: ok (clean pass recorded; a clean fail recorded and voiding reuse of that gate on that tree; dirty-before, dirty-after and tree-changing runs record nothing; runs that never started record nothing, timed-out and killed runs record result=interrupted that lookup ignores but names; the coordinator void appends, names one fail line with its evidence log and never deletes; lookup --tree reads a given tree and needs --change; unknown results refuse; tabs and newlines refused in every field; exact gate match, covering gates, dirty lookup refused; decoy GIT_DIR ignored)"
}

case "${1:-}" in
    record) shift; cmd_record "$@" ;;
    lookup) shift; cmd_lookup "$@" ;;
    void) shift; cmd_void "$@" ;;
    --self-test) self_test ;;
    *) echo "usage: gate-trees.sh record <worktree> --gate G [--change C] [--log-ref P] -- <cmd...> | lookup <worktree> --gate G... [--change C] [--tree T] | void <checkout> --change C --line TS --gate G --log P --reason R | --self-test" >&2; exit 2 ;;
esac
