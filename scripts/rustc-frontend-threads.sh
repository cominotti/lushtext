#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Cargo rustc-wrapper that gives this repository's own crates rustc's
# parallel front end (`-Zthreads=N`), and nothing else.
#
# scripts/heavy.sh installs it for every heavy command: it exports
#   RUSTC_WRAPPER=<this script>
#   LUSHTEXT_RUSTC_FRONTEND_THREADS=<N>         (unset or 0: plain passthrough)
#   LUSHTEXT_RUSTC_FRONTEND_ROOT=<checkout>     (physical path)
#   LUSHTEXT_NEXT_RUSTC_WRAPPER=<wrapper cargo would have used, or empty>
# Cargo runs every compiler call as `<wrapper> <compiler> <arguments...>`;
# this script adds the flag when the call compiles a crate of the checkout,
# then hands the call to the next wrapper (CST's sccache wrapper on this
# workstation), or runs it directly.
#
# The parallel front end is unstable in Rust 1.98, so the flag needs
# RUSTC_BOOTSTRAP. It is set to the crate's own name (rustc then allows
# unstable options for that crate only), and only in this process: it never
# reaches cargo, so no dependency's build script sees it. A global
# RUSTC_BOOTSTRAP=1 would: anyhow, thiserror and proc-macro2 probe for it and
# would compile nightly-only code paths into the dependencies.
#
# The flag is added only when ALL hold, otherwise the call passes through
# untouched:
#   - LUSHTEXT_RUSTC_FRONTEND_THREADS is an integer >= 1 (heavy.sh gives 8
#     to a command that has the machine to itself and 1 under contention,
#     where the parallel front end measured slower; 1 still sets
#     RUSTC_BOOTSTRAP, because rustc's incremental cache is discarded whenever
#     unstable options turn on or off, while the thread count is untracked);
#   - the unit is unoptimized (no -C opt-level other than 0): dev, test and
#     check builds. Release and bench units, which ship or feed a timing
#     verdict, stay on the sequential front end like the shipped binary;
#   - the compiler is rustc or clippy-driver (not kani-compiler, not the
#     mutation lanes' RUSTC_WORKSPACE_WRAPPER passthrough);
#   - the caller has not set RUSTC_BOOTSTRAP itself;
#   - no --cap-lints argument (cargo passes it to every registry or git
#     dependency and never to a path package);
#   - CARGO_MANIFEST_DIR lies under LUSHTEXT_RUSTC_FRONTEND_ROOT;
#   - the --crate-name argument is CARGO_CRATE_NAME, the crate cargo itself is
#     compiling: a build script's probe compile carries neither;
#   - no threads option is present already.
#
# The flag is invisible to cargo: it is in no fingerprint and no metadata
# hash, so switching it on or off rebuilds nothing, and an artifact compiled
# with it is reused by a build without it. The front end's output is meant to
# be the same either way (upstream reports no miscompilation, only
# reproducibility bugs in output order), and no optimized unit ever gets it
# (.agents/rules/build.md, "Compiler front-end threads").
#
# Threads beyond one take jobserver tokens, so a rustc runs at most
# CARGO_BUILD_JOBS threads in all; heavy.sh sizes both.

run() {
    if [ -n "${LUSHTEXT_NEXT_RUSTC_WRAPPER:-}" ]; then
        exec "$LUSHTEXT_NEXT_RUSTC_WRAPPER" "$@"
    fi
    exec "$@"
}

threads=${LUSHTEXT_RUSTC_FRONTEND_THREADS:-}
case "$threads" in
    '' | *[!0-9]* | 0) run "$@" ;;
esac
case "${1##*/}" in
    rustc | clippy-driver) ;;
    *) run "$@" ;;
esac
[ -z "${RUSTC_BOOTSTRAP:-}" ] || run "$@"
root=${LUSHTEXT_RUSTC_FRONTEND_ROOT:-}
[ -n "$root" ] || run "$@"
case "${CARGO_MANIFEST_DIR:-}/" in
    "$root"/*) ;;
    *) run "$@" ;;
esac

crate=
prev=
for arg in "$@"; do
    case "$arg" in
        --cap-lints | --cap-lints=* | -Zthreads* | threads=*) run "$@" ;;
        opt-level=0 | -Copt-level=0) ;;
        opt-level=* | -Copt-level=*) run "$@" ;;
    esac
    [ "$prev" = --crate-name ] && crate=$arg
    prev=$arg
done
[ -n "$crate" ] && [ "$crate" = "${CARGO_CRATE_NAME:-}" ] || run "$@"

RUSTC_BOOTSTRAP=$crate
export RUSTC_BOOTSTRAP
run "$@" "-Zthreads=$threads"
