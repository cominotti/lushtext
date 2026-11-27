#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The `crash-kill-points` Cargo feature gates exactly one thing: the body of
# `services::kill_point::reach`. The crash smoke runs its victim launches on a
# binary built with the feature, and that binary is the ordinary binary only
# while nothing else is gated on it. This check keys on the places that can
# gate code or enable the feature:
#
#   1. `feature = "crash-kill-points"` in Rust sources appears only in
#      crates/lushtext-services/src/kill_point.rs;
#   2. the feature is declared only by its three Cargo.toml lines (services,
#      core's forward, and the binary's forward), and no other manifest line names it;
#   3. `--features crash-kill-points` (or `-F`, `--features=`, or a feature
#      list holding it) appears only in the Makefile's smoke build line.
#
# Mentions in comments, scripts and documentation are not gating sites.
#
# Usage: scripts/check-kill-point-gating.sh [--root DIR] | --self-test

set -euo pipefail

SELF="scripts/check-kill-point-gating.sh"
GATE_FILE="crates/lushtext-services/src/kill_point.rs"
# shellcheck disable=SC2016,SC1003
SMOKE_BUILD_LINE='CARGO_PROFILE_DEV_DEBUG=$(SMOKE_PROFILE_DEBUG) cargo build -p lushtext --features crash-kill-points --target-dir "$(SMOKE_TARGET_DIR)" && \'

# Forget any repository a calling git hook exported (testing.md).
while IFS= read -r var; do
    unset "$var"
done < <(git rev-parse --local-env-vars 2>/dev/null || true)

fail() {
    echo "check-kill-point-gating: $*" >&2
    return 1
}

# Every candidate file of the tree under $1, relative, one per line: tracked
# files in a git checkout, every regular file otherwise (the self-test).
list_files() {
    local root=$1
    if git -C "$root" rev-parse --is-inside-work-tree >/dev/null 2>&1 \
        && [[ "$(git -C "$root" rev-parse --show-toplevel)" == "$(cd "$root" && pwd -P)" ]]; then
        git -C "$root" ls-files --cached --others --exclude-standard
    else
        (cd "$root" && find . -type f -not -path './.git/*' | sed 's|^\./||')
    fi
}

check_tree() {
    local root=$1 status=0 file line hits
    local -a files
    mapfile -t files < <(list_files "$root" | LC_ALL=C sort)

    # 1. Rust gating sites.
    for file in "${files[@]}"; do
        [[ "$file" == *.rs ]] || continue
        [[ "$file" == "$GATE_FILE" ]] && continue
        [[ -f "$root/$file" ]] || continue
        if hits=$(grep -nE 'feature[[:space:]]*=[[:space:]]*"crash-kill-points"' "$root/$file"); then
            while IFS= read -r line; do
                fail "$file:${line%%:*}: gates code on crash-kill-points outside $GATE_FILE" || status=1
            done <<<"$hits"
        fi
    done

    # 2. Manifest declarations.
    local -A allowed_manifest=(
        ["crates/lushtext-services/Cargo.toml"]='crash-kill-points = []'
        ["crates/lushtext-ui/Cargo.toml"]='crash-kill-points = ["lushtext-services/crash-kill-points"]'
        ["crates/lushtext/Cargo.toml"]='crash-kill-points = ["lushtext-ui/crash-kill-points"]'
    )
    local -A seen_manifest=()
    for file in "${files[@]}"; do
        [[ "$(basename "$file")" == "Cargo.toml" ]] || continue
        [[ -f "$root/$file" ]] || continue
        while IFS= read -r line; do
            local number=${line%%:*} text=${line#*:}
            local trimmed
            trimmed=$(sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//' <<<"$text")
            [[ "$trimmed" == \#* ]] && continue
            if [[ -n "${allowed_manifest[$file]:-}" && "$trimmed" == "${allowed_manifest[$file]}" ]]; then
                seen_manifest[$file]=1
            else
                fail "$file:$number: names crash-kill-points outside its allowed declarations" || status=1
            fi
        done < <(grep -n 'crash-kill-points' "$root/$file" || true)
    done
    for file in "${!allowed_manifest[@]}"; do
        [[ -n "${seen_manifest[$file]:-}" ]] \
            || { fail "$file: the crash-kill-points declaration is missing" || status=1; }
    done

    # 3. Feature enablement on a command line.
    local smoke_lines=0
    for file in "${files[@]}"; do
        [[ "$file" == "$SELF" || "$file" == *.md || "$file" == openspec/* ]] && continue
        [[ -f "$root/$file" ]] || continue
        if hits=$(grep -nE -- '(--features[= ]|-F[ ])[^#]*crash-kill-points' "$root/$file"); then
            while IFS= read -r line; do
                local number=${line%%:*} text=${line#*:}
                text=$(sed -E 's/^[[:space:]]+//' <<<"$text")
                if [[ "$file" == "Makefile" && "$text" == "$SMOKE_BUILD_LINE" ]]; then
                    smoke_lines=$((smoke_lines + 1))
                else
                    fail "$file:$number: enables crash-kill-points outside the Makefile's smoke build line" || status=1
                fi
            done <<<"$hits"
        fi
    done
    if [[ "$smoke_lines" -ne 1 ]]; then
        fail "Makefile: expected exactly one smoke build line enabling crash-kill-points, found $smoke_lines" || status=1
    fi
    return "$status"
}

# A minimal tree that passes, plus one planted violation of each kind.
SELF_TEST_DIR=""
self_test() {
    SELF_TEST_DIR=$(mktemp -d "${TMPDIR:-/tmp}/check-kill-point-gating.XXXXXX")
    trap 'rm -rf "$SELF_TEST_DIR"' EXIT INT TERM
    local tmp=$SELF_TEST_DIR
    make_tree() {
        local dir=$1
        mkdir -p "$dir/crates/lushtext-services/src" "$dir/crates/lushtext-ui/src" "$dir/crates/lushtext"
        printf '#[cfg(feature = "crash-kill-points")]\npub fn reach() {}\n' \
            >"$dir/$GATE_FILE"
        printf '[features]\ncrash-kill-points = []\n' >"$dir/crates/lushtext-services/Cargo.toml"
        printf '[features]\ncrash-kill-points = ["lushtext-services/crash-kill-points"]\n' \
            >"$dir/crates/lushtext-ui/Cargo.toml"
        printf '[features]\ncrash-kill-points = ["lushtext-ui/crash-kill-points"]\n' \
            >"$dir/crates/lushtext/Cargo.toml"
        printf 'smoke:\n\t%s\n# crash-kill-points is mentioned here\n' "$SMOKE_BUILD_LINE" >"$dir/Makefile"
        printf 'Build with --features crash-kill-points.\n' >"$dir/README.md"
    }
    local -a cases=(clean rust manifest default-feature command missing-smoke)
    local name dir rc failures=0
    for name in "${cases[@]}"; do
        dir="$tmp/$name"
        make_tree "$dir"
        case "$name" in
            rust) printf '#[cfg(feature = "crash-kill-points")]\nfn second() {}\n' \
                >"$dir/crates/lushtext-ui/src/other.rs" ;;
            manifest) printf '[features]\nsmoke = ["crash-kill-points"]\n' >"$dir/crates/extra.Cargo.toml" \
                && mkdir -p "$dir/crates/extra" && mv "$dir/crates/extra.Cargo.toml" "$dir/crates/extra/Cargo.toml" ;;
            default-feature) printf 'default = ["crash-kill-points"]\n' >>"$dir/crates/lushtext/Cargo.toml" ;;
            command) printf 'cargo build --release --features "smoke,crash-kill-points"\n' >"$dir/build.sh" ;;
            missing-smoke) printf 'smoke:\n\ttrue\n' >"$dir/Makefile" ;;
        esac
        set +e
        check_tree "$dir" 2>"$tmp/$name.err"
        rc=$?
        set -e
        if [[ "$name" == clean ]]; then
            [[ "$rc" -eq 0 ]] || { echo "self-test: the clean tree failed:" >&2; cat "$tmp/$name.err" >&2; failures=1; }
        else
            [[ "$rc" -ne 0 ]] || { echo "self-test: the $name violation was not detected" >&2; failures=1; }
        fi
    done
    [[ "$failures" -eq 0 ]] || return 1
    echo "check-kill-point-gating: self-test passed (${#cases[@]} cases)"
}

main() {
    local root
    root=$(git rev-parse --show-toplevel 2>/dev/null || pwd)
    case "${1:-}" in
        --self-test) self_test; return ;;
        --root) root=$2 ;;
        "") ;;
        *) echo "usage: $SELF [--root DIR] | --self-test" >&2; return 2 ;;
    esac
    check_tree "$root"
    echo "check-kill-point-gating: crash-kill-points gates only services::kill_point::reach"
}

main "$@"
