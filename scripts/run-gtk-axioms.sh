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
#   scripts/run-gtk-axioms.sh --self-test
#       Check the warning classification against known lines, without GTK.
#
# Probes and --check samples never touch the developer's live desktop: they run
# under `dbus-run-session -- mutter --headless`, with the environment of
# scripts/run-widget-tests.sh.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXAMPLES_DIR="$REPO_ROOT/crates/gtk-lush/axioms/examples"
UNSUPPORTED_HOST_EXIT_CODE=77
MONITOR="${GTK_AXIOMS_MONITOR:-1280x1024}"
# Any warning or critical is a defect here, as in the widget lane, whatever
# process or log domain printed it: a GLib-style "** (prog:pid): WARNING **"
# from a helper (xdg-dbus-proxy logs as "(process:pid)", with no domain), a
# Broken pipe, or a runtime directory the session could not remove all fail the
# lane. The private session's own mutter and dbus-daemon chatter is not ours;
# BENIGN_NOISE_REGEX names exactly those lines, taken from the widget runner's
# list of what the same headless session prints on the hosts and CI images.
WARNING_REGEX='(WARNING|CRITICAL|Broken pipe|cannot remove|^MESA: error:)'
BENIGN_NOISE_REGEX='(^dbus-daemon\[[0-9]+\]: |^libmutter-Message:|^\*\* Message: .*Obtained a high priority EGL context$|^\*\* \(mutter:[0-9]+\): WARNING \*\*: ([0-9:.]+: )?Skipping layers 1\.\.n of your pipeline since the first layer is sliced\. |^\(mutter:[0-9]+\): mutter-WARNING \*\*: .*Failed to acquire org\.freedesktop\.locale1 proxy: Could not connect: No such file or directory$|^\(mutter:[0-9]+\): libmutter-WARNING \*\*: .*Failed to connect to colord daemon: Could not connect: No such file or directory$)'

# An SDK sandbox gets no D-Bus proxy and no document portal: the sample checks
# need neither, and each is a Flatpak-side helper racing a sample's exit. With
# the session bus proxied, every sample's GIO gvfs client connects through the
# sandbox's xdg-dbus-proxy, which intermittently logged "Error writing
# credentials to socket: Error sending message: Broken pipe" when a sample
# exited mid-handshake; the document portal left its FUSE mount in the private
# runtime directory, which then could not be removed.
SANDBOX_FLAGS=(--no-session-bus --no-a11y-bus --no-documents-portal)

usage() {
    sed -n '4,28p' "$0" | sed 's/^# \{0,1\}//'
}

require_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "UNSUPPORTED-HOST: '$1' is required to run GTK axiom samples headless." >&2
        exit "$UNSUPPORTED_HOST_EXIT_CODE"
    fi
}

# The one list of the samples' headless environment: exported on the host,
# passed as --env flags into an SDK sandbox. A caller's GSK_RENDERER wins.
HEADLESS_ENV=(
    GTK_LUSH_AXIOMS_HEADLESS=1
    NO_AT_BRIDGE=1
    GTK_A11Y=none
    ADW_DISABLE_PORTAL=1
    GDK_DEBUG=no-portals
    GTK_USE_PORTAL=0
    GTK_IM_MODULE=gtk-im-context-simple
    "GSK_RENDERER=${GSK_RENDERER:-cairo}"
    GDK_BACKEND=wayland
)

export_headless_env() {
    export "${HEADLESS_ENV[@]}"
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

# Build the named example, or every example with no argument, and record the
# target directory that example_binary reads.
build_examples() {
    local targets=(--examples)
    if [[ $# -gt 0 ]]; then
        targets=(--example "$1")
    fi
    cargo build -p gtk-lush-axioms "${targets[@]}" --quiet
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
    found="$(grep -E "$WARNING_REGEX" "$log_file" | grep -Ev "$BENIGN_NOISE_REGEX" || true)"
    if [[ -n "$found" ]]; then
        printf '%s\n' "$found" >&2
        echo "Error: unexpected warnings while running GTK axiom probes." >&2
        return 1
    fi
}

# The benign session lines must pass, and every defect class must fail,
# including the ones seen from the SDK sandbox's helpers.
self_test() {
    local log_file line
    log_file="$(mktemp)"
    local benign=(
        'libmutter-Message: 02:16:19.694: Running Mutter (using mutter 50.4) as a Wayland display server'
        '** Message: 02:16:19.858: Obtained a high priority EGL context'
        "dbus-daemon[3411502]: [session uid=1000 pid=3411502 pidfd=5] Activated service 'org.freedesktop.systemd1' failed: Process org.freedesktop.systemd1 exited with status 1"
        '(mutter:582): mutter-WARNING **: 09:42:29.034: Failed to acquire org.freedesktop.locale1 proxy: Could not connect: No such file or directory'
        '{"axiom":"A1","verdict":"holds","measured":{},"gtk":"4.22.2","adw":"1.9.0"}'
    )
    local defects=(
        '** (process:3411594): WARNING **: 02:16:21.611: Error writing credentials to socket: Error sending message: Broken pipe'
        "rm: cannot remove '/tmp/gtk-axioms-zcE6Sm/doc': Is a directory"
        'Gdk-Message: 02:16:21.611: Error flushing display: Broken pipe'
        '(a01_realized_rows_are_capped:12): Gtk-CRITICAL **: 02:16:21.611: gtk_widget_snapshot_child: assertion failed'
        '(process:12): GLib-GIO-WARNING **: 02:16:21.611: Error creating IO channel'
        '(process:12): Adwaita-WARNING **: 02:16:21.611: unknown style class'
        '(mutter:582): mutter-WARNING **: 09:42:29.034: an unlisted compositor warning'
    )
    local failed=0
    printf '%s\n' "${benign[@]}" >"$log_file"
    if ! scan_warnings "$log_file"; then
        echo "Error: GTK axiom warning self-test rejected benign session output." >&2
        failed=1
    fi
    for line in "${defects[@]}"; do
        printf '%s\n' "$line" >"$log_file"
        if scan_warnings "$log_file" >/dev/null 2>&1; then
            echo "Error: GTK axiom warning self-test accepted: $line" >&2
            failed=1
        fi
    done
    rm -f "$log_file"
    if [[ "$failed" -ne 0 ]]; then
        return 1
    fi
    echo "GTK axiom warning classification self-test passed."
}

# Run "$@" with its output teed to the log file $1, failing on a nonzero exit
# or on a toolkit warning in the log.
run_logged() {
    local log_file="$1"
    shift
    local status
    set +e
    "$@" 2>&1 | tee "$log_file"
    status="${PIPESTATUS[0]}"
    set -e
    if [[ "$status" -ne 0 ]]; then
        return "$status"
    fi
    scan_warnings "$log_file"
}

# Run every sample binary with --check, one after another, inside whatever
# runs this script: the host session or an SDK sandbox.
SAMPLE_CHECK_LOOP='
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
'

# Run the sample checks through "$@" (a command that ends where the loop's
# arguments begin), logged to $1, and require one observation per binary in
# SAMPLE_BINARIES.
check_samples() {
    local log_file="$1"
    shift
    if ! run_logged "$log_file" run_headless "$@" -c "$SAMPLE_CHECK_LOOP" gtk-axiom-samples \
        "${SAMPLE_BINARIES[@]}"; then
        echo "Error: at least one sample --check did not hold." >&2
        return 1
    fi
    local checked
    checked="$(grep -c '^{"axiom"' "$log_file" || true)"
    if [[ "$checked" -ne "${#SAMPLE_BINARIES[@]}" ]]; then
        echo "Error: expected ${#SAMPLE_BINARIES[@]} sample observations, saw $checked." >&2
        return 1
    fi
}

run_all() {
    LOG_DIR="$(mktemp -d)"
    trap 'rm -rf "$LOG_DIR"' EXIT
    export_headless_env

    # Build the samples first so a sample that does not compile fails before
    # the probe run rather than after it.
    build_examples
    SAMPLE_BINARIES=()
    local example
    while IFS= read -r example; do
        SAMPLE_BINARIES+=("$(example_binary "$example")")
    done < <(all_examples)

    echo "==> Probe binary (gtk-lush-adoption-lab axiom_probes)"
    if ! run_logged "$LOG_DIR/probes.log" cargo test -p gtk-lush-adoption-lab --test axiom_probes; then
        echo "Error: the probe binary failed." >&2
        return 1
    fi

    echo "==> Sample --check runs"
    check_samples "$LOG_DIR/samples.log" bash
    echo "GTK axioms: probe binary passed; ${#SAMPLE_BINARIES[@]} sample checks held."
}

run_sample() {
    local id="$1"
    local mode="${2:-}"
    local example
    example="$(example_for "$id")"
    build_examples "$example"
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

run_runtimes() {
    require_command flatpak
    local refs=("$@")
    if [[ ${#refs[@]} -eq 0 ]]; then
        refs=(org.gnome.Sdk//50 org.gnome.Sdk//master)
    fi
    LOG_DIR="$(mktemp -d)"
    trap 'rm -rf "$LOG_DIR"' EXIT
    local env_flags=("${HEADLESS_ENV[@]/#/--env=}")
    local failed=0 ran=0 ref
    for ref in "${refs[@]}"; do
        if ! flatpak info "$ref" >/dev/null 2>&1; then
            echo "SKIPPED: $ref is not installed (install it and org.freedesktop.Sdk.Extension.rust-stable to check it)."
            continue
        fi
        local slug="${ref##*//}"
        local target="target/gtk-axioms-sdk-${slug}"
        echo "==> $ref: building the samples inside the SDK ($target)"
        if ! flatpak run --command=bash --filesystem=home --share=network "${SANDBOX_FLAGS[@]}" "$ref" -c \
            "source /usr/lib/sdk/rust-stable/enable.sh && cd '$REPO_ROOT' && CARGO_TARGET_DIR='$target' cargo build -p gtk-lush-axioms --examples --quiet"; then
            echo "FAILED: $ref could not build the samples." >&2
            failed=1
            continue
        fi
        SAMPLE_BINARIES=()
        local example
        while IFS= read -r example; do
            SAMPLE_BINARIES+=("$REPO_ROOT/$target/debug/examples/$example")
        done < <(all_examples)
        echo "==> $ref: sample --check runs against the SDK's toolkit"
        unset DISPLAY WAYLAND_DISPLAY
        # One sandbox per SDK runs every sample, each still its own process.
        if ! check_samples "$LOG_DIR/runtime-${slug}.log" \
            flatpak run --command=bash --filesystem=home --socket=wayland --nosocket=x11 \
            "${SANDBOX_FLAGS[@]}" "${env_flags[@]}" "$ref"; then
            echo "FAILED: sample checks on $ref." >&2
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
        --self-test) self_test ;;
        -h | --help) usage ;;
        *)
            usage >&2
            exit 2
            ;;
    esac
}

main "$@"
