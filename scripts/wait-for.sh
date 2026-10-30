#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Wait for one condition safely, instead of a hand-written polling loop:
#
#   scripts/wait-for.sh --pid N             [--timeout S]  # until process N has exited
#   scripts/wait-for.sh --name EXACT_NAME   [--timeout S]  # until no process has that exact name
#   scripts/wait-for.sh --file PATH [--nonempty] [--timeout S]  # until PATH exists (and is non-empty)
#   scripts/wait-for.sh --self-test
#
# Exits 0 when the condition holds, 124 on timeout, 2 on a usage error.
#
# Never `pgrep -f`: it matches whole command lines, and the shell running the
# wait loop carries the pattern in its own command line, so the loop matches
# itself and never ends (agents' loops ran for up to 32 hours). --name matches
# the kernel's process name exactly (`pgrep -x`), which is at most 15
# characters; a longer or space-containing name could never match and would
# report "gone" at once, so it is refused. Prefer --pid whenever the PID is
# known: it waits on that one process (`tail --pid`), with no polling of the
# process table. See "Shared-machine agent work" in .agents/rules/build.md.
#
# Never call this while holding the measurement locks exclusively.

set -euo pipefail

usage() {
    echo "usage: $0 (--pid N | --name EXACT_NAME | --file PATH [--nonempty]) [--timeout S]" >&2
    echo "       $0 --self-test" >&2
    exit 2
}

self_test() {
    local me=$0 dir status pid name
    dir=$(mktemp -d "${TMPDIR:-/tmp}/wait-for-self-test.XXXXXX")
    # shellcheck disable=SC2064 # expand now: the trap must remove this dir
    trap "rm -rf '$dir'" EXIT

    expect() {
        local want=$1 got=0
        shift
        "$me" "$@" 2>/dev/null || got=$?
        [ "$got" -eq "$want" ] || { echo "wait-for self-test: '$*' exited $got, expected $want" >&2; return 1; }
    }

    expect 2
    expect 2 --pid abc
    expect 2 --name 'cargo build'
    expect 2 --name abcdefghijklmnop
    expect 2 --file "$dir/x" --timeout soon
    expect 2 --pid 1 --name init

    expect 124 --file "$dir/absent" --timeout 1
    : >"$dir/empty"
    expect 0 --file "$dir/empty"
    expect 124 --file "$dir/empty" --nonempty --timeout 1
    (sleep 0.5; echo x >"$dir/later") &
    expect 0 --file "$dir/later" --nonempty --timeout 10
    wait

    sleep 0.3 &
    pid=$!
    expect 0 --pid "$pid" --timeout 10
    sleep 30 &
    pid=$!
    expect 124 --pid "$pid" --timeout 1
    kill "$pid"
    wait "$pid" 2>/dev/null || true

    # A process with a unique exact name: a copy of sleep named wf<pid>.
    name="wf$$"
    cp "$(command -v sleep)" "$dir/$name"
    "$dir/$name" 30 &
    pid=$!
    expect 124 --name "$name" --timeout 1
    kill "$pid"
    wait "$pid" 2>/dev/null || true
    expect 0 --name "$name" --timeout 5

    # No agent-facing script or make target may wait with `pgrep -f`.
    local root offenders
    root=$(cd "$(dirname "$me")/.." && pwd)
    offenders=$(grep -rnE 'pgrep([[:space:]]+[^|;&[:space:]]+)*[[:space:]]+(-[[:alnum:]]*f[[:alnum:]]*|--full)([[:space:]]|$)' \
        "$root/scripts" "$root/Makefile" "$root/.githooks" "$root/.github" 2>/dev/null \
        | grep -v '^[^:]*\.md:' | grep -vF "$root/scripts/wait-for.sh:" || true)
    [ -z "$offenders" ] || { echo "wait-for self-test: 'pgrep -f' found:" >&2; echo "$offenders" >&2; return 1; }

    echo "wait-for self-test: ok (--pid, --name, --file, --nonempty satisfied and timed out with 124; bad names and arguments refused; no pgrep -f in scripts, Makefile, hooks or workflows)"
}

if [ "${1:-}" = "--self-test" ]; then
    self_test
    exit
fi

mode=
target=
nonempty=0
timeout_s=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --pid | --name | --file)
            [ -z "$mode" ] && [ "$#" -ge 2 ] || usage
            mode=${1#--}
            target=$2
            shift 2
            ;;
        --nonempty)
            nonempty=1
            shift
            ;;
        --timeout)
            [ "$#" -ge 2 ] || usage
            timeout_s=$2
            shift 2
            ;;
        *) usage ;;
    esac
done

[ -n "$mode" ] || usage
if [ -n "$timeout_s" ] && ! [[ "$timeout_s" =~ ^[0-9]+$ ]]; then
    echo "wait-for: --timeout takes whole seconds" >&2
    usage
fi
[ "$nonempty" = 0 ] || [ "$mode" = file ] || usage

case "$mode" in
    pid)
        [[ "$target" =~ ^[0-9]+$ ]] || { echo "wait-for: --pid takes a number" >&2; usage; }
        if [ -n "$timeout_s" ]; then
            status=0
            timeout "$timeout_s" tail --pid="$target" -f /dev/null || status=$?
            exit "$status"
        fi
        exec tail --pid="$target" -f /dev/null
        ;;
    name)
        if ! [[ "$target" =~ ^[^[:space:]/]{1,15}$ ]]; then
            echo "wait-for: --name takes an exact process name: no spaces or slashes, at most 15 characters (the kernel truncates longer names). Use --pid for a specific process." >&2
            usage
        fi
        ;;
    file) ;;
esac

met() {
    case "$mode" in
        name) ! pgrep -x -- "$target" >/dev/null ;;
        file)
            if [ "$nonempty" = 1 ]; then
                [ -s "$target" ]
            else
                [ -e "$target" ]
            fi
            ;;
    esac
}

interval=2
[ "$mode" = file ] && interval=1
deadline=
[ -z "$timeout_s" ] || deadline=$((SECONDS + timeout_s))
while ! met; do
    if [ -n "$deadline" ] && [ "$SECONDS" -ge "$deadline" ]; then
        exit 124
    fi
    sleep "$interval"
done
