#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Run ONE heavy, non-measurement command (cargo build/check/clippy/test, kani,
# mutants, a smoke lane, `git commit` with its Clippy hook) the way the
# shared-machine protocol requires (see "Shared-machine agent work" in
# .agents/rules/verification-runs.md):
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
# 4. Run the command as a child, with the compiler parallelism below and the
#    CPU affinity this process inherited, intersected with the slice's
#    cpuset, inside `scripts/with-memory-cap.sh <cap>` (its scope nests under
#    the user manager's user.slice and inherits its cpuset; a self-imposed
#    mask only narrows it).
#
# Compiler parallelism (.agents/rules/build.md, "Compiler parallelism"), set
# once LOCK is held: every heavy command holds it shared, so its shared
# holders in /proc/locks (this one included) are the heavy commands now
# running, on the host and in every toolbox (they share the host's PID
# namespace). Alone, a command gets CARGO_BUILD_JOBS = its CPUs (at most 16)
# and rustc's parallel front end with 8 threads for the checkout's own
# unoptimized crates (scripts/rustc-frontend-threads.sh, installed as
# RUSTC_WRAPPER and chained to the wrapper cargo would otherwise run).
# Sharing the machine, it gets CARGO_BUILD_JOBS=4 and 1 thread, which
# measured fastest there; the thread count changes, never whether the wrapper
# scopes RUSTC_BOOTSTRAP, because rustc discards its incremental cache when
# that turns on or off. LUSHTEXT_RUSTC_FRONTEND_THREADS=0 from the caller
# turns the front-end wrapper into a passthrough for that command.
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
# shellcheck source=scripts/cpu-lists.sh
source "$here/cpu-lists.sh"

# Pure: the compiler parallelism of a heavy command with <cpus> CPUs while
# <holders> heavy commands (itself included) hold LOCK. Prints
# "<CARGO_BUILD_JOBS> <front-end threads>". The 16-job ceiling is the largest
# count whose memory peaks were measured (build.md, "Compiler parallelism").
heavy_parallelism() {
    local cpus=$1 holders=$2 jobs
    if [ "$holders" -le 1 ] && [ "$cpus" -gt 4 ]; then
        jobs=$cpus
        [ "$jobs" -le 16 ] || jobs=16
        echo "$jobs 8"
    else
        echo "4 1"
    fi
}

# Shared holders of LOCK now: one /proc/locks line per open file description,
# which outlives the flock(1) process the line names (agent_lock_holders).
# Matched by inode only (btrfs reports another device there than stat). A
# waiter's line starts with "->" and is not counted. Unreadable: report 2, so
# the command takes the contended setting.
heavy_lock_shared_holders() {
    local inode
    if ! inode=$(stat -c %i "$AGENT_LOCK_MEASURE" 2>/dev/null) || [ ! -r /proc/locks ]; then
        echo 2
        return
    fi
    awk -v suffix=":$inode" '
        $2 == "FLOCK" && $4 == "READ" && substr($6, length($6) - length(suffix) + 1) == suffix { n++ }
        END { print n + 0 }' /proc/locks
}

# Export the compiler parallelism for the command: CARGO_BUILD_JOBS, and the
# front-end wrapper with its thread count, chained to the rustc-wrapper cargo
# would otherwise use from here. Of a caller's LUSHTEXT_RUSTC_FRONTEND_THREADS
# only 0 (off) is kept: any other value may be a leftover of an earlier
# heavy.sh's environment.
heavy_export_parallelism() {
    local jobs=$1 threads=$2 next
    export CARGO_BUILD_JOBS=$jobs
    if [ "${LUSHTEXT_RUSTC_FRONTEND_THREADS:-}" != 0 ]; then
        export LUSHTEXT_RUSTC_FRONTEND_THREADS=$threads
    fi
    if ! next=$(python3 -I "$here/next-rustc-wrapper.py" "$PWD"); then
        echo "heavy: cannot resolve cargo's rustc-wrapper; running without the front-end wrapper" >&2
        unset LUSHTEXT_RUSTC_FRONTEND_THREADS
        return 0
    fi
    LUSHTEXT_RUSTC_FRONTEND_ROOT=$(cd "$here/.." && pwd -P)
    export RUSTC_WRAPPER="$here/rustc-frontend-threads.sh" LUSHTEXT_NEXT_RUSTC_WRAPPER="$next" LUSHTEXT_RUSTC_FRONTEND_ROOT
}

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
    agent_lock_paths
    # Whatever started the self-test (a `heavy.sh -- git commit` whose hook
    # runs check-policy, a capped `make`) must not leak its markers into the
    # cases below.
    unset LUSHTEXT_AGENT_LOCK_HELD LUSHTEXT_MEMORY_CAP_KIB LUSHTEXT_ADMISSION_NOWAIT
    # Nor the parallelism an enclosing heavy.sh exported.
    unset CARGO_BUILD_JOBS RUSTC_WRAPPER LUSHTEXT_RUSTC_FRONTEND_THREADS \
        LUSHTEXT_RUSTC_FRONTEND_ROOT LUSHTEXT_NEXT_RUSTC_WRAPPER
    # The cases below run real measurements (`measure.sh -- true`). They run
    # on this process's own affinity as the reference, whatever was inherited:
    # with the default 8-23 they would exit 78 while a game narrows the
    # cpuset, and on hosts without CPUs 8-23, and every commit hook would fail
    # for the host's CPU state.
    LUSHTEXT_REFERENCE_CPUS=$(cpu_list_canonical "$(cpu_own_affinity)")
    export LUSHTEXT_REFERENCE_CPUS
    local intent=$dir/responsiveness-measure.intent lock=$dir/responsiveness-measure.lock

    status=0; "$me" --mem none -- sh -c 'exit 7' || status=$?
    [ "$status" -eq 7 ] || fail "exit status 7 came back as $status" || return

    status=0; "$me" true 2>/dev/null || status=$?
    [ "$status" -eq 2 ] || fail "a command without '--' was accepted (status $status)" || return

    # The parallelism policy, pure.
    local case_ want got
    for case_ in "16 1:16 8" "24 1:16 8" "12 1:12 8" "5 1:5 8" "4 1:4 1" "2 1:4 1" "16 2:4 1" "16 5:4 1" "16 0:16 8"; do
        want=${case_#*:}
        # shellcheck disable=SC2086 # two words on purpose
        got=$(heavy_parallelism ${case_%%:*})
        [ "$got" = "$want" ] || fail "heavy_parallelism ${case_%%:*} gave '$got', expected '$want'" || return
    done

    # Alone (this run's private LOCK has no other holder): all the CPUs, at
    # most 16, and 8 front-end threads through the wrapper, chained to the
    # wrapper cargo would otherwise run.
    local cpus alone parallel_probe
    cpus=$(cpu_list_expand "$(cpu_own_affinity)" | wc -l)
    alone=$(heavy_parallelism "$cpus" 1)
    # shellcheck disable=SC2016 # expanded by the inner shell
    parallel_probe='echo "$CARGO_BUILD_JOBS $LUSHTEXT_RUSTC_FRONTEND_THREADS ${RUSTC_WRAPPER##*/} $LUSHTEXT_RUSTC_FRONTEND_ROOT"'
    out=$("$me" --mem none -- sh -c "$parallel_probe")
    want="$alone rustc-frontend-threads.sh $(cd "$here/.." && pwd -P)"
    [ "$out" = "$want" ] || fail "alone: '$out', expected '$want'" || return
    # shellcheck disable=SC2016 # expanded by the inner shell
    out=$("$me" --mem none -- sh -c 'printf "%s" "$LUSHTEXT_NEXT_RUSTC_WRAPPER"')
    want=$(env -u RUSTC_WRAPPER python3 -I "$here/next-rustc-wrapper.py" "$PWD")
    [ "$out" = "$want" ] || fail "next rustc-wrapper '$out', expected '$want'" || return

    # A second holder of LOCK shared: the contended setting. A nested
    # heavy.sh keeps what the outer one was given; a caller's 0 stays off.
    # shellcheck disable=SC2016 # expanded by the inner shell
    sh -c 'exec 7>>"$0"; flock -s 7; exec sleep 30' "$lock" &
    local sibling=$!
    for _ in $(seq 50); do
        [ "$(heavy_lock_shared_holders)" -ge 1 ] && break
        sleep 0.05
    done
    out=$("$me" --mem none -- sh -c "$parallel_probe")
    want="4 1 rustc-frontend-threads.sh $(cd "$here/.." && pwd -P)"
    [ "$out" = "$want" ] || { kill "$sibling"; fail "contended: '$out', expected '$want'"; return; }
    kill "$sibling"
    wait "$sibling" 2>/dev/null || true
    # shellcheck disable=SC2016 # expanded by the inner shells
    out=$("$me" --mem none -- sh -c 'CARGO_BUILD_JOBS=3 "$0" --mem none -- sh -c "echo \$CARGO_BUILD_JOBS"' "$me" 2>/dev/null)
    [ "$out" = 3 ] || fail "a nested heavy.sh reset the outer parallelism to '$out'" || return
    # shellcheck disable=SC2016 # expanded by the inner shell
    out=$(LUSHTEXT_RUSTC_FRONTEND_THREADS=0 "$me" --mem none -- sh -c 'echo "$LUSHTEXT_RUSTC_FRONTEND_THREADS"')
    [ "$out" = 0 ] || fail "a caller's LUSHTEXT_RUSTC_FRONTEND_THREADS=0 became '$out'" || return
    # shellcheck disable=SC2016 # expanded by the inner shell
    out=$(LUSHTEXT_RUSTC_FRONTEND_THREADS=16 "$me" --mem none -- sh -c 'echo "$LUSHTEXT_RUSTC_FRONTEND_THREADS"')
    [ "$out" = "${alone#* }" ] || fail "a leftover LUSHTEXT_RUSTC_FRONTEND_THREADS=16 was kept as '$out'" || return

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
    # pass byte for byte, the command runs inside the heavy slice (named by
    # with-memory-cap.sh --print-slice, its one source) on exactly the one CPU
    # it was given, LOCK is held across `systemd-run --scope`, and fd 9 (the
    # wrapper's admission lock) is not a lock of the protocol. The CPU is one
    # that this process's affinity and the inner user.slice's effective cpuset
    # share (the slice inherits that cpuset); when they share none, the scope
    # gets the slice's whole set (the kernel falls back to the cpuset).
    local scope=skipped
    if command -v systemd-run >/dev/null 2>&1 \
        && systemd-run --user --scope --quiet -p MemoryMax=1M true >/dev/null 2>&1; then
        local cpu slice slice_cg user_cg allowed shared expected_cpus
        slice=$("$here/with-memory-cap.sh" --print-slice) || fail "with-memory-cap.sh --print-slice failed" || return
        slice_cg=$(systemctl --user show -p ControlGroup --value "$slice" 2>/dev/null || true)
        user_cg=$(systemctl --user show -p ControlGroup --value user.slice 2>/dev/null || true)
        allowed=$(cat "/sys/fs/cgroup$user_cg/cpuset.cpus.effective" 2>/dev/null || cpu_online)
        shared=$(cpu_lines_minus "$(cpu_list_expand "$(cpu_own_affinity)")" \
            "$(cpu_lines_minus "$(cpu_list_expand "$(cpu_online)")" "$(cpu_list_expand "$allowed")")" | head -n 1)
        if [ -n "$shared" ]; then
            cpu=$shared
            expected_cpus=$shared
        else
            cpu=$(cpu_own_affinity | sed 's/[-,].*//')
            expected_cpus=$(cpu_list_canonical "$allowed")
        fi
        # shellcheck disable=SC2016 # expanded by the inner shell; literal dollars are the point
        out=$(taskset -c "$cpu" "$me" --mem 64M -- sh -c '
            printf "%s|" "$1" "$2" "$3"
            c=$(sed -n "s/^0:://p" /proc/self/cgroup)
            case $c in "$4"/*) printf "in|" ;; *) printf "out:%s|" "$c" ;; esac
            sed -n "s/^Cpus_allowed_list:[[:space:]]*//p" /proc/self/status | tr "\n" "|"
            flock -n -x "$0" true && printf free || printf held
            [ /proc/self/fd/9 -ef "$0" ] && printf "|fd9"
            true' "$lock" 'a$$b' '$0' '${HOME}' "${slice_cg:-unset}" 2>/dev/null)
        # shellcheck disable=SC2016 # literal dollars are the point
        [ -n "$slice_cg" ] && [ "$out" = "a\$\$b|\$0|\${HOME}|in|$expected_cpus|held" ] \
            || fail "capped run reported '$out', expected 'a\$\$b|\$0|\${HOME}|in|$expected_cpus|held' (slice $slice at '$slice_cg')" || return
        scope="args unexpanded, inside $slice, CPU $cpu given and $expected_cpus kept, LOCK held across the scope"
    fi

    echo "heavy self-test: ok (exit status and SIGTERM propagate; parallelism $alone alone and 4 1 contended, kept when nested; LOCK held shared by the helper, not the command; a pending measurement blocks new starts; nested calls run in place; stale markers do not; a leftover daemon does not hold LOCK; measure.sh self-test passes inside heavy.sh; capped scope: $scope)"
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

if mode=$(agent_lock_inherited); then
    # Nothing to hold here: the enclosing helper keeps LOCK until this exits.
    # A cap is admitted inside it unless the enclosing command is capped too
    # (then the wrapper runs in place). Inside a heavy.sh the command keeps
    # the parallelism it was given; inside anything else (a measurement), the
    # contended setting.
    echo "heavy: LOCK already held $mode by an enclosing helper; running in place: $*" >&2
    if [ "${RUSTC_WRAPPER:-}" != "$here/rustc-frontend-threads.sh" ] || [ -z "${CARGO_BUILD_JOBS:-}" ]; then
        # shellcheck disable=SC2046 # two words on purpose
        heavy_export_parallelism $(heavy_parallelism 1 2)
    fi
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

# shellcheck disable=SC2046 # two words on purpose
heavy_export_parallelism $(heavy_parallelism "$(cpu_list_expand "$(cpu_own_affinity)" | wc -l)" "$(heavy_lock_shared_holders)")
agent_trap_signals
LUSHTEXT_AGENT_LOCK_HELD=$(agent_lock_marker shared)
export LUSHTEXT_AGENT_LOCK_HELD
agent_run_child "$here/with-memory-cap.sh" "$cap" "$@"
exec 7>&-
exit "$agent_child_status"
