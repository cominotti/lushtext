#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Run ONE measurement (responsiveness lane, interleaved bench, any timing
# verdict) under the shared-machine protocol's writer preference (see
# "Shared-machine agent work" in .agents/rules/verification-runs.md):
#
#   scripts/measure.sh [--label L] [--log FILE] [--lock-timeout S] -- <command> [args...]
#   scripts/measure.sh --print-reference-cpus
#   scripts/measure.sh --self-test
#
# 0. Pin itself to the reference CPU set, before any lock (below), so every
#    measurement runs on the same CPUs, or on none.
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
#
# The reference CPU set. Every timing verdict runs on one named set of CPUs:
# MEASURE_REFERENCE_CPUS_DEFAULT (8-23, the E-cores every earlier verdict ran
# on), or LUSHTEXT_REFERENCE_CPUS as an override; `--print-reference-cpus`
# prints the value in force, so docs and tests never copy the literal. After
# its arguments and the inside-heavy-work refusal (exit 2, which stays first),
# and before it takes any lock (so a refused measurement never drains other
# agents' work), measure.sh:
#   1. parses the reference (a malformed list exits 2);
#   2. requires it to be a subset of the CPUs its cgroup allows: the nearest
#      cpuset.cpus.effective above its own cgroup (the online CPUs when there
#      is none), never the caller's mask, so a caller pinned narrower is
#      widened to the reference;
#   3. sets its own affinity to it (`taskset -pc`), which the command and the
#      load snapshots inherit;
#   4. reads Cpus_allowed_list back and requires it to equal the reference.
# A failing step 2 or 4 is read once more before refusing (the host's CPU
# tooling flips the cpuset at a game's start and end); a refusal exits 78
# (EX_CONFIG) and names both sets. It never measures on the intersection:
# that would silently change the reference. The read-back, not taskset's exit
# status, is the check, because the kernel accepts a mask that only partly
# overlaps the cpuset and silently intersects it (H3: a disjoint mask fails
# with EINVAL, a partial overlap succeeds on fewer CPUs). The affinity is read
# again once both locks are held (the wait can last --lock-timeout): a change
# releases the locks and exits 78 without running the command. It is read
# once more after the command: a change marks the after record
# `cpus_changed=<now>` beside the command's `status=`, and measure.sh exits 78
# whatever that status was. During a game (CST narrows the toolbox cpuset to
# 12-23) the default refuses; measuring anyway takes an explicit override,
# recorded `source=override`, which is not comparable with reference runs.
# The before and after records carry `cpus=<read back> reference=<R>
# source=<default|override>`, and the command gets LUSHTEXT_MEASURE_CPUS
# (R, canonical) and LUSHTEXT_MEASURE_CPUS_SOURCE.

set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=scripts/agent-locks.sh
source "$here/agent-locks.sh"
# shellcheck source=scripts/cpu-lists.sh
source "$here/cpu-lists.sh"

MEASURE_REFERENCE_CPUS_DEFAULT=8-23

usage() {
    echo "usage: $0 [--label L] [--log FILE] [--lock-timeout S] -- <command> [args...]" >&2
    echo "       $0 --print-reference-cpus" >&2
    echo "       $0 --self-test" >&2
    exit 2
}

# The CPUs this process's cgroup allows. LUSHTEXT_MEASURE_TEST_FLIP_ONCE=1
# (self-test only) makes the first of these readings lack the reference's
# first CPU, as if a cpuset flip fell between two reads; it can only cause a
# re-read, never a pass.
# The caller counts the readings (allowed_reads): this runs in a command
# substitution, where an increment would be lost.
allowed_reads=0
allowed_cpus() {
    local allowed
    allowed=$(cgroup_effective_cpus)
    if [ "${LUSHTEXT_MEASURE_TEST_FLIP_ONCE:-}" = 1 ] && [ "$allowed_reads" = 1 ]; then
        allowed=$(cpu_lines_minus "$(cpu_list_expand "$allowed")" "$(cpu_list_expand "$reference" | head -n 1)" | cpu_list_compress)
    fi
    echo "$allowed"
}

# Steps 2-4 of the reference check, once: sets allowed and pinned; returns 1
# when the reference is not allowed, 2 when the read-back differs.
pin_reference() {
    allowed_reads=$((allowed_reads + 1))
    allowed=$(allowed_cpus)
    pinned=
    cpu_list_subset "$reference" "$allowed" || return 1
    taskset -pc "$reference" "$$" >/dev/null 2>&1 || true
    pinned=$(cpu_own_affinity)
    cpu_list_equal "$pinned" "$reference" || return 2
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
    unset LUSHTEXT_AGENT_LOCK_HELD LUSHTEXT_MEMORY_CAP_KIB LUSHTEXT_ADMISSION_NOWAIT LUSHTEXT_MEASURE_TEST_FLIP_ONCE
    # Every measurement below runs on this process's own affinity as the
    # reference, whatever was inherited (the Makefile hands heavy.sh an
    # unusable one): it is always within the allowed set, it keeps a hook's
    # or an agent's taskset pin, and it makes the cases pass on hosts without
    # the default CPUs (CI) and while a game narrows the cpuset.
    reference=$(cpu_list_canonical "$(cpu_own_affinity)")
    export LUSHTEXT_REFERENCE_CPUS=$reference
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

    # The reference CPU set (design D8). Every case below names CPUs computed
    # at run time from this process's affinity and the host's CPU files,
    # never a number; a case that needs a CPU the host lacks skips.
    local failures=0 own c c1 c2 unavailable possible_last out marker
    own=$(cpu_list_expand "$reference")
    c=$(head -n 1 <<<"$own")
    c1=$c
    c2=$(sed -n 2p <<<"$own")
    unavailable=$(cpu_outside_cpuset "$(cgroup_effective_cpus)")
    possible_last=$(sed 's/.*[-,]//' /sys/devices/system/cpu/possible 2>/dev/null || echo 1023)
    [ -n "$unavailable" ] || unavailable=$((possible_last + 1))
    measure_fail() {
        echo "measure self-test: $*" >&2
        failures=$((failures + 1))
    }

    # The default, printed without copying the literal anywhere else.
    out=$(env -u LUSHTEXT_REFERENCE_CPUS "$me" --print-reference-cpus 2>&1 || true)
    [ "$out" = 8-23 ] || measure_fail "--print-reference-cpus with no override printed '$out', expected 8-23"

    # An override within the allowed set is applied exactly, recorded and
    # exported.
    : >"$dir/ref.log"
    # shellcheck disable=SC2016 # expanded by the inner shell
    out=$(LUSHTEXT_REFERENCE_CPUS=$c "$me" --label ref --log "$dir/ref.log" -- sh -c 'sed -n "s/^Cpus_allowed_list:[[:space:]]*//p" /proc/self/status; echo "${LUSHTEXT_MEASURE_CPUS:-}|${LUSHTEXT_MEASURE_CPUS_SOURCE:-}"' 2>&1 | tr '\n' ' ')
    [ "$out" = "$c $c|override " ] || measure_fail "a reference of CPU $c gave the command '$out', expected '$c $c|override '"
    if ! grep -q "before label=ref cpus=$c reference=$c source=override " "$dir/ref.log" \
        || ! grep -q "after label=ref status=0 seconds=[0-9.]* cpus=$c reference=$c source=override " "$dir/ref.log"; then
        measure_fail "the records lack cpus=$c reference=$c source=override: $(grep label=ref "$dir/ref.log" | tr '\n' ' ')"
    fi

    # An unavailable reference is refused (78) before any lock: with LOCK held
    # shared by a holder that never finishes and a 1 s lock timeout, a check
    # after the locks would exit 75 instead. A partial overlap is refused too,
    # never intersected (H3), and a malformed list is a usage error.
    # shellcheck disable=SC2016 # expanded by the inner shell
    sh -c 'exec 7>>"$0"; flock -s 7; exec sleep 30' "$lock" &
    local holder=$!
    for i in $(seq 50); do
        flock -n -x "$lock" true || break
        sleep 0.05
    done
    local ref_case expected
    for ref_case in "78|$unavailable" "78|$c,$unavailable" "2|abc"; do
        expected=${ref_case%%|*}
        marker=$dir/marker
        rm -f "$marker"
        status=0
        out=$(LUSHTEXT_REFERENCE_CPUS=${ref_case#*|} timeout 10 "$me" --log "$dir/log" --lock-timeout 1 -- touch "$marker" 2>&1) || status=$?
        [ "$status" -eq "$expected" ] && [ ! -e "$marker" ] \
            || measure_fail "reference '${ref_case#*|}' gave status $status (expected $expected, command not run): $out"
        if [ "$expected" = 78 ] && [[ "$out" != *"reference CPU set $(cpu_list_canonical "${ref_case#*|}") is not available (this cgroup allows "* ]]; then
            measure_fail "the refusal of '${ref_case#*|}' does not name both sets: $out"
        fi
    done
    kill "$holder"
    wait "$holder" 2>/dev/null || true

    # Inside heavy work the exit-2 refusal still comes first.
    status=0
    LUSHTEXT_REFERENCE_CPUS=$unavailable timeout 10 "$heavy" --mem none -- "$me" --log "$dir/log" -- true 2>/dev/null || status=$?
    [ "$status" -eq 2 ] || measure_fail "measure.sh inside heavy.sh with an unavailable reference gave status $status, expected 2"

    # A transient disagreement (one cpuset flip between two reads) is read
    # again, and the measurement runs.
    status=0
    LUSHTEXT_MEASURE_TEST_FLIP_ONCE=1 "$me" --log "$dir/log" -- true 2>/dev/null || status=$?
    [ "$status" -eq 0 ] || measure_fail "a disagreement the re-read resolves refused the measurement (status $status)"

    if [ -n "$c2" ]; then
        # The set changes while the command runs (here the command narrows
        # measure.sh itself, as a game profile narrows the cpuset): the after
        # record says so beside the command's status, the exit is 78, the
        # locks are released.
        : >"$dir/change.log"
        status=0
        # shellcheck disable=SC2016 # expanded by the inner shell
        LUSHTEXT_REFERENCE_CPUS=$c1,$c2 "$me" --label change --log "$dir/change.log" -- sh -c 'taskset -pc "$0" "$PPID" >/dev/null' "$c1" 2>/dev/null || status=$?
        [ "$status" -eq 78 ] || measure_fail "a CPU set changed during the command gave status $status, expected 78"
        grep -q "after label=change status=0 .*cpus_changed=$c1" "$dir/change.log" \
            || measure_fail "the after record lacks status=0 and cpus_changed=$c1: $(grep label=change "$dir/change.log" | tr '\n' ' ')"
        if ! flock -n -x "$intent" true || ! flock -n -x "$lock" true; then
            measure_fail "the locks outlived a measurement whose CPU set changed"
        fi

        # The set changes during the lock wait: the measurement releases both
        # locks and exits 78 without running its command.
        # shellcheck disable=SC2016 # expanded by the inner shell
        sh -c 'exec 7>>"$0"; flock -s 7; exec sleep 30' "$lock" &
        holder=$!
        for i in $(seq 50); do
            flock -n -x "$lock" true || break
            sleep 0.05
        done
        rm -f "$marker"
        LUSHTEXT_REFERENCE_CPUS=$c1,$c2 "$me" --log "$dir/log" -- touch "$marker" 2>/dev/null &
        local waiter=$!
        for i in $(seq 50); do
            flock -n -s "$intent" true || break
            sleep 0.05
        done
        taskset -pc "$c1" "$waiter" >/dev/null
        kill "$holder"
        wait "$holder" 2>/dev/null || true
        status=0
        wait "$waiter" || status=$?
        [ "$status" -eq 78 ] && [ ! -e "$marker" ] \
            || measure_fail "a CPU set changed during the lock wait gave status $status (expected 78, command not run)"
        if ! flock -n -x "$intent" true || ! flock -n -x "$lock" true; then
            measure_fail "the locks outlived a measurement refused after its lock wait"
        fi
    else
        echo "measure self-test: skipped the two CPU-change cases (this process has one CPU)" >&2
    fi
    [ "$failures" -eq 0 ] || { echo "measure self-test: $failures reference CPU case(s) failed" >&2; return 1; }

    echo "measure self-test: ok (reference CPU set $reference applied, read back, recorded and exported, default 8-23; an unavailable, partly unavailable or changed set refused with 78 before any lock, after the lock wait and after the command; a malformed set refused with 2; one flip re-read; measuring inside heavy work refused first; exit status and SIGTERM propagate; load recorded before/after; both locks exclusive; a pending measurement blocks new heavy starts and runs before them; the command runs with admission NOWAIT; a lock wait gives up after --lock-timeout, releasing INTENT and naming the holder)"
}

if [ "${1:-}" = "--self-test" ]; then
    self_test
    exit
fi

reference=${LUSHTEXT_REFERENCE_CPUS:-$MEASURE_REFERENCE_CPUS_DEFAULT}
reference_source=default
[ -z "${LUSHTEXT_REFERENCE_CPUS:-}" ] || reference_source=override
if ! reference=$(cpu_list_canonical "$reference") || [ -z "$reference" ]; then
    echo "measure: LUSHTEXT_REFERENCE_CPUS='${LUSHTEXT_REFERENCE_CPUS:-}' is not a CPU list (expected e.g. 8-23 or 8-11,16)" >&2
    exit 2
fi

if [ "${1:-}" = "--print-reference-cpus" ]; then
    echo "$reference"
    exit 0
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
in_place=0
if mode=$(agent_lock_inherited); then
    if [ "$mode" != exclusive ]; then
        echo "measure: refusing to measure inside heavy work (LOCK is held shared by an enclosing scripts/heavy.sh); waiting for LOCK exclusively there would deadlock" >&2
        exit 2
    fi
    in_place=1
fi

# The reference CPU set, before any lock (header). A nested in-place run
# re-checks the same inherited environment, a no-op.
pin_status=0
pin_reference || pin_status=$?
if [ "$pin_status" != 0 ]; then
    pin_status=0
    pin_reference || pin_status=$?
fi
if [ "$pin_status" = 1 ]; then
    echo "measure: reference CPU set $reference is not available (this cgroup allows $allowed); not measuring elsewhere" >&2
    echo "measure: a CST game profile narrows the toolbox cpuset while a game runs: wait for it to end, or measure on another set explicitly with LUSHTEXT_REFERENCE_CPUS (recorded as an override, not comparable with reference runs)" >&2
    exit 78
elif [ "$pin_status" = 2 ]; then
    echo "measure: reference CPU set $reference was not applied (affinity read back $pinned; this cgroup allows $allowed); not measuring elsewhere" >&2
    exit 78
fi

if [ "$in_place" = 1 ]; then
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
# The lock wait may have lasted long enough for the cpuset to change.
cpus=$(cpu_own_affinity)
if ! cpu_list_equal "$cpus" "$reference"; then
    exec 7>&- 6>&-
    echo "measure: the CPU set changed during the lock wait (reference $reference, now $cpus); released the locks without measuring" >&2
    exit 78
fi
export LUSHTEXT_ADMISSION_NOWAIT=1
export LUSHTEXT_MEASURE_CPUS=$reference
export LUSHTEXT_MEASURE_CPUS_SOURCE=$reference_source

# From here the script owns both locks: a signal must not kill it while a
# snapshot or the command still holds a copy of them, so it is trapped and
# forwarded (or, before the command starts, recorded and the command skipped).
agent_trap_signals

record before "cpus=$cpus reference=$reference source=$reference_source" 6>&- 7>&-
started=$(date +%s.%N)

agent_run_child "$@"
status=$agent_child_status

seconds=$(awk -v a="$started" -v b="$(date +%s.%N)" 'BEGIN { printf "%.3f", b - a }')
after=$(cpu_own_affinity)
changed=
cpu_list_equal "$after" "$reference" || changed=" cpus_changed=$after"
record after "status=$status seconds=$seconds cpus=$after reference=$reference source=$reference_source$changed" 6>&- 7>&-
exec 7>&- 6>&-
if [ -n "$changed" ]; then
    echo "measure: the CPU set changed during the measurement (reference $reference, now $after; the command exited $status); not a reference run" >&2
    exit 78
fi
exit "$status"
