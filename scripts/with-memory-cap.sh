#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Run one command inside a memory-capped cgroup scope, so a model checker that
# explodes is killed alone instead of pushing the desktop into swap.
#
#   scripts/with-memory-cap.sh <cap> <command> [args...]
#   scripts/with-memory-cap.sh --self-test
#
# <cap> is a systemd memory size (`16G`, `12288M`) or `none`. The scope sets
# `MemoryMax=<cap>` and `MemorySwapMax=0`: the command and every descendant
# share the cap, and when they exceed it the kernel OOM-kills inside the scope
# only (exit status 137). A harness that hits the cap is a finding — shrink its
# bounds — not a reason to raise the cap.
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

self_test() {
    local me=$0 status
    "$me" none true || { echo "self-test: 'none' did not pass through" >&2; return 1; }
    status=0; "$me" 16GiB true 2>/dev/null || status=$?
    [ "$status" -eq 2 ] || { echo "self-test: invalid cap accepted (status $status)" >&2; return 1; }
    if command -v systemd-run >/dev/null 2>&1 \
        && systemd-run --user --scope --quiet -p MemoryMax=1M true >/dev/null 2>&1; then
        "$me" 256M true 2>/dev/null || { echo "self-test: capped command failed" >&2; return 1; }
        status=0
        "$me" 64M python3 -c 'b = bytearray(512 * 1024 * 1024)' 2>/dev/null || status=$?
        [ "$status" -ne 0 ] || { echo "self-test: 512 MiB allocation survived a 64M cap" >&2; return 1; }
        echo "with-memory-cap self-test: ok (over-cap command killed, status $status)"
    else
        echo "with-memory-cap self-test: ok (no systemd user manager; cap enforcement not exercised)"
    fi
}

if [ "${1:-}" = "--self-test" ]; then
    self_test
    exit
fi

if [ "$#" -lt 2 ]; then
    echo "usage: $0 <cap|none> <command> [args...]" >&2
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

if command -v systemd-run >/dev/null 2>&1 \
    && systemd-run --user --scope --quiet -p MemoryMax=1M true >/dev/null 2>&1; then
    echo "with-memory-cap: MemoryMax=$cap MemorySwapMax=0 for: $*" >&2
    exec systemd-run --user --scope --quiet \
        -p MemoryMax="$cap" -p MemorySwapMax=0 -- "$@"
fi

if [ -n "${CI:-}" ]; then
    echo "with-memory-cap: no systemd user manager on CI; running uncapped: $*" >&2
    exec "$@"
fi

echo "with-memory-cap: cannot create a memory-capped scope (systemd-run --user unavailable)." >&2
echo "Refusing to run uncapped. Pass the cap 'none' (e.g. KANI_MEMORY_MAX=none) to opt out explicitly." >&2
exit 1
