#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Implementation slots for parallel programme changes
# (.agents/rules/parallel-agent-work.md, "Implementation slots and ordering").
#
#   scripts/impl-slots.sh acquire <change> [--timeout S]  wait in the foreground for a slot
#   scripts/impl-slots.sh release <change>                remove that change's slot only
#   scripts/impl-slots.sh cap                             print the cap and the ready units behind it
#   scripts/impl-slots.sh --self-test
#
# The cap is 5, or 3 while MEASURE-QUEUE.md holds at least one ready unit. A
# unit is a `## ` entry, or each `### ` part of an entry that has parts (the
# text of such an entry before its first part is a unit of its own when it
# carries a Status line, so a status there is never ignored). A unit is ready
# when its LAST `Status:` line (with or without the list dash, and with any
# Markdown `**` around the label or the value) starts with READY: queue
# entries keep their history of Status lines, and only the last one says
# where the rounds stand.
#
# acquire retries every LUSHTEXT_IMPL_SLOTS_RETRY seconds (default 30) for at
# most --timeout seconds (default 540, inside one 600 s tool call), and exits
# 0 once the slot exists, 124 at the timeout, 2 when <change> has no worktree
# <main>/.claude/worktrees/<change> (a reviewer that runs the helper with a
# made-up name takes nothing). It counts every directory under
# IMPL-SLOTS/ (coordinator placeholders too), and never evicts a slot held
# above the cap. The slot is created under IMPL-SLOTS/.lock; the queue is read
# without the queue's own lock (a torn read miscounts one attempt at most),
# so no lock is ever taken while another is held.
#
# The handoff directory is the main checkout's .claude/handoffs, resolved from
# git's common dir; LUSHTEXT_HANDOFF_DIR overrides it (the self-test uses a
# temporary one and touches no real queue or slot).

set -euo pipefail
export LC_ALL=C

DEFAULT_CAP=5
READY_CAP=3

handoff_dir() {
    if [ -n "${LUSHTEXT_HANDOFF_DIR:-}" ]; then
        printf '%s\n' "$LUSHTEXT_HANDOFF_DIR"
        return
    fi
    local common
    common=$(git rev-parse --path-format=absolute --git-common-dir) || {
        echo "impl-slots: not inside a git checkout and LUSHTEXT_HANDOFF_DIR is unset" >&2
        exit 2
    }
    printf '%s/.claude/handoffs\n' "$(dirname "$common")"
}

# Print one line per ready unit ("<heading>\t<last status>") of queue $1.
ready_units() {
    local queue=$1
    [ -f "$queue" ] || return 0
    awk '
        function flush() {
            if (unit != "" && last ~ /^READY/) printf "%s\t%s\n", unit, last
            unit = ""; last = ""
        }
        /^## / { flush(); entry = substr($0, 4); unit = entry; next }
        /^### / { flush(); unit = entry " / " substr($0, 5); next }
        /^[[:space:]]*(-[[:space:]]+)?(\*\*)?Status:/ {
            if (unit == "") next
            s = $0
            gsub(/\*/, "", s)
            sub(/^[[:space:]]*(-[[:space:]]+)?Status:[[:space:]]*/, "", s)
            last = s
            next
        }
        END { flush() }
    ' "$queue"
}

current_cap() {
    local queue=$1
    if [ -n "$(ready_units "$queue")" ]; then
        echo "$READY_CAP"
    else
        echo "$DEFAULT_CAP"
    fi
}

# One acquisition attempt: 0 when the slot exists afterwards, 1 when every slot is taken.
try_acquire() {
    local dir=$1 change=$2 cap=$3
    mkdir -p "$dir"
    # shellcheck disable=SC2016 # the inner sh expands its own positional parameters
    flock -w 10 "$dir/.lock" sh -c '
        [ -d "$0/$1" ] && exit 0
        [ "$(find "$0" -mindepth 1 -maxdepth 1 -type d | wc -l)" -lt "$2" ] || exit 1
        mkdir "$0/$1" && date -Is >"$0/$1/since"' "$dir" "$change" "$cap"
}

valid_change() {
    case "$1" in
        ''|*/*|.*) echo "impl-slots: invalid change name '$1'" >&2; exit 2 ;;
    esac
}

cmd_acquire() {
    local change=${1:-} timeout=540
    shift || true
    valid_change "$change"
    while [ $# -gt 0 ]; do
        case "$1" in
            --timeout) timeout=${2:?--timeout needs seconds}; shift 2 ;;
            *) echo "impl-slots: unknown acquire option $1" >&2; exit 2 ;;
        esac
    done
    local h retry start cap
    h=$(handoff_dir)
    if [ ! -d "$(dirname "$h")/worktrees/$change" ]; then
        echo "impl-slots: no change worktree $(dirname "$h")/worktrees/$change; a slot belongs to a change with a worktree" >&2
        exit 2
    fi
    retry=${LUSHTEXT_IMPL_SLOTS_RETRY:-30}
    start=$(date +%s)
    while :; do
        cap=$(current_cap "$h/MEASURE-QUEUE.md")
        if try_acquire "$h/IMPL-SLOTS" "$change" "$cap"; then
            echo "impl-slots: $change holds a slot (cap $cap)"
            return 0
        fi
        if [ $(( $(date +%s) - start )) -ge "$timeout" ]; then
            echo "impl-slots: no slot for $change within ${timeout}s (cap $cap); repeat the call" >&2
            return 124
        fi
        echo "impl-slots: waiting for a slot for $change (cap $cap, all taken)" >&2
        sleep "$retry"
    done
}

cmd_release() {
    local change=${1:-}
    valid_change "$change"
    local h
    h=$(handoff_dir)
    rm -rf "${h:?}/IMPL-SLOTS/${change:?}"
}

cmd_cap() {
    local h queue units
    h=$(handoff_dir)
    queue="$h/MEASURE-QUEUE.md"
    units=$(ready_units "$queue")
    echo "cap $(current_cap "$queue")"
    if [ -n "$units" ]; then
        printf '%s\n' "$units" | sed 's/^/ready: /'
    fi
}

self_test_fail() {
    echo "impl-slots self-test FAILED: $*" >&2
    exit 1
}

self_test() {
    SELF_TEST_TMP=$(mktemp -d)
    trap 'rm -rf "$SELF_TEST_TMP"' EXIT INT TERM
    local tmp=$SELF_TEST_TMP
    export LUSHTEXT_HANDOFF_DIR="$tmp/h"
    mkdir -p "$LUSHTEXT_HANDOFF_DIR"
    local q="$LUSHTEXT_HANDOFF_DIR/MEASURE-QUEUE.md"
    expect_cap() {
        local want=$1 label=$2 got
        got=$(current_cap "$q")
        [ "$got" = "$want" ] || self_test_fail "$label: cap $got, expected $want"
    }

    : >"$q"; expect_cap 5 "no entries"
    rm -f "$q"; expect_cap 5 "no queue file"

    printf '## a\n- Kind: VERDICT\n- Status: READY\n' >"$q"; expect_cap 3 "last status READY"
    printf '## a\n- Status: READY 1a2b3c4d\n' >"$q"; expect_cap 3 "stage-queued verdict READY <sha>"
    printf '## a\n- Kind: SELECTION ROUNDS (branch only)\n- Status: READY 5855754a (binary x)\n' >"$q"
    expect_cap 3 "selection rounds READY <sha>"
    printf '## a\n- Status: PENDING (NOT READY: waits for the final commit)\n' >"$q"
    expect_cap 5 "PENDING (NOT READY) verdict"
    printf '## a\n- Status: READY 1a2b\n- Status: RAN (coordinator)\n' >"$q"; expect_cap 5 "READY then RAN"
    printf '## c\n- Kind: CALIBRATION\n- Status: PENDING (runs while busy)\n' >"$q"; expect_cap 5 "PENDING calibration"
    printf '## a\n- Status: READY 1a2b\n- Status: SUPERSEDED\n## b\n- Status: DONE\n' >"$q"
    expect_cap 5 "earlier READY superseded, later entry DONE"
    printf '## a\n- Status: READY 1a2b\n## b\n- Status: DONE\n' >"$q"
    expect_cap 3 "a READY entry before a DONE entry"
    printf '## e\n- **RUNNABLE**\n### Part A\n- Status: DONE\n### Part B\n- Status: PENDING\n- Status: READY d482\n### Part C\n- Status: DONE MET\n' >"$q"
    expect_cap 3 "part B READY, part C DONE"
    printf '## e\n### Part A\n- Status: READY x\n- Status: DONE\n### Part B\n- Status: DONE\n' >"$q"
    expect_cap 5 "parts all done"
    # shellcheck disable=SC2016 # literal backticks in the fixture
    printf '## a\n- note: run after the `Status:` line below reads READY\n- Status: PENDING\n' >"$q"
    expect_cap 5 "Status mentioned in prose"
    printf '  - Status: READY stray\n## a\n- Status: DONE\n' >"$q"; expect_cap 5 "status before any entry"
    # "Measurements" writes the line as `Status: READY <sha>`; a line without
    # the list dash is still a status line, or the cap would fail open.
    printf '## a\nStatus: READY 1a2b\n' >"$q"; expect_cap 3 "a Status line without the list dash"
    printf '## a\n- Status: READY 1a2b\nStatus: DONE\n' >"$q"; expect_cap 5 "a dashless DONE after READY"
    # Markdown emphasis around the label or the value is still a status line.
    printf '## a\n- **Status:** READY 1a2b\n' >"$q"; expect_cap 3 "a bold Status label"
    printf '## a\n- Status: **READY** 1a2b\n' >"$q"; expect_cap 3 "a bold READY value"
    printf '## a\n**Status: READY 1a2b**\n' >"$q"; expect_cap 3 "a bold Status line"
    printf '## a\n- **Status:** READY 1a2b\n- **Status:** DONE\n' >"$q"; expect_cap 5 "bold READY then bold DONE"

    # The live queue's heading and Status structure on 2026-11-18 (stage 3):
    # the D25 entry and its amendment are two ready units of one campaign, so
    # the cap is 3; the other entries are done, superseded, moved or pending.
    cat >"$q" <<'EOF'
## r — D3 rounds
- Status: DONE (ran)
## s — verdict campaign
- **NOT RUNNABLE YET.** Run only after the stage replaces the `Status:` line below with `READY <sha>`.
- Status: PENDING (NOT READY: waits for the final commit)
## s: attribution
- Kind: ATTRIBUTION
- Status: DONE (coordinator)
## s: constants round
- Status: SUPERSEDED (design R11)
## t: timing verdicts
- **RUNNABLE: part A is DONE; part C is READY again.**
### Part A: chunk size
- Status: DONE (256 KiB)
### Part B: wall time
- Status: DONE MET
### Part C: responsiveness cases
- Status: DONE MET (8/8)
## t: attribution profiles
- Status: PENDING
- Status: MOVED (not owed)
## s: R11 design attribution
- Status: RAN (coordinator)
- Status: CONSUMED, read and done
## s: D25 gates G1, G1b, G1c, G2 and G4
- Kind: SELECTION ROUNDS (branch only)
- Status: READY 5855754a (binary rev6/bin/widget-attr-d25)
## Amendment to "D25 gates"
- Status: READY 5855754a (amended)
## Calibration: G0 rounds under outside load
- Kind: CALIBRATION
- Status: PENDING (runs inside queue10.sh while the host is busy)
- Amendment refinement:
  - A G1 round is kept when it exits 0.
EOF
    expect_cap 3 "live queue structure"
    [ "$(ready_units "$q" | wc -l)" -eq 2 ] || self_test_fail "live queue: expected 2 ready units, got $(ready_units "$q" | wc -l)"
    local printed
    printed=$(cmd_cap)
    printf '%s\n' "$printed" | grep -qx 'cap 3' || self_test_fail "cap did not print 'cap 3'"
    [ "$(printf '%s\n' "$printed" | grep -c '^ready: ')" -eq 2 ] || self_test_fail "cap did not list the two ready units"

    # Acquire, wait, timeout, release on a temporary slot directory.
    : >"$q"
    export LUSHTEXT_IMPL_SLOTS_RETRY=0.2
    local s="$LUSHTEXT_HANDOFF_DIR/IMPL-SLOTS" i rc
    # Only a change with a worktree takes a slot: a reviewer that runs the
    # helper with a made-up name (a code-review fork did) takes nothing.
    for i in 1 2 3 4 5 6 7; do mkdir -p "$tmp/worktrees/c$i"; done
    rc=0; ( cmd_acquire ghost --timeout 1 ) >/dev/null 2>&1 || rc=$?
    [ "$rc" = 2 ] || self_test_fail "acquire for a change with no worktree exited $rc, expected 2"
    [ ! -d "$s/ghost" ] || self_test_fail "a change with no worktree got a slot"
    for i in 1 2 3 4 5; do
        cmd_acquire "c$i" --timeout 1 >/dev/null 2>&1 || self_test_fail "acquire c$i under cap 5"
    done
    [ -s "$s/c1/since" ] || self_test_fail "slot c1 has no since file"
    cmd_acquire c1 --timeout 1 >/dev/null 2>&1 || self_test_fail "re-acquire of a held slot"
    rc=0; cmd_acquire c6 --timeout 1 >/dev/null 2>&1 || rc=$?
    [ "$rc" = 124 ] || self_test_fail "sixth acquire exited $rc, expected 124"
    [ ! -d "$s/c6" ] || self_test_fail "c6 got a slot above the cap"
    ( sleep 0.5; cmd_release c2 ) &
    cmd_acquire c6 --timeout 5 >/dev/null 2>&1 || self_test_fail "acquire did not take the slot a release freed"
    wait
    [ -d "$s/c6" ] && [ ! -d "$s/c2" ] || self_test_fail "release or wait left the wrong slots"
    printf '## a\n- Status: READY 1a2b\n' >"$q"
    cmd_release c3; cmd_release c4
    rc=0; cmd_acquire c7 --timeout 1 >/dev/null 2>&1 || rc=$?
    [ "$rc" = 124 ] || self_test_fail "acquire at 3 held with a READY unit exited $rc, expected 124"
    [ -d "$s/c5" ] || self_test_fail "a slot above the cap was evicted"
    cmd_release c5
    cmd_acquire c7 --timeout 1 >/dev/null 2>&1 || self_test_fail "acquire below the READY cap"
    cmd_release c7
    [ ! -d "$s/c7" ] && [ -d "$s/c1" ] || self_test_fail "release removed the wrong slot"
    rc=0; ( cmd_release "../x" ) 2>/dev/null || rc=$?
    [ "$rc" = 2 ] || self_test_fail "release accepted a path"
    echo "impl-slots self-test: ok (caps from READY units, entries and ### parts, last status only, live queue structure; acquire, wait, 124 timeout, release, no eviction)"
}

case "${1:-}" in
    acquire) shift; cmd_acquire "$@" ;;
    release) shift; cmd_release "$@" ;;
    cap) cmd_cap ;;
    --self-test) self_test ;;
    *) echo "usage: impl-slots.sh acquire <change> [--timeout S] | release <change> | cap | --self-test" >&2; exit 2 ;;
esac
