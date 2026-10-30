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
# Exits 0 when the condition holds, 124 on timeout, 2 on a usage error (or
# when the process table cannot be read). `--timeout 0` checks once in every
# mode. A timeout is never overshot by more than a fraction of a second.
#
# Never `pgrep -f`: it matches whole command lines, and the shell running the
# wait loop carries the pattern in its own command line, so the loop matches
# itself and never ends (agents' loops ran for up to 32 hours). --name matches
# the kernel's process name exactly (`pgrep -x`), which is at most 15
# characters; a longer or space-containing name could never match and would
# report "gone" at once, so it is refused; regex metacharacters in it are
# matched literally. Prefer --pid whenever the PID is known: it waits on that
# one process (`tail --pid`), with no polling of the process table. A PID the
# kernel reuses for an unrelated process after the awaited one exits keeps the
# wait going; that is inherent to waiting by PID, so wait on a child or a
# process you just started rather than on an old, remembered number. See "Shared-machine agent work" in .agents/rules/build.md.
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
    expect 2 --pid 0
    expect 2 --pid 007
    expect 2 --name 'a/b'

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
    # --timeout 0 checks once in every mode (coreutils' `timeout 0` would
    # instead mean "no timeout" and wait forever).
    expect 124 --pid "$pid" --timeout 0
    expect 124 --file "$dir/absent" --timeout 0
    expect 0 --file "$dir/empty" --timeout 0
    expect 0 --pid 999999999 --timeout 0
    kill "$pid"
    wait "$pid" 2>/dev/null || true

    # A process with a unique exact name: a copy of sleep named wf<pid>.
    name="wf$$"
    cp "$(command -v sleep)" "$dir/$name"
    "$dir/$name" 30 &
    pid=$!
    expect 124 --name "$name" --timeout 1
    expect 124 --name "$name" --timeout 0
    # Metacharacters match literally: 'w.<pid>' would match 'wf<pid>' as a
    # regex but is another name, and an unbalanced '[' is no regex error
    # (pgrep's error used to read as "gone").
    expect 0 --name "w.$$" --timeout 0
    expect 0 --name "wf[$$" --timeout 0
    # The deadline is not overshot by the polling interval.
    local started=$SECONDS
    expect 124 --name "$name" --timeout 1
    [ $((SECONDS - started)) -le 2 ] || { echo "wait-for self-test: --timeout 1 took $((SECONDS - started)) s" >&2; return 1; }
    kill "$pid"
    wait "$pid" 2>/dev/null || true
    expect 0 --name "$name" --timeout 5

    # A process-table failure is an error, never "gone": run with a pgrep
    # that fails as pgrep does on an error (exit 3).
    mkdir "$dir/bin"
    printf '#!/bin/sh\nexit 3\n' >"$dir/bin/pgrep"
    chmod +x "$dir/bin/pgrep"
    local got=0
    PATH="$dir/bin:$PATH" "$me" --name "$name" --timeout 1 2>/dev/null || got=$?
    [ "$got" -eq 2 ] || { echo "wait-for self-test: a failing pgrep gave $got, expected 2" >&2; return 1; }

    # No agent-facing script or make target may use `pgrep -f`/`pkill -f`:
    # scripts, the Makefile, hooks, workflows and the skills' scripts. A
    # one-shot snapshot that filters out its own shell (never a wait loop)
    # may stay with an `agent-pgrep-f-ok: <reason>` comment on its line.
    local root offenders
    root=$(cd "$(dirname "$me")/.." && pwd)
    offenders=$(grep -rnE '(pgrep|pkill)([[:space:]]+[^|;&[:space:]]+)*[[:space:]]+(-[[:alnum:]]*f[[:alnum:]]*|--full)([[:space:]]|$)' \
        "$root/scripts" "$root/Makefile" "$root/.githooks" "$root/.github" "$root/.agents/skills" 2>/dev/null \
        | grep -v '^[^:]*\.md:' | grep -vF "$root/scripts/wait-for.sh:" | grep -v 'agent-pgrep-f-ok: ' || true)
    [ -z "$offenders" ] || { echo "wait-for self-test: 'pgrep -f' or 'pkill -f' found:" >&2; echo "$offenders" >&2; return 1; }
    # The scan itself catches what it claims to.
    local probe
    for probe in 'pgrep -f x' 'pgrep -af x' 'pgrep -u me -f x' 'pkill -f x' 'pgrep --full x'; do
        printf '%s\n' "$probe" >"$dir/probe.sh"
        grep -qE '(pgrep|pkill)([[:space:]]+[^|;&[:space:]]+)*[[:space:]]+(-[[:alnum:]]*f[[:alnum:]]*|--full)([[:space:]]|$)' "$dir/probe.sh" \
            || { echo "wait-for self-test: the scan misses '$probe'" >&2; return 1; }
    done

    echo "wait-for self-test: ok (--pid, --name, --file, --nonempty satisfied and timed out with 124; --timeout 0 checks once in every mode; deadline not overshot; --pid 0 and bad names refused; --name metacharacters literal; a pgrep failure is exit 2; no pgrep -f/pkill -f in scripts, Makefile, hooks, workflows or skill scripts)"
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
        [[ "$target" =~ ^[1-9][0-9]*$ ]] || { echo "wait-for: --pid takes a positive process number" >&2; usage; }
        if [ "$timeout_s" = 0 ]; then
            [ -e "/proc/$target" ] && exit 124
            exit 0
        fi
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

# pgrep takes an extended regex: escape it so the name matches literally.
literal=$(printf '%s' "$target" | sed 's/[][\\.^$*+?(){}|]/\\&/g')

met() {
    case "$mode" in
        name)
            local status=0
            pgrep -x -- "$literal" >/dev/null || status=$?
            case "$status" in
                0) return 1 ;;
                1) return 0 ;;
                *)
                    echo "wait-for: pgrep failed (status $status); cannot tell whether '$target' runs" >&2
                    exit 2
                    ;;
            esac
            ;;
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
# Whole seconds from `date +%s.%N`, kept in milliseconds, so a deadline is
# not missed by SECONDS' one-second granularity.
now_ms() {
    local t
    t=$(date +%s%N)
    echo $((t / 1000000))
}
[ -z "$timeout_s" ] || deadline=$(($(now_ms) + timeout_s * 1000))
while ! met; do
    if [ -n "$deadline" ]; then
        left=$((deadline - $(now_ms)))
        [ "$left" -gt 0 ] || exit 124
        # Sleep at most the time left, so the timeout is not overshot.
        [ "$left" -ge $((interval * 1000)) ] || { sleep "$((left / 1000)).$(printf '%03d' $((left % 1000)))"; continue; }
    fi
    sleep "$interval"
done
