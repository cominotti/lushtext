#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Run ONE heavy, non-measurement command (cargo build/check/clippy/test, kani,
# mutants, a smoke lane, `git commit` with its Clippy hook) the way the
# shared-machine protocol requires (see "Shared-machine agent work" in
# .agents/rules/build.md):
#
#   scripts/heavy.sh [--mem <cap|none>] -- <command> [args...]
#   scripts/heavy.sh --self-test
#
# 1. Wait, holding nothing, until scripts/with-memory-cap.sh would admit <cap>
#    (default 12G) into the host-wide heavy slice, whose budget is the
#    wrapper's own default; this script never overrides it. Memory comes
#    first so the script never waits for memory while holding LOCK.
# 2. Probe the INTENT lock shared: it blocks while a measurement
#    (scripts/measure.sh) is pending or running, so new heavy work cannot keep
#    a waiting measurement starved.
# 3. Take the LOCK shared for this one command: heavy commands run alongside
#    each other, never during a measurement. If memory became short meanwhile,
#    release LOCK and go back to step 1.
# 4. Run the command as a child, with CARGO_BUILD_JOBS=4 and the CPU affinity
#    this process inherited, inside `scripts/with-memory-cap.sh <cap>`.
#
# Take it per command, never around a multi-hour script: a pending measurement
# then runs between steps.
#
# This script, not the command, holds the locks (on fds 6 and 7: fd 9 is
# with-memory-cap.sh's admission lock). The command does not inherit them, so
# a daemon it leaves behind (dbus-daemon, mutter, an sccache server) cannot
# keep LOCK held after it exits and freeze a pending measurement. SIGINT,
# SIGTERM and SIGHUP sent to this script are forwarded to the command, and its
# exit status is returned.
#
# A heavy.sh started by the command (directly or inside a measurement) runs in
# place instead of taking the locks again, which would deadlock behind a
# pending measurement. That requires PROOF that an enclosing helper holds
# LOCK: the exported marker names the holder's PID, which must be an ancestor
# whose fd 7 is still LOCK (scripts/agent-locks.sh).

set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=scripts/agent-locks.sh
source "$here/agent-locks.sh"

usage() {
    echo "usage: $0 [--mem <cap|none>] -- <command> [args...]" >&2
    echo "       $0 --self-test" >&2
    exit 2
}

fail() {
    echo "heavy self-test: $*" >&2
    return 1
}

self_test() {
    local me=$here/heavy.sh dir status out pid m
    dir=$(mktemp -d "${TMPDIR:-/tmp}/heavy-self-test.XXXXXX")
    # shellcheck disable=SC2064 # expand now: the trap must remove this dir
    trap "rm -rf '$dir'" EXIT
    export LUSHTEXT_AGENT_LOCK_DIR=$dir
    # Whatever started the self-test (a `heavy.sh -- git commit` whose hook
    # runs check-policy, a capped `make`) must not leak its markers into the
    # cases below.
    unset LUSHTEXT_AGENT_LOCK_HELD LUSHTEXT_MEMORY_CAP_KIB LUSHTEXT_ADMISSION_NOWAIT
    local intent=$dir/responsiveness-measure.intent lock=$dir/responsiveness-measure.lock

    status=0; "$me" --mem none -- sh -c 'exit 7' || status=$?
    [ "$status" -eq 7 ] || fail "exit status 7 came back as $status" || return

    status=0; "$me" true 2>/dev/null || status=$?
    [ "$status" -eq 2 ] || fail "a command without '--' was accepted (status $status)" || return

    # shellcheck disable=SC2016 # expanded by the inner shell
    out=$("$me" --mem none -- sh -c 'echo "$CARGO_BUILD_JOBS"')
    [ "$out" = 4 ] || fail "CARGO_BUILD_JOBS was '$out', expected 4" || return

    # LOCK is held shared while the command runs (an exclusive try fails), by
    # this script: the command holds neither lock descriptor.
    # shellcheck disable=SC2016 # expanded by the inner shell
    out=$("$me" --mem none -- sh -c 'flock -n -x "$0" true && echo free; for f in /proc/self/fd/*; do [ "$f" -ef "$0" ] && echo inherited; done; true' "$lock")
    [ -z "$out" ] || fail "LOCK during the command: '$out' (expected held, and not by the command)" || return

    # A signal sent to the script is forwarded to the command.
    "$me" --mem none -- sleep 30 &
    pid=$!
    sleep 0.3
    kill -TERM "$pid"
    status=0; wait "$pid" || status=$?
    [ "$status" -eq 143 ] || fail "SIGTERM gave status $status, expected 143" || return
    flock -n -x "$lock" true || fail "LOCK outlived a signalled command" || return

    # A pending measurement (INTENT held exclusively) blocks new starts.
    exec 8>>"$intent"
    flock -x 8
    status=0; timeout 1 "$me" --mem none -- true 8>&- || status=$?
    exec 8>&-
    [ "$status" -eq 124 ] || fail "started while INTENT was held exclusively (status $status)" || return

    # A nested heavy.sh runs in place: with a measurement pending, taking the
    # locks again would deadlock (outer holds LOCK, measurement holds INTENT).
    # shellcheck disable=SC2016 # expanded by the inner shell
    "$me" --mem none -- sh -c 'sleep 0.6; "$0" --mem none -- true' "$me" &
    pid=$!
    sleep 0.2
    "$here/measure.sh" --log "$dir/log" -- true &
    m=$!
    status=0; timeout 15 tail --pid="$pid" -f /dev/null || status=$?
    if [ "$status" -ne 0 ]; then
        kill "$pid" "$m" 2>/dev/null || true
        fail "a nested heavy.sh deadlocked behind a pending measurement"
        return
    fi
    wait "$pid" "$m"

    # Stale markers never skip the protocol. A measurement (here a sibling
    # holding LOCK exclusively on its fd 7) is running; a process carrying a
    # leftover marker must wait for it, whether the marker names no holder,
    # a holder that is not its ancestor, or no live process.
    # shellcheck disable=SC2016 # expanded by the inner shell
    sh -c 'exec 7>>"$0"; flock -x 7; exec sleep 60' "$lock" &
    local holder=$!
    for _ in $(seq 50); do
        flock -n -s "$lock" true || break
        sleep 0.05
    done
    local marker
    for marker in exclusive shared "exclusive:$holder" "shared:$holder" "exclusive:999999999" "exclusive:$$"; do
        status=0; LUSHTEXT_AGENT_LOCK_HELD=$marker timeout 1 "$me" --mem none -- true 2>/dev/null || status=$?
        [ "$status" -eq 124 ] || { kill "$holder" 2>/dev/null; fail "stale marker '$marker' ran heavy work during a measurement (status $status)"; return; }
    done
    kill "$holder"
    wait "$holder" 2>/dev/null || true

    # A daemon the command leaves behind does not keep LOCK held: a
    # measurement right after it gets the locks at once.
    # shellcheck disable=SC2016 # expanded by the inner shell
    "$me" --mem none -- sh -c 'setsid sleep 30 </dev/null >/dev/null 2>&1 & echo $! >"$0"' "$dir/daemon"
    status=0; timeout 5 "$here/measure.sh" --log "$dir/log" -- true || status=$?
    kill "$(cat "$dir/daemon")" 2>/dev/null || true
    [ "$status" -eq 0 ] || fail "a leftover daemon kept LOCK held (measurement status $status)" || return

    # The documented commit path: `heavy.sh -- git commit` runs the hook,
    # whose check-policy runs these self-tests with the outer helper's
    # marker set, against the outer helper's real lock.
    status=0; "$me" --mem none -- "$here/measure.sh" --self-test >/dev/null || status=$?
    [ "$status" -eq 0 ] || fail "measure.sh --self-test failed inside heavy.sh (status $status)" || return

    # The default path: a real capped scope (64M skips admission). Arguments
    # pass byte for byte, the command runs inside the heavy slice on exactly
    # the one CPU it was given, LOCK is held across `systemd-run --scope`, and
    # fd 9 (the wrapper's admission lock) is not a lock of the protocol.
    local scope=skipped
    if command -v systemd-run >/dev/null 2>&1 \
        && systemd-run --user --scope --quiet -p MemoryMax=1M true >/dev/null 2>&1; then
        local cpu
        cpu=$(taskset -pc $$ | sed 's/.*: //; s/[-,].*//')
        # shellcheck disable=SC2016 # expanded by the inner shell; literal dollars are the point
        out=$(taskset -c "$cpu" "$me" --mem 64M -- sh -c '
            printf "%s|" "$1" "$2" "$3"
            grep -c "/lushtext-heavy.slice/" /proc/self/cgroup | tr "\n" "|"
            sed -n "s/^Cpus_allowed_list:[[:space:]]*//p" /proc/self/status | tr "\n" "|"
            flock -n -x "$0" true && printf free || printf held
            [ /proc/self/fd/9 -ef "$0" ] && printf "|fd9"
            true' "$lock" 'a$$b' '$0' '${HOME}' 2>/dev/null)
        # shellcheck disable=SC2016 # literal dollars are the point
        [ "$out" = "a\$\$b|\$0|\${HOME}|1|$cpu|held" ] \
            || fail "capped run reported '$out', expected 'a\$\$b|\$0|\${HOME}|1|$cpu|held'" || return
        scope="args unexpanded, heavy slice, CPU $cpu kept, LOCK held across the scope"
    fi

    echo "heavy self-test: ok (exit status and SIGTERM propagate; CARGO_BUILD_JOBS=4; LOCK held shared by the helper, not the command; a pending measurement blocks new starts; nested calls run in place; stale markers do not; a leftover daemon does not hold LOCK; measure.sh self-test passes inside heavy.sh; capped scope: $scope)"
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
export CARGO_BUILD_JOBS=4

if mode=$(agent_lock_inherited); then
    # Nothing to hold here: the enclosing helper keeps LOCK until this exits.
    # A cap is admitted inside it unless the enclosing command is capped too
    # (then the wrapper runs in place).
    echo "heavy: LOCK already held $mode by an enclosing helper; running in place: $*" >&2
    exec "$here/with-memory-cap.sh" "$cap" "$@"
fi

while :; do
    "$here/with-memory-cap.sh" --wait-admissible "$cap"
    exec 6>>"$AGENT_LOCK_INTENT"
    flock -s 6
    exec 6>&-
    exec 7>>"$AGENT_LOCK_MEASURE"
    flock -s 7
    # Memory may have gone while waiting for the locks: never wait for it
    # while holding LOCK.
    status=0
    LUSHTEXT_ADMISSION_NOWAIT=1 "$here/with-memory-cap.sh" --wait-admissible "$cap" 7>&- || status=$?
    [ "$status" -ne 0 ] || break
    exec 7>&-
    [ "$status" -eq 75 ] || exit "$status"
done

agent_trap_signals
LUSHTEXT_AGENT_LOCK_HELD=$(agent_lock_marker shared)
export LUSHTEXT_AGENT_LOCK_HELD
agent_run_child "$here/with-memory-cap.sh" "$cap" "$@"
exec 7>&-
exit "$agent_child_status"
