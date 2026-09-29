#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Run one test or smoke command with a private TMPDIR and fail if the command
# leaves anything behind in it.
#
# Every `std::env::temp_dir()`, `tempfile`, `mktemp`, Python `tempfile`, and
# GLib `g_get_tmp_dir()` call made by the command and its descendants resolves
# inside the scope, so the check is exact and scoped to this run: parallel runs
# each get their own scope and cannot see or blame one another's files. The
# scope is removed on every exit path the shell can observe (success, failure,
# SIGINT, SIGTERM); a run killed outright is reclaimed by the next run's sweep.
#
# Before creating its scope, the wrapper sweeps the base temp directory for
# stale LushText test roots: exact known prefixes followed by a PID, real
# directories owned by the current user, whose PID no longer exists and which
# have been idle for an hour. Entries of a live run are never touched.

set -euo pipefail

STALE_MIN_AGE_MINUTES=60
LEGACY_PIDLESS_MIN_AGE_MINUTES=1440
# Prefixes followed by `<pid>` or `<pid>-<suffix>` in names our tooling creates.
PID_PREFIXES=(
    lt-scope-
    lushtext-test-
    lt-proof-
    gtk-lush-proof-run-
    gtk-lush-proof-runtime-
)
# Prefixes whose legacy names carried no PID; reclaimed after a day idle.
PIDLESS_LEGACY_PREFIXES=(
    gtk-lush-proof-runtime-
)
MAX_REPORTED_LEFTOVERS=20

usage() {
    cat <<'EOF'
Usage: scripts/with-temp-scope.sh [--session-runtime] LABEL -- COMMAND [ARGS...]
       scripts/with-temp-scope.sh --sweep-only
       scripts/with-temp-scope.sh --self-test

Run COMMAND with TMPDIR set to a private scope directory, then fail if the
command left any entry in it. The scope is always removed.

--session-runtime also exports XDG_RUNTIME_DIR as a short, mode-0700
directory inside the scope for a private dbus-run-session/Mutter session, and
removes it once no process still uses it (see release_session_runtime_dir).
EOF
}

# Print the PID a harness-created name carries after PREFIX, or nothing.
owning_pid() {
    local rest="$1"
    local digits="${rest%%-*}"
    if [[ -n "$digits" && "$digits" =~ ^[0-9]+$ && ${#digits} -le 10 ]]; then
        printf '%s\n' "$digits"
    fi
}

sweep_stale_entries() {
    local base="$1"
    [[ -d /proc/self ]] || return 0
    local uid prefix entry name rest pid
    uid="$(id -u)"
    for prefix in "${PID_PREFIXES[@]}"; do
        while IFS= read -r -d '' entry; do
            name="${entry##*/}"
            rest="${name#"$prefix"}"
            pid="$(owning_pid "$rest")"
            [[ -n "$pid" ]] || continue
            [[ "$pid" != "$$" && ! -e "/proc/$pid" ]] || continue
            rm -rf -- "$entry" 2>/dev/null || true
        done < <(find "$base" -mindepth 1 -maxdepth 1 -type d -user "$uid" \
            -name "${prefix}*" -mmin "+$STALE_MIN_AGE_MINUTES" -print0 2>/dev/null)
    done
    for prefix in "${PIDLESS_LEGACY_PREFIXES[@]}"; do
        while IFS= read -r -d '' entry; do
            name="${entry##*/}"
            rest="${name#"$prefix"}"
            [[ -z "$(owning_pid "$rest")" ]] || continue
            rm -rf -- "$entry" 2>/dev/null || true
        done < <(find "$base" -mindepth 1 -maxdepth 1 -type d -user "$uid" \
            -name "${prefix}*" -mmin "+$LEGACY_PIDLESS_MIN_AGE_MINUTES" -print0 2>/dev/null)
    done
}

SESSION_RUNTIME_NAME=".session-runtime"
SESSION_RELEASE_POLLS=200   # x 0.05 s = 10 s
SESSION_RELEASE_POLL_SECONDS=0.05

# Whether any readable process environment carries DIR as its XDG_RUNTIME_DIR.
session_runtime_in_use() {
    local dir="$1"
    grep -qlzxF -- "XDG_RUNTIME_DIR=$dir" /proc/[0-9]*/environ 2>/dev/null
}

# Remove a private session's runtime directory once its processes are gone.
#
# dbus-run-session returns when its direct child exits, while the services its
# private bus activated are still shutting down; xdg-document-portal still has
# its FUSE mount on doc/, so an immediate `rm -rf` fails with "cannot remove
# .../doc" and the directory is left behind. Waiting for every process that
# carries the directory as XDG_RUNTIME_DIR to exit makes removal deterministic.
release_session_runtime_dir() {
    local dir="$1"
    local polls=0
    while (( polls < SESSION_RELEASE_POLLS )) && session_runtime_in_use "$dir"; do
        sleep "$SESSION_RELEASE_POLL_SECONDS"
        polls=$((polls + 1))
    done
    local attempt
    for attempt in 1 2 3 4 5; do
        rm -rf -- "$dir" 2>/dev/null || true
        [[ -e "$dir" ]] || return 0
        sleep 0.2
    done
    echo "TEMP-LEAK: session runtime directory $dir is still in use and could not be removed." >&2
    return 1
}

report_leftovers() {
    local label="$1"
    local scope="$2"
    local leftovers=()
    local entry
    while IFS= read -r -d '' entry; do
        leftovers+=("${entry##*/}")
    done < <(find "$scope" -mindepth 1 -maxdepth 1 -print0 2>/dev/null | sort -z)
    if (( ${#leftovers[@]} == 0 )); then
        return 0
    fi
    echo "TEMP-LEAK: '$label' left ${#leftovers[@]} temporary entr$( (( ${#leftovers[@]} == 1 )) && echo y || echo ies) behind:" >&2
    local shown=0
    for entry in "${leftovers[@]}"; do
        if (( shown == MAX_REPORTED_LEFTOVERS )); then
            echo "    ... and $(( ${#leftovers[@]} - shown )) more" >&2
            break
        fi
        echo "    $entry" >&2
        shown=$((shown + 1))
    done
    echo "Every temporary file a test creates must be removed by its owner; see the" >&2
    echo "temporary-file rule in .agents/rules/build.md." >&2
    return 1
}

run_scoped() {
    local session_runtime="$1"
    local label="$2"
    shift 2
    if ! [[ "$label" =~ ^[A-Za-z0-9_-]+$ ]]; then
        echo "Error: scope label must match [A-Za-z0-9_-]+, got '$label'." >&2
        return 2
    fi
    local base="${TMPDIR:-/tmp}"
    base="${base%/}"
    sweep_stale_entries "$base"

    local scope
    scope="$(mktemp -d "$base/lt-scope-$$-$label-XXXXXX")"
    # shellcheck disable=SC2064 # expand the scope path now, not at exit
    trap "rm -rf -- '$scope'" EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM

    local status=0
    if [[ "$session_runtime" == true ]]; then
        local runtime_dir="$scope/$SESSION_RUNTIME_NAME"
        mkdir -m 700 -- "$runtime_dir"
        TMPDIR="$scope" XDG_RUNTIME_DIR="$runtime_dir" "$@" || status=$?
        if ! release_session_runtime_dir "$runtime_dir"; then
            (( status != 0 )) || status=1
        fi
    else
        TMPDIR="$scope" "$@" || status=$?
    fi

    if ! report_leftovers "$label" "$scope"; then
        (( status != 0 )) || status=1
    fi
    return "$status"
}

self_test() {
    local base failed=0
    base="$(mktemp -d)"
    # shellcheck disable=SC2064
    trap "rm -rf -- '$base'" RETURN

    if ! TMPDIR="$base" "$0" clean -- true 2>/dev/null; then
        echo "Error: a command that leaves nothing behind was rejected." >&2
        failed=1
    fi
    if TMPDIR="$base" "$0" leaky -- sh -c 'mkdir "$TMPDIR/lushtext-test-4242"' 2>/dev/null; then
        echo "Error: a command that leaked a directory was accepted." >&2
        failed=1
    fi
    if ! TMPDIR="$base" "$0" --session-runtime session -- \
        sh -c 'test -d "$XDG_RUNTIME_DIR" && mkdir "$XDG_RUNTIME_DIR/doc"' 2>/dev/null; then
        echo "Error: a session runtime directory was not provided or not released." >&2
        failed=1
    fi
    if TMPDIR="$base" "$0" failing -- false 2>/dev/null; then
        echo "Error: the command's own failure was not propagated." >&2
        failed=1
    fi
    if [[ -n "$(find "$base" -mindepth 1 -maxdepth 1 -name 'lt-scope-*' -print -quit)" ]]; then
        echo "Error: a scope directory survived its run." >&2
        failed=1
    fi

    # Stale dead-PID roots and legacy names are swept; live, fresh, foreign
    # prefixes and plain files are not.
    local dead_pid=4000000000
    mkdir -p "$base/lushtext-test-$dead_pid" "$base/lt-proof-$dead_pid-abc/x" \
        "$base/lushtext-test-$$-live" "$base/lushtext-test-$((dead_pid + 1))-fresh" \
        "$base/gtk-lush-proof-runtime-Ab3xYz" "$base/unrelated-$dead_pid"
    touch "$base/lushtext-test-$((dead_pid + 2))"
    touch -d '2 hours ago' "$base/lushtext-test-$dead_pid" "$base/lt-proof-$dead_pid-abc" \
        "$base/lushtext-test-$$-live" "$base/unrelated-$dead_pid" "$base/lushtext-test-$((dead_pid + 2))"
    touch -d '2 days ago' "$base/gtk-lush-proof-runtime-Ab3xYz"
    sweep_stale_entries "$base"
    local path
    for path in "lushtext-test-$dead_pid" "lt-proof-$dead_pid-abc" "gtk-lush-proof-runtime-Ab3xYz"; do
        if [[ -e "$base/$path" ]]; then
            echo "Error: the sweep kept stale entry $path." >&2
            failed=1
        fi
    done
    for path in "lushtext-test-$$-live" "lushtext-test-$((dead_pid + 1))-fresh" \
        "unrelated-$dead_pid" "lushtext-test-$((dead_pid + 2))"; do
        if [[ ! -e "$base/$path" ]]; then
            echo "Error: the sweep removed protected entry $path." >&2
            failed=1
        fi
    done

    if (( failed != 0 )); then
        return 1
    fi
    echo "Temporary-file scope self-test passed."
}

case "${1:-}" in
    --self-test)
        self_test
        ;;
    --sweep-only)
        base="${TMPDIR:-/tmp}"
        sweep_stale_entries "${base%/}"
        ;;
    -h|--help)
        usage
        ;;
    "")
        usage >&2
        exit 2
        ;;
    *)
        session_runtime=false
        if [[ "$1" == "--session-runtime" ]]; then
            session_runtime=true
            shift
        fi
        label="${1:-}"
        shift || true
        if [[ "${1:-}" != "--" ]]; then
            usage >&2
            exit 2
        fi
        shift
        if (( $# == 0 )); then
            usage >&2
            exit 2
        fi
        run_scoped "$session_runtime" "$label" "$@"
        ;;
esac
