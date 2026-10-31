#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Run ONE measurement (responsiveness lane, interleaved bench, any timing
# verdict) under the shared-machine protocol's writer preference (see
# "Shared-machine agent work" in .agents/rules/build.md):
#
#   scripts/measure.sh [--label L] [--log FILE] [--lock-timeout S] -- <command> [args...]
#   scripts/measure.sh --self-test
#
# 1. Take INTENT exclusively: from now on scripts/heavy.sh refuses to START new
#    heavy work, so the running work drains instead of overlapping forever.
# 2. Take LOCK exclusively: waits only for heavy commands already running, at
#    most --lock-timeout seconds (default 1800). On expiry it releases INTENT,
#    prints the processes holding LOCK and exits 75, instead of keeping every
#    new heavy start blocked behind a holder that may never finish.
# 3. Record the load average and the top five CPU processes, run the command,
#    record them again (with the exit status and wall time), release both.
#
# The record goes to --log (appended) or, without it, to stderr. The script
# waits for nothing else while it holds the locks: no process polling, no
# sleeping on other work. Test-suite wall-time comparisons are not verdicts;
# run those through scripts/heavy.sh instead.
#
# The locks stay with this script, not the command (the command's copy of the
# descriptors is closed), so a daemon the command leaves behind cannot hold
# them. The command's exit status is propagated; SIGINT, SIGTERM and SIGHUP
# sent to this script are forwarded to it, and the script still releases the
# locks and exits with the command's status. The command runs with
# LUSHTEXT_ADMISSION_NOWAIT=1: a scripts/with-memory-cap.sh scope inside it
# that does not fit at once fails (75) rather than wait for memory while the
# exclusive locks are held.

set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=scripts/agent-locks.sh
source "$here/agent-locks.sh"

usage() {
    echo "usage: $0 [--label L] [--log FILE] [--lock-timeout S] -- <command> [args...]" >&2
    echo "       $0 --self-test" >&2
    exit 2
}

# Instantaneous CPU share: top's second frame covers the last 0.5 s, unlike
# ps, whose %CPU is a lifetime average.
snapshot() {
    local phase=$1
    printf '%s %s label=%s %s loadavg=%s\n' "$(date -Is)" "$phase" "$label" \
        "${2:-}" "$(cut -d' ' -f1-3 /proc/loadavg)"
    if command -v top >/dev/null 2>&1; then
        top -b -n 2 -d 0.5 -o %CPU -w 200 2>/dev/null \
            | awk '/^top -/ { frame++ } frame == 2 && /^ *PID/ { on = 1; print "  " $0; next } on && shown < 5 { print "  " $0; shown++ }'
    else
        ps -eo pid,pcpu,etimes,comm --sort=-pcpu 2>/dev/null | head -6 | sed 's/^/  /' || true
    fi
}

record() {
    if [ -n "$log" ]; then
        snapshot "$@" >>"$log"
    else
        snapshot "$@" >&2
    fi
}

self_test() {
    local me=$here/measure.sh heavy=$here/heavy.sh dir status order
    dir=$(mktemp -d "${TMPDIR:-/tmp}/measure-self-test.XXXXXX")
    # shellcheck disable=SC2064 # expand now: the trap must remove this dir
    trap "rm -rf '$dir'" EXIT
    export LUSHTEXT_AGENT_LOCK_DIR=$dir
    # Whatever started the self-test (a `heavy.sh -- git commit` whose hook
    # runs check-policy) must not leak its markers into the cases below.
    unset LUSHTEXT_AGENT_LOCK_HELD LUSHTEXT_MEMORY_CAP_KIB LUSHTEXT_ADMISSION_NOWAIT
    local intent=$dir/responsiveness-measure.intent lock=$dir/responsiveness-measure.lock

    status=0; "$me" --label t --log "$dir/log" -- sh -c 'exit 5' || status=$?
    [ "$status" -eq 5 ] || { echo "measure self-test: exit status 5 came back as $status" >&2; return 1; }
    if ! grep -q 'before label=t .*loadavg=' "$dir/log" || ! grep -q 'after label=t status=5 ' "$dir/log"; then
        echo "measure self-test: the log lacks the before/after load records" >&2
        cat "$dir/log" >&2
        return 1
    fi

    # Both locks are held exclusively while the command runs.
    # shellcheck disable=SC2016 # expanded by the inner shell
    "$me" --log "$dir/log" -- sh -c 'flock -n -s "$0" true || flock -n -s "$1" true' "$intent" "$lock" \
        && { echo "measure self-test: INTENT or LOCK was not held exclusively" >&2; return 1; }

    # Writer preference: heavy h1 is running (LOCK shared); measurement m is
    # started and must wait for h1; heavy h2, started while m is pending, must
    # wait for m. Expected order: h1 m h2.
    order=$dir/order
    # shellcheck disable=SC2016 # the inner shells expand "$0"
    "$heavy" --mem none -- sh -c 'sleep 1; echo h1 >>"$0"' "$order" &
    local h1=$!
    local i
    for i in $(seq 50); do
        flock -n -x "$lock" true || break
        sleep 0.05
    done
    [ "$i" -lt 50 ] || { echo "measure self-test: h1 never took LOCK" >&2; return 1; }
    # shellcheck disable=SC2016 # expanded by the inner shell
    "$me" --log "$dir/log" -- sh -c 'echo m >>"$0"' "$order" &
    local m=$!
    for i in $(seq 50); do
        flock -n -s "$intent" true || break
        sleep 0.05
    done
    [ "$i" -lt 50 ] || { echo "measure self-test: the measurement never took INTENT" >&2; return 1; }
    # shellcheck disable=SC2016 # expanded by the inner shell
    "$heavy" --mem none -- sh -c 'echo h2 >>"$0"' "$order" &
    local h2=$!
    wait "$h1" "$m" "$h2"
    [ "$(tr '\n' ' ' <"$order")" = "h1 m h2 " ] \
        || { echo "measure self-test: order was '$(tr '\n' ' ' <"$order")', expected 'h1 m h2 '" >&2; return 1; }

    # A forwarded SIGTERM reaches the command; the locks are released after.
    "$me" --log "$dir/log" -- sleep 30 &
    m=$!
    for i in $(seq 50); do
        flock -n -s "$lock" true || break
        sleep 0.05
    done
    sleep 0.2
    kill -TERM "$m"
    status=0; wait "$m" || status=$?
    [ "$status" -eq 143 ] || { echo "measure self-test: SIGTERM gave status $status, expected 143" >&2; return 1; }
    if ! flock -n -x "$intent" true || ! flock -n -x "$lock" true; then
        echo "measure self-test: the locks outlived the measurement" >&2
        return 1
    fi

    # Measuring inside heavy work is refused rather than deadlocking.
    status=0
    timeout 10 "$heavy" --mem none -- "$me" --log "$dir/log" -- true 2>/dev/null || status=$?
    [ "$status" -eq 2 ] || { echo "measure self-test: measure.sh inside heavy.sh gave status $status, expected 2" >&2; return 1; }

    # The command runs with LUSHTEXT_ADMISSION_NOWAIT=1.
    # shellcheck disable=SC2016 # expanded by the inner shell
    status=$("$me" --log "$dir/log" -- sh -c 'echo "${LUSHTEXT_ADMISSION_NOWAIT:-}"')
    [ "$status" = 1 ] || { echo "measure self-test: the command saw LUSHTEXT_ADMISSION_NOWAIT='$status', expected 1" >&2; return 1; }

    # A LOCK holder that never finishes (a hung heavy command, a leaked
    # daemon) does not keep the measurement holding INTENT forever: after
    # --lock-timeout it releases INTENT, names the holder and exits 75.
    # shellcheck disable=SC2016 # expanded by the inner shell
    sh -c 'exec 7>>"$0"; flock -s 7; exec sleep 30' "$lock" &
    local holder=$! report
    for i in $(seq 50); do
        flock -n -x "$lock" true || break
        sleep 0.05
    done
    status=0
    report=$("$me" --log "$dir/log" --lock-timeout 1 -- true 2>&1) || status=$?
    local intent_free=1
    flock -n -x "$intent" true || intent_free=0
    kill "$holder"
    wait "$holder" 2>/dev/null || true
    [ "$status" -eq 75 ] || { echo "measure self-test: --lock-timeout gave status $status, expected 75" >&2; return 1; }
    [ "$intent_free" = 1 ] || { echo "measure self-test: INTENT stayed held after the lock timeout" >&2; return 1; }
    grep -qE "^ +$holder .*sleep 30" <<<"$report" || { echo "measure self-test: the timeout report does not name the holder $holder:" >&2; echo "$report" >&2; return 1; }
    status=0; "$me" --lock-timeout 0 -- true 2>/dev/null || status=$?
    [ "$status" -eq 2 ] || { echo "measure self-test: --lock-timeout 0 was accepted (status $status)" >&2; return 1; }

    echo "measure self-test: ok (measuring inside heavy work refused; exit status and SIGTERM propagate; load recorded before/after; both locks exclusive; a pending measurement blocks new heavy starts and runs before them; the command runs with admission NOWAIT; a lock wait gives up after --lock-timeout, releasing INTENT and naming the holder)"
}

if [ "${1:-}" = "--self-test" ]; then
    self_test
    exit
fi

label=measurement
log=
lock_timeout=1800
while [ "$#" -gt 0 ]; do
    case "$1" in
        --label)
            [ "$#" -ge 2 ] || usage
            label=$2
            shift 2
            ;;
        --log)
            [ "$#" -ge 2 ] || usage
            log=$2
            shift 2
            ;;
        --lock-timeout)
            [ "$#" -ge 2 ] && [[ "$2" =~ ^[1-9][0-9]*$ ]] || usage
            lock_timeout=$2
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
if mode=$(agent_lock_inherited); then
    if [ "$mode" != exclusive ]; then
        echo "measure: refusing to measure inside heavy work (LOCK is held shared by an enclosing scripts/heavy.sh); waiting for LOCK exclusively there would deadlock" >&2
        exit 2
    fi
    echo "measure: LOCK already held exclusively by an enclosing measurement; running in place" >&2
else
    exec 6>>"$AGENT_LOCK_INTENT"
    flock -x 6
    exec 7>>"$AGENT_LOCK_MEASURE"
    if ! flock -w "$lock_timeout" -x 7; then
        exec 7>&- 6>&-
        echo "measure: LOCK still held after ${lock_timeout} s; released INTENT and gave up. Holders of $AGENT_LOCK_MEASURE:" >&2
        agent_lock_holders "$AGENT_LOCK_MEASURE" >&2
        exit 75
    fi
    LUSHTEXT_AGENT_LOCK_HELD=$(agent_lock_marker exclusive)
    export LUSHTEXT_AGENT_LOCK_HELD
fi
export LUSHTEXT_ADMISSION_NOWAIT=1

# From here the script owns both locks: a signal must not kill it while a
# snapshot or the command still holds a copy of them, so it is trapped and
# forwarded (or, before the command starts, recorded and the command skipped).
agent_trap_signals

record before 6>&- 7>&-
started=$(date +%s.%N)

agent_run_child "$@"
status=$agent_child_status

record after "status=$status seconds=$(awk -v a="$started" -v b="$(date +%s.%N)" 'BEGIN { printf "%.3f", b - a }')" 6>&- 7>&-
exec 7>&- 6>&-
exit "$status"
