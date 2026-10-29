#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Run one command inside a memory-capped cgroup scope, so a model checker that
# explodes is killed alone instead of pushing the desktop into swap.
#
#   scripts/with-memory-cap.sh [--address-space] <cap> <command> [args...]
#   scripts/with-memory-cap.sh --self-test
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
# `lushtext-heavy.slice`, whose own `MemoryMax` (LUSHTEXT_HEAVY_MEMORY_MAX,
# default 72G, no swap) bounds the SUM of all capped work on the host, however
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

HEAVY_SLICE=lushtext-heavy.slice
ADMISSION_LOCK="${XDG_RUNTIME_DIR:-/tmp}/lushtext-heavy-admission.lock"

mem_available_kib() {
    awk '/^MemAvailable:/ { print $2 }' /proc/meminfo
}

# Sum, in KiB, of the caps of the scopes currently in the heavy slice.
admitted_kib() {
    local cgroup dir total=0 limit
    cgroup=$(systemctl --user show -p ControlGroup --value "$HEAVY_SLICE" 2>/dev/null || true)
    dir="/sys/fs/cgroup$cgroup"
    if [ -n "$cgroup" ] && [ -d "$dir" ]; then
        for limit in "$dir"/*.scope/memory.max; do
            [ -r "$limit" ] || continue
            read -r limit <"$limit" || continue
            [ "$limit" = max ] && continue
            total=$((total + limit / 1024))
        done
    fi
    echo "$total"
}

# Hold the admission lock until this cap fits the slice budget and the
# desktop reserve; fd 9 stays locked until the caller's scope exists
# (release_admission_once_scoped).
admit() {
    local cap_kib=$1 heavy_kib=$2 reserve_kib=$3 admitted available announced=0
    exec 9>"$ADMISSION_LOCK"
    flock 9
    while :; do
        admitted=$(admitted_kib)
        available=$(mem_available_kib)
        if [ $((admitted + cap_kib)) -le "$heavy_kib" ] \
            && [ "$available" -ge $((cap_kib + reserve_kib)) ]; then
            return 0
        fi
        if [ "$announced" = 0 ]; then
            echo "with-memory-cap: waiting for memory: $((admitted / 1048576)) GiB of $((heavy_kib / 1048576)) GiB admitted to $HEAVY_SLICE, $((available / 1048576)) GiB available, need $((cap_kib / 1048576)) GiB plus a $((reserve_kib / 1048576)) GiB reserve" >&2
            announced=1
        fi
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

# Lower RLIMIT_AS (KiB) to $1, never raising a lower limit already in force.
lower_address_space() {
    local current
    current=$(ulimit -v)
    if [ "$current" = unlimited ] || [ "$current" -gt "$1" ]; then
        ulimit -v "$1"
    fi
}

self_test() {
    local me=$0 status
    # Probe fresh scopes whatever this process inherited: run inside a capped
    # scope (a capped `make` or `git commit` running the policy checks), every
    # probe below would otherwise run in place and read the outer scope's
    # limits. The nested-call check sets the variable explicitly.
    unset LUSHTEXT_MEMORY_CAP_KIB
    "$me" none true || { echo "self-test: 'none' did not pass through" >&2; return 1; }
    status=0; "$me" 16GiB true 2>/dev/null || status=$?
    [ "$status" -eq 2 ] || { echo "self-test: invalid cap accepted (status $status)" >&2; return 1; }
    if command -v systemd-run >/dev/null 2>&1 \
        && systemd-run --user --scope --quiet -p MemoryMax=1M true >/dev/null 2>&1; then
        local policy
        # shellcheck disable=SC2016 # expanded by the inner shell, inside the scope
        policy=$("$me" 64M sh -c 'u=$(sed -n "s|^0::.*/||p" /proc/self/cgroup); systemctl --user show -p OOMPolicy --value "$u"' 2>/dev/null)
        [ "$policy" = "continue" ] || { echo "self-test: scope OOMPolicy was '$policy', expected 'continue'" >&2; return 1; }
        # Read the scope's own limits rather than provoking the OOM killer: an
        # OOM kill in a user scope raises a desktop notification on GNOME.
        local limits
        # shellcheck disable=SC2016 # expanded by the inner shell, inside the scope
        limits=$("$me" 64M sh -c 'p=$(sed -n "s/^0:://p" /proc/self/cgroup); cat "/sys/fs/cgroup$p/memory.max" "/sys/fs/cgroup$p/memory.swap.max"' 2>/dev/null | tr '\n' ' ')
        [ "$limits" = "67108864 0 " ] || { echo "self-test: scope limits were '$limits', expected '67108864 0 '" >&2; return 1; }
        local slice
        # shellcheck disable=SC2016 # expanded by the inner shell, inside the scope
        slice=$("$me" 64M sh -c 'grep -c "/$0/" /proc/self/cgroup' "$HEAVY_SLICE" 2>/dev/null)
        [ "$slice" = 1 ] || { echo "self-test: scope is not inside $HEAVY_SLICE" >&2; return 1; }
        local nested
        # shellcheck disable=SC2016 # expanded by the inner shell
        nested=$(LUSHTEXT_MEMORY_CAP_KIB=1 "$me" 64M sh -c 'echo "$LUSHTEXT_MEMORY_CAP_KIB"' 2>/dev/null)
        [ "$nested" = 1 ] || { echo "self-test: a nested call did not run in place (got '$nested')" >&2; return 1; }
        status=0
        "$me" --address-space 1G python3 -c 'b = bytearray(2 * 1024 * 1024 * 1024)' 2>/dev/null || status=$?
        [ "$status" -eq 1 ] || { echo "self-test: --address-space did not fail the allocation cleanly (status $status)" >&2; return 1; }
        # The command must see Ctrl-C (SIGINT) and SIGQUIT as the caller does.
        # Started as a background job of this non-interactive shell, it used to
        # inherit both as ignored, so a Ctrl-C left the capped work running.
        local outer inner
        outer=$(sed -n 's/^SigIgn:[[:space:]]*//p' /proc/self/status)
        inner=$("$me" 64M sed -n 's/^SigIgn:[[:space:]]*//p' /proc/self/status 2>/dev/null)
        [ $((0x$inner & 6)) -eq $((0x$outer & 6)) ] || { echo "self-test: the capped command ignores SIGINT/SIGQUIT (SigIgn $inner, caller $outer)" >&2; return 1; }
        # RLIMIT_AS belongs to the command alone: the wrapper and systemd-run
        # used to run under it too, and a small cap broke the wrapper itself.
        status=0
        "$me" --address-space 64M true 2>/dev/null || status=$?
        [ "$status" -eq 0 ] || { echo "self-test: --address-space 64M broke the wrapper (status $status)" >&2; return 1; }
        # A nested call keeps --address-space, at 7/8 of the smaller cap, and
        # never raises a lower limit it inherited.
        local expected inherited
        inherited=$(ulimit -v)
        expected=$((1048576 * 7 / 8))
        [ "$inherited" = unlimited ] || [ "$inherited" -ge "$expected" ] || expected=$inherited
        nested=$(LUSHTEXT_MEMORY_CAP_KIB=1048576 "$me" --address-space 4G sh -c 'ulimit -v' 2>/dev/null)
        [ "$nested" = "$expected" ] || { echo "self-test: a nested --address-space call set RLIMIT_AS '$nested', expected '$expected'" >&2; return 1; }
        echo "with-memory-cap self-test: ok (scope sets MemoryMax, no swap and OOMPolicy=continue inside $HEAVY_SLICE; nested calls run in place; --address-space fails an allocation cleanly; Ctrl-C reaches the command)"
    else
        echo "with-memory-cap self-test: ok (no systemd user manager; cap enforcement not exercised)"
    fi
}

# Internal: the scope's first process applies --address-space to the command
# alone (not to this wrapper or systemd-run), then becomes the command.
if [ "${1:-}" = "--exec-with-address-space" ]; then
    lower_address_space "$2"
    shift 2
    exec "$@"
fi

if [ "${1:-}" = "--self-test" ]; then
    self_test
    exit
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

if [ -n "${LUSHTEXT_MEMORY_CAP_KIB:-}" ]; then
    echo "with-memory-cap: already inside a ${LUSHTEXT_MEMORY_CAP_KIB} KiB capped scope; running in place: $*" >&2
    if [ "$address_space" = 1 ]; then
        # The outer scope binds, so the per-process limit follows the smaller cap.
        inner_kib=$cap_kib
        [[ "$LUSHTEXT_MEMORY_CAP_KIB" =~ ^[0-9]+$ ]] && [ "$LUSHTEXT_MEMORY_CAP_KIB" -lt "$inner_kib" ] && inner_kib=$LUSHTEXT_MEMORY_CAP_KIB
        lower_address_space $((inner_kib * 7 / 8))
    fi
    exec "$@"
fi

if command -v systemd-run >/dev/null 2>&1 \
    && systemd-run --user --scope --quiet -p MemoryMax=1M true >/dev/null 2>&1; then
    heavy_max=${LUSHTEXT_HEAVY_MEMORY_MAX:-72G}
    reserve=${LUSHTEXT_MEMORY_RESERVE:-16G}
    for size in "$heavy_max" "$reserve"; do
        [[ "$size" =~ ^[0-9]+[KMGT]?$ ]] || { echo "with-memory-cap: invalid size '$size'" >&2; exit 2; }
    done
    heavy_kib=$(cap_in_kib "$heavy_max")
    if [ "$cap_kib" -gt "$heavy_kib" ]; then
        echo "with-memory-cap: cap $cap exceeds the host-wide budget $heavy_max (LUSHTEXT_HEAVY_MEMORY_MAX)" >&2
        exit 2
    fi
    systemctl --user set-property --runtime "$HEAVY_SLICE" \
        MemoryMax="$heavy_max" MemorySwapMax=0 >/dev/null 2>&1 || true
    admitted=0
    if [ "$cap_kib" -gt $((1024 * 1024)) ]; then
        admit "$cap_kib" "$heavy_kib" "$(cap_in_kib "$reserve")"
        admitted=1
    fi
    launch=("$@")
    if [ "$address_space" = 1 ]; then
        launch=(bash "${BASH_SOURCE[0]}" --exec-with-address-space $((cap_kib * 7 / 8)) "$@")
        echo "with-memory-cap: MemoryMax=$cap MemorySwapMax=0 OOMPolicy=continue in $HEAVY_SLICE ($heavy_max), RLIMIT_AS=$((cap_kib * 7 / 8)) KiB per process, for: $*" >&2
    else
        echo "with-memory-cap: MemoryMax=$cap MemorySwapMax=0 OOMPolicy=continue in $HEAVY_SLICE ($heavy_max) for: $*" >&2
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
    exec systemd-run --user --scope --quiet --slice="$HEAVY_SLICE" \
        -p MemoryMax="$cap" -p MemorySwapMax=0 -p OOMPolicy=continue -- "${launch[@]}" 9>&-
fi

if [ -n "${CI:-}" ]; then
    echo "with-memory-cap: no systemd user manager on CI; running uncapped: $*" >&2
    exec "$@"
fi

echo "with-memory-cap: cannot create a memory-capped scope (systemd-run --user unavailable)." >&2
echo "Refusing to run uncapped. Pass the cap 'none' (e.g. KANI_MEMORY_MAX=none) to opt out explicitly." >&2
exit 1
