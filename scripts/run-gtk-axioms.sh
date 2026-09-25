#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Run the gtk-lush-axioms probes and samples.
#
#   scripts/run-gtk-axioms.sh all
#       The probe binary (one isolated test per probed axiom, headless), then
#       every sample's --check in one private headless session. This is
#       `make gtk-axioms`, and the CI `gtk-axioms` job.
#   scripts/run-gtk-axioms.sh sample <id> [--check]
#       One sample. With --check it runs headless and exits with the verdict
#       (0 holds, 1 violated, 2 fixture invalid). Without it the sample opens
#       its interactive window on the current desktop session, for a person to
#       watch. This is `make gtk-axiom-sample AXIOM=<id> [CHECK=1]`.
#   scripts/run-gtk-axioms.sh runtimes [<sdk-ref>...]
#       LOCAL ONLY. Build the samples inside each installed GNOME SDK (default
#       org.gnome.Sdk//50 and org.gnome.Sdk//master, each with the
#       rust-stable extension) and run every --check against that SDK's own
#       GTK and Libadwaita, under a private headless session on the host. The
#       gtk-rs feature flags bind a binary to the SDK it was built against, so
#       each SDK gets its own target directory. An SDK that is not installed
#       is skipped and reported; nothing is installed. This is
#       `make gtk-axioms-runtimes`.
#   scripts/run-gtk-axioms.sh sample <id> --screenshot <png>
#       The interactive window under a private headless session, captured to
#       <png> after it settles. Evidence for review only; never committed.
#
# Probes and --check samples never touch the developer's live desktop: they run
# under `dbus-run-session -- mutter --headless`, with the environment of
# scripts/run-widget-tests.sh.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXAMPLES_DIR="$REPO_ROOT/crates/gtk-lush/axioms/examples"
UNSUPPORTED_HOST_EXIT_CODE=77
MONITOR="${GTK_AXIOMS_MONITOR:-1280x1024}"
# Toolkit warnings are defects here, as in the widget lane. The private
# session's own dbus-daemon and mutter chatter is not ours.
WARNING_REGEX='(Gtk-CRITICAL|Gtk-WARNING|GLib-CRITICAL|GLib-WARNING|GLib-GObject-(CRITICAL|WARNING)|Adwaita-(CRITICAL|WARNING)|Gdk-(CRITICAL|WARNING)|Gsk-(CRITICAL|WARNING))'

usage() {
    sed -n '4,20p' "$0" | sed 's/^# \{0,1\}//'
}

require_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "UNSUPPORTED-HOST: '$1' is required to run GTK axiom samples headless." >&2
        exit "$UNSUPPORTED_HOST_EXIT_CODE"
    fi
}

export_headless_env() {
    export GTK_LUSH_AXIOMS_HEADLESS=1
    export NO_AT_BRIDGE=1
    export GTK_A11Y=none
    export ADW_DISABLE_PORTAL=1
    export GDK_DEBUG=no-portals
    export GTK_USE_PORTAL=0
    export GTK_IM_MODULE=gtk-im-context-simple
    : "${GSK_RENDERER:=cairo}"
    export GSK_RENDERER
    export GDK_BACKEND=wayland
    unset DISPLAY WAYLAND_DISPLAY
}

# The example stem for an axiom id such as A9, a9, 9, or A09.
example_for() {
    local raw="${1#[Aa]}"
    if ! [[ "$raw" =~ ^[0-9]+$ ]]; then
        echo "unknown axiom id: $1" >&2
        return 2
    fi
    local padded
    padded="$(printf 'a%02d_' "$((10#$raw))")"
    local match
    match="$(find "$EXAMPLES_DIR" -maxdepth 1 -name "${padded}*.rs" -printf '%f\n' | sed 's/\.rs$//')"
    if [[ -z "$match" ]]; then
        echo "axiom $1 has no sample in crates/gtk-lush/axioms/examples (see the ledger row)" >&2
        return 2
    fi
    printf '%s\n' "$match"
}

all_examples() {
    find "$EXAMPLES_DIR" -maxdepth 1 -name 'a[0-9][0-9]_*.rs' -printf '%f\n' | sed 's/\.rs$//' | sort
}

build_examples() {
    cargo build -p gtk-lush-axioms --examples --quiet
    TARGET_DIR="$(target_dir)"
}

target_dir() {
    cargo metadata --format-version 1 --no-deps \
        | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
}

example_binary() {
    printf '%s/debug/examples/%s\n' "$TARGET_DIR" "$1"
}

# Run "$@" inside a private headless session. The runtime directory is short
# because Wayland socket paths are length-limited.
run_headless() {
    require_command dbus-run-session
    require_command mutter
    local runtime_dir
    runtime_dir="$(mktemp -d /tmp/gtk-axioms-XXXXXX)"
    local status=0
    XDG_RUNTIME_DIR="$runtime_dir" dbus-run-session -- \
        mutter --headless --wayland --no-x11 --virtual-monitor "$MONITOR" -- "$@" || status=$?
    rm -rf "$runtime_dir"
    return "$status"
}

scan_warnings() {
    local log_file="$1"
    local found
    found="$(grep -E "$WARNING_REGEX" "$log_file" || true)"
    if [[ -n "$found" ]]; then
        printf '%s\n' "$found" >&2
        echo "Error: unexpected toolkit warnings while running GTK axiom probes." >&2
        return 1
    fi
}

run_all() {
    LOG_DIR="$(mktemp -d)"
    trap 'rm -rf "$LOG_DIR"' EXIT
    local log_dir="$LOG_DIR"
    export_headless_env

    echo "==> Probe binary (gtk-lush-adoption-lab axiom_probes)"
    local status
    set +e
    cargo test -p gtk-lush-adoption-lab --test axiom_probes 2>&1 | tee "$log_dir/probes.log"
    status="${PIPESTATUS[0]}"
    set -e
    if [[ "$status" -ne 0 ]]; then
        echo "Error: the probe binary failed (exit $status)." >&2
        return "$status"
    fi
    scan_warnings "$log_dir/probes.log"

    echo "==> Sample --check runs"
    build_examples
    local binaries=()
    local example
    while IFS= read -r example; do
        binaries+=("$(example_binary "$example")")
    done < <(all_examples)
    set +e
    run_headless bash -c '
        failed=0
        for binary in "$@"; do
            code=0
            "$binary" --check || code=$?
            if [ "$code" -ne 0 ]; then
                echo "FAILED: $(basename "$binary") --check exited $code" >&2
                failed=1
            fi
        done
        exit "$failed"
    ' gtk-axiom-samples "${binaries[@]}" 2>&1 | tee "$log_dir/samples.log"
    status="${PIPESTATUS[0]}"
    set -e
    if [[ "$status" -ne 0 ]]; then
        echo "Error: at least one sample --check did not hold." >&2
        return "$status"
    fi
    scan_warnings "$log_dir/samples.log"
    local checked
    checked="$(grep -c '^{"axiom"' "$log_dir/samples.log" || true)"
    if [[ "$checked" -ne "${#binaries[@]}" ]]; then
        echo "Error: expected ${#binaries[@]} sample observations, saw $checked." >&2
        return 1
    fi
    echo "GTK axioms: probe binary passed; ${#binaries[@]} sample checks held."
}

run_sample() {
    local id="$1"
    local mode="${2:-}"
    local example
    example="$(example_for "$id")"
    build_examples
    local binary
    binary="$(example_binary "$example")"
    case "$mode" in
        --check)
            export_headless_env
            run_headless "$binary" --check
            ;;
        --screenshot)
            local output="${3:?--screenshot needs an output path}"
            export_headless_env
            unset GTK_LUSH_AXIOMS_HEADLESS
            export GTK_LUSH_AXIOMS_SCREENSHOT="$output"
            mkdir -p "$(dirname "$output")"
            run_headless "$binary"
            ;;
        "")
            # Interactive: deliberately the current desktop session.
            exec "$binary"
            ;;
        *)
            usage >&2
            return 2
            ;;
    esac
}

# The flatpak --env flags that mirror export_headless_env inside a sandbox.
flatpak_env_flags() {
    printf '%s\n' --env=GTK_LUSH_AXIOMS_HEADLESS=1 --env=NO_AT_BRIDGE=1 --env=GTK_A11Y=none \
        --env=ADW_DISABLE_PORTAL=1 --env=GDK_DEBUG=no-portals --env=GTK_USE_PORTAL=0 \
        --env=GSK_RENDERER=cairo --env=GTK_IM_MODULE=gtk-im-context-simple \
        --env=GDK_BACKEND=wayland
}

run_runtimes() {
    require_command flatpak
    local refs=("$@")
    if [[ ${#refs[@]} -eq 0 ]]; then
        refs=(org.gnome.Sdk//50 org.gnome.Sdk//master)
    fi
    local env_flags=()
    mapfile -t env_flags < <(flatpak_env_flags)
    local failed=0 ran=0 ref
    for ref in "${refs[@]}"; do
        if ! flatpak info "$ref" >/dev/null 2>&1; then
            echo "SKIPPED: $ref is not installed (install it and org.freedesktop.Sdk.Extension.rust-stable to check it)."
            continue
        fi
        local slug="${ref##*//}"
        local target="target/gtk-axioms-sdk-${slug}"
        echo "==> $ref: building the samples inside the SDK ($target)"
        if ! flatpak run --command=bash --filesystem=home --share=network "$ref" -c \
            "source /usr/lib/sdk/rust-stable/enable.sh && cd '$REPO_ROOT' && CARGO_TARGET_DIR='$target' cargo build -p gtk-lush-axioms --examples --quiet"; then
            echo "FAILED: $ref could not build the samples." >&2
            failed=1
            continue
        fi
        local binaries=()
        local example
        while IFS= read -r example; do
            binaries+=("$REPO_ROOT/$target/debug/examples/$example")
        done < <(all_examples)
        echo "==> $ref: sample --check runs against the SDK's toolkit"
        unset DISPLAY WAYLAND_DISPLAY
        if ! run_headless bash -c '
            ref="$1"; shift
            flags=()
            while [ "$1" != "--" ]; do flags+=("$1"); shift; done
            shift
            failed=0
            for binary in "$@"; do
                code=0
                flatpak run --command="$binary" --filesystem=home --socket=wayland --nosocket=x11 \
                    "${flags[@]}" "$ref" --check || code=$?
                if [ "$code" -ne 0 ]; then
                    echo "FAILED: $(basename "$binary") --check exited $code on $ref" >&2
                    failed=1
                fi
            done
            exit "$failed"
        ' gtk-axiom-runtimes "$ref" "${env_flags[@]}" -- "${binaries[@]}"; then
            failed=1
        fi
        ran=$((ran + 1))
    done
    echo "GTK axiom runtimes: checked $ran SDK(s)."
    return "$failed"
}

main() {
    case "${1:-}" in
        all) run_all ;;
        runtimes)
            shift
            run_runtimes "$@"
            ;;
        sample)
            shift
            [[ $# -ge 1 ]] || { usage >&2; exit 2; }
            run_sample "$@"
            ;;
        -h | --help) usage ;;
        *)
            usage >&2
            exit 2
            ;;
    esac
}

main "$@"
