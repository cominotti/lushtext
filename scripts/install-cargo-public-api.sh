#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Install the pinned cargo-public-api from its checksum-pinned crate, with one
# reviewed lockfile override (restore-main-ci, design D3):
#
#   CARGO_PUBLIC_API_VERSION=0.52.0 \
#   CARGO_PUBLIC_API_SHA256=<sha256 of the .crate> \
#   CARGO_PUBLIC_API_OPENSSL_SYS_VERSION=0.9.118 \
#     scripts/install-cargo-public-api.sh
#   scripts/install-cargo-public-api.sh --self-test
#
# Why: cargo-public-api 0.52.0 (the newest release) locks openssl-sys 0.9.88,
# whose build script refuses OpenSSL 4, which the Fedora 45 CI image ships.
# `cargo install --locked` cannot select a dependency's feature or version, so
# this script installs from the crate source instead:
#
# 1. download cargo-public-api-<version>.crate from static.crates.io and
#    verify it against CARGO_PUBLIC_API_SHA256;
# 2. refuse when the crate's own lock already pins openssl-sys at or above
#    the override (or no longer depends on it): the override is then no
#    longer needed, and the precise update would downgrade it. Remove the
#    override instead. This makes the retirement condition enforce itself;
# 3. `cargo update --package openssl-sys --precise <override>`;
# 4. refuse any lockfile change except that one openssl-sys entry;
# 5. `cargo install --locked --path .` from inside the crate
#    (CARGO_PUBLIC_API_INSTALL_ROOT, when set, is passed as --root).
#
# The job also installs libcurl-devel, so curl-sys links the image's libcurl
# (which supports OpenSSL 4) instead of building its bundled curl 8.10.1,
# which does not compile against OpenSSL 4. Drop the override, and
# libcurl-devel if nothing else needs it, when a cargo-public-api release locks
# openssl-sys >= 0.9.114 and a curl-sys that builds there; step 2 fails on
# exactly that release.
#
# CARGO_PUBLIC_API_CRATE_FILE, when set, names a local .crate to verify instead
# of downloading one (offline runs and the self-test). Every temporary file
# lives in one directory under $TMPDIR, removed on every exit path, so the
# script leaves nothing behind under scripts/with-temp-scope.sh.

set -euo pipefail

CRATE_URL_BASE=https://static.crates.io/crates/cargo-public-api

die() {
    echo "install-cargo-public-api: $*" >&2
    exit 1
}

work_dir=""
cleanup() {
    if [[ -n "$work_dir" ]]; then
        rm -rf -- "$work_dir"
    fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# Print each record of a Cargo.lock (blocks separated by a blank line) as one
# line, sorted, so two locks can be compared entry by entry with comm.
lock_records() {
    awk 'BEGIN { RS = ""; ORS = "\n" } { gsub(/\n/, " | "); print }' "$1" | LC_ALL=C sort
}

# The version the lock pins for openssl-sys, empty when it has none.
locked_openssl_sys_version() {
    awk '
        /^\[\[package\]\]$/ { in_pkg = 1; name = ""; next }
        in_pkg && /^name = / { name = $3; gsub(/"/, "", name); next }
        in_pkg && /^version = / && name == "openssl-sys" {
            v = $3; gsub(/"/, "", v); print v; exit
        }
    ' "$1"
}

# Succeeds when version $1 >= version $2.
version_at_least() {
    [[ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -n1)" == "$2" ]]
}

install_tool() {
    local version=${CARGO_PUBLIC_API_VERSION:?CARGO_PUBLIC_API_VERSION is required}
    local sha256=${CARGO_PUBLIC_API_SHA256:?CARGO_PUBLIC_API_SHA256 is required}
    local override=${CARGO_PUBLIC_API_OPENSSL_SYS_VERSION:?CARGO_PUBLIC_API_OPENSSL_SYS_VERSION is required}
    local crate src locked removed added expected_removed expected_added

    work_dir=$(mktemp -d "${TMPDIR:-/tmp}/install-cargo-public-api.XXXXXX")
    crate="$work_dir/cargo-public-api-$version.crate"
    if [[ -n "${CARGO_PUBLIC_API_CRATE_FILE:-}" ]]; then
        cp -- "$CARGO_PUBLIC_API_CRATE_FILE" "$crate"
    else
        # HTTPS only, redirects included: the checksum below is the integrity
        # check, and this keeps a redirect from leaving TLS before it.
        curl --proto '=https' --proto-redir '=https' --tlsv1.2 \
            --fail --silent --show-error --location --retry 3 \
            "$CRATE_URL_BASE/cargo-public-api-$version.crate" -o "$crate"
    fi
    echo "$sha256  $crate" | sha256sum --check --status \
        || die "checksum mismatch for cargo-public-api-$version.crate (expected $sha256)"

    tar -xzf "$crate" -C "$work_dir"
    src="$work_dir/cargo-public-api-$version"
    [[ -f "$src/Cargo.lock" ]] || die "the crate ships no Cargo.lock"

    locked=$(locked_openssl_sys_version "$src/Cargo.lock")
    if [[ -z "$locked" ]]; then
        die "cargo-public-api $version locks no openssl-sys: the override is no longer needed; remove CARGO_PUBLIC_API_OPENSSL_SYS_VERSION"
    fi
    if version_at_least "$locked" "$override"; then
        die "cargo-public-api $version already locks openssl-sys $locked >= $override: the override is no longer needed; remove CARGO_PUBLIC_API_OPENSSL_SYS_VERSION"
    fi

    lock_records "$src/Cargo.lock" >"$work_dir/lock.before"
    (cd "$src" && cargo update --package openssl-sys --precise "$override")
    lock_records "$src/Cargo.lock" >"$work_dir/lock.after"

    removed=$(LC_ALL=C comm -23 "$work_dir/lock.before" "$work_dir/lock.after")
    added=$(LC_ALL=C comm -13 "$work_dir/lock.before" "$work_dir/lock.after")
    expected_removed="[[package]] | name = \"openssl-sys\" | version = \"$locked\" |"
    expected_added="[[package]] | name = \"openssl-sys\" | version = \"$override\" |"
    if [[ $(printf '%s\n' "$removed" | grep -c .) -ne 1 \
        || $(printf '%s\n' "$added" | grep -c .) -ne 1 \
        || "$removed" != "$expected_removed"* \
        || "$added" != "$expected_added"* ]]; then
        echo "lock entries removed:" >&2
        printf '%s\n' "$removed" >&2
        echo "lock entries added:" >&2
        printf '%s\n' "$added" >&2
        die "the precise update changed more than the openssl-sys entry"
    fi
    echo "install-cargo-public-api: openssl-sys $locked -> $override, no other lock change"

    local root_args=()
    if [[ -n "${CARGO_PUBLIC_API_INSTALL_ROOT:-}" ]]; then
        root_args=(--root "$CARGO_PUBLIC_API_INSTALL_ROOT")
    fi
    # From inside the crate, so the caller's directory (a rust-toolchain.toml)
    # cannot pick the toolchain: it is the one the precise update ran with.
    (cd "$src" && cargo install --locked --path . "${root_args[@]}")
}

# Offline: a fake crate and a fake cargo on PATH stand in for crates.io.
self_test() {
    local dir fake_bin status failures=0
    dir=$(mktemp -d "${TMPDIR:-/tmp}/install-cargo-public-api-self-test.XXXXXX")
    # shellcheck disable=SC2064 # expand now: the trap must remove this dir
    trap "rm -rf -- '$dir'" EXIT
    fake_bin="$dir/bin"
    mkdir -p "$fake_bin"
    cat >"$fake_bin/cargo" <<'FAKE'
#!/usr/bin/env bash
set -euo pipefail
case "$1" in
    update)
        # cargo update --package openssl-sys --precise V
        sed -i -e "/^name = \"openssl-sys\"$/{n;s/^version = .*/version = \"$5\"/;}" \
            -e "s/^checksum = \"0ss1\"$/checksum = \"0ss2\"/" Cargo.lock
        if [[ "${FAKE_CARGO_EXTRA_CHANGE:-0}" == 1 ]]; then
            sed -i 's/^version = "1.0.0"$/version = "1.0.1"/' Cargo.lock
        fi
        ;;
    install)
        echo "$*" >"$FAKE_CARGO_INSTALLED"
        ;;
    *) exit 2 ;;
esac
FAKE
    chmod +x "$fake_bin/cargo"

    make_crate() { # $1 = openssl-sys version in the lock, $2 = output .crate
        local root="$dir/src/cargo-public-api-9.9.9"
        rm -rf "$dir/src"
        mkdir -p "$root"
        cat >"$root/Cargo.lock" <<LOCK
# This file is automatically @generated by Cargo.
version = 3

[[package]]
name = "curl-sys"
version = "1.0.0"
dependencies = [
 "openssl-sys",
]

[[package]]
name = "openssl-sys"
version = "$1"
checksum = "0ss1"
LOCK
        tar -czf "$2" -C "$dir/src" cargo-public-api-9.9.9
    }

    run_case() { # $1 = name, $2 = expected status (0 or 1), $3 = message grep, rest = env
        local name=$1 expected=$2 needle=$3
        shift 3
        rm -f "$dir/installed"
        if env PATH="$fake_bin:$PATH" TMPDIR="$dir" FAKE_CARGO_INSTALLED="$dir/installed" \
            CARGO_PUBLIC_API_VERSION=9.9.9 CARGO_PUBLIC_API_OPENSSL_SYS_VERSION=0.9.118 \
            CARGO_PUBLIC_API_CRATE_FILE="$dir/c.crate" "$@" \
            "$0" >"$dir/out" 2>&1; then
            status=0
        else
            status=1
        fi
        if [[ $status -ne $expected ]] || ! grep -q -- "$needle" "$dir/out" \
            || { [[ $expected -eq 0 ]] && ! grep -q -- "--locked --path" "$dir/installed"; } \
            || { [[ $expected -ne 0 ]] && [[ -e "$dir/installed" ]]; }; then
            echo "install-cargo-public-api self-test FAILED: $name" >&2
            cat "$dir/out" >&2
            failures=$((failures + 1))
        else
            echo "ok: $name"
        fi
        # Nothing but the fixtures may be left in TMPDIR after a run.
        if compgen -G "$dir/install-cargo-public-api.*" >/dev/null; then
            echo "install-cargo-public-api self-test FAILED: $name leaked its work dir" >&2
            failures=$((failures + 1))
        fi
    }

    make_crate 0.9.88 "$dir/c.crate"
    local sha
    sha=$(sha256sum "$dir/c.crate" | cut -d' ' -f1)
    run_case "installs with exactly the override" 0 "0.9.88 -> 0.9.118" \
        CARGO_PUBLIC_API_SHA256="$sha"
    # Flip the last digit (a fixed replacement equals it one time in 16).
    local tampered=${sha%?}0
    [[ "$tampered" != "$sha" ]] || tampered=${sha%?}1
    run_case "a tampered checksum is refused" 1 "checksum mismatch" \
        CARGO_PUBLIC_API_SHA256="$tampered"
    run_case "an unexpected lock change is refused" 1 "changed more than" \
        CARGO_PUBLIC_API_SHA256="$sha" FAKE_CARGO_EXTRA_CHANGE=1
    run_case "a missing pin is refused" 1 "CARGO_PUBLIC_API_SHA256 is required" \
        CARGO_PUBLIC_API_SHA256=
    make_crate 0.9.120 "$dir/c.crate"
    sha=$(sha256sum "$dir/c.crate" | cut -d' ' -f1)
    run_case "an override no longer needed is refused" 1 "no longer needed" \
        CARGO_PUBLIC_API_SHA256="$sha"
    make_crate 0.9.118 "$dir/c.crate"
    sha=$(sha256sum "$dir/c.crate" | cut -d' ' -f1)
    run_case "an override equal to the lock is refused" 1 "no longer needed" \
        CARGO_PUBLIC_API_SHA256="$sha"

    if [[ $failures -ne 0 ]]; then
        echo "install-cargo-public-api self-test: $failures failure(s)" >&2
        return 1
    fi
    echo "install-cargo-public-api self-test passed"
}

case "${1:-}" in
    --self-test) self_test ;;
    "") install_tool ;;
    *)
        echo "usage: $0 [--self-test]" >&2
        exit 2
        ;;
esac
