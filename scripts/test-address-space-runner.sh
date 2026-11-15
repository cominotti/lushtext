#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Cargo target runner for local mutation runs (design D5 of
# openspec/changes/reduce-build-disk-writes): run ONE test binary under an
# address-space limit, so a runaway mutant fails its own test process instead
# of taking the host's memory or the whole run's scope.
#
#   CARGO_TARGET_<HOST>_RUNNER=scripts/test-address-space-runner.sh
#   scripts/test-address-space-runner.sh <test-binary> [args...]
#   scripts/test-address-space-runner.sh --self-test
#
# Cargo starts a target runner only for the binaries it runs (tests, benches,
# `cargo run`); nextest does too, for its listing pass and for each test
# process. rustc, the linker, build scripts, nextest and cargo-mutants never go
# through it, so they are not limited: rustc and lld reserve far more address
# space than they use.
#
# The limit is LUSHTEXT_TEST_ADDRESS_SPACE: a size with a K, M, G or T suffix
# (powers of 1024), or `none`. The Makefile sets the derived default; without
# it the runner applies none. The runner `exec`s the binary, so its arguments,
# exit status and signals pass through unchanged.

set -euo pipefail

to_kib() {
    local value=$1 number unit
    number=${value%[KkMmGgTt]}
    unit=${value#"$number"}
    [[ $number =~ ^[0-9]+$ ]] || { echo "test-address-space-runner: bad size '$value'" >&2; return 1; }
    case $unit in
        K|k) echo "$number" ;;
        M|m) echo $((number * 1024)) ;;
        G|g) echo $((number * 1024 * 1024)) ;;
        T|t) echo $((number * 1024 * 1024 * 1024)) ;;
        *) echo "test-address-space-runner: '$value' needs a K, M, G or T suffix" >&2; return 1 ;;
    esac
}

fail() {
    echo "test-address-space-runner self-test: $*" >&2
    return 1
}

self_test() {
    local me status out pid
    me=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/test-address-space-runner.sh
    command -v python3 >/dev/null || fail "python3 is required" || return

    # Below the limit an allocation succeeds; past it the process fails alone.
    LUSHTEXT_TEST_ADDRESS_SPACE=1G "$me" python3 -c 'bytearray(64 << 20)' \
        || fail "a 64 MiB allocation failed under a 1G limit" || return
    status=0
    LUSHTEXT_TEST_ADDRESS_SPACE=1G "$me" python3 -c 'bytearray(2 << 30)' 2>/dev/null || status=$?
    [ "$status" -ne 0 ] || fail "a 2 GiB allocation succeeded under a 1G limit" || return
    # `none` and an unset limit apply nothing.
    LUSHTEXT_TEST_ADDRESS_SPACE=none "$me" python3 -c 'bytearray(2 << 30)' \
        || fail "LUSHTEXT_TEST_ADDRESS_SPACE=none still limited the process" || return
    status=0; LUSHTEXT_TEST_ADDRESS_SPACE=12 "$me" true 2>/dev/null || status=$?
    [ "$status" -eq 2 ] || fail "a size without a suffix was accepted (status $status)" || return

    # Arguments and the exit status pass through unchanged.
    out=$(LUSHTEXT_TEST_ADDRESS_SPACE=1G "$me" printf '%s|' 'a b' '' '--flag=x')
    [ "$out" = 'a b||--flag=x|' ] || fail "arguments came through as '$out'" || return
    status=0; LUSHTEXT_TEST_ADDRESS_SPACE=1G "$me" sh -c 'exit 7' || status=$?
    [ "$status" -eq 7 ] || fail "exit status 7 came back as $status" || return

    # The runner execs: the PID it was started with is the test's, so a
    # signal sent to it reaches the test directly.
    LUSHTEXT_TEST_ADDRESS_SPACE=1G "$me" sleep 30 &
    pid=$!
    sleep 0.3
    [ "$(cat "/proc/$pid/comm" 2>/dev/null)" = sleep ] || { kill "$pid" 2>/dev/null; fail "the runner did not exec its command"; return; }
    kill -TERM "$pid"
    status=0; wait "$pid" || status=$?
    [ "$status" -eq 143 ] || fail "SIGTERM came back as status $status" || return

    echo "test-address-space-runner self-test: ok"
}

if [ "${1:-}" = --self-test ]; then
    self_test
    exit
fi
[ $# -ge 1 ] || { echo "usage: $0 <test-binary> [args...] | --self-test" >&2; exit 2; }

limit=${LUSHTEXT_TEST_ADDRESS_SPACE:-none}
if [ "$limit" != none ]; then
    kib=$(to_kib "$limit") || exit 2
    ulimit -v "$kib"
fi
exec "$@"
