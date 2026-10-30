#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Run ONE heavy, non-measurement command (cargo build/check/clippy/test, kani,
# mutants, a smoke lane, `git commit` with its Clippy hook) the way the
# shared-machine protocol requires (see "Shared-machine agent work" in
# .agents/rules/build.md):
#
#   scripts/heavy.sh [--mem <cap>] -- <command> [args...]
#   scripts/heavy.sh --self-test
#
# 1. Probe the INTENT lock shared: it blocks while a measurement
#    (scripts/measure.sh) is pending or running, so new heavy work cannot keep
#    a waiting measurement starved.
# 2. Take the LOCK shared for this one command: heavy commands run alongside
#    each other, never during a measurement.
# 3. Run it with CARGO_BUILD_JOBS=4, pinned to the CPU affinity this process
#    inherited (never widened), inside scripts/with-memory-cap.sh <cap>
#    (default 12G, admitted into the host-wide heavy slice, whose budget is the
#    wrapper's own default; this script never overrides it).
#
# Take it per command, never around a multi-hour script: a pending measurement
# then runs between steps. Every stage `exec`s, so the command keeps this
# script's PID: its exit status and any signal sent to that PID reach it
# directly. The lock descriptors are inherited by the command, so the shared
# lock is held exactly as long as it (and anything it left running) lives.

set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=scripts/agent-locks.sh
source "$here/agent-locks.sh"

usage() {
    echo "usage: $0 [--mem <cap|none>] -- <command> [args...]" >&2
    echo "       $0 --self-test" >&2
    exit 2
}

self_test() {
    local me=$here/heavy.sh dir status out
    dir=$(mktemp -d "${TMPDIR:-/tmp}/heavy-self-test.XXXXXX")
    # shellcheck disable=SC2064 # expand now: the trap must remove this dir
    trap "rm -rf '$dir'" EXIT
    export LUSHTEXT_AGENT_LOCK_DIR=$dir

    status=0; "$me" --mem none -- sh -c 'exit 7' || status=$?
    [ "$status" -eq 7 ] || { echo "heavy self-test: exit status 7 came back as $status" >&2; return 1; }

    status=0; "$me" true 2>/dev/null || status=$?
    [ "$status" -eq 2 ] || { echo "heavy self-test: a command without '--' was accepted (status $status)" >&2; return 1; }

    # shellcheck disable=SC2016 # expanded by the inner shell
    out=$("$me" --mem none -- sh -c 'echo "$CARGO_BUILD_JOBS"')
    [ "$out" = 4 ] || { echo "heavy self-test: CARGO_BUILD_JOBS was '$out', expected 4" >&2; return 1; }

    local outer inner
    outer=$(taskset -pc $$ | sed 's/.*: //')
    inner=$("$me" --mem none -- sh -c 'taskset -pc $$' | sed 's/.*: //')
    [ "$inner" = "$outer" ] || { echo "heavy self-test: affinity '$inner' differs from the caller's '$outer'" >&2; return 1; }

    # The command holds LOCK shared while it runs: an exclusive try fails.
    # shellcheck disable=SC2016 # expanded by the inner shell
    "$me" --mem none -- sh -c 'flock -n -x "$0" true' "$dir/responsiveness-measure.lock" \
        && { echo "heavy self-test: LOCK was not held shared during the command" >&2; return 1; }

    # The command keeps this script's PID, so a signal reaches it.
    local pid
    "$me" --mem none -- sleep 30 &
    pid=$!
    sleep 0.3
    kill -TERM "$pid"
    status=0; wait "$pid" || status=$?
    [ "$status" -eq 143 ] || { echo "heavy self-test: SIGTERM gave status $status, expected 143" >&2; return 1; }

    # A pending measurement (INTENT held exclusively) blocks new starts.
    exec 8>>"$dir/responsiveness-measure.intent"
    flock -x 8
    status=0; timeout 1 "$me" --mem none -- true 8>&- || status=$?
    exec 8>&-
    [ "$status" -eq 124 ] || { echo "heavy self-test: started while INTENT was held exclusively (status $status)" >&2; return 1; }

    # A nested heavy.sh runs in place: with a measurement pending, taking the
    # locks again would deadlock (outer holds LOCK, measurement holds INTENT).
    local m
    # shellcheck disable=SC2016 # expanded by the inner shell
    "$me" --mem none -- sh -c 'sleep 0.6; "$0" --mem none -- true' "$me" &
    pid=$!
    sleep 0.2
    "$here/measure.sh" --log "$dir/log" -- true &
    m=$!
    status=0; timeout 15 tail --pid="$pid" -f /dev/null || status=$?
    if [ "$status" -ne 0 ]; then
        kill "$pid" "$m" 2>/dev/null || true
        echo "heavy self-test: a nested heavy.sh deadlocked behind a pending measurement" >&2
        return 1
    fi
    wait "$pid" "$m"

    # A stale variable with nobody holding LOCK does not skip the protocol.
    exec 8>>"$dir/responsiveness-measure.intent"
    flock -x 8
    status=0; LUSHTEXT_AGENT_LOCK_HELD=shared timeout 1 "$me" --mem none -- true 8>&- || status=$?
    exec 8>&-
    [ "$status" -eq 124 ] || { echo "heavy self-test: a stale LUSHTEXT_AGENT_LOCK_HELD skipped the locks (status $status)" >&2; return 1; }

    echo "heavy self-test: ok (exit status and SIGTERM propagate; CARGO_BUILD_JOBS=4; affinity inherited; LOCK shared while running; a pending measurement blocks new starts; nested calls run in place, a stale marker does not)"
}

if [ "${1:-}" = "--self-test" ]; then
    self_test
    exit
fi

cap=12G
while [ "$#" -gt 0 ]; do
    case "$1" in
        --mem)
            [ "$#" -ge 2 ] || usage
            cap=$2
            shift 2
            ;;
        --)
            shift
            break
            ;;
        *) usage ;;
    esac
done
[ "$#" -ge 1 ] || usage

agent_lock_paths

if agent_lock_inherited; then
    echo "heavy: LOCK already held ${LUSHTEXT_AGENT_LOCK_HELD} by an enclosing helper; running in place: $*" >&2
else
    # Probe only: holding INTENT would block a measurement from announcing
    # itself. fd 9 belongs to with-memory-cap.sh (its admission lock), so the
    # locks use 6 and 7; reopening fd 9 there would silently drop a lock held
    # on it.
    exec 6>>"$AGENT_LOCK_INTENT"
    flock -s 6
    exec 6>&-
    exec 7>>"$AGENT_LOCK_MEASURE"
    flock -s 7
    export LUSHTEXT_AGENT_LOCK_HELD=shared
fi

cpus=$(taskset -pc $$ | sed 's/.*: //')
export CARGO_BUILD_JOBS=4
exec taskset -c "$cpus" "$here/with-memory-cap.sh" "$cap" "$@"
