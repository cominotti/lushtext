#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Run one command inside a memory-capped cgroup scope, so a model checker that
# explodes is killed alone instead of pushing the desktop into swap.
#
#   scripts/with-memory-cap.sh [--address-space] <cap> <command> [args...]
#   scripts/with-memory-cap.sh --wait-admissible <cap>
#   scripts/with-memory-cap.sh --print-slice
#   scripts/with-memory-cap.sh --self-test
#
# `--print-slice` prints the heavy slice's unit name, the one source of it
# (scripts/heavy.sh's self-test reads it from here).
#
# The command's arguments reach it byte for byte: systemd-run would otherwise
# expand `$VAR`/`${VAR}` and turn `$$` into `$` in them (systemd >= 254 does by
# default), silently rewriting `sh -c` scripts, commit messages and regexes, so
# the scope is started with `--expand-environment=no`.
#
# `--wait-admissible <cap>` waits, holding nothing, until a scope of <cap>
# would be admitted right now, and reserves nothing (with
# LUSHTEXT_ADMISSION_NOWAIT=1 it checks once and exits 75 on a misfit).
# scripts/heavy.sh calls it before taking the shared-machine locks, so it does
# not wait for memory while holding LOCK (see "Shared-machine agent work" in
# .agents/rules/verification-runs.md). With LUSHTEXT_ADMISSION_NOWAIT=1 (exported by
# scripts/measure.sh to its command) admission never waits: a cap that does
# not fit at once fails with exit 75, because waiting for memory inside a
# measurement's exclusive locks would stall every agent for as long as the
# memory stays short.
#
# `--address-space` also sets each process's RLIMIT_AS to 7/8 of the cap, so a
# single process that outgrows it (CBMC) fails its own allocation and exits
# cleanly before the scope's OOM killer fires. That matters on GNOME: every
# OOM kill in a user scope makes gsd-housekeeping notify that memory is nearly
# full. The cgroup cap stays the backstop for the sum of all processes.
#
# <cap> is a systemd memory size (`16G`, `12288M`) or `none`. The scope sets
# `MemoryMax=<cap>` and `MemorySwapMax=0`: the command and every descendant
# share the cap, and when they exceed it the kernel OOM-kills inside the scope
# only (exit status 137). A harness that hits the cap is a finding — shrink its
# bounds — not a reason to raise the cap.
#
# The scope also sets `OOMPolicy=continue`. With systemd's default (`stop`) the
# first kernel OOM kill stopped the whole scope and marked it `oom-kill`, which
# is the result gsd-housekeeping turns into a CRITICAL "memory nearly full"
# notification (shown through Do Not Disturb and over fullscreen apps). With
# `continue` only the process that outgrew the cap dies, the caller sees that
# one build or test fail, and the scope is not marked `oom-kill`. That is a
# backstop, not a licence: a command's own parallelism (cargo jobs, test
# threads, cargo-mutants jobs) must fit inside the cap, because a mutant whose
# test was OOM-killed reads as caught and contaminates the run.
#
# Every capped scope also runs inside one shared user slice,
# `user-lushtext-heavy.slice`, whose own `MemoryMax` (LUSHTEXT_HEAVY_MEMORY_MAX, else
# ~/.config/lushtext/heavy-memory-max, default 48G, no swap) bounds the SUM of all capped work on the host, however
# many agents or terminals start it. A per-command cap alone did not: three
# agents each held to 12G still added up (2026-10-20). Before a scope starts,
# admission (serialized by a lock under $XDG_RUNTIME_DIR) waits until the caps
# already admitted to the slice plus this one fit the slice budget, and until
# MemAvailable covers this cap plus a reserve for the desktop
# (LUSHTEXT_MEMORY_RESERVE, default 16G). Caps of 1G or less skip admission:
# they are bounded and trivially small (the self-test uses them, and it runs in
# the pre-commit hook). A command already running under a cap (the wrapper
# exports LUSHTEXT_MEMORY_CAP_KIB) runs nested commands in place, inside the
# outer scope, instead of admitting a second scope; `--address-space` still
# applies there, at 7/8 of the smaller of the two caps.
#
# Where the slice sits, and why. `systemd-run --user --scope` starts the scope
# from the user manager, so it takes the placement of its slice, not the
# caller's. The `user-` prefix nests the slice as
# `user@<uid>.service/user.slice/user-lushtext.slice/user-lushtext-heavy.slice`
# (H1 below), inside the user manager's inner `user.slice`, where the toolboxes
# run. The host's CPU tooling (CST) gives that slice the toolboxes' CPU policy:
# a cpuset, `CPUWeight=30`, and during a game `CPUWeight=idle` on CPUs 12-23.
# Heavy work started from a toolbox inherits all of it, so a build never
# competes with a game at the game's weight. The slice used to be
# `lushtext-heavy.slice`, a sibling of `app.slice` (the desktop and games)
# under the manager itself, at weight 100 on every CPU. This wrapper sets NO
# CPU property (`CPUWeight`, `AllowedCPUs`, `CPUQuota`) on its scopes or
# slices: CST owns that policy and its values change (CST's answer,
# .claude/handoffs/nest-heavy-slices-under-user-slice/cst-answer.md). Memory
# properties, caps, the budget and the reserve are unchanged by the rename.
#
# The scope checks its own placement. Its first process is this script
# (`--exec-in-scope`), which runs placement_verdict before it execs the
# command: its cgroup path must lie under the ControlGroups systemd reports
# for the inner user.slice and for the heavy slice, and, when the inner
# user.slice has a configured cpuset, its CPU affinity must be a subset of that
# slice's cpuset.cpus.effective, read from /sys/fs/cgroup at run time and never
# a literal (it is 0,2-6,8-23 today, was 8-23, and is 12-23 during a game). A
# CPU disagreement is read once more (CST flips the cpuset at a game's start
# and end). A failure exits 78 (EX_CONFIG) before the command runs, with one
# `with-memory-cap: scope placement: expected …, got …` line. Nested in-place
# runs, `none` caps and the CI uncapped path start no scope and run no check.
#
# Legacy accounting during the rename. Copies of this wrapper from before it
# (worktrees that have not merged main) keep starting scopes in
# `lushtext-heavy.slice`. Admission therefore counts the scopes of the legacy
# slice of the same family (legacy_slice_of: the name without `user-`) with
# the same per-scope rule, and the admission lock keeps its file name, so old
# and new copies serialize. This wrapper never stops, reverts or resizes a
# legacy slice: stopping it would stop its scopes while an old copy may be
# starting one, and lowering its MemoryMax would OOM-kill old work. An empty
# legacy slice holds nothing and goes away when the user manager stops.
#
# Host behaviour relied on (measured 2026-11-15, kernel 7.2, systemd 262):
#   H1. systemd places `a-b-c.slice` under `a.slice/a-b.slice`.
#   H2. A cgroup v2 cpuset restricts every descendant, whether or not the
#       controller is enabled below it (our slices below user-lushtext.slice
#       have no cpuset files, yet their tasks run on the cpuset).
#   H3. On attach or on a cpuset change the kernel sets a task's affinity to
#       its requested mask intersected with the cpuset, or to the cpuset when
#       they are disjoint; setting a disjoint mask fails (EINVAL), and a
#       partial overlap is silently intersected.
# The self-test re-probes H1 and H3 (a caller on every CPU and one pinned
# outside the cpuset); H2 is covered by its placement case.
#
# Why this exists: on 2026-10-15 one unmeasured Kani harness drove CBMC to
# 59.6 GB resident on a 94 GB workstation, leaving ~250 MB free and stalling
# the desktop. CPU affinity bounded the cores, nothing bounded the memory.
#
# The scope needs a systemd user manager (`systemd-run --user`). Where there is
# none, the command runs uncapped only on CI (`CI` set: disposable runners whose
# memory is the shard table's own budget); locally the wrapper refuses, because
# running uncapped is exactly what this guards against. `<cap> = none` opts out
# explicitly.

set -euo pipefail

# shellcheck source=scripts/cpu-lists.sh
source "$(dirname "${BASH_SOURCE[0]}")/cpu-lists.sh"

cap_in_kib() {
    local value=${1%[KMGT]} unit=${1##*[0-9]}
    case "$unit" in
        K) echo "$value" ;;
        M) echo $((value * 1024)) ;;
        G) echo $((value * 1024 * 1024)) ;;
        T) echo $((value * 1024 * 1024 * 1024)) ;;
        *) echo $((value / 1024)) ;;
    esac
}

# LUSHTEXT_HEAVY_SLICE exists for the self-test only: its fake budgets must
# never be applied to the real host-wide slice. The `user-` prefix is what
# nests the slice under the user manager's user.slice (H1 in the header).
HEAVY_SLICE=${LUSHTEXT_HEAVY_SLICE:-user-lushtext-heavy.slice}
case $HEAVY_SLICE in
    user-lushtext-heavy*.slice) ;;
    lushtext-heavy*.slice)
        echo "with-memory-cap: LUSHTEXT_HEAVY_SLICE=$HEAVY_SLICE: the heavy slice family was renamed to user-lushtext-heavy*.slice so it nests under the user manager's user.slice" >&2
        exit 2
        ;;
    *) echo "with-memory-cap: LUSHTEXT_HEAVY_SLICE must name a user-lushtext-heavy*.slice unit" >&2; exit 2 ;;
esac
ADMISSION_LOCK="${XDG_RUNTIME_DIR:-/tmp}/lushtext-heavy-admission.lock"

mem_available_kib() {
    awk '/^MemAvailable:/ { print $2 }' /proc/meminfo
}

# Sum, in KiB, of the caps of the scopes currently in the heavy slice and in
# its legacy slice (legacy_slice_of), each slice by the same per-scope rule,
# so a scope an old wrapper copy started is never ignored during the rename.
admitted_kib() {
    local slice cgroup dir total=0 limit
    for slice in "$HEAVY_SLICE" "$(legacy_slice_of "$HEAVY_SLICE")"; do
        cgroup=$(systemctl --user show -p ControlGroup --value "$slice" 2>/dev/null || true)
        dir="/sys/fs/cgroup$cgroup"
        if [ -n "$cgroup" ] && [ -d "$dir" ]; then
            for limit in "$dir"/*.scope/memory.max; do
                [ -r "$limit" ] || continue
                read -r limit <"$limit" || continue
                [ "$limit" = max ] && continue
                total=$((total + limit / 1024))
            done
        fi
    done
    echo "$total"
}

# The slice an older copy of this wrapper (from before the rename) uses for
# the same family: the name without its `user-` prefix (lushtext-heavy.slice
# for user-lushtext-heavy.slice). Admission counts its scopes too; nothing
# here ever stops, reverts or resizes it, because an old copy may be starting
# a scope in it.
legacy_slice_of() {
    echo "${1#user-}"
}

# The contents of a cpuset file, or nothing when it is missing (the cpuset
# controller is not enabled for that cgroup).
read_cpuset_file() {
    if [ -r "$1" ]; then
        cat "$1"
    fi
}

# shellcheck disable=SC2329 # placement_check's default reader, called by name
# Read the six inputs of placement_verdict for the heavy slice $1 into pl_own,
# pl_inner, pl_heavy, pl_aff, pl_cpus and pl_eff. A read that fails leaves its
# value empty, which the verdict refuses.
placement_readings() {
    local line n=0 shown
    pl_own=$(sed -n 's/^0:://p' /proc/self/cgroup 2>/dev/null || true)
    pl_inner=
    pl_heavy=
    # One call for both: without --value every unit prints a `ControlGroup=`
    # line, empty or not, in the order asked.
    shown=$(systemctl --user show -p ControlGroup user.slice "$1" 2>/dev/null || true)
    while IFS= read -r line; do
        case $line in
            ControlGroup=*)
                n=$((n + 1))
                if [ "$n" = 1 ]; then pl_inner=${line#ControlGroup=}; else pl_heavy=${line#ControlGroup=}; fi
                ;;
        esac
    done <<<"$shown"
    pl_aff=$(cpu_own_affinity)
    pl_cpus=
    pl_eff=
    if [ -n "$pl_inner" ]; then
        pl_cpus=$(read_cpuset_file "/sys/fs/cgroup$pl_inner/cpuset.cpus")
        pl_eff=$(read_cpuset_file "/sys/fs/cgroup$pl_inner/cpuset.cpus.effective")
    fi
}

# Pure: does a scope belong where systemd was asked to put it? Inputs: own
# cgroup path, the ControlGroup of the inner user.slice, that of the heavy
# slice, own Cpus_allowed_list, the inner user.slice's configured cpuset.cpus
# (may be empty) and its cpuset.cpus.effective. Prints the refusal and
# returns 0 on a pass, 1 when only the CPU half disagrees (worth one re-read:
# CST flips the cpuset at a game's start and end), 2 otherwise.
placement_verdict() {
    local own=$1 inner=$2 heavy=$3 aff=$4 cpus=$5 eff=$6 prefix="with-memory-cap: scope placement: expected"
    if [ -z "$own" ]; then
        echo "$prefix this process's cgroup path, got none (no 0:: line in /proc/self/cgroup)"
        return 2
    fi
    if [ -z "$inner" ]; then
        echo "$prefix the ControlGroup of user.slice, got none (systemctl --user show -p ControlGroup user.slice failed or printed none)"
        return 2
    fi
    if [ -z "$heavy" ]; then
        echo "$prefix the ControlGroup of the heavy slice, got none (systemctl --user show -p ControlGroup failed or printed none)"
        return 2
    fi
    case $own in
        "$inner"/*) ;;
        *) echo "$prefix a cgroup under user.slice ($inner/), got $own"; return 2 ;;
    esac
    case $own in
        "$heavy"/*) ;;
        *) echo "$prefix a cgroup under the heavy slice ($heavy/), got $own"; return 2 ;;
    esac
    # No configured cpuset on the inner user.slice (a host without CST):
    # nothing to inherit, so only the path is checked.
    [ -n "$cpus" ] || return 0
    local subset=0
    cpu_list_subset "$aff" "$eff" || subset=$?
    case $subset in
        0) return 0 ;;
        1) echo "$prefix CPU affinity within user.slice's cpuset.cpus.effective $eff (configured $cpus), got $aff"; return 1 ;;
        *) echo "$prefix CPU lists, got a malformed one (affinity '$aff', user.slice cpuset.cpus.effective '$eff')"; return 2 ;;
    esac
}

# Check this process's placement in the heavy slice $1, reading with $2
# (default placement_readings). A CPU disagreement is read once more before
# it refuses. Prints the refusal on stderr and returns 78 (EX_CONFIG).
placement_check() {
    local reader=${2:-placement_readings} message status=0
    "$reader" "$1"
    message=$(placement_verdict "$pl_own" "$pl_inner" "$pl_heavy" "$pl_aff" "$pl_cpus" "$pl_eff") || status=$?
    if [ "$status" = 1 ]; then
        "$reader" "$1"
        status=0
        message=$(placement_verdict "$pl_own" "$pl_inner" "$pl_heavy" "$pl_aff" "$pl_cpus" "$pl_eff") || status=$?
    fi
    [ "$status" = 0 ] && return 0
    echo "$message" >&2
    return 78
}

# True when a cap of $1 KiB fits the slice budget ($2) and the desktop
# reserve ($3) right now; sets admitted and available for the message.
fits() {
    admitted=$(admitted_kib)
    available=$(mem_available_kib)
    [ $((admitted + $1)) -le "$2" ] && [ "$available" -ge $(($1 + $3)) ]
}

waiting_message() {
    echo "with-memory-cap: waiting for memory: $((admitted / 1048576)) GiB of $((heavy_kib / 1048576)) GiB admitted to $HEAVY_SLICE, $((available / 1048576)) GiB available, need $((cap_kib / 1048576)) GiB plus a $((reserve_kib / 1048576)) GiB reserve" >&2
}

# Take the admission lock once this cap fits the slice budget and the
# desktop reserve; fd 9 stays locked until the caller's scope exists
# (release_admission_once_scoped). The lock is held only while checking and
# admitting, never while sleeping for memory: a waiter holding it would block
# every other admission, NOWAIT ones included. Under
# LUSHTEXT_ADMISSION_NOWAIT=1 a cap that does not fit at once exits 75
# instead of waiting, and a lock busy for 2 s counts as a misfit: only an
# admission in flight holds it now, and that normally takes milliseconds.
admit() {
    local announced=0
    exec 9>"$ADMISSION_LOCK"
    while :; do
        if [ "${LUSHTEXT_ADMISSION_NOWAIT:-}" = 1 ]; then
            flock -w 2 9 || {
                echo "with-memory-cap: not waiting (LUSHTEXT_ADMISSION_NOWAIT=1): the admission lock stayed busy" >&2
                exit 75
            }
        else
            flock 9
        fi
        fits "$cap_kib" "$heavy_kib" "$reserve_kib" && return 0
        if [ "${LUSHTEXT_ADMISSION_NOWAIT:-}" = 1 ]; then
            waiting_message
            echo "with-memory-cap: not waiting (LUSHTEXT_ADMISSION_NOWAIT=1, inside a measurement's locks): free memory, or run this capped command before the measurement" >&2
            exit 75
        fi
        if [ "$announced" = 0 ]; then
            waiting_message
            announced=1
        fi
        flock -u 9
        sleep 15
    done
}

# Runs detached, holding the inherited admission lock (fd 9) until process
# $1 has moved into a scope in the heavy slice, so the next admission counts
# its cap; gives up after 10 s or when the process is gone.
release_admission_once_scoped() {
    local pid=$1
    for _ in $(seq 200); do
        grep -q "/$HEAVY_SLICE/" "/proc/$pid/cgroup" 2>/dev/null && return 0
        kill -0 "$pid" 2>/dev/null || return 0
        sleep 0.05
    done
}

# True when systemd can create a user scope here.
scope_available() {
    command -v systemd-run >/dev/null 2>&1 \
        && systemd-run --user --scope --quiet -p MemoryMax=1M true >/dev/null 2>&1
}

# Resolve the slice budget and desktop reserve (heavy_max, heavy_kib,
# reserve_kib); exits 2 on an invalid size or a cap above the budget.
budget_setup() {
    # Operator override without editing every checkout: the env var wins, then the first
    # line of ~/.config/lushtext/heavy-memory-max, then the default.
    local heavy_file heavy_default reserve size
    heavy_file="${XDG_CONFIG_HOME:-$HOME/.config}/lushtext/heavy-memory-max"
    heavy_default=48G
    [ -r "$heavy_file" ] && read -r heavy_default <"$heavy_file" || true
    heavy_max=${LUSHTEXT_HEAVY_MEMORY_MAX:-$heavy_default}
    reserve=${LUSHTEXT_MEMORY_RESERVE:-16G}
    for size in "$heavy_max" "$reserve"; do
        [[ "$size" =~ ^[0-9]+[KMGT]?$ ]] || { echo "with-memory-cap: invalid size '$size'" >&2; exit 2; }
    done
    heavy_kib=$(cap_in_kib "$heavy_max")
    reserve_kib=$(cap_in_kib "$reserve")
    if [ "$cap_kib" -gt "$heavy_kib" ]; then
        echo "with-memory-cap: cap $cap exceeds the host-wide budget $heavy_max (LUSHTEXT_HEAVY_MEMORY_MAX)" >&2
        exit 2
    fi
}

# Lower RLIMIT_AS (KiB) to $1, never raising a lower limit already in force.
lower_address_space() {
    local current
    current=$(ulimit -v)
    if [ "$current" = unlimited ] || [ "$current" -gt "$1" ]; then
        ulimit -v "$1"
    fi
}

self_test_failures=0
# Record one self-test failure and keep going, so one run reports every
# failing case (a red run of a new case must not hide the next one).
self_test_fail() {
    echo "self-test: $*" >&2
    self_test_failures=$((self_test_failures + 1))
}

# The self-test's private slices: stopped, then reverted (a stopped slice keeps
# its runtime drop-in directory until it is reverted), on every exit path.
# Nothing but this run uses them.
# shellcheck disable=SC2329 # the self-test's EXIT trap
self_test_cleanup() {
    local pid
    for pid in "${self_test_sleepers[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    systemctl --user stop "${self_test_slices[@]}" >/dev/null 2>&1 || true
    systemctl --user revert "${self_test_slices[@]}" >/dev/null 2>&1 || true
}

# Start `sleep 10` in a 2G scope of slice $1 and wait until the scope's cgroup
# exists, so an admission after this call counts it.
self_test_sleeper() {
    local slice=$1 cgroup
    systemd-run --user --scope --quiet --slice="$slice" -p MemoryMax=2G -p MemorySwapMax=0 sleep 10 &
    self_test_sleepers+=("$!")
    for _ in $(seq 100); do
        cgroup=$(systemctl --user show -p ControlGroup --value "$slice" 2>/dev/null || true)
        if [ -n "$cgroup" ] && compgen -G "/sys/fs/cgroup$cgroup/*.scope/memory.max" >/dev/null; then
            return 0
        fi
        sleep 0.05
    done
    return 1
}

# The readings of one fixture: the first call disagrees on the CPU half, the
# second agrees (a host cpuset flip between two reads).
# shellcheck disable=SC2329 # a reader placement_check calls by name
self_test_flip_reader() {
    pl_own=/u/user.slice/user-lushtext.slice/user-lushtext-heavy.slice/run-1.scope
    pl_inner=/u/user.slice
    pl_heavy=/u/user.slice/user-lushtext.slice/user-lushtext-heavy.slice
    pl_aff=8-23
    pl_cpus=8-23
    if [ "${self_test_flipped:-0}" = 0 ]; then
        self_test_flipped=1
        pl_eff=12-23
    else
        pl_eff=8-23
    fi
}

self_test() {
    local me=$0 status
    # Probe fresh scopes whatever this process inherited: run inside a capped
    # scope (a capped `make` or `git commit` running the policy checks), every
    # probe below would otherwise run in place and read the outer scope's
    # limits. The nested-call check sets the variable explicitly.
    unset LUSHTEXT_MEMORY_CAP_KIB LUSHTEXT_ADMISSION_NOWAIT
    # Every scope goes to a private, per-run slice below the real one. A scope
    # start applies the budget (LUSHTEXT_HEAVY_MEMORY_MAX) to its slice, and
    # the misfit cases below use 2G and 3G budgets: on the real slice that left
    # every agent's heavy work capped at 2 GiB host-wide until the next
    # ordinary call. Per run, not shared: with legacy accounting a shared
    # self-test slice would count other worktrees' concurrent self-test scopes
    # in both directions.
    local real_slice=$HEAVY_SLICE real_max
    real_max=$(systemctl --user show -p MemoryMax --value "$real_slice" 2>/dev/null || true)
    local shared_slice=${real_slice%.slice}-selftest.slice
    export LUSHTEXT_HEAVY_SLICE=${real_slice%.slice}-selftest-$$.slice
    HEAVY_SLICE=$LUSHTEXT_HEAVY_SLICE
    local legacy_slice=${HEAVY_SLICE#user-}
    self_test_slices=("$HEAVY_SLICE")
    [ "$legacy_slice" = "$HEAVY_SLICE" ] || self_test_slices+=("$legacy_slice")
    self_test_sleepers=()
    trap self_test_cleanup EXIT
    "$me" none true || self_test_fail "'none' did not pass through"
    status=0; "$me" 16GiB true 2>/dev/null || status=$?
    [ "$status" -eq 2 ] || self_test_fail "invalid cap accepted (status $status)"
    # The guard: an old slice name is refused with the new prefix named.
    local guard
    status=0; guard=$(LUSHTEXT_HEAVY_SLICE=lushtext-heavy-selftest.slice "$me" 64M true 2>&1) || status=$?
    [ "$status" -eq 2 ] && [[ "$guard" == *user-lushtext-heavy* ]] \
        || self_test_fail "LUSHTEXT_HEAVY_SLICE=lushtext-heavy-selftest.slice gave status $status ('$guard'), expected 2 naming user-lushtext-heavy"
    # The placement checker, on fixtures (no scope needed).
    local r=/u/user.slice h=/u/user.slice/user-lushtext.slice/user-lushtext-heavy.slice fixture expected
    for fixture in \
        "0|$h/run-1.scope|$r|$h|8-23|8-23|8-23" \
        "78|/u/user.slice/libpod-1.scope/container|$r|$h|8-23|8-23|8-23" \
        "78|/u/lushtext.slice/lushtext-heavy.slice/run-1.scope|$r|/u/lushtext.slice/lushtext-heavy.slice|8-23|8-23|8-23" \
        "78|$h/run-1.scope|$r|$h|0-23|8-23|8-23" \
        "0|$h/run-1.scope|$r|$h|0-23||0-23" \
        "0|$h/run-1.scope|$r|$h|8-11,16,20-23|8-23|8-23" \
        "78||$r|$h|8-23|8-23|8-23"; do
        IFS='|' read -r expected a b c d e f <<<"$fixture"
        status=0; "$me" --placement-verdict "$a" "$b" "$c" "$d" "$e" "$f" 2>/dev/null || status=$?
        [ "$status" -eq "$expected" ] || self_test_fail "--placement-verdict '$a' '$b' '$c' '$d' '$e' '$f' gave status $status, expected $expected"
    done
    local message
    message=$("$me" --placement-verdict "$h/run-1.scope" "$r" "$h" 0-23 8-23 8-23 2>&1 || true)
    [[ "$message" == "with-memory-cap: scope placement: expected "*", got "* && "$message" == *0-23* && "$message" == *8-23* ]] \
        || self_test_fail "a CPU refusal did not name both lists: '$message'"
    message=$("$me" --placement-verdict "" "$r" "$h" 8-23 8-23 8-23 2>&1 || true)
    [[ "$message" == *"/proc/self/cgroup"* ]] || self_test_fail "a failed read did not name the read: '$message'"
    self_test_flipped=0
    status=0; placement_check unused self_test_flip_reader 2>/dev/null || status=$?
    [ "$status" -eq 0 ] || self_test_fail "a CPU disagreement that the re-read resolves was refused (status $status)"
    [ "$(read_cpuset_file /nonexistent/cpuset.cpus)" = "" ] || self_test_fail "a missing cpuset.cpus file did not read as empty"
    if command -v systemd-run >/dev/null 2>&1 \
        && systemd-run --user --scope --quiet -p MemoryMax=1M true >/dev/null 2>&1; then
        local policy
        # shellcheck disable=SC2016 # expanded by the inner shell, inside the scope
        policy=$("$me" 64M sh -c 'u=$(sed -n "s|^0::.*/||p" /proc/self/cgroup); systemctl --user show -p OOMPolicy --value "$u"' 2>/dev/null)
        [ "$policy" = "continue" ] || self_test_fail "scope OOMPolicy was '$policy', expected 'continue'"
        # Read the scope's own limits rather than provoking the OOM killer: an
        # OOM kill in a user scope raises a desktop notification on GNOME.
        local limits
        # shellcheck disable=SC2016 # expanded by the inner shell, inside the scope
        limits=$("$me" 64M sh -c 'p=$(sed -n "s/^0:://p" /proc/self/cgroup); cat "/sys/fs/cgroup$p/memory.max" "/sys/fs/cgroup$p/memory.swap.max"' 2>/dev/null | tr '\n' ' ')
        [ "$limits" = "67108864 0 " ] || self_test_fail "scope limits were '$limits', expected '67108864 0 '"
        # Placement: under the inner user.slice and the slice --print-slice
        # names (the one source of the slice name).
        local printed own inner_cg heavy_cg
        printed=$("$me" --print-slice 2>/dev/null || true)
        [ "$printed" = "$HEAVY_SLICE" ] || self_test_fail "--print-slice printed '$printed', expected '$HEAVY_SLICE'"
        own=$("$me" 64M sed -n 's/^0:://p' /proc/self/cgroup 2>/dev/null || true)
        inner_cg=$(systemctl --user show -p ControlGroup --value user.slice 2>/dev/null || true)
        heavy_cg=$(systemctl --user show -p ControlGroup --value "${printed:-$HEAVY_SLICE}" 2>/dev/null || true)
        [ -n "$inner_cg" ] && [ -n "$heavy_cg" ] && [[ "$own" == "$inner_cg/"* && "$own" == "$heavy_cg/"* ]] \
            || self_test_fail "scope path '$own' is not under user.slice ('$inner_cg') and the heavy slice ('$heavy_cg')"
        # No CPU property on the scope, the heavy slice or the family slice.
        local family=${real_slice%-heavy.slice}.slice props
        # shellcheck disable=SC2016 # expanded by the inner shell, inside the scope
        props=$("$me" 64M sh -c 'u=$(sed -n "s|^0::.*/||p" /proc/self/cgroup); systemctl --user show -p CPUWeight -p AllowedCPUs "$u" "$0" "$1"' "$HEAVY_SLICE" "$family" 2>/dev/null)
        [ "$(grep -cx 'CPUWeight=\[not set\]' <<<"$props")" = 3 ] && [ "$(grep -cx 'AllowedCPUs=' <<<"$props")" = 3 ] \
            || self_test_fail "a CPU property is set on the scope, $HEAVY_SLICE or $family: $(tr '\n' ' ' <<<"$props")"
        # The scope's CPUs come from the inner user.slice's cpuset, not from a
        # caller that is wider (every online CPU) or disjoint (one CPU outside
        # it). Skipped when the cpuset is unconfigured or covers every online
        # CPU: there is nothing to narrow to.
        local inner_cpus inner_eff outside cpus_case=skipped
        inner_cpus=$(read_cpuset_file "/sys/fs/cgroup$inner_cg/cpuset.cpus")
        inner_eff=$(read_cpuset_file "/sys/fs/cgroup$inner_cg/cpuset.cpus.effective")
        outside=
        [ -z "$inner_cpus" ] || [ -z "$inner_eff" ] || outside=$(cpu_outside_cpuset "$inner_eff")
        if [ -n "$outside" ]; then
            local wide
            wide=$(taskset -c "$(cpu_online)" "$me" 64M sed -n 's/^Cpus_allowed_list:[[:space:]]*//p' /proc/self/status 2>/dev/null || true)
            if [ -z "$wide" ] || ! cpu_list_subset "$wide" "$inner_eff"; then
                self_test_fail "a caller on every online CPU got '$wide' in the scope, expected a subset of user.slice's $inner_eff"
            fi
            # A caller that cannot exist inside the toolbox (the kernel refuses
            # a mask disjoint from its cpuset): start it in an app.slice scope,
            # with an explicit unit name (taskset and the wrapper exec, so the
            # wrapper's own systemd-run keeps this PID and the default
            # run-p<pid> name would collide).
            local disjoint
            # shellcheck disable=SC2016 # expanded by the inner shell, inside the scope
            disjoint=$(systemd-run --user --scope --quiet --unit="lushtext-selftest-caller-$$" --slice=app.slice -p MemoryMax=64M \
                taskset -c "$outside" "$me" 64M sh -c 'sed -n "s/^Cpus_allowed_list:[[:space:]]*//p" /proc/self/status; taskset -c "$0" true 2>/dev/null && echo accepted || echo refused' "$outside" 2>/dev/null | tr '\n' '|')
            if [[ "$disjoint" != *"|refused|" ]] || ! cpu_list_equal "${disjoint%%|*}" "$inner_eff"; then
                self_test_fail "a caller pinned to CPU $outside got '$disjoint' in the scope, expected user.slice's $inner_eff and a refused taskset -c $outside"
            fi
            cpus_case="a caller on every online CPU narrowed, one pinned to CPU $outside given $inner_eff"
        fi
        # Refusal in a real process: the shim run outside its slice (in this
        # process's own cgroup) exits 78 and does not run the command. After
        # the scopes above, so the slice has a ControlGroup and the refusal is
        # the placement, not a failed read.
        local marker
        marker=$(mktemp -u "${TMPDIR:-/tmp}/with-memory-cap-marker.XXXXXX")
        status=0
        message=$(bash "${BASH_SOURCE[0]}" --exec-in-scope "$HEAVY_SLICE" - touch "$marker" 2>&1) || status=$?
        [ "$status" -eq 78 ] && [ ! -e "$marker" ] && [[ "$message" == *"$heavy_cg"* ]] \
            || self_test_fail "--exec-in-scope outside $HEAVY_SLICE gave status $status ('$message'), expected 78 naming '$heavy_cg' and no marker"
        rm -f "$marker"
        local slice
        # shellcheck disable=SC2016 # expanded by the inner shell, inside the scope
        slice=$("$me" 64M sh -c 'grep -c "/$0/" /proc/self/cgroup' "$HEAVY_SLICE" 2>/dev/null)
        [ "$slice" = 1 ] || self_test_fail "scope is not inside $HEAVY_SLICE"
        local nested
        # shellcheck disable=SC2016 # expanded by the inner shell
        nested=$(LUSHTEXT_MEMORY_CAP_KIB=1 "$me" 64M sh -c 'echo "$LUSHTEXT_MEMORY_CAP_KIB"' 2>/dev/null)
        [ "$nested" = 1 ] || self_test_fail "a nested call did not run in place (got '$nested')"
        status=0
        "$me" --address-space 1G python3 -c 'b = bytearray(2 * 1024 * 1024 * 1024)' 2>/dev/null || status=$?
        [ "$status" -eq 1 ] || self_test_fail "--address-space did not fail the allocation cleanly (status $status)"
        # The command must see Ctrl-C (SIGINT) and SIGQUIT as the caller does.
        # Started as a background job of this non-interactive shell, it used to
        # inherit both as ignored, so a Ctrl-C left the capped work running.
        local outer inner
        outer=$(sed -n 's/^SigIgn:[[:space:]]*//p' /proc/self/status)
        inner=$("$me" 64M sed -n 's/^SigIgn:[[:space:]]*//p' /proc/self/status 2>/dev/null)
        [ $((0x${inner:-ff} & 6)) -eq $((0x$outer & 6)) ] || self_test_fail "the capped command ignores SIGINT/SIGQUIT (SigIgn $inner, caller $outer)"
        # RLIMIT_AS belongs to the command alone: the wrapper and systemd-run
        # used to run under it too, and a small cap broke the wrapper itself.
        status=0
        "$me" --address-space 64M true 2>/dev/null || status=$?
        [ "$status" -eq 0 ] || self_test_fail "--address-space 64M broke the wrapper (status $status)"
        # A nested call keeps --address-space, at 7/8 of the smaller cap, and
        # never raises a lower limit it inherited.
        local expected inherited
        inherited=$(ulimit -v)
        expected=$((1048576 * 7 / 8))
        [ "$inherited" = unlimited ] || [ "$inherited" -ge "$expected" ] || expected=$inherited
        nested=$(LUSHTEXT_MEMORY_CAP_KIB=1048576 "$me" --address-space 4G sh -c 'ulimit -v' 2>/dev/null)
        [ "$nested" = "$expected" ] || self_test_fail "a nested --address-space call set RLIMIT_AS '$nested', expected '$expected'"
        # Arguments reach the command byte for byte: systemd-run's default
        # environment expansion turned `$$` into `$`, expanded `$HOME` and
        # `${HOME}`, and dropped a lone `$0`.
        local args
        # shellcheck disable=SC2016 # literal dollars are the point
        args=$("$me" 64M printf '[%s]' 'a$$b' '$0' '${HOME}' '$HOME' 2>/dev/null)
        # shellcheck disable=SC2016 # literal dollars are the point
        [ "$args" = '[a$$b][$0][${HOME}][$HOME]' ] || self_test_fail "arguments were rewritten in the scope: '$args'"
        # Admission that cannot fit: --wait-admissible waits (holding nothing),
        # LUSHTEXT_ADMISSION_NOWAIT=1 fails with 75 without starting a scope.
        # An impossible reserve guarantees the misfit; nothing is admitted.
        status=0
        LUSHTEXT_HEAVY_MEMORY_MAX=2G LUSHTEXT_MEMORY_RESERVE=999999G timeout 1 "$me" --wait-admissible 2G 2>/dev/null || status=$?
        [ "$status" -eq 124 ] || self_test_fail "--wait-admissible did not wait for a misfit (status $status)"
        status=0
        "$me" --wait-admissible 64M || status=$?
        [ "$status" -eq 0 ] || self_test_fail "--wait-admissible waited for a small cap (status $status)"
        status=0
        LUSHTEXT_ADMISSION_NOWAIT=1 LUSHTEXT_HEAVY_MEMORY_MAX=2G LUSHTEXT_MEMORY_RESERVE=999999G timeout 5 "$me" 2G true 2>/dev/null || status=$?
        [ "$status" -eq 75 ] || self_test_fail "LUSHTEXT_ADMISSION_NOWAIT=1 did not fail a misfit with 75 (status $status)"
        # An admission waiting for memory must not hold the admission lock
        # while it sleeps, or a NOWAIT admission behind it blocks instead of
        # failing fast with 75.
        LUSHTEXT_HEAVY_MEMORY_MAX=2G LUSHTEXT_MEMORY_RESERVE=999999G "$me" 2G true 2>/dev/null &
        local waiter=$!
        sleep 1
        status=0
        LUSHTEXT_ADMISSION_NOWAIT=1 LUSHTEXT_HEAVY_MEMORY_MAX=2G LUSHTEXT_MEMORY_RESERVE=999999G timeout 5 "$me" 2G true 2>/dev/null || status=$?
        kill "$waiter" 2>/dev/null; wait "$waiter" 2>/dev/null || true
        [ "$status" -eq 75 ] || self_test_fail "a NOWAIT admission blocked behind an admission waiting for memory (status $status)"
        # Isolation: a scope in the shared self-test slice (where no run counts
        # scopes, and where other worktrees' self-tests of either wrapper put
        # theirs) is not counted by this run's admission, and an admission
        # with no legacy scope says nothing about a legacy slice.
        local admission
        if self_test_sleeper "$shared_slice"; then
            status=0
            admission=$(LUSHTEXT_ADMISSION_NOWAIT=1 LUSHTEXT_HEAVY_MEMORY_MAX=3G LUSHTEXT_MEMORY_RESERVE=1G timeout 10 "$me" 2G true 2>&1) || status=$?
            [ "$status" -eq 0 ] || self_test_fail "a scope in the shared $shared_slice was counted against this run (status $status: $admission)"
            [[ " $admission" != *[!-]"$legacy_slice"* ]] || [ "$legacy_slice" = "$HEAVY_SLICE" ] || self_test_fail "an admission with no legacy scope mentioned $legacy_slice: $admission"
        else
            self_test_fail "the isolation sleeper never started in $shared_slice"
        fi
        # Legacy accounting: a scope of an old wrapper copy (the legacy slice of
        # the same family) counts against admission, and an admission to the
        # current slice leaves the legacy slice's limits and drop-ins alone.
        local legacy_case=skipped
        if [ "$legacy_slice" != "$HEAVY_SLICE" ]; then
            if self_test_sleeper "$legacy_slice"; then
                status=0
                LUSHTEXT_ADMISSION_NOWAIT=1 LUSHTEXT_HEAVY_MEMORY_MAX=3G LUSHTEXT_MEMORY_RESERVE=1G timeout 10 "$me" 2G true 2>/dev/null || status=$?
                [ "$status" -eq 75 ] || self_test_fail "a 2G scope in the legacy $legacy_slice was not counted (status $status, expected 75)"
                local legacy_before legacy_after
                legacy_before=$(systemctl --user show -p MemoryMax -p DropInPaths "$legacy_slice" 2>/dev/null || true)
                status=0
                LUSHTEXT_ADMISSION_NOWAIT=1 LUSHTEXT_HEAVY_MEMORY_MAX=8G LUSHTEXT_MEMORY_RESERVE=1G timeout 10 "$me" 2G true 2>/dev/null || status=$?
                legacy_after=$(systemctl --user show -p MemoryMax -p DropInPaths "$legacy_slice" 2>/dev/null || true)
                [ "$status" -eq 0 ] || self_test_fail "an admission that fits beside the legacy scope failed (status $status)"
                [ "$legacy_before" = "$legacy_after" ] || self_test_fail "an admission changed the legacy slice: '$legacy_before' became '$legacy_after'"
                legacy_case="a legacy scope counted, the legacy slice untouched"
            else
                self_test_fail "the legacy sleeper never started in $legacy_slice"
            fi
        fi
        local after_max
        after_max=$(systemctl --user show -p MemoryMax --value "$real_slice" 2>/dev/null || true)
        [ "$after_max" = "$real_max" ] || self_test_fail "the real $real_slice MemoryMax changed from '$real_max' to '$after_max'"
        [ "$self_test_failures" -eq 0 ] || { echo "with-memory-cap self-test: $self_test_failures case(s) failed" >&2; return 1; }
        echo "with-memory-cap self-test: ok (scope sets MemoryMax, no swap and OOMPolicy=continue inside $HEAVY_SLICE, under the user manager's user.slice, with no CPU property; CPUs: $cpus_case; the placement check refuses a process outside its slice (78) and re-reads a CPU disagreement; old slice names refused; nested calls run in place; --address-space fails an allocation cleanly; Ctrl-C reaches the command; arguments with \$ pass unexpanded; --wait-admissible waits without reserving; NOWAIT admission fails fast, even behind a waiter; a shared self-test slice's scope not counted; legacy: $legacy_case; the real slice's budget untouched)"
    else
        [ "$self_test_failures" -eq 0 ] || { echo "with-memory-cap self-test: $self_test_failures case(s) failed" >&2; return 1; }
        echo "with-memory-cap self-test: ok (no systemd user manager; cap enforcement not exercised)"
    fi
}

# Internal: every scope's first process. It checks its own placement (exit 78
# before the command runs when systemd put it elsewhere), then applies
# --address-space to the command alone (not to this wrapper or systemd-run;
# `-` for none), then becomes the command.
if [ "${1:-}" = "--exec-in-scope" ]; then
    [ "$#" -ge 4 ] || { echo "usage: $0 --exec-in-scope <slice> <as-kib|-> <command> [args...]" >&2; exit 2; }
    placement_check "$2" || exit 78
    [ "$3" = - ] || lower_address_space "$3"
    shift 3
    exec "$@"
fi

# Self-test entry to the pure checker: six values, exit 0 or 78.
if [ "${1:-}" = "--placement-verdict" ]; then
    [ "$#" -eq 7 ] || { echo "usage: $0 --placement-verdict <own-cgroup> <user.slice-cgroup> <heavy-cgroup> <affinity> <cpuset.cpus> <cpuset.cpus.effective>" >&2; exit 2; }
    shift
    if message=$(placement_verdict "$@"); then
        exit 0
    fi
    echo "$message" >&2
    exit 78
fi

if [ "${1:-}" = "--print-slice" ]; then
    echo "$HEAVY_SLICE"
    exit 0
fi

if [ "${1:-}" = "--self-test-run" ]; then
    # A signal still runs the EXIT trap that removes the private slices.
    trap 'exit 130' INT
    trap 'exit 143' TERM
    trap 'exit 129' HUP
    self_test
    exit
fi

if [ "${1:-}" = "--self-test" ]; then
    # The cases run in a child, whose PID names its private slices, so this
    # process can check after it returns that nothing of them was left behind
    # (a stopped but unreverted slice keeps its runtime drop-in directory
    # until logout). Exactly this child's PID: other worktrees' self-tests
    # have live directories of their own.
    bash "${BASH_SOURCE[0]}" --self-test-run &
    self_test_child=$!
    trap 'kill -TERM "$self_test_child" 2>/dev/null || true' INT TERM HUP
    while :; do
        wait "$self_test_child" && status=0 || status=$?
        kill -0 "$self_test_child" 2>/dev/null || break
    done
    leftovers=$(compgen -G "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/systemd/user.control/*selftest-$self_test_child.slice.d" || true)
    if [ -n "$leftovers" ]; then
        echo "self-test: the private slices left drop-in directories behind: $leftovers" >&2
        exit 1
    fi
    exit "$status"
fi

if [ "${1:-}" = "--wait-admissible" ]; then
    [ "$#" -eq 2 ] || { echo "usage: $0 --wait-admissible <cap|none>" >&2; exit 2; }
    cap=$2
    [ "$cap" != none ] || exit 0
    [[ "$cap" =~ ^[0-9]+[KMGT]?$ ]] || { echo "with-memory-cap: invalid cap '$cap' (expected e.g. 16G, 12288M, or none)" >&2; exit 2; }
    cap_kib=$(cap_in_kib "$cap")
    # Nested calls run in place and small caps skip admission: nothing to wait for.
    [ -z "${LUSHTEXT_MEMORY_CAP_KIB:-}" ] || exit 0
    [ "$cap_kib" -gt $((1024 * 1024)) ] || exit 0
    scope_available || exit 0
    budget_setup
    announced=0
    while ! fits "$cap_kib" "$heavy_kib" "$reserve_kib"; do
        # A check-once probe (scripts/heavy.sh re-checks after its locks).
        [ "${LUSHTEXT_ADMISSION_NOWAIT:-}" != 1 ] || exit 75
        [ "$announced" = 1 ] || { waiting_message; announced=1; }
        sleep 15
    done
    exit 0
fi

address_space=0
if [ "${1:-}" = "--address-space" ]; then
    address_space=1
    shift
fi

if [ "$#" -lt 2 ]; then
    echo "usage: $0 [--address-space] <cap|none> <command> [args...]" >&2
    exit 2
fi

cap=$1
shift

if [ "$cap" = "none" ]; then
    exec "$@"
fi

if ! [[ "$cap" =~ ^[0-9]+[KMGT]?$ ]]; then
    echo "with-memory-cap: invalid cap '$cap' (expected e.g. 16G, 12288M, or none)" >&2
    exit 2
fi

cap_kib=$(cap_in_kib "$cap")

# The command as this wrapper reports it: whole when short, else its first
# words and a count, so a command carrying hundreds of test names (a widget
# shard) does not flood the log with one line.
shown_command="$*"
if [ "${#shown_command}" -gt 200 ]; then
    shown_command="${*:1:4} ... (+$(($# - 4)) more arguments)"
fi

if [ -n "${LUSHTEXT_MEMORY_CAP_KIB:-}" ]; then
    echo "with-memory-cap: already inside a ${LUSHTEXT_MEMORY_CAP_KIB} KiB capped scope; running in place: $shown_command" >&2
    if [ "$address_space" = 1 ]; then
        # The outer scope binds, so the per-process limit follows the smaller cap.
        inner_kib=$cap_kib
        [[ "$LUSHTEXT_MEMORY_CAP_KIB" =~ ^[0-9]+$ ]] && [ "$LUSHTEXT_MEMORY_CAP_KIB" -lt "$inner_kib" ] && inner_kib=$LUSHTEXT_MEMORY_CAP_KIB
        lower_address_space $((inner_kib * 7 / 8))
    fi
    exec "$@"
fi

if scope_available; then
    budget_setup
    systemctl --user set-property --runtime "$HEAVY_SLICE" \
        MemoryMax="$heavy_max" MemorySwapMax=0 >/dev/null 2>&1 || true
    admitted=0
    if [ "$cap_kib" -gt $((1024 * 1024)) ]; then
        admit
        admitted=1
    fi
    launch=(bash "${BASH_SOURCE[0]}" --exec-in-scope "$HEAVY_SLICE" - "$@")
    if [ "$address_space" = 1 ]; then
        launch=(bash "${BASH_SOURCE[0]}" --exec-in-scope "$HEAVY_SLICE" $((cap_kib * 7 / 8)) "$@")
        echo "with-memory-cap: MemoryMax=$cap MemorySwapMax=0 OOMPolicy=continue in $HEAVY_SLICE ($heavy_max), RLIMIT_AS=$((cap_kib * 7 / 8)) KiB per process, for: $shown_command" >&2
    else
        echo "with-memory-cap: MemoryMax=$cap MemorySwapMax=0 OOMPolicy=continue in $HEAVY_SLICE ($heavy_max) for: $shown_command" >&2
    fi
    export LUSHTEXT_MEMORY_CAP_KIB=$cap_kib
    if [ "$admitted" = 1 ]; then
        # The admission lock passes to a detached watcher (the subshell exits at
        # once, so the command never inherits a child it did not start), which
        # releases it once this process's scope exists.
        ( release_admission_once_scoped "$$" </dev/null >/dev/null 2>&1 & )
    fi
    # exec, not a background job: a background job of a non-interactive shell
    # ignores SIGINT and SIGQUIT, and every descendant inherited that, so a
    # Ctrl-C left the capped work running. As the same process, the command
    # gets the terminal's signals and its exit status reaches the caller
    # directly. fd 9 is closed so the command does not hold the lock.
    exec systemd-run --user --scope --quiet --expand-environment=no --slice="$HEAVY_SLICE" \
        -p MemoryMax="$cap" -p MemorySwapMax=0 -p OOMPolicy=continue -- "${launch[@]}" 9>&-
fi

if [ -n "${CI:-}" ]; then
    echo "with-memory-cap: no systemd user manager on CI; running uncapped: $shown_command" >&2
    exec "$@"
fi

echo "with-memory-cap: cannot create a memory-capped scope (systemd-run --user unavailable)." >&2
echo "Refusing to run uncapped. Pass the cap 'none' (e.g. KANI_MEMORY_MAX=none) to opt out explicitly." >&2
exit 1
