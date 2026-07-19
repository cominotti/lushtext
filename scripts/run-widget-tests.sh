#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

RETRIES=0
MONITOR="2560x1600"
TEST_ARGS=()
SELF_TEST=false
UNSUPPORTED_HOST_EXIT_CODE=77
BENIGN_WIDGET_NOISE_REGEX='(dbus-daemon\[[0-9]+\]: .*org\.(freedesktop\.(portal|impl\.portal\.|systemd1)|a11y\.Bus)|\(/usr/libexec/xdg-desktop-portal:.*WARNING \*\*:|\*\* \(xdg-desktop-portal-gtk:.*WARNING \*\*:|^\(xdg-desktop-portal-gtk:[0-9]+\): xdg-desktop-portal-gtk-WARNING \*\*: ([0-9:.]+: )?error: Could not connect: No such file or directory$|Gtk-CRITICAL \*\*: .*org\.a11y\.atspi\.Registry|Gdk-Message: .*Broken pipe$|rm: cannot remove '\''/tmp/.*/doc'\'': Is a directory$|^libmutter-Message:|^\*\* Message: .*Obtained a high priority EGL context$|^\*\* \(mutter:[0-9]+\): WARNING \*\*: ([0-9:.]+: )?Skipping layers 1\.\.n of your pipeline since the first layer is sliced\. We don'\''t currently support any multi-texturing with sliced textures but assume layer 0 is the most important to keep$|^\(mutter:[0-9]+\): mutter-WARNING \*\*: .*Failed to acquire org\.freedesktop\.locale1 proxy: Could not connect: No such file or directory$|^\(mutter:[0-9]+\): libmutter-WARNING \*\*: .*Failed to connect to colord daemon: Could not connect: No such file or directory$|.*WARNING: Glycin running without sandbox\.$)'
# Cargo diagnostics spell this as a standalone `warning:` token. Requiring a
# line/whitespace boundary keeps `--list` output such as `...visible_warning:
# test` from being mistaken for a toolkit or compiler warning.
WIDGET_WARNING_REGEX='((^|[[:space:]])warning:|WARNING|CRITICAL|Gdk-Message:|Broken pipe|cannot remove|^MESA: error:)'

usage() {
    cat <<'EOF'
Usage: scripts/run-widget-tests.sh [--headless] [--retries N] [--monitor WxH] [-- [test-binary-args...]]

Run the LushText widget test harness under a transient Mutter headless compositor.
The harness intentionally has no native/live-display mode.

Options:
  --headless    Always run under `mutter --headless`.
  --retries N   Retry the full harness up to N times after the first failure.
  --monitor WxH Virtual monitor size for headless runs (default: 2560x1600).
  --self-test   Verify warning classification without launching GTK.
  -h, --help    Show this help text.

Arguments after `--` are passed to the widget test binary, for example:
  scripts/run-widget-tests.sh -- --exact window::test_primary_menu_button_exists
EOF
}

require_command() {
    local command_name="$1"
    if ! command -v "$command_name" >/dev/null 2>&1; then
        echo "UNSUPPORTED-HOST: '$command_name' is required for widget test execution." >&2
        exit "$UNSUPPORTED_HOST_EXIT_CODE"
    fi
}

export_widget_test_env() {
    export NO_AT_BRIDGE=1
    export GDK_DEBUG=no-portals
    export GTK_USE_PORTAL=0
    # Cairo keeps widget tests on GTK's CPU fallback renderer. That avoids
    # headless Mesa/EGL device probing in containers while still letting a
    # caller override GSK_RENDERER when intentionally debugging a GPU renderer.
    : "${GSK_RENDERER:=cairo}"
    export GSK_RENDERER
}

emit_sanitized_widget_log() {
    local log_file="$1"
    local filtered_log
    filtered_log="$(mktemp)"
    if [[ -n "${BENIGN_WIDGET_NOISE_REGEX}" ]]; then
        grep -Ev "$BENIGN_WIDGET_NOISE_REGEX" "$log_file" >"$filtered_log" || true
    else
        cp "$log_file" "$filtered_log"
    fi
    awk 'NF { print; blank = 0; next } !blank { print; blank = 1 }' "$filtered_log"
    rm -f "$filtered_log"
}

check_for_unexpected_widget_warnings() {
    local log_file="$1"
    local unexpected
    unexpected="$(grep -E "$WIDGET_WARNING_REGEX" "$log_file" | grep -Ev "$BENIGN_WIDGET_NOISE_REGEX" || true)"
    if [[ -n "$unexpected" ]]; then
        printf '%s\n' "$unexpected" >&2
        echo "Error: unexpected warning output during widget tests." >&2
        return 1
    fi
}

self_test_warning_classification() {
    local log_file
    log_file="$(mktemp)"
    trap 'rm -f "$log_file"' RETURN

    printf '%s\n' "** (mutter:582): WARNING **: 09:42:29.034: Skipping layers 1..n of your pipeline since the first layer is sliced. We don't currently support any multi-texturing with sliced textures but assume layer 0 is the most important to keep" >"$log_file"
    check_for_unexpected_widget_warnings "$log_file"

    printf '%s\n' "Gtk-WARNING **: allocation failed" >"$log_file"
    if check_for_unexpected_widget_warnings "$log_file" >/dev/null 2>&1; then
        echo "Error: widget warning self-test accepted an unexpected GTK warning." >&2
        return 1
    fi

    echo "Widget warning classification self-test passed."
}

run_with_widget_log() {
    local log_file
    log_file="$(mktemp)"
    local status
    if "$@" >"$log_file" 2>&1; then
        status=0
    else
        status=$?
    fi

    emit_sanitized_widget_log "$log_file"
    if ! check_for_unexpected_widget_warnings "$log_file"; then
        rm -f "$log_file"
        return 1
    fi

    rm -f "$log_file"
    return "$status"
}

run_headless() {
    require_command dbus-run-session
    require_command mutter

    local runtime_dir
    runtime_dir="$(mktemp -d)"
    local status
    if (
        export XDG_RUNTIME_DIR="$runtime_dir"
        export GDK_BACKEND=wayland
        export LUSHTEXT_WIDGET_HEADLESS_RUNNER=1
        export LUSHTEXT_WIDGET_HEADLESS_MONITOR="$MONITOR"
        export_widget_test_env
        unset DISPLAY WAYLAND_DISPLAY
        run_with_widget_log \
            dbus-run-session -- \
            mutter --headless --wayland --no-x11 --virtual-monitor "$MONITOR" -- \
            cargo test -p lushtext --test widget -- "${TEST_ARGS[@]}"
    ); then
        status=0
    else
        status=$?
    fi
    rm -rf "$runtime_dir"
    return "$status"
}

run_once() {
    echo "Running widget tests under mutter --headless..."
    run_headless
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --headless)
            shift
            ;;
        --auto|--native)
            echo "Error: '$1' was removed; widget tests are headless-only." >&2
            exit 1
            ;;
        --retries)
            [[ $# -lt 2 ]] && { echo "Error: --retries requires a value." >&2; exit 1; }
            RETRIES="$2"
            shift 2
            ;;
        --monitor)
            [[ $# -lt 2 ]] && { echo "Error: --monitor requires a value." >&2; exit 1; }
            MONITOR="$2"
            shift 2
            ;;
        --self-test)
            SELF_TEST=true
            shift
            ;;
        --)
            shift
            TEST_ARGS=("$@")
            break
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Error: unknown argument '$1'." >&2
            usage
            exit 1
            ;;
    esac
done

if ! [[ "$RETRIES" =~ ^[0-9]+$ ]]; then
    echo "Error: --retries must be a non-negative integer." >&2
    exit 1
fi

if [[ "$SELF_TEST" == true ]]; then
    self_test_warning_classification
    exit 0
fi

attempt=1
max_attempts=$((RETRIES + 1))
while (( attempt <= max_attempts )); do
    if run_once; then
        exit 0
    else
        status=$?
    fi

    if (( attempt == max_attempts )); then
        exit "$status"
    fi

    echo "Widget test run failed on attempt $attempt/$max_attempts; retrying..." >&2
    attempt=$((attempt + 1))
done
