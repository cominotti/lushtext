#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Run one command inside a memory-capped cgroup scope, so a model checker that
# explodes is killed alone instead of pushing the desktop into swap.
#
#   scripts/with-memory-cap.sh [--address-space] <cap> <command> [args...]
#   scripts/with-memory-cap.sh --envelope <size> <command> [args...]
#   scripts/with-memory-cap.sh --wait-admissible <cap>
#   scripts/with-memory-cap.sh --preflight <cap>
#   scripts/with-memory-cap.sh --ticket <cap> <owner-pid>
#   scripts/with-memory-cap.sh --admission-verdict <admitted> <available> <budget> <reserve|-> <cap> [<head>]
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
# LUSHTEXT_ADMISSION_NOWAIT=1 it checks once and exits 75 on a misfit; with
# LUSHTEXT_ADMISSION_TICKET it waits in that ticket's place in the order).
# scripts/heavy.sh calls it before taking the shared-machine locks (see
# "Shared-machine agent work" in .agents/rules/verification-runs.md). With LUSHTEXT_ADMISSION_NOWAIT=1 (exported by
# scripts/measure.sh to its command) admission never waits: a cap that does
# not fit at once fails with exit 75, because waiting for memory inside a
# measurement's exclusive locks would stall every agent for as long as the
# memory stays short.
#
# No admission waits for memory under a shared LOCK. scripts/heavy.sh's own
# admission after LOCK checks once (LUSHTEXT_ADMISSION_ONCE=<report>: the
# admission lock is waited for at most 2 s, and a misfit writes one
# `misfit: <message>` line to the report and exits 75 before any scope, so
# heavy.sh can release LOCK and wait). Any other capped call above 1G whose
# process tree provably holds LOCK shared (the agent-locks.sh proof: the
# marker's PID is an ancestor whose fd 7 is LOCK) is refused with exit 2,
# naming `heavy.sh --envelope`; a proven exclusive LOCK (a measurement)
# checks once and exits 75. `--preflight <cap>` applies that check and admits
# nothing, so a Makefile recipe that admits its own scopes refuses
# `heavy.sh --mem none` before its first phase.
#
# `--envelope <size> <command>` is for a command that admits its own scopes
# (scripts/heavy.sh --envelope). It is admitted like a scope of <size> and
# reserved by a placeholder `lushtext-cap-<P>-<ns>.scope` in the heavy slice
# (counted by every wrapper copy since D10, for as long as P runs); the
# command runs in the sub-slice `<heavy stem>-env<P>t<ns>.slice`
# (MemoryMax=<size>, no swap) in a root scope with no limit of its own. A
# capped call inside it gets its own scope in the sub-slice, admitted against
# the sub-slice's memory.max with no reserve check, no legacy slice, no
# NOWAIT and the envelope's own queue: it waits only for the same command's
# earlier scopes. When the command exits, the placeholder reads the
# sub-slice's memory.peak and oom_kill, reverts its runtime limits and exits,
# ending the reservation even if a daemon stays behind; with a report file
# (LUSHTEXT_ENVELOPE_REPORT) it writes `envelope: peak=<MiB> oom_kill=<n>
# slice=<sub> placeholder=<P>` there. A sentinel scope keeps the sub-slice
# alive until then; then the placeholder stops the sub-slice if nothing is
# left in it. The next envelope admission sweeps the drop-in and queue of an
# envelope whose placeholder was SIGKILLed, and stops every envelope
# sub-slice whose placeholder is gone and that no process is left in. Inside a capped scope or an
# envelope, `--envelope` runs its command in place.
#
# Admission order. A waiting admission holds a ticket in its slice's queue
# (${XDG_RUNTIME_DIR}/lushtext-heavy-admission.queue/<slice>/, one file
# `<ns>-<owner pid>` holding `<cap KiB> <owner start time>`; a dead owner's
# ticket is swept). scripts/heavy.sh takes one per invocation (`--ticket`) and
# keeps it across its retries. While the oldest live waiter has the host
# memory for its cap plus the reserve, its cap is reserved on the budget: a
# later request is admitted only if it also fits beside it. A waiter short of
# host memory reserves nothing. NOWAIT and measurement admissions take no
# ticket and ignore the order. One pure function, admission_verdict
# (`--admission-verdict`, for fixtures), decides for the probe and every
# admission. LUSHTEXT_ADMISSION_POLL_SECONDS (default 15) and
# LUSHTEXT_ADMISSION_LOCK exist for the self-tests only.
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
# Measured 2026-11-17 (kernel 7.2.15, systemd 262.1), for envelopes:
#   H4. A `<stem>-env<P>t<ns>.slice` nests directly under `<stem>.slice` (H1),
#       and its scopes are not `*.scope` children of the parent slice.
#   H5. `systemctl --user revert` of a sub-slice holding a live scope removes
#       its runtime drop-ins; memory.max then reads `max`, and the scope keeps
#       running.
#   H6. The sub-slice's memory.peak and the oom_kill line of memory.events
#       can be read while a scope is in it.
# The self-test re-probes H1 and H3 (a caller on every CPU and one pinned
# outside the cpuset); H2 is covered by its placement case, H4 and H6 by the
# envelope cases, and H5 directly.
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
# The LOCK proof (agent_lock_inherited) of the shared-machine protocol.
# shellcheck source=scripts/agent-locks.sh
source "$(dirname "${BASH_SOURCE[0]}")/agent-locks.sh"

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
# The admission lock serializes every admission on the host (old and new
# copies of this wrapper share its file name). LUSHTEXT_ADMISSION_LOCK and
# LUSHTEXT_ADMISSION_POLL_SECONDS exist for the self-tests only.
ADMISSION_LOCK=${LUSHTEXT_ADMISSION_LOCK:-${XDG_RUNTIME_DIR:-/tmp}/lushtext-heavy-admission.lock}
POLL_SECONDS=${LUSHTEXT_ADMISSION_POLL_SECONDS:-15}
[[ "$POLL_SECONDS" =~ ^[0-9]+(\.[0-9]+)?$ ]] || POLL_SECONDS=15
# One queue of waiting admissions per slice (tickets, see admit).
QUEUE_ROOT=${XDG_RUNTIME_DIR:-/tmp}/lushtext-heavy-admission.queue
# Where `systemctl --user set-property --runtime` keeps its drop-ins.
DROPIN_ROOT=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/systemd/user.control

# The hand-off variables scripts/heavy.sh sets for this one call, read here
# and removed from the environment before any path can exec the command
# (`none`, nested in place, a scope, CI uncapped): no command, and no capped
# call inside it, may inherit them.
admission_once=${LUSHTEXT_ADMISSION_ONCE:-}
admission_ticket=${LUSHTEXT_ADMISSION_TICKET:-}
envelope_report=${LUSHTEXT_ENVELOPE_REPORT:-}
unset LUSHTEXT_ADMISSION_ONCE LUSHTEXT_ADMISSION_TICKET LUSHTEXT_ENVELOPE_REPORT

# Admission context: the host-level heavy slice by default, or the envelope
# sub-slice this process runs in (envelope_context).
adm_slice=$HEAVY_SLICE
inside_envelope=0
env_cgroup=
env_limit_kib=0

mem_available_kib() {
    awk '/^MemAvailable:/ { print $2 }' /proc/meminfo
}

# Sum, in KiB, of the caps of the scopes directly in the cgroup directories
# given. A scope counts only while its command runs. A scope this script
# starts is named lushtext-cap-<pid>-<ns>.scope after the command's PID
# (systemd-run --scope execs the command in place); once that PID has left
# the scope, the processes still in it are daemons the command left behind
# (an sccache server started by its first compile) and its cap is no longer
# reserved. Their memory still counts against the slice's MemoryMax and
# MemAvailable. Scopes started by an older copy of this script
# (run-p<pid>-*.scope, or any other name) count while they exist; scopes with
# no limit (an envelope's root and sentinel) never count.
admitted_in() {
    local dir total=0 limit scope pid
    for dir in "$@"; do
        [ -d "$dir" ] || continue
        for limit in "$dir"/*.scope/memory.max; do
            [ -r "$limit" ] || continue
            scope=${limit%/memory.max}
            if [[ "${scope##*/}" =~ ^lushtext-cap-([0-9]+)-[0-9]+\.scope$ ]]; then
                pid=${BASH_REMATCH[1]}
                grep -qx "$pid" "$scope/cgroup.procs" 2>/dev/null || continue
            fi
            read -r limit <"$limit" || continue
            [ "$limit" = max ] && continue
            total=$((total + limit / 1024))
        done
    done
    echo "$total"
}

# The caps admitted in the current admission context: inside an envelope its
# own sub-slice only; at the host level the heavy slice and its legacy slice
# (legacy_slice_of), each by the same per-scope rule, so a scope an old
# wrapper copy started is never ignored during the rename.
admitted_kib() {
    if [ "$inside_envelope" = 1 ]; then
        admitted_in "/sys/fs/cgroup$env_cgroup"
        return
    fi
    local slice cgroup dirs=()
    for slice in "$HEAVY_SLICE" "$(legacy_slice_of "$HEAVY_SLICE")"; do
        cgroup=$(systemctl --user show -p ControlGroup --value "$slice" 2>/dev/null || true)
        [ -z "$cgroup" ] || dirs+=("/sys/fs/cgroup$cgroup")
    done
    admitted_in "${dirs[@]}"
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

# Pure: may a cap of $5 KiB be admitted now? $1 the caps admitted (KiB), $2
# MemAvailable (KiB), $3 the budget (KiB), $4 the desktop reserve (KiB), or
# `-` inside an envelope (no MemAvailable check, and the head always
# reserves), $6 the cap of the head, the oldest live waiter ahead of this one
# (empty: none). Admit iff the cap fits the budget and the reserve, and either
# there is no head, the head is short of host memory for its own cap plus the
# reserve (it then reserves nothing), or the cap also fits beside the head's.
# Returns 0 to admit, 1 to wait. The probe (--wait-admissible) and every
# admission decide through this one function.
admission_verdict() {
    local admitted=$1 available=$2 budget=$3 reserve=$4 cap=$5 head=${6:-}
    [ $((admitted + cap)) -le "$budget" ] || return 1
    if [ "$reserve" != - ]; then
        [ "$available" -ge $((cap + reserve)) ] || return 1
    fi
    [ -n "$head" ] || return 0
    if [ "$reserve" != - ] && [ "$available" -lt $((head + reserve)) ]; then
        return 0
    fi
    [ $((admitted + head + cap)) -le "$budget" ]
}

# Field 22 of /proc/<pid>/stat (the start time), which tells a live owner
# from a later process that reused its PID.
proc_start_time() {
    local stat rest fields
    stat=$(cat "/proc/$1/stat" 2>/dev/null) || return 1
    rest=${stat##*) }
    read -ra fields <<<"$rest"
    [ -n "${fields[19]:-}" ] || return 1
    echo "${fields[19]}"
}

# The admission queue of slice $1: one ticket file `<ns>-<owner pid>` per
# waiting admission, holding `<cap KiB> <owner start time>`. Names sort by
# arrival (the nanosecond stamp has a fixed width).
queue_dir_of() {
    echo "$QUEUE_ROOT/$1"
}

# True when ticket $1's owner still runs.
ticket_live() {
    local name=${1##*/} cap start
    read -r cap start <"$1" 2>/dev/null || return 1
    [ "$(proc_start_time "${name#*-}" 2>/dev/null)" = "$start" ]
}

# Create a ticket for owner $2 with a cap of $1 KiB in queue directory $3, and
# print its path. Called under the admission lock.
ticket_create() {
    local start path
    start=$(proc_start_time "$2") || return 1
    mkdir -p "$3"
    path=$3/$(date +%s%N)-$2
    printf '%s %s\n' "$1" "$start" >"$path"
    echo "$path"
}

# The cap of the head of queue $1 for a caller holding ticket $2 (empty: none,
# the caller is the newest): the oldest live ticket older than the caller's.
# With $3 = sweep (under the admission lock) dead tickets ahead are removed.
queue_head_cap() {
    local dir=$1 own=${2:-} sweep=${3:-} ticket cap start
    [ -d "$dir" ] || return 0
    for ticket in "$dir"/*-*; do
        [ -e "$ticket" ] || continue
        if [ -n "$own" ] && [[ ! "${ticket##*/}" < "${own##*/}" ]]; then
            return 0
        fi
        if ticket_live "$ticket"; then
            read -r cap start <"$ticket" || continue
            echo "$cap"
            return 0
        fi
        [ "$sweep" != sweep ] || rm -f "$ticket"
    done
}

# Sets inside_envelope, adm_slice and env_cgroup when this process runs inside
# a live envelope of the heavy slice: its own cgroup path has, directly below
# the heavy slice's component, an `<heavy stem>-env<P>t<ns>.slice` whose
# memory.max is a number. The proof is the cgroup path, never an environment
# variable (a process cannot leave or forge it).
envelope_context() {
    local own rest component stem=${HEAVY_SLICE%.slice} limit
    own=$(sed -n 's/^0:://p' /proc/self/cgroup 2>/dev/null || true)
    case $own in
        */"$HEAVY_SLICE"/*) ;;
        *) return 1 ;;
    esac
    rest=${own#*/"$HEAVY_SLICE"/}
    component=${rest%%/*}
    [[ "$component" =~ ^${stem}-env[0-9]+t[0-9]+\.slice$ ]] || return 1
    env_cgroup=${own%%/"$HEAVY_SLICE"/*}/$HEAVY_SLICE/$component
    read -r limit <"/sys/fs/cgroup$env_cgroup/memory.max" 2>/dev/null || return 1
    [[ "$limit" =~ ^[0-9]+$ ]] || return 1
    inside_envelope=1
    adm_slice=$component
    env_limit_kib=$((limit / 1024))
}

# True when the placeholder P ($2) of an envelope still holds its
# lushtext-cap-P-*.scope in the heavy slice's cgroup $1. An unknown cgroup
# counts as alive: never sweep blind.
envelope_placeholder_alive() {
    local cgroup=$1 pid=$2 procs
    [ -n "$cgroup" ] || return 0
    for procs in "/sys/fs/cgroup$cgroup/lushtext-cap-$pid-"*.scope/cgroup.procs; do
        grep -qx "$pid" "$procs" 2>/dev/null && return 0
    done
    return 1
}

# True when the cgroup directory $1 holds no process in its whole sub-tree.
cgroup_unpopulated() {
    grep -qx 'populated 0' "$1/cgroup.events" 2>/dev/null
}

# Under the admission lock, before an envelope is admitted: revert the
# runtime drop-in and remove the queue of every envelope sub-slice of this
# heavy slice whose placeholder is gone (SIGKILLed; a placeholder that exits
# normally reverts its own), and stop every such sub-slice that no process is
# left in (a daemon its command left behind, such as the sccache server,
# keeps it until it exits; stopping it then would kill that daemon). Each
# sub-slice name is unique, so nothing else would revert or stop it before
# logout.
envelope_sweep() {
    local stem=${HEAVY_SLICE%.slice} cgroup entry unit
    cgroup=$(systemctl --user show -p ControlGroup --value "$HEAVY_SLICE" 2>/dev/null || true)
    for entry in "$DROPIN_ROOT/$stem"-env*t*.slice.d "$QUEUE_ROOT/$stem"-env*t*.slice; do
        [ -e "$entry" ] || continue
        unit=${entry##*/}
        unit=${unit%.d}
        [[ "$unit" =~ ^${stem}-env([0-9]+)t[0-9]+\.slice$ ]] || continue
        envelope_placeholder_alive "$cgroup" "${BASH_REMATCH[1]}" && continue
        systemctl --user revert "$unit" >/dev/null 2>&1 || true
        rm -rf "${QUEUE_ROOT:?}/$unit"
        echo "with-memory-cap: swept envelope $unit: its placeholder (pid ${BASH_REMATCH[1]}) is gone" >&2
    done
    [ -n "$cgroup" ] || return 0
    for entry in "/sys/fs/cgroup$cgroup/$stem"-env*t*.slice; do
        [ -d "$entry" ] || continue
        unit=${entry##*/}
        [[ "$unit" =~ ^${stem}-env([0-9]+)t[0-9]+\.slice$ ]] || continue
        envelope_placeholder_alive "$cgroup" "${BASH_REMATCH[1]}" && continue
        cgroup_unpopulated "$entry" || continue
        systemctl --user stop "$unit" >/dev/null 2>&1 || true
    done
}

# D4: how this caller may admit, from the LOCK proof of scripts/agent-locks.sh
# (the marker's PID is an ancestor whose fd 7 is LOCK). Sets lock_admission:
#   wait    may wait (no lock proven, or inside an envelope, where it waits
#           only for its own command's scopes);
#   once    check once, report a misfit to the heavy.sh report file (exit 75);
#   nowait  check once, exit 75 on a misfit (LUSHTEXT_ADMISSION_NOWAIT=1, or a
#           proven exclusive LOCK: inside a measurement).
# A proven shared LOCK without the check-once hand-off of heavy.sh is refused
# with exit 2: waiting there would stall every pending measurement.
lock_policy() {
    local mode holder
    if [ "$inside_envelope" = 1 ]; then
        lock_admission="wait"
        [ -z "$admission_once" ] || lock_admission=once
        return 0
    fi
    lock_admission="wait"
    [ "${LUSHTEXT_ADMISSION_NOWAIT:-}" != 1 ] || lock_admission=nowait
    [ -z "$admission_once" ] || lock_admission=once
    [ -n "${LUSHTEXT_AGENT_LOCK_HELD:-}" ] || return 0
    agent_lock_paths
    mode=$(agent_lock_inherited) || return 0
    holder=${LUSHTEXT_AGENT_LOCK_HELD#*:}
    case $mode in
        exclusive)
            [ "$lock_admission" = once ] || lock_admission=nowait
            ;;
        shared)
            if [ -z "$admission_once" ]; then
                echo "with-memory-cap: refusing to admit $cap under LOCK held shared by heavy.sh (pid $holder): an admission that waits for memory there stalls every pending measurement. Run the command as scripts/heavy.sh --envelope <size> -- ... (.agents/rules/verification-runs.md)" >&2
                exit 2
            fi
            ;;
    esac
}

# The current readings for the cap ($cap_kib) and this caller's ticket ($1,
# may be empty); with $2 = 1 the head is ignored (NOWAIT and exclusive mode);
# $3 is passed to queue_head_cap. Sets admitted, available and head.
admission_check() {
    admitted=$(admitted_kib)
    available=$(mem_available_kib)
    head=
    [ "${2:-0}" = 1 ] || head=$(queue_head_cap "$(queue_dir_of "$adm_slice")" "${1:-}" "${3:-}")
    admission_verdict "$admitted" "$available" "$heavy_kib" "$reserve_arg" "$cap_kib" "$head"
}

waiting_text() {
    local text
    text="with-memory-cap: waiting for memory: $((admitted / 1048576)) GiB of $((heavy_kib / 1048576)) GiB admitted to $adm_slice"
    if [ "$reserve_arg" != - ]; then
        text+=", $((available / 1048576)) GiB available, need $((cap_kib / 1048576)) GiB plus a $((reserve_kib / 1048576)) GiB reserve"
    else
        text+=" (an envelope: only its own command's scopes), need $((cap_kib / 1048576)) GiB"
    fi
    [ -z "$head" ] || text+=", behind an older $((head / 1048576)) GiB request"
    echo "$text"
}

waiting_message() {
    waiting_text >&2
}

# Admit the cap ($cap_kib) in mode $1 (lock_policy). Takes the admission lock
# (fd 9), which stays locked until the caller's scope exists
# (release_admission_once_scoped, or the envelope's placeholder): it is held
# only while checking and admitting, never while sleeping for memory, because
# a waiter holding it would block every other admission, NOWAIT ones
# included.
#   wait    a blocking admission. It takes a ticket in its slice's queue at
#           its first check unless scripts/heavy.sh handed one over, and polls
#           every LUSHTEXT_ADMISSION_POLL_SECONDS (default 15 s).
#   once    scripts/heavy.sh's admission after LOCK: the lock is waited for at
#           most 2 s (busy counts as a misfit), and a misfit writes one
#           `misfit: <message>` line to the report file and exits 75 before
#           any scope starts.
#   nowait  inside a measurement: the same 2 s and one check, exit 75 with a
#           message; no ticket, and the queue's head is ignored.
# The ticket is removed under the lock at the moment of admission, so the
# admitted command never counts as its own head.
admit() {
    local mode=$1 announced=0 own=$admission_ticket bypass=0
    # NOWAIT ignores the order in every mode, as its probe did: a check-once
    # admission that honoured a head its probe ignored would misfit forever.
    # Inside an envelope NOWAIT does not apply (lock_policy).
    if [ "$mode" = nowait ] || { [ "${LUSHTEXT_ADMISSION_NOWAIT:-}" = 1 ] && [ "$inside_envelope" = 0 ]; }; then
        own=
        bypass=1
    fi
    exec 9>"$ADMISSION_LOCK"
    while :; do
        if [ "$mode" = wait ]; then
            flock 9
        elif ! flock -w 2 9; then
            if [ "$mode" = once ]; then
                echo "misfit: the admission lock stayed busy for 2 s" >>"$admission_once"
            else
                echo "with-memory-cap: not waiting (LUSHTEXT_ADMISSION_NOWAIT=1): the admission lock stayed busy" >&2
            fi
            exit 75
        fi
        [ "${envelope_mode:-0}" != 1 ] || envelope_sweep
        if [ "$mode" = wait ] && [ -z "$own" ]; then
            own=$(ticket_create "$cap_kib" "$$" "$(queue_dir_of "$adm_slice")") || own=
        fi
        if admission_check "$own" "$bypass" sweep; then
            [ -z "$own" ] || rm -f "$own"
            return 0
        fi
        case $mode in
            once)
                echo "misfit: $(waiting_text)" >>"$admission_once"
                exit 75
                ;;
            nowait)
                waiting_message
                echo "with-memory-cap: not waiting (LUSHTEXT_ADMISSION_NOWAIT=1, inside a measurement's locks): free memory, or run this capped command before the measurement" >&2
                exit 75
                ;;
        esac
        if [ "$announced" = 0 ]; then
            waiting_message
            announced=1
        fi
        flock -u 9
        sleep "$POLL_SECONDS"
    done
}

# Runs detached, holding the inherited admission lock (fd 9) until process
# $1 is in the scope $2 (a unit name without `.scope`), so the next admission
# counts its cap; gives up after 10 s or when the process is gone. The unit,
# not the slice, is matched: inside an envelope the caller already runs in
# the slice its new scope goes to.
release_admission_once_scoped() {
    local pid=$1 unit=$2
    for _ in $(seq 200); do
        grep -qF "/$unit.scope" "/proc/$pid/cgroup" 2>/dev/null && return 0
        kill -0 "$pid" 2>/dev/null || return 0
        sleep 0.05
    done
}

# Start the placeholder of an envelope (named after nanosecond stamp $1),
# writing its PID to $2; its stderr goes to the caller's only when there is no
# report file to write its line to.
envelope_spawn_placeholder() {
    local report=${envelope_report:--}
    if [ "$report" = - ]; then
        exec 4>&2
    else
        exec 4>/dev/null
    fi
    # shellcheck disable=SC2016 # expanded by the placeholder's own shell
    setsid bash -c 'echo "$$" >"$0"; exec systemd-run --user --scope --quiet --expand-environment=no --slice="$1" --unit="lushtext-cap-$$-$2" -p MemoryMax="$3" -p MemorySwapMax=0 -p OOMPolicy=continue -- bash "$4" --envelope-placeholder "$5" "$2" "$6"' \
        "$2" "$HEAVY_SLICE" "$1" "$cap" "${BASH_SOURCE[0]}" "$$" "$report" </dev/null >/dev/null 2>&4 4>&- 9>&- &
    exec 4>&-
}

# Run "$@" in an envelope of $cap (after its admission, with the admission
# lock still held on fd 9 when it was admitted), and exec it. Three units:
#   - the placeholder lushtext-cap-<P>-<ns>.scope directly in the heavy slice
#     (MemoryMax=<size>, no swap): the reservation every wrapper copy since
#     D10 counts, for as long as P runs (--envelope-placeholder);
#   - the sub-slice <heavy stem>-env<P>t<ns>.slice (MemoryMax=<size>, no
#     swap), set while the admission lock is still held and only once the
#     placeholder is in its scope, so a concurrent sweep never reverts a live
#     envelope; a sentinel scope in it (no limit, OOMPolicy=continue) keeps it
#     alive until the placeholder has read its peak and OOM kills;
#   - the root scope lushtext-envroot-<pid>-<ns>.scope in the sub-slice (no
#     limit, OOMPolicy=continue, the placement check), which this process
#     becomes. It does not get LUSHTEXT_MEMORY_CAP_KIB: capped calls inside it
#     start their own scopes in the sub-slice, admitted against the envelope.
# With a report file (LUSHTEXT_ENVELOPE_REPORT, from scripts/heavy.sh) an
# `envelope-start:` line is written before the command starts, and the
# placeholder writes the `envelope:` line after it ends.
# The envelope pid file's directory: `lt-envelope-<pid>-XXXXXX` in TMPDIR,
# which with-temp-scope.sh's stale sweep reclaims if a SIGKILL lands before
# envelope_run removes it (it ends in exec, so no trap would).
envelope_pid_dir() {
    mktemp -d "${TMPDIR:-/tmp}/lt-envelope-$$-XXXXXX"
}

envelope_run() {
    local ns pid_dir pidfile placeholder="" sub cgroup limit="" expected
    ns=$(date +%s%N)
    pid_dir=$(envelope_pid_dir)
    pidfile=$pid_dir/pid
    envelope_spawn_placeholder "$ns" "$pidfile"
    for _ in $(seq 200); do
        placeholder=$(cat "$pidfile" 2>/dev/null || true)
        if [ -n "$placeholder" ] && grep -qF "/lushtext-cap-$placeholder-$ns.scope" "/proc/$placeholder/cgroup" 2>/dev/null; then
            break
        fi
        placeholder=
        sleep 0.05
    done
    rm -rf "$pid_dir"
    if [ -z "$placeholder" ]; then
        echo "with-memory-cap: the envelope's placeholder scope did not start in $HEAVY_SLICE within 10 s" >&2
        exit 1
    fi
    sub=${HEAVY_SLICE%.slice}-env${placeholder}t$ns.slice
    if ! systemctl --user set-property --runtime "$sub" MemoryMax="$cap" MemorySwapMax=0 >/dev/null 2>&1; then
        kill "$placeholder" 2>/dev/null || true
        echo "with-memory-cap: cannot set MemoryMax=$cap on the envelope sub-slice $sub" >&2
        exit 1
    fi
    # The reservation and the limit exist: other admissions may go on.
    exec 9>&-
    setsid systemd-run --user --scope --quiet --slice="$sub" --unit="lushtext-envsentinel-$placeholder-$ns" \
        -p OOMPolicy=continue -- tail --pid="$placeholder" -s 0.2 -f /dev/null </dev/null >/dev/null 2>&1 &
    cgroup=$(systemctl --user show -p ControlGroup --value "$HEAVY_SLICE" 2>/dev/null || true)
    expected=$((cap_kib * 1024))
    for _ in $(seq 200); do
        if read -r limit <"/sys/fs/cgroup$cgroup/$sub/memory.max" 2>/dev/null && [ "$limit" = "$expected" ]; then
            break
        fi
        sleep 0.05
    done
    if [ "$limit" != "$expected" ]; then
        kill "$placeholder" 2>/dev/null || true
        echo "with-memory-cap: the envelope sub-slice $sub reads memory.max '${limit:-unreadable}', expected $expected; refusing to run unbounded" >&2
        exit 1
    fi
    [ -z "$envelope_report" ] || echo "envelope-start: slice=$sub placeholder=$placeholder" >>"$envelope_report"
    echo "with-memory-cap: envelope MemoryMax=$cap MemorySwapMax=0 in $sub (reserved in $HEAVY_SLICE, $heavy_max, by lushtext-cap-$placeholder-$ns.scope) for: $shown_command" >&2
    exec systemd-run --user --scope --quiet --expand-environment=no --slice="$sub" \
        --unit="lushtext-envroot-$$-$ns" -p OOMPolicy=continue \
        -- bash "${BASH_SOURCE[0]}" --exec-in-scope "$sub" - "$@" 9>&-
}

# True when systemd can create a user scope here.
scope_available() {
    command -v systemd-run >/dev/null 2>&1 \
        && systemd-run --user --scope --quiet -p MemoryMax=1M true >/dev/null 2>&1
}

# Resolve the budget and desktop reserve of the admission context (heavy_max,
# heavy_kib, reserve_kib, reserve_arg); exits 2 on an invalid size or a cap
# above the budget. Inside an envelope the budget is the sub-slice's own
# memory.max, read from the cgroup (never from the environment or the config
# file), and there is no reserve: the envelope's admission checked its size
# plus the reserve once.
budget_setup() {
    if [ "$inside_envelope" = 1 ]; then
        heavy_kib=$env_limit_kib
        heavy_max="$((heavy_kib / 1048576))G"
        reserve_kib=0
        reserve_arg=-
        if [ "$cap_kib" -gt "$heavy_kib" ]; then
            echo "with-memory-cap: cap $cap exceeds the envelope's budget $heavy_max ($adm_slice): start the command under a larger scripts/heavy.sh --envelope" >&2
            exit 2
        fi
        return 0
    fi
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
    reserve_arg=$reserve_kib
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
    local pid slice units=() entry
    for pid in "${self_test_sleepers[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    # The envelope sub-slices of the private slices, live or only a drop-in.
    for slice in "${self_test_slices[@]}"; do
        while read -r entry _; do
            [ -z "$entry" ] || units+=("$entry")
        done < <(systemctl --user list-units --all --plain --no-legend "${slice%.slice}-env*.slice" 2>/dev/null || true)
        for entry in "$DROPIN_ROOT/${slice%.slice}"-env*.slice.d; do
            [ -d "$entry" ] || continue
            entry=${entry##*/}
            units+=("${entry%.d}")
        done
    done
    systemctl --user stop "${units[@]}" "${self_test_slices[@]}" >/dev/null 2>&1 || true
    systemctl --user revert "${units[@]}" "${self_test_slices[@]}" >/dev/null 2>&1 || true
    for slice in "${self_test_slices[@]}"; do
        rm -rf "${QUEUE_ROOT:?}/$slice"
        for entry in "$QUEUE_ROOT/${slice%.slice}"-env*.slice; do
            [ ! -e "$entry" ] || rm -rf "$entry"
        done
    done
    [ -z "${self_test_admission_lock:-}" ] || rm -f "$self_test_admission_lock"
    [ -z "${self_test_root:-}" ] || rm -rf "$self_test_root"
}

# What the private slices of self-test run $1 (its PID, or a fake name) left
# behind: runtime drop-ins of the slices and of their envelope sub-slices, and
# admission queues. Matched exactly (`-<run>.slice`, `-<run>-env…`), so a
# run whose PID has this one as a prefix is never blamed.
self_test_leftovers() {
    local run=$1
    compgen -G "$DROPIN_ROOT/*selftest-$run.slice.d" || true
    compgen -G "$DROPIN_ROOT/*selftest-$run-env*.slice.d" || true
    compgen -G "$QUEUE_ROOT/*selftest-$run.slice" || true
    compgen -G "$QUEUE_ROOT/*selftest-$run-env*.slice" || true
}

# Start `sleep 10` in a scope of slice $1 capped at $2 (default 2G) and wait
# until the scope's cgroup exists, so an admission after this call counts it.
self_test_sleeper() {
    local slice=$1 cap=${2:-2G} cgroup
    systemd-run --user --scope --quiet --slice="$slice" -p MemoryMax="$cap" -p MemorySwapMax=0 sleep 10 &
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

# --admission-verdict on fixtures (KiB): no head, a head that reserves its
# cap, a head that fits beside, a head short of host memory, and an
# envelope (reserve `-`: no MemAvailable check, and the head always reserves).
self_test_verdict_fixtures() {
    local me=$0 g=1048576 fixture expected status
    for fixture in \
        "0|$((3 * g))|$((100 * g))|$((6 * g))|$g|$((2 * g))|" \
        "1|$((3 * g))|$((100 * g))|$((6 * g))|$g|$((4 * g))|" \
        "1|$((3 * g))|$((2 * g))|$((6 * g))|$g|$((2 * g))|" \
        "1|$((3 * g))|$((100 * g))|$((6 * g))|$g|$((2 * g))|$((4 * g))" \
        "0|$((3 * g))|$((100 * g))|$((10 * g))|$g|$((2 * g))|$((4 * g))" \
        "0|$((3 * g))|$((40 * g))|$((48 * g))|$((16 * g))|$((2 * g))|$((36 * g))" \
        "0|0|0|$((2 * g))|-|$((2 * g))|" \
        "1|0|0|$((2 * g))|-|$((2 * g))|$((2 * g))" \
        "1|$g|0|$((2 * g))|-|$((2 * g))|"; do
        IFS='|' read -r expected a b c d e f <<<"$fixture"
        status=0
        # shellcheck disable=SC2086 # an empty head is no argument
        "$me" --admission-verdict "$a" "$b" "$c" "$d" "$e" $f 2>/dev/null || status=$?
        [ "$status" -eq "$expected" ] || self_test_fail "--admission-verdict $a $b $c $d $e $f gave status $status, expected $expected"
    done
}

# A capped admission's wait for the admission lock and its order. Every case
# uses this run's private slice and admission lock.
self_test_admission_cases() {
    local me=$0 status report marker holder start elapsed pid
    # Start from empty slices: the earlier cases' sleepers (the legacy one is
    # counted too) would otherwise still hold their caps.
    for pid in "${self_test_sleepers[@]}"; do
        kill "$pid" 2>/dev/null || true
        timeout 5 tail --pid="$pid" -f /dev/null 2>/dev/null || true
    done
    # The check-once admission scripts/heavy.sh makes after LOCK waits at most
    # 2 s for the admission lock: a lock still busy is a misfit, reported in
    # the file and with exit 75, and nothing starts.
    report=$(mktemp)
    marker=$(mktemp -u "${TMPDIR:-/tmp}/with-memory-cap-marker.XXXXXX")
    # shellcheck disable=SC2016 # expanded by the inner shell
    sh -c 'exec 9>>"$0"; flock -x 9; exec sleep 6' "$LUSHTEXT_ADMISSION_LOCK" &
    holder=$!
    for _ in $(seq 100); do
        flock -n "$LUSHTEXT_ADMISSION_LOCK" true || break
        sleep 0.05
    done
    start=$(date +%s%N)
    status=0
    LUSHTEXT_ADMISSION_ONCE=$report LUSHTEXT_HEAVY_MEMORY_MAX=3G LUSHTEXT_MEMORY_RESERVE=1G timeout 5 "$me" 2G touch "$marker" 2>/dev/null || status=$?
    elapsed=$((($(date +%s%N) - start) / 1000000))
    kill "$holder" 2>/dev/null || true
    wait "$holder" 2>/dev/null || true
    [ "$status" -eq 75 ] && [ "$elapsed" -lt 4000 ] && grep -q '^misfit: ' "$report" && [ ! -e "$marker" ] \
        || self_test_fail "a check-once admission behind a busy admission lock gave status $status after ${elapsed} ms (report '$(tr '\n' ' ' <"$report")', marker $([ -e "$marker" ] && echo created || echo absent)), expected 75 within 4 s, a 'misfit: ' line and no marker"
    rm -f "$report" "$marker"

    # Fair order: a 2G request does not overtake an older 4G waiter that has
    # the host memory, while a NOWAIT one still does; the 4G waiter is
    # admitted once the scope ahead of it ends.
    local available need
    available=$(mem_available_kib)
    need=$(((4 + 1 + 4) * 1048576))
    if [ "$available" -lt "$need" ]; then
        echo "with-memory-cap self-test: fair-order case skipped (MemAvailable $((available / 1048576)) GiB < 9 GiB)" >&2
        return 0
    fi
    if ! self_test_sleeper "$HEAVY_SLICE" 3G; then
        self_test_fail "the fair-order sleeper never started in $HEAVY_SLICE"
        return 0
    fi
    local sleeper=${self_test_sleepers[-1]} waiter
    marker=$(mktemp -u "${TMPDIR:-/tmp}/with-memory-cap-marker.XXXXXX")
    LUSHTEXT_HEAVY_MEMORY_MAX=6G LUSHTEXT_MEMORY_RESERVE=1G timeout 20 "$me" 4G touch "$marker" 2>/dev/null &
    waiter=$!
    sleep 1
    status=0
    LUSHTEXT_HEAVY_MEMORY_MAX=6G LUSHTEXT_MEMORY_RESERVE=1G timeout 3 "$me" 2G true 2>/dev/null || status=$?
    [ "$status" -eq 124 ] || self_test_fail "a 2G request was not held behind an older 4G waiter in a 6G slice with 3G admitted (status $status, expected 124)"
    status=0
    LUSHTEXT_ADMISSION_NOWAIT=1 LUSHTEXT_HEAVY_MEMORY_MAX=6G LUSHTEXT_MEMORY_RESERVE=1G timeout 5 "$me" 2G true 2>/dev/null || status=$?
    [ "$status" -eq 0 ] || self_test_fail "a NOWAIT 2G request was held behind an older waiter (status $status, expected 0)"
    # So is scripts/heavy.sh's check-once admission under NOWAIT: its probe
    # ignored the order, so the admission must too, or it misfits forever.
    local once_report
    once_report=$(mktemp)
    status=0
    LUSHTEXT_ADMISSION_ONCE=$once_report LUSHTEXT_ADMISSION_NOWAIT=1 LUSHTEXT_HEAVY_MEMORY_MAX=6G LUSHTEXT_MEMORY_RESERVE=1G timeout 5 "$me" 2G true 2>/dev/null || status=$?
    [ "$status" -eq 0 ] || self_test_fail "a check-once NOWAIT 2G admission was held behind an older waiter (status $status, report '$(tr '\n' ' ' <"$once_report")', expected 0)"
    rm -f "$once_report"
    kill "$sleeper" 2>/dev/null || true
    for _ in $(seq 100); do
        [ -e "$marker" ] && break
        sleep 0.1
    done
    [ -e "$marker" ] || self_test_fail "the 4G waiter was not admitted within 10 s of the scope ahead of it ending"
    kill "$waiter" 2>/dev/null || true
    wait "$waiter" 2>/dev/null || true
    rm -f "$marker"
}

# Wait up to $3 seconds for a line matching ERE $2 in file $1; print it.
self_test_wait_line() {
    local line
    for _ in $(seq $(($3 * 10))); do
        line=$(grep -E -m1 "$2" "$1" 2>/dev/null || true)
        [ -z "$line" ] || { echo "$line"; return 0; }
        sleep 0.1
    done
    return 1
}

# The `name=value` field $2 of line $1.
self_test_field() {
    local word
    for word in $1; do
        [ "${word%%=*}" != "$2" ] || { echo "${word#*=}"; return 0; }
    done
    return 1
}

# Wait for an envelope's `envelope:` line in report $1 and for its
# placeholder to be gone (the line is written just before it exits); print
# the line.
self_test_envelope_done() {
    local line placeholder
    line=$(self_test_wait_line "$1" '^envelope: ' 15) || return 1
    placeholder=$(self_test_field "$line" placeholder) || return 1
    timeout 5 tail --pid="$placeholder" -f /dev/null 2>/dev/null || return 1
    echo "$line"
}

# Envelopes (D2, D3) and the LOCK refusal (D4), in this run's private slice.
self_test_envelope_cases() {
    local me=$0 status report out line sub placeholder heavy_cg bg
    local rundir
    rundir=$(mktemp -d "${TMPDIR:-/tmp}/with-memory-cap-envelope-test.XXXXXX")
    export LUSHTEXT_HEAVY_MEMORY_MAX=3G LUSHTEXT_MEMORY_RESERVE=1G

    # The envelope is counted on the host: its placeholder
    # lushtext-cap-<P>-<ns>.scope sits in the heavy slice with the envelope's
    # memory.max (what admitted_in reads), a 2G NOWAIT admission beside it in
    # a 3G slice is refused, and the sub-slice nests under the heavy slice
    # without being one of its scopes (H4).
    report=$rundir/counted
    : >"$report"
    LUSHTEXT_ENVELOPE_REPORT=$report timeout 20 "$me" --envelope 2G sleep 3 2>/dev/null &
    bg=$!
    if line=$(self_test_wait_line "$report" '^envelope-start: ' 10); then
        sub=$(self_test_field "$line" slice)
        placeholder=$(self_test_field "$line" placeholder)
        heavy_cg=$(systemctl --user show -p ControlGroup --value "$HEAVY_SLICE" 2>/dev/null || true)
        status=0
        LUSHTEXT_ADMISSION_NOWAIT=1 timeout 5 "$me" 2G true 2>/dev/null || status=$?
        [ "$status" -eq 75 ] || self_test_fail "a 2G NOWAIT admission beside a running 2G envelope in a 3G slice gave status $status, expected 75"
        out=$(cat "/sys/fs/cgroup$heavy_cg/lushtext-cap-$placeholder-"*.scope/memory.max 2>/dev/null || true)
        grep -qx "$placeholder" "/sys/fs/cgroup$heavy_cg/lushtext-cap-$placeholder-"*.scope/cgroup.procs 2>/dev/null && [ "$out" = 2147483648 ] \
            || self_test_fail "the envelope's placeholder scope lushtext-cap-$placeholder-*.scope holds memory.max '$out' or not its own PID, expected 2147483648"
        [ -d "/sys/fs/cgroup$heavy_cg/$sub" ] \
            || self_test_fail "H4: the envelope sub-slice $sub is not directly under $HEAVY_SLICE"
        out=$(admitted_in "/sys/fs/cgroup$heavy_cg")
        [ "$out" = 2097152 ] \
            || self_test_fail "the heavy slice counts $out KiB with one 2G envelope running, expected 2097152 (the placeholder alone, never the sub-slice's scopes)"
    else
        self_test_fail "a 2G envelope wrote no envelope-start line ($(tr '\n' ' ' <"$report"))"
    fi
    wait "$bg" 2>/dev/null || true
    line=$(self_test_envelope_done "$report") || line=
    [[ "$line" =~ ^envelope:\ peak=[0-9]+\ oom_kill=0\  ]] \
        || self_test_fail "an envelope's normal run reported '$line', expected 'envelope: peak=<MiB> oom_kill=0 ...' (H6)"
    # An envelope left empty is stopped by its placeholder, not left loaded
    # until logout.
    heavy_cg=$(systemctl --user show -p ControlGroup --value "$HEAVY_SLICE" 2>/dev/null || true)
    if [ -n "${sub:-}" ] && [ -n "$heavy_cg" ] && [ -d "/sys/fs/cgroup$heavy_cg/$sub" ]; then
        self_test_fail "the empty sub-slice $sub of a finished envelope is still loaded"
    fi

    # A leftover daemon keeps no admission, and the sub-slice loses its limit.
    report=$rundir/daemon
    : >"$report"
    # shellcheck disable=SC2016 # expanded by the inner shell
    LUSHTEXT_ENVELOPE_REPORT=$report timeout 20 "$me" --envelope 2G \
        sh -c 'setsid sleep 30 </dev/null >/dev/null 2>&1 & echo $! >"$0"' "$rundir/daemon.pid" 2>/dev/null || true
    if line=$(self_test_envelope_done "$report"); then
        sub=$(self_test_field "$line" slice)
        status=0
        LUSHTEXT_ADMISSION_NOWAIT=1 timeout 5 "$me" 2G true 2>/dev/null || status=$?
        [ "$status" -eq 0 ] || self_test_fail "a daemon left in a finished envelope kept its 2G admitted (status $status)"
        [ ! -e "$DROPIN_ROOT/$sub.d" ] || [ -z "$(ls -A "$DROPIN_ROOT/$sub.d")" ] \
            || self_test_fail "the envelope sub-slice $sub kept its runtime drop-in after its command ended: $(ls "$DROPIN_ROOT/$sub.d")"
        # The daemon keeps its sub-slice (stopping it would kill the daemon);
        # once the daemon has exited, the next envelope admission stops it.
        local daemon
        daemon=$(cat "$rundir/daemon.pid" 2>/dev/null || true)
        { [ -n "$daemon" ] && kill -0 "$daemon" 2>/dev/null && [ -d "/sys/fs/cgroup$heavy_cg/$sub" ]; } \
            || self_test_fail "the envelope sub-slice $sub or the daemon left in it did not survive the envelope's end"
        [ -z "$daemon" ] || kill "$daemon" 2>/dev/null || true
        [ -z "$daemon" ] || timeout 5 tail --pid="$daemon" -f /dev/null 2>/dev/null || true
        timeout 10 "$me" --envelope 2G true 2>/dev/null || self_test_fail "an envelope after a daemon's exit failed"
        [ ! -d "/sys/fs/cgroup$heavy_cg/$sub" ] \
            || self_test_fail "the sub-slice $sub stayed loaded after its leftover daemon exited and another envelope was admitted"
    else
        self_test_fail "an envelope whose command left a daemon wrote no envelope line"
    fi
    kill "$(cat "$rundir/daemon.pid" 2>/dev/null)" 2>/dev/null || true

    # Inside an envelope: two 2G calls in a 2G envelope run one after the
    # other, each in its own scope; the sub-slice keeps its 2G limit after an
    # inner admission; an inner 4G call is refused with 2; --address-space
    # still sets RLIMIT_AS to 7/8 of the inner cap.
    local inner expected inherited
    # shellcheck disable=SC2016 # expanded by the inner shells
    inner='date +%s%N >"$1.start"; cat "$(dirname "/sys/fs/cgroup$(sed -n "s/^0:://p" /proc/self/cgroup)")/memory.max" >"$1.limit"; sleep 0.5; date +%s%N >"$1.end"'
    # shellcheck disable=SC2016 # expanded by the inner shell
    timeout 30 "$me" --envelope 2G sh -c '"$0" 2G sh -c "$1" - "$2/a" & "$0" 2G sh -c "$1" - "$2/b"; s=$?; wait $! || s=1; exit $s' "$me" "$inner" "$rundir" 2>/dev/null \
        || self_test_fail "two 2G calls in a 2G envelope did not both succeed"
    if [ -s "$rundir/a.end" ] && [ -s "$rundir/b.end" ]; then
        if ! { [ "$(cat "$rundir/a.end")" -le "$(cat "$rundir/b.start")" ] || [ "$(cat "$rundir/b.end")" -le "$(cat "$rundir/a.start")" ]; }; then
            self_test_fail "two 2G calls ran side by side in a 2G envelope"
        fi
        [ "$(cat "$rundir/a.limit")" = 2147483648 ] && [ "$(cat "$rundir/b.limit")" = 2147483648 ] \
            || self_test_fail "the envelope sub-slice's memory.max read '$(cat "$rundir/a.limit")' and '$(cat "$rundir/b.limit")' after inner admissions, expected 2147483648"
    else
        self_test_fail "the inner calls of a 2G envelope did not run"
    fi
    status=0
    timeout 10 "$me" --envelope 2G "$me" 4G true 2>/dev/null || status=$?
    [ "$status" -eq 2 ] || self_test_fail "an inner 4G call in a 2G envelope gave status $status, expected 2"
    inherited=$(ulimit -v)
    expected=$((2097152 * 7 / 8))
    [ "$inherited" = unlimited ] || [ "$inherited" -ge "$expected" ] || expected=$inherited
    out=$(timeout 10 "$me" --envelope 2G "$me" --address-space 2G sh -c 'ulimit -v' 2>/dev/null || true)
    [ "$out" = "$expected" ] || self_test_fail "an inner --address-space 2G call in an envelope set RLIMIT_AS '$out', expected '$expected'"

    # Nested envelopes run in place: inside a capped scope and inside an
    # envelope, with no placeholder or sub-slice of their own.
    out=$(LUSHTEXT_MEMORY_CAP_KIB=1 timeout 10 "$me" --envelope 2G sed -n 's/^0:://p' /proc/self/cgroup 2>/dev/null || true)
    [ "$out" = "$(sed -n 's/^0:://p' /proc/self/cgroup)" ] || self_test_fail "--envelope inside a capped scope did not run in place (cgroup '$out')"
    # shellcheck disable=SC2016 # expanded by the inner shell
    out=$(timeout 10 "$me" --envelope 2G sh -c 'sed -n "s/^0:://p" /proc/self/cgroup; "$0" --envelope 2G sed -n "s/^0:://p" /proc/self/cgroup' "$me" 2>/dev/null | sort -u | wc -l)
    [ "$out" = 1 ] || self_test_fail "--envelope inside an envelope did not run in place ($out distinct cgroups)"

    # The LOCK proof (D4): under an ancestor that provably holds LOCK shared,
    # a capped call and --preflight are refused with 2 naming --envelope; a
    # stale marker proves nothing; outside, the preflight passes.
    local lockdir
    lockdir=$(mktemp -d "${TMPDIR:-/tmp}/with-memory-cap-locks.XXXXXX")
    status=0
    # shellcheck disable=SC2016 # expanded by the inner shell
    out=$(LUSHTEXT_AGENT_LOCK_DIR=$lockdir timeout 10 bash -c 'exec 7>>"$0/responsiveness-measure.lock"; flock -s 7; export LUSHTEXT_AGENT_LOCK_HELD=shared:$$; "$1" --preflight 2G 7>&-' "$lockdir" "$me" 2>&1) || status=$?
    [ "$status" -eq 2 ] && [[ "$out" == *--envelope* ]] || self_test_fail "--preflight 2G under a proven shared LOCK gave status $status ('$out'), expected 2 naming --envelope"
    status=0
    # shellcheck disable=SC2016 # expanded by the inner shell
    LUSHTEXT_AGENT_LOCK_DIR=$lockdir timeout 5 bash -c 'exec 7>>"$0/responsiveness-measure.lock"; flock -s 7; export LUSHTEXT_AGENT_LOCK_HELD=shared:$$; "$1" 2G true 7>&-' "$lockdir" "$me" 2>/dev/null || status=$?
    [ "$status" -eq 2 ] || self_test_fail "a 2G call under a proven shared LOCK gave status $status, expected 2"
    status=0
    LUSHTEXT_AGENT_LOCK_DIR=$lockdir timeout 10 "$me" --preflight 2G || status=$?
    [ "$status" -eq 0 ] || self_test_fail "--preflight 2G outside any LOCK gave status $status, expected 0"
    status=0
    LUSHTEXT_AGENT_LOCK_DIR=$lockdir LUSHTEXT_AGENT_LOCK_HELD=shared:999999999 timeout 5 "$me" 2G true 2>/dev/null || status=$?
    [ "$status" -eq 0 ] || self_test_fail "a 2G call with a stale marker gave status $status, expected 0 (admitted as an ordinary call)"
    rm -rf "${lockdir:?}"

    # Order corner cases: a head short of host memory (an 800G waiter in a
    # 900G slice) reserves nothing; a dead ticket neither reserves nor blocks;
    # an older host-level ticket does not hold back an envelope's inner call.
    local waiter owner ticket
    LUSHTEXT_HEAVY_MEMORY_MAX=900G timeout 10 "$me" 800G true 2>/dev/null &
    waiter=$!
    sleep 1
    status=0
    LUSHTEXT_HEAVY_MEMORY_MAX=900G timeout 3 "$me" 2G true 2>/dev/null || status=$?
    [ "$status" -eq 0 ] || self_test_fail "a 2G request was held behind an 800G waiter short of host memory (status $status)"
    kill "$waiter" 2>/dev/null || true
    wait "$waiter" 2>/dev/null || true
    if self_test_sleeper "$HEAVY_SLICE" 3G; then
        sleep 30 &
        owner=$!
        ticket=$("$me" --ticket 4G "$owner")
        kill "$owner" 2>/dev/null || true
        wait "$owner" 2>/dev/null || true
        status=0
        LUSHTEXT_HEAVY_MEMORY_MAX=6G timeout 3 "$me" 2G true 2>/dev/null || status=$?
        [ "$status" -eq 0 ] || self_test_fail "a dead 4G ticket held a 2G request back (status $status)"
        [ ! -e "$ticket" ] || self_test_fail "a dead ticket was not swept by the admission behind it"
        kill "${self_test_sleepers[-1]}" 2>/dev/null || true
        timeout 5 tail --pid="${self_test_sleepers[-1]}" -f /dev/null 2>/dev/null || true
    else
        self_test_fail "the dead-ticket sleeper never started"
    fi
    sleep 30 &
    owner=$!
    ticket=$("$me" --ticket 4G "$owner")
    status=0
    # shellcheck disable=SC2016 # expanded by the inner shell
    LUSHTEXT_HEAVY_MEMORY_MAX=6G timeout 10 "$me" --envelope 2G sh -c 'timeout 3 "$0" 2G true' "$me" 2>/dev/null || status=$?
    [ "$status" -eq 0 ] || self_test_fail "an older host-level 4G ticket held back a 2G call inside a 2G envelope (status $status)"
    rm -f "${ticket:?}"
    # No ticket where no host admission would remove it: inside a capped
    # scope (the command runs in place) or inside an envelope (its calls take
    # the envelope's queue), a ticket would hold its place, and reserve its
    # cap, for the command's whole run.
    ticket=$(LUSHTEXT_MEMORY_CAP_KIB=4194304 "$me" --ticket 4G "$owner" 2>/dev/null || true)
    [ -z "$ticket" ] || { self_test_fail "--ticket inside a capped scope created $ticket"; rm -f "$ticket"; }
    ticket=$(LUSHTEXT_HEAVY_MEMORY_MAX=6G timeout 10 "$me" --envelope 2G "$me" --ticket 4G "$owner" 2>/dev/null || true)
    [ -z "$ticket" ] || { self_test_fail "--ticket inside an envelope created $ticket"; rm -f "$ticket"; }
    kill "$owner" 2>/dev/null || true
    wait "$owner" 2>/dev/null || true

    # A SIGKILLed placeholder leaves its sub-slice's drop-in; the next
    # envelope admission sweeps it, with its queue.
    report=$rundir/killed
    : >"$report"
    LUSHTEXT_ENVELOPE_REPORT=$report timeout 40 "$me" --envelope 2G sleep 30 2>/dev/null &
    bg=$!
    if line=$(self_test_wait_line "$report" '^envelope-start: ' 10); then
        sub=$(self_test_field "$line" slice)
        placeholder=$(self_test_field "$line" placeholder)
        mkdir -p "$QUEUE_ROOT/$sub"
        kill -KILL "$placeholder" 2>/dev/null || true
        kill "$bg" 2>/dev/null || true
        wait "$bg" 2>/dev/null || true
        if [ -d "$DROPIN_ROOT/$sub.d" ]; then
            timeout 10 "$me" --envelope 2G true 2>/dev/null || self_test_fail "an envelope after a killed one failed"
            [ ! -d "$DROPIN_ROOT/$sub.d" ] && [ ! -e "$QUEUE_ROOT/$sub" ] \
                || self_test_fail "the drop-in or queue of an envelope whose placeholder was SIGKILLed survived the next envelope admission"
        else
            self_test_fail "a SIGKILLed placeholder left no drop-in to sweep (the case lost its precondition)"
        fi
    else
        self_test_fail "the SIGKILL case's envelope wrote no envelope-start line"
        kill "$bg" 2>/dev/null || true
    fi

    # H5: a revert under a live scope leaves memory.max at max, the scope
    # running.
    local probe=${HEAVY_SLICE%.slice}-env0t$$.slice cg=""
    if systemctl --user set-property --runtime "$probe" MemoryMax=512M MemorySwapMax=0 >/dev/null 2>&1; then
        systemd-run --user --scope --quiet --slice="$probe" sleep 5 &
        bg=$!
        for _ in $(seq 100); do
            cg=$(systemctl --user show -p ControlGroup --value "$probe" 2>/dev/null || true)
            [ -z "$cg" ] || break
            sleep 0.05
        done
        systemctl --user revert "$probe" >/dev/null 2>&1 || true
        out=$(cat "/sys/fs/cgroup$cg/memory.max" 2>/dev/null || true)
        { [ "$out" = max ] && kill -0 "$bg" 2>/dev/null; } || self_test_fail "H5: after a revert under a live scope memory.max read '$out' (expected max) or the scope stopped"
        kill "$bg" 2>/dev/null || true
        wait "$bg" 2>/dev/null || true
    else
        self_test_fail "H5: set-property on $probe failed"
    fi

    # The leftover check itself fails on a queue it is shown.
    local fake="${QUEUE_ROOT:?}/user-lushtext-heavy-selftest-fake$$.slice"
    mkdir -p "$fake"
    [ -n "$(self_test_leftovers "fake$$")" ] || self_test_fail "the leftover check missed an admission queue"
    rmdir "$fake"
    rm -rf "${rundir:?}"
    unset LUSHTEXT_HEAVY_MEMORY_MAX LUSHTEXT_MEMORY_RESERVE
}

self_test() {
    local me=$0 status
    # Probe fresh scopes whatever this process inherited: run inside a capped
    # scope (a capped `make` or `git commit` running the policy checks), every
    # probe below would otherwise run in place and read the outer scope's
    # limits. The nested-call check sets the variable explicitly.
    unset LUSHTEXT_MEMORY_CAP_KIB LUSHTEXT_ADMISSION_NOWAIT
    # Nor an enclosing helper's LOCK marker: the commit hook runs this under a
    # genuinely held shared LOCK, where every capped case would be refused.
    unset LUSHTEXT_AGENT_LOCK_HELD
    # All scratch of the cases below (markers, reports, the admission lock,
    # lock and envelope-test directories, the leftover probe, and the
    # envelopes' own pid directories) lives under one root that the cleanup
    # trap removes; a marker a late background command touches after that
    # fails harmlessly instead of recreating a file in /tmp. The trap is set
    # first, so a failure before the cases still removes the root.
    self_test_sleepers=()
    self_test_slices=()
    self_test_root=
    trap self_test_cleanup EXIT
    self_test_root=$(mktemp -d "${TMPDIR:-/tmp}/with-memory-cap-self-test.XXXXXX")
    export TMPDIR=$self_test_root
    # A private admission lock: the cases below hold it for seconds, which on
    # the real one would hold up every agent's admissions. Waiters poll fast.
    export LUSHTEXT_ADMISSION_LOCK
    LUSHTEXT_ADMISSION_LOCK=$(mktemp "${TMPDIR:-/tmp}/with-memory-cap-admission.XXXXXX")
    self_test_admission_lock=$LUSHTEXT_ADMISSION_LOCK
    export LUSHTEXT_ADMISSION_POLL_SECONDS=0.2
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
    # The envelope's pid file lives in an `lt-envelope-<pid>-` directory of
    # TMPDIR, which with-temp-scope.sh's stale sweep reclaims if a SIGKILL
    # lands before envelope_run removes it.
    local pid_dir
    pid_dir=$(envelope_pid_dir 2>/dev/null) || pid_dir=
    if [[ "${pid_dir##*/}" =~ ^lt-envelope-$$-[A-Za-z0-9]{6}$ && -d "$pid_dir" && "${pid_dir%/*}" == "${TMPDIR:-/tmp}" ]]; then
        rm -rf -- "$pid_dir"
    else
        self_test_fail "the envelope pid directory was '$pid_dir', expected \${TMPDIR:-/tmp}/lt-envelope-$$-XXXXXX"
    fi
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
    self_test_verdict_fixtures
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
        # A daemon a capped command leaves behind (an sccache server started by
        # the first compile) keeps its scope alive, but not its admission: the
        # cap is reserved for the command, and only while it runs. A leftover
        # used to pin its whole cap until the daemon exited (2026-11-15: a
        # 36G scope held by sccache refused every measured mutation round).
        local leftover
        leftover=$(mktemp)
        # shellcheck disable=SC2016 # expanded by the inner shell, inside the scope
        LUSHTEXT_HEAVY_MEMORY_MAX=3G LUSHTEXT_MEMORY_RESERVE=1G "$me" 2G \
            sh -c 'setsid sleep 30 </dev/null >/dev/null 2>&1 & echo $! >"$0"' "$leftover" 2>/dev/null
        status=0
        LUSHTEXT_ADMISSION_NOWAIT=1 LUSHTEXT_HEAVY_MEMORY_MAX=3G LUSHTEXT_MEMORY_RESERVE=1G timeout 5 "$me" 2G true 2>/dev/null || status=$?
        kill "$(cat "$leftover")" 2>/dev/null || true
        rm -f "$leftover"
        [ "$status" -eq 0 ] || self_test_fail "a daemon left in a finished command's scope kept its cap admitted (status $status)"
        # ... while a command that still runs keeps its cap admitted.
        LUSHTEXT_HEAVY_MEMORY_MAX=3G LUSHTEXT_MEMORY_RESERVE=1G "$me" 2G sleep 10 2>/dev/null &
        waiter=$!
        sleep 1
        status=0
        LUSHTEXT_ADMISSION_NOWAIT=1 LUSHTEXT_HEAVY_MEMORY_MAX=3G LUSHTEXT_MEMORY_RESERVE=1G timeout 5 "$me" 2G true 2>/dev/null || status=$?
        kill "$waiter" 2>/dev/null; wait "$waiter" 2>/dev/null || true
        [ "$status" -eq 75 ] || self_test_fail "a running command's cap was not counted (status $status)"
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
        self_test_admission_cases
        self_test_envelope_cases
        # Every envelope above removed its pid directory before its command ran.
        local left
        left=$(find "$self_test_root" -mindepth 1 -maxdepth 1 \( -name 'lt-envelope-*' -o -name 'with-memory-cap-envelope.*' \) -printf '%f ' 2>/dev/null)
        [ -z "$left" ] || self_test_fail "the envelopes left pid entries behind: $left"
        local after_max
        after_max=$(systemctl --user show -p MemoryMax --value "$real_slice" 2>/dev/null || true)
        [ "$after_max" = "$real_max" ] || self_test_fail "the real $real_slice MemoryMax changed from '$real_max' to '$after_max'"
        [ "$self_test_failures" -eq 0 ] || { echo "with-memory-cap self-test: $self_test_failures case(s) failed" >&2; return 1; }
        echo "with-memory-cap self-test: ok (scope sets MemoryMax, no swap and OOMPolicy=continue inside $HEAVY_SLICE, under the user manager's user.slice, with no CPU property; CPUs: $cpus_case; the placement check refuses a process outside its slice (78) and re-reads a CPU disagreement; old slice names refused; nested calls run in place; --address-space fails an allocation cleanly; Ctrl-C reaches the command; arguments with \$ pass unexpanded; --wait-admissible waits without reserving; NOWAIT admission fails fast, even behind a waiter; a leftover daemon does not keep its cap admitted; a shared self-test slice's scope not counted; legacy: $legacy_case; the check-once admission gives up a busy admission lock in 2 s; fair order: a small request waits behind an older waiter that has the memory, not behind one short of it or a dead one, NOWAIT never; envelopes: counted on the host by their placeholder, inner scopes admitted against the envelope one after another and never behind the host queue, the sub-slice keeps its limit, peak and oom_kill reported, a leftover daemon keeps no admission, a killed placeholder swept, nested envelopes in place, an oversized inner cap refused; a proven shared LOCK refused with 2 (and by --preflight), a stale marker not; H5 re-probed; no drop-in or queue left; the real slice's budget untouched)"
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
    leftovers=$(self_test_leftovers "$self_test_child")
    if [ -n "$leftovers" ]; then
        echo "self-test: the private slices left drop-in directories or admission queues behind: $leftovers" >&2
        exit 1
    fi
    exit "$status"
fi

# Self-test entry to the pure admission decision: KiB values, exit 0 (admit)
# or 1 (wait).
if [ "${1:-}" = "--admission-verdict" ]; then
    [ "$#" -eq 6 ] || [ "$#" -eq 7 ] || { echo "usage: $0 --admission-verdict <admitted> <available> <budget> <reserve|-> <cap> [<head cap>]" >&2; exit 2; }
    shift
    admission_verdict "$@"
    exit
fi

# A size argument: a systemd memory size or `none`; exits 2 otherwise.
check_cap() {
    [ "$1" = none ] || [[ "$1" =~ ^[0-9]+[KMGT]?$ ]] || {
        echo "with-memory-cap: invalid cap '$1' (expected e.g. 16G, 12288M, or none)" >&2
        exit 2
    }
}

# `--ticket <cap> <owner pid>`: take a place in the heavy slice's admission
# order for a waiter (scripts/heavy.sh, which keeps it across its retries and
# removes it on exit) and print the ticket's path. Prints nothing where no
# host admission would ever remove it: caps that are not admitted (`none`, 1G
# or less), inside a capped scope (the command runs in place), with no user
# manager (it runs uncapped) and inside an envelope (its calls take the
# envelope's own queue). A ticket there would hold its place, and reserve its
# cap, for the command's whole run.
if [ "${1:-}" = "--ticket" ]; then
    [ "$#" -eq 3 ] || { echo "usage: $0 --ticket <cap> <owner-pid>" >&2; exit 2; }
    check_cap "$2"
    [ "$2" != none ] || exit 0
    cap_kib=$(cap_in_kib "$2")
    [ "$cap_kib" -gt $((1024 * 1024)) ] || exit 0
    [ -z "${LUSHTEXT_MEMORY_CAP_KIB:-}" ] || exit 0
    ! envelope_context || exit 0
    scope_available || exit 0
    exec 9>"$ADMISSION_LOCK"
    flock 9
    ticket_create "$cap_kib" "$3" "$(queue_dir_of "$HEAVY_SLICE")"
    exit
fi

# `--preflight <cap>`: the LOCK check of an admission of <cap> (D4 in
# lock_policy) without admitting anything. A Makefile recipe that admits its
# own scopes starts with it, so `heavy.sh --mem none -- make <lane>` fails at
# once instead of after its first phase. Every exemption of the real
# admission applies: `none`, a nested in-place call, 1G or less, no marker, an
# envelope.
if [ "${1:-}" = "--preflight" ]; then
    [ "$#" -eq 2 ] || { echo "usage: $0 --preflight <cap|none>" >&2; exit 2; }
    cap=$2
    check_cap "$cap"
    [ "$cap" != none ] || exit 0
    cap_kib=$(cap_in_kib "$cap")
    [ -z "${LUSHTEXT_MEMORY_CAP_KIB:-}" ] || exit 0
    [ "$cap_kib" -gt $((1024 * 1024)) ] || exit 0
    [ -n "${LUSHTEXT_AGENT_LOCK_HELD:-}" ] || exit 0
    envelope_context || true
    lock_policy
    exit 0
fi

if [ "${1:-}" = "--wait-admissible" ]; then
    [ "$#" -eq 2 ] || { echo "usage: $0 --wait-admissible <cap|none>" >&2; exit 2; }
    cap=$2
    check_cap "$cap"
    [ "$cap" != none ] || exit 0
    cap_kib=$(cap_in_kib "$cap")
    # Nested calls run in place and small caps skip admission: nothing to wait for.
    [ -z "${LUSHTEXT_MEMORY_CAP_KIB:-}" ] || exit 0
    [ "$cap_kib" -gt $((1024 * 1024)) ] || exit 0
    scope_available || exit 0
    envelope_context || true
    lock_policy
    budget_setup
    bypass=0
    [ "$lock_admission" != nowait ] || bypass=1
    announced=0
    while ! admission_check "$admission_ticket" "$bypass"; do
        # A check-once probe: inside a measurement, never wait.
        [ "$lock_admission" = wait ] || exit 75
        [ "$announced" = 1 ] || { waiting_message; announced=1; }
        sleep "$POLL_SECONDS"
    done
    exit 0
fi

# Internal: an envelope's placeholder, the first process of its
# lushtext-cap-<P>-<ns>.scope in the heavy slice (P is this process). It
# waits for the envelope's root command ($2) to exit; then it reads the
# sub-slice's memory.peak and oom_kill (cgroup v2 counts them over the whole
# sub-tree; the sentinel scope keeps the sub-slice alive until now), reverts
# the sub-slice's runtime limits, removes its admission queue, and, last,
# writes `envelope: peak=<MiB> oom_kill=<n> slice=<sub> placeholder=<P>` to
# the report file ($4, `-` for stderr). Exiting ends the reservation.
if [ "${1:-}" = "--envelope-placeholder" ]; then
    [ "$#" -eq 4 ] || { echo "usage: $0 --envelope-placeholder <root-pid> <ns> <report|->" >&2; exit 2; }
    sub=${HEAVY_SLICE%.slice}-env$$t$3.slice
    if [ "$4" != - ]; then
        exec 3>>"$4"
    else
        exec 3>&2
    fi
    tail --pid="$2" -s 0.2 -f /dev/null 2>/dev/null || true
    peak=unknown
    oom=unknown
    sub_cg=$(systemctl --user show -p ControlGroup --value "$sub" 2>/dev/null || true)
    if [ -n "$sub_cg" ] && read -r bytes <"/sys/fs/cgroup$sub_cg/memory.peak" 2>/dev/null; then
        peak=$((bytes / 1048576))
        oom=$(awk '$1 == "oom_kill" { print $2 }' "/sys/fs/cgroup$sub_cg/memory.events" 2>/dev/null || true)
        oom=${oom:-unknown}
    fi
    systemctl --user revert "$sub" >/dev/null 2>&1 || true
    rm -rf "${QUEUE_ROOT:?}/$sub"
    # The sentinel has done its job; with it gone, an empty sub-slice is
    # stopped now rather than left loaded until logout. One that a leftover
    # daemon still occupies stays, and the next envelope admission's sweep
    # stops it once the daemon has exited.
    systemctl --user stop "lushtext-envsentinel-$$-$3.scope" >/dev/null 2>&1 || true
    if [ -n "$sub_cg" ] && cgroup_unpopulated "/sys/fs/cgroup$sub_cg"; then
        systemctl --user stop "$sub" >/dev/null 2>&1 || true
    fi
    echo "envelope: peak=$peak oom_kill=$oom slice=$sub placeholder=$$" >&3
    exit 0
fi

address_space=0
envelope_mode=0
case "${1:-}" in
    --address-space)
        address_space=1
        shift
        ;;
    --envelope)
        envelope_mode=1
        shift
        ;;
esac

if [ "$#" -lt 2 ]; then
    echo "usage: $0 [--address-space] <cap|none> <command> [args...]" >&2
    echo "       $0 --envelope <size> <command> [args...]" >&2
    exit 2
fi

cap=$1
shift

if [ "$envelope_mode" = 1 ] && ! [[ "$cap" =~ ^[0-9]+[KMGT]?$ ]]; then
    echo "with-memory-cap: invalid envelope size '$cap' (expected e.g. 16G; an envelope has a size)" >&2
    exit 2
fi

if [ "$cap" = "none" ]; then
    exec "$@"
fi

check_cap "$cap"
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
    envelope_context || true
    if [ "$envelope_mode" = 1 ] && [ "$inside_envelope" = 1 ]; then
        # The outer envelope bounds the command and reports its OOM kills.
        echo "with-memory-cap: already inside the envelope $adm_slice; running in place: $shown_command" >&2
        exec "$@"
    fi
    admitting=0
    [ "$cap_kib" -le $((1024 * 1024)) ] || admitting=1
    [ "$admitting" = 0 ] || lock_policy
    budget_setup
    if [ "$inside_envelope" = 0 ]; then
        systemctl --user set-property --runtime "$HEAVY_SLICE" \
            MemoryMax="$heavy_max" MemorySwapMax=0 >/dev/null 2>&1 || true
    fi
    [ "$admitting" = 0 ] || admit "$lock_admission"
    if [ "$envelope_mode" = 1 ]; then
        envelope_run "$@"
    fi
    unit=lushtext-cap-$$-$(date +%s%N)
    launch=(bash "${BASH_SOURCE[0]}" --exec-in-scope "$adm_slice" - "$@")
    if [ "$address_space" = 1 ]; then
        launch=(bash "${BASH_SOURCE[0]}" --exec-in-scope "$adm_slice" $((cap_kib * 7 / 8)) "$@")
        echo "with-memory-cap: MemoryMax=$cap MemorySwapMax=0 OOMPolicy=continue in $adm_slice ($heavy_max), RLIMIT_AS=$((cap_kib * 7 / 8)) KiB per process, for: $shown_command" >&2
    else
        echo "with-memory-cap: MemoryMax=$cap MemorySwapMax=0 OOMPolicy=continue in $adm_slice ($heavy_max) for: $shown_command" >&2
    fi
    export LUSHTEXT_MEMORY_CAP_KIB=$cap_kib
    if [ "$admitting" = 1 ]; then
        # The admission lock passes to a detached watcher (the subshell exits at
        # once, so the command never inherits a child it did not start), which
        # releases it once this process's scope exists.
        ( release_admission_once_scoped "$$" "$unit" </dev/null >/dev/null 2>&1 & )
    fi
    # exec, not a background job: a background job of a non-interactive shell
    # ignores SIGINT and SIGQUIT, and every descendant inherited that, so a
    # Ctrl-C left the capped work running. As the same process, the command
    # gets the terminal's signals and its exit status reaches the caller
    # directly. fd 9 is closed so the command does not hold the lock.
    # The unit is named after this PID, which becomes the command's (see
    # admitted_in): a daemon left in the scope then holds no admission.
    exec systemd-run --user --scope --quiet --expand-environment=no --slice="$adm_slice" \
        --unit="$unit" \
        -p MemoryMax="$cap" -p MemorySwapMax=0 -p OOMPolicy=continue -- "${launch[@]}" 9>&-
fi

if [ -n "${CI:-}" ]; then
    echo "with-memory-cap: no systemd user manager on CI; running uncapped: $shown_command" >&2
    exec "$@"
fi

echo "with-memory-cap: cannot create a memory-capped scope (systemd-run --user unavailable)." >&2
echo "Refusing to run uncapped. Pass the cap 'none' (e.g. KANI_MEMORY_MAX=none) to opt out explicitly." >&2
exit 1
