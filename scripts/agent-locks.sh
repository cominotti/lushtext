# SPDX-License-Identifier: GPL-3.0-or-later
# shellcheck shell=bash
#
# Lock paths of the shared-machine protocol, sourced by scripts/heavy.sh and
# scripts/measure.sh (see "Shared-machine agent work" in
# .agents/rules/build.md).
#
# The locks live in the MAIN checkout's target/ directory, whatever worktree a
# helper runs from: every worktree of the repository competes for the same
# CPU, memory and disk, so they must share one lock pair. The main checkout is
# the parent of `git rev-parse --git-common-dir`.
#
# LUSHTEXT_AGENT_LOCK_DIR overrides the directory. The self-tests use it so
# they never touch the real locks (they run in the pre-commit hook, which is
# itself started under the shared lock).

agent_lock_dir() {
    if [ -n "${LUSHTEXT_AGENT_LOCK_DIR:-}" ]; then
        printf '%s\n' "$LUSHTEXT_AGENT_LOCK_DIR"
        return
    fi
    local here common
    here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
    if common=$(git -C "$here" rev-parse --path-format=absolute --git-common-dir 2>/dev/null); then
        printf '%s/target\n' "$(dirname "$common")"
    else
        printf '%s/target\n' "$(dirname "$here")"
    fi
}

# Sets AGENT_LOCK_INTENT and AGENT_LOCK_MEASURE, creating their directory.
# shellcheck disable=SC2034 # read by the scripts that source this file
agent_lock_paths() {
    local dir
    dir=$(agent_lock_dir)
    mkdir -p "$dir"
    AGENT_LOCK_INTENT="$dir/responsiveness-measure.intent"
    AGENT_LOCK_MEASURE="$dir/responsiveness-measure.lock"
}

# The marker a helper exports while it holds LOCK on fd 7 itself:
# LUSHTEXT_AGENT_LOCK_HELD=<shared|exclusive>:<holder pid>.
agent_lock_marker() {
    printf '%s:%s\n' "$1" "$$"
}

# True when process $1 is this process or one of its ancestors.
agent_is_ancestor() {
    local want=$1 pid=$$ stat rest
    while [ "$pid" -gt 1 ]; do
        [ "$pid" = "$want" ] && return 0
        stat=$(cat "/proc/$pid/stat" 2>/dev/null) || return 1
        # The process name (field 2) may hold spaces and parentheses; the
        # parent PID is the second field after its closing parenthesis.
        rest=${stat##*) }
        pid=${rest#* }
        pid=${pid%% *}
    done
    return 1
}

# Succeeds, printing the mode (shared|exclusive), when an enclosing helper
# PROVABLY holds LOCK for this process tree: the marker names a holder PID
# that is an ancestor of this process and whose fd 7 is still the LOCK file
# (and someone holds it). A nested helper then runs in place: taking the locks
# again would deadlock as soon as a measurement is pending (the outer command
# holds LOCK shared, the measurement holds INTENT and waits for LOCK, the inner
# call waits for INTENT).
#
# The marker alone proves nothing: a measured command, a daemon left behind
# by a heavy command, or a shell that copied the environment can carry a stale
# one while some unrelated helper holds LOCK. Without the PID check such a
# process ran heavy work in place in the middle of a measurement.
agent_lock_inherited() {
    local marker=${LUSHTEXT_AGENT_LOCK_HELD:-} mode pid
    mode=${marker%%:*}
    pid=${marker#*:}
    case "$mode" in shared | exclusive) ;; *) return 1 ;; esac
    [[ "$pid" =~ ^[1-9][0-9]*$ ]] || return 1
    [ "$pid" != "$$" ] || return 1
    agent_is_ancestor "$pid" || return 1
    [ "/proc/$pid/fd/7" -ef "$AGENT_LOCK_MEASURE" ] || return 1
    ! flock -n -x "$AGENT_LOCK_MEASURE" true || return 1
    printf '%s\n' "$mode"
}

# Print the processes that have lock file $1 open (holders and waiters), for
# a report when a lock wait gives up. /proc/locks is no use here: it records
# the PID that took the lock, which for `flock` on an inherited descriptor is
# a flock(1) process long gone, while the lock lives on in its parent.
agent_lock_holders() {
    local file=$1 fd pid
    for fd in /proc/[0-9]*/fd/*; do
        [ "$fd" -ef "$file" ] 2>/dev/null || continue
        pid=${fd#/proc/}
        printf '%s\n' "${pid%%/*}"
    done 2>/dev/null | sort -un | while read -r pid; do
        [ "$pid" != "$$" ] || continue
        ps -o pid=,etime=,args= -p "$pid" 2>/dev/null | cut -c1-240 | sed 's/^/    /' || true
    done
}

# Trap SIGINT, SIGTERM and SIGHUP from the moment a helper holds its locks:
# a signal must not kill it while a copy of the locks is still open, so a
# signal is recorded (before the command starts, which then is skipped) or
# forwarded to the command by agent_run_child.
agent_trap_signals() {
    agent_child=
    agent_aborted=
    trap 'agent_forward INT' INT
    trap 'agent_forward TERM' TERM
    trap 'agent_forward HUP' HUP
}

# shellcheck disable=SC2329 # invoked by the traps of agent_trap_signals
agent_forward() {
    if [ -n "${agent_child:-}" ]; then
        kill "-$1" "$agent_child" 2>/dev/null || true
    else
        agent_aborted=$1
    fi
}

# shellcheck disable=SC2034 # agent_child_status is read by the callers
# After agent_trap_signals: run "$@" as a child that does NOT inherit the lock
# descriptors (fds 6 and 7), so nothing it leaves running keeps a lock held,
# while this process keeps them; forward signals to it. Sets
# agent_child_status to the command's exit status (128+N for a signal) and
# removes the traps.
agent_run_child() {
    agent_child_status=0
    if [ -z "$agent_aborted" ]; then
        # A background job of a non-interactive shell ignores SIGINT and
        # SIGQUIT; `env --default-signal` restores them so the command sees
        # Ctrl-C.
        env --default-signal=INT,QUIT -- "$@" 6>&- 7>&- &
        agent_child=$!
        # A signal that arrived between the fork and this point was only
        # recorded.
        [ -z "$agent_aborted" ] || kill "-$agent_aborted" "$agent_child" 2>/dev/null || true
        # `wait` returns early when a trapped signal arrives; wait again until
        # the command has really exited, then keep its status.
        while :; do
            wait "$agent_child" && agent_child_status=0 || agent_child_status=$?
            kill -0 "$agent_child" 2>/dev/null || break
        done
    else
        agent_child_status=$((128 + $(kill -l "$agent_aborted")))
    fi
    trap - INT TERM HUP
}
