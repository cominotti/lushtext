#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

MODE="auto"
RETRIES=0
MONITOR="2560x1600"
TEST_ARGS=()
BENIGN_WIDGET_NOISE_REGEX='(dbus-daemon\[[0-9]+\]: .*org\.(freedesktop\.(portal|impl\.portal\.|systemd1)|a11y\.Bus)|\(/usr/libexec/xdg-desktop-portal:.*WARNING \*\*:|\*\* \(xdg-desktop-portal-gtk:.*WARNING \*\*:|Gtk-CRITICAL \*\*: .*org\.a11y\.atspi\.Registry|Gdk-Message: .*Broken pipe$|rm: cannot remove '\''/tmp/.*/doc'\'': Is a directory$|^libmutter-Message:|^\*\* Message: .*Obtained a high priority EGL context$)'
WIDGET_WARNING_REGEX='(warning:|WARNING|CRITICAL|Gdk-Message:|Broken pipe|cannot remove)'

usage() {
    cat <<'EOF'
Usage: scripts/run-widget-tests.sh [--auto|--native|--headless] [--retries N] [--monitor WxH] [-- [test-binary-args...]]

Run the LushText widget test harness either against the current desktop session
or under a transient Mutter headless compositor.

Options:
  --auto        Use the current display when available, otherwise fall back to
                headless mode if Mutter and dbus-run-session are installed.
  --native      Require an existing display server and run the harness directly.
  --headless    Always run under `mutter --headless`.
  --retries N   Retry the full harness up to N times after the first failure.
  --monitor WxH Virtual monitor size for headless runs (default: 2560x1600).
  -h, --help    Show this help text.

Arguments after `--` are passed to the widget test binary, for example:
  scripts/run-widget-tests.sh -- --exact window::test_primary_menu_button_exists
EOF
}

require_command() {
    local command_name="$1"
    if ! command -v "$command_name" >/dev/null 2>&1; then
        echo "Error: '$command_name' is required for widget test execution." >&2
        exit 1
    fi
}

has_live_display() {
    [[ -n "${WAYLAND_DISPLAY:-}" || -n "${DISPLAY:-}" ]]
}

export_widget_test_env() {
    export NO_AT_BRIDGE=1
    export GDK_DEBUG=no-portals
    export GTK_USE_PORTAL=0
    export GSK_RENDERER=gl
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

run_native() {
    export_widget_test_env
    run_with_widget_log cargo test -p lushtext --test widget -- "${TEST_ARGS[@]}"
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
        export_widget_test_env
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
    case "$MODE" in
        auto)
            if has_live_display; then
                echo "Running widget tests against the current display session..."
                run_native
            elif command -v dbus-run-session >/dev/null 2>&1 && command -v mutter >/dev/null 2>&1; then
                echo "No live display detected; running widget tests under mutter --headless..."
                run_headless
            else
                echo "Error: no display server detected and headless prerequisites are missing." >&2
                echo "Install 'mutter' and 'dbus-run-session', or rerun inside a live desktop session." >&2
                exit 1
            fi
            ;;
        native)
            if ! has_live_display; then
                echo "Error: --native requires DISPLAY or WAYLAND_DISPLAY to be set." >&2
                exit 1
            fi
            echo "Running widget tests against the current display session..."
            run_native
            ;;
        headless)
            echo "Running widget tests under mutter --headless..."
            run_headless
            ;;
        *)
            echo "Error: unknown mode '$MODE'." >&2
            exit 1
            ;;
    esac
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --auto)
            MODE="auto"
            shift
            ;;
        --native)
            MODE="native"
            shift
            ;;
        --headless)
            MODE="headless"
            shift
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
