# SPDX-License-Identifier: GPL-3.0-or-later
# shellcheck shell=bash
#
# CPU list helpers shared by scripts/with-memory-cap.sh and scripts/measure.sh
# (sourced, never run). A CPU list is the kernel's syntax, `a-b,c` (the format
# of Cpus_allowed_list, cpuset.cpus and /sys/devices/system/cpu/online); the
# empty string is the empty set. Sets are compared as sets, never as strings:
# `8-11,12-23` and `8-23` are equal.
#
# Host behaviour these helpers are read against (measured 2026-11-15, kernel
# 7.2, cgroup v2; design of nest-heavy-slices-under-user-slice):
#   H2. A cgroup v2 cpuset restricts every descendant, whether or not the
#       cpuset controller is enabled below it, so the nearest
#       cpuset.cpus.effective above a cgroup is the set its tasks may use.
#   H3. The kernel sets a task's affinity to its requested mask intersected
#       with the cpuset, or to the cpuset when they are disjoint;
#       sched_setaffinity with a disjoint mask fails (EINVAL), and a partial
#       overlap succeeds and is silently intersected. Only a read-back of
#       Cpus_allowed_list tells which CPUs a task actually got.

# Print one CPU per line, ascending and without duplicates. Returns 2 on a
# malformed list (callers run it in a command substitution and check).
cpu_list_expand() {
    local list=$1 part a b cpu out=()
    [ -n "$list" ] || return 0
    [[ "$list" =~ ^[0-9]+(-[0-9]+)?(,[0-9]+(-[0-9]+)?)*$ ]] || return 2
    local IFS=,
    for part in $list; do
        if [[ "$part" == *-* ]]; then
            a=$((10#${part%-*}))
            b=$((10#${part#*-}))
            [ "$a" -le "$b" ] || return 2
        else
            a=$((10#$part))
            b=$a
        fi
        for ((cpu = a; cpu <= b; cpu++)); do
            out+=("$cpu")
        done
    done
    printf '%s\n' "${out[@]}" | sort -n -u
}

# Read CPUs (one per line) on stdin and print them as a canonical CPU list.
cpu_list_compress() {
    sort -n -u | awk '
        NF == 0 { next }
        start == "" { start = $1; prev = $1; next }
        $1 == prev + 1 { prev = $1; next }
        { out = out sep (start == prev ? start : start "-" prev); sep = ","; start = $1; prev = $1 }
        END { if (start != "") out = out sep (start == prev ? start : start "-" prev); print out }'
}

# Canonical form of a CPU list (returns 2 when it is malformed).
cpu_list_canonical() {
    local cpus
    cpus=$(cpu_list_expand "$1") || return 2
    printf '%s\n' "$cpus" | cpu_list_compress
}

# The CPUs (one per line) of $1 that are not in $2, both given one per line.
cpu_lines_minus() {
    awk 'NR == FNR { if (NF) drop[$1] = 1; next } NF && !($1 in drop)' \
        <(printf '%s\n' "$2") <(printf '%s\n' "$1")
}

# True when every CPU of list $1 is in list $2. Returns 2 when either list is
# malformed.
cpu_list_subset() {
    local a b
    a=$(cpu_list_expand "$1") || return 2
    b=$(cpu_list_expand "$2") || return 2
    [ -z "$(cpu_lines_minus "$a" "$b")" ]
}

# True when lists $1 and $2 name the same set.
cpu_list_equal() {
    local a b
    a=$(cpu_list_canonical "$1") || return 2
    b=$(cpu_list_canonical "$2") || return 2
    [ "$a" = "$b" ]
}

# The CPUs online on this host.
cpu_online() {
    cat /sys/devices/system/cpu/online 2>/dev/null || true
}

# The CPUs this process's cgroup allows: the nearest cpuset.cpus.effective at
# or above the `0::` path of /proc/self/cgroup, or the online CPUs when there
# is none (no cpuset controller, cgroup v1).
cgroup_effective_cpus() {
    local path dir
    path=$(sed -n 's/^0:://p' /proc/self/cgroup 2>/dev/null || true)
    if [ -n "$path" ]; then
        dir="/sys/fs/cgroup${path%/}"
        while :; do
            if [ -r "$dir/cpuset.cpus.effective" ]; then
                cat "$dir/cpuset.cpus.effective"
                return 0
            fi
            [ "$dir" != /sys/fs/cgroup ] && [ "$dir" != /sys/fs ] || break
            dir=${dir%/*}
        done
    fi
    cpu_online
}

# The first online CPU outside list $1 (default: this cgroup's effective
# cpuset), or nothing when every online CPU is in it.
cpu_outside_cpuset() {
    local set online
    set=${1-$(cgroup_effective_cpus)}
    online=$(cpu_list_expand "$(cpu_online)") || return 2
    set=$(cpu_list_expand "$set") || return 2
    cpu_lines_minus "$online" "$set" | head -n 1
}

# This shell's own CPU affinity, as the kernel reports it ($$, not a
# subshell's /proc/self, although both carry the same mask).
cpu_own_affinity() {
    sed -n 's/^Cpus_allowed_list:[[:space:]]*//p' "/proc/$$/status"
}
