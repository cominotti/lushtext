#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Self-test of scripts/rustc-frontend-threads.sh (the parallel front-end
# rustc-wrapper heavy.sh installs) against fake compilers. Run it as
# `make check-rustc-frontend-threads`, which also self-tests
# scripts/next-rustc-wrapper.py and shellchecks both shell scripts.

set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
wrapper="$here/rustc-frontend-threads.sh"

scratch=$(mktemp -d "${TMPDIR:-/tmp}/lt-frontend-threads-XXXXXX")
trap 'rm -rf -- "$scratch"' EXIT
failures=0

# A fake compiler (named as the case needs) that prints its arguments and the
# RUSTC_BOOTSTRAP it saw.
fake_compiler() {
    mkdir -p "$scratch/bin"
    cat >"$scratch/bin/$1" <<'EOF'
#!/bin/sh
printf 'args=%s bootstrap=%s\n' "$*" "${RUSTC_BOOTSTRAP:-}"
EOF
    chmod +x "$scratch/bin/$1"
    printf '%s\n' "$scratch/bin/$1"
}
rustc=$(fake_compiler rustc)
clippy=$(fake_compiler clippy-driver)
kani=$(fake_compiler kani-compiler)
passthrough=$(fake_compiler rustc-passthrough.sh)

# A next wrapper that tags the line, to prove the chain.
cat >"$scratch/next" <<'EOF'
#!/bin/sh
printf 'next '
exec "$@"
EOF
chmod +x "$scratch/next"

root="$scratch/checkout"
mkdir -p "$root/crates/core"

# call [VAR=value...] -- <compiler> <args...>: run the wrapper as cargo would
# for a crate of the checkout, with the given overrides.
call() {
    local -a overrides=()
    while [ "$1" != -- ]; do
        overrides+=("$1")
        shift
    done
    shift
    env -u RUSTC_BOOTSTRAP \
        LUSHTEXT_RUSTC_FRONTEND_THREADS=8 \
        LUSHTEXT_RUSTC_FRONTEND_ROOT="$root" \
        LUSHTEXT_NEXT_RUSTC_WRAPPER= \
        CARGO_MANIFEST_DIR="$root/crates/core" \
        CARGO_CRATE_NAME=core \
        "${overrides[@]}" "$wrapper" "$@"
}

expect() {
    local name="$1" want="$2" got="$3"
    if [ "$got" = "$want" ]; then
        echo "ok - $name"
    else
        echo "FAIL - $name"
        echo "  want: $want"
        echo "  got:  $got"
        failures=$((failures + 1))
    fi
}

base=(--crate-name core --edition=2024 src/lib.rs)
plain="args=${base[*]} bootstrap="
threaded="args=${base[*]} -Zthreads=8 bootstrap=core"

expect "a crate of the checkout gets the threads, bootstrap scoped to it" \
    "$threaded" "$(call -- "$rustc" "${base[@]}")"
expect "clippy-driver gets them too" \
    "args=$rustc ${base[*]} -Zthreads=8 bootstrap=core" "$(call -- "$clippy" "$rustc" "${base[@]}")"
expect "the call goes on to the next wrapper" \
    "next $threaded" "$(call LUSHTEXT_NEXT_RUSTC_WRAPPER="$scratch/next" -- "$rustc" "${base[@]}")"
expect "an untouched call goes on to the next wrapper too" \
    "next $plain" "$(call LUSHTEXT_NEXT_RUSTC_WRAPPER="$scratch/next" LUSHTEXT_RUSTC_FRONTEND_THREADS=0 -- "$rustc" "${base[@]}")"
expect "a dependency (--cap-lints) is untouched" \
    "args=${base[*]} --cap-lints allow bootstrap=" "$(call -- "$rustc" "${base[@]}" --cap-lints allow)"
expect "a dependency (--cap-lints=allow) is untouched" \
    "args=${base[*]} --cap-lints=allow bootstrap=" "$(call -- "$rustc" "${base[@]}" --cap-lints=allow)"
expect "a crate outside the checkout is untouched" \
    "$plain" "$(call CARGO_MANIFEST_DIR="$scratch/registry/dep" -- "$rustc" "${base[@]}")"
expect "a sibling directory sharing the root's prefix is outside it" \
    "$plain" "$(call CARGO_MANIFEST_DIR="${root}-other/crates/core" -- "$rustc" "${base[@]}")"
expect "a build-script probe (crate name not cargo's) is untouched" \
    "args=--crate-name probe0 src/lib.rs bootstrap=" "$(call -- "$rustc" --crate-name probe0 src/lib.rs)"
expect "a call without CARGO_CRATE_NAME is untouched" \
    "$plain" "$(call CARGO_CRATE_NAME= -- "$rustc" "${base[@]}")"
expect "kani-compiler is untouched" \
    "args=${base[*]} bootstrap=" "$(call -- "$kani" "${base[@]}")"
expect "the mutation lanes' workspace passthrough is untouched" \
    "args=$rustc ${base[*]} bootstrap=" "$(call -- "$passthrough" "$rustc" "${base[@]}")"
expect "a caller's own RUSTC_BOOTSTRAP is left alone" \
    "args=${base[*]} bootstrap=1" "$(call RUSTC_BOOTSTRAP=1 -- "$rustc" "${base[@]}")"
expect "an explicit -Zthreads is left alone" \
    "args=${base[*]} -Zthreads=2 bootstrap=" "$(call -- "$rustc" "${base[@]}" -Zthreads=2)"
expect "an explicit -Z threads=N is left alone" \
    "args=${base[*]} -Z threads=2 bootstrap=" "$(call -- "$rustc" "${base[@]}" -Z threads=2)"
expect "one thread still scopes RUSTC_BOOTSTRAP (the incremental cache tracks it)" \
    "args=${base[*]} -Zthreads=1 bootstrap=core" "$(call LUSHTEXT_RUSTC_FRONTEND_THREADS=1 -- "$rustc" "${base[@]}")"
expect "an optimized unit (release, bench) is untouched" \
    "args=${base[*]} -C opt-level=3 bootstrap=" "$(call -- "$rustc" "${base[@]}" -C opt-level=3)"
expect "an optimized unit written -Copt-level=s is untouched" \
    "args=${base[*]} -Copt-level=s bootstrap=" "$(call -- "$rustc" "${base[@]}" -Copt-level=s)"
expect "an explicit opt-level=0 unit is threaded" \
    "args=${base[*]} -C opt-level=0 -Zthreads=8 bootstrap=core" "$(call -- "$rustc" "${base[@]}" -C opt-level=0)"
for value in "" 0 eight; do
    expect "LUSHTEXT_RUSTC_FRONTEND_THREADS='$value' is a passthrough" \
        "$plain" "$(call LUSHTEXT_RUSTC_FRONTEND_THREADS="$value" -- "$rustc" "${base[@]}")"
done
expect "no LUSHTEXT_RUSTC_FRONTEND_ROOT is a passthrough" \
    "$plain" "$(call LUSHTEXT_RUSTC_FRONTEND_ROOT= -- "$rustc" "${base[@]}")"
expect "16 threads" \
    "args=${base[*]} -Zthreads=16 bootstrap=core" "$(call LUSHTEXT_RUSTC_FRONTEND_THREADS=16 -- "$rustc" "${base[@]}")"
expect "the compiler's exit status comes back" \
    "3" "$(set +e; call -- sh -c 'exit 3' 2>/dev/null; echo $?)"

if [ "$failures" -gt 0 ]; then
    echo "rustc-frontend-threads self-test: $failures failure(s)" >&2
    exit 1
fi
echo "rustc-frontend-threads self-test: all checks passed"
