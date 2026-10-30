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

# True when an ancestor helper already holds LOCK for this process tree
# (LUSHTEXT_AGENT_LOCK_HELD=shared|exclusive, exported by heavy.sh and
# measure.sh) and the lock really is held by someone, so a stale variable
# inherited by an unrelated shell cannot skip the protocol. A nested helper
# then runs in place: taking the locks again would deadlock as soon as a
# measurement is pending (the outer command holds LOCK shared, the
# measurement holds INTENT and waits for LOCK, the inner call waits for
# INTENT).
agent_lock_inherited() {
    [ -n "${LUSHTEXT_AGENT_LOCK_HELD:-}" ] || return 1
    ! flock -n -x "$AGENT_LOCK_MEASURE" true
}
