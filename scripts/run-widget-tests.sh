#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

MODE="auto"
RETRIES=0
MONITOR="2560x1600"
TEST_ARGS=()

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

run_native() {
    cargo test -p lushtext --test widget -- "${TEST_ARGS[@]}"
}

run_headless() {
    require_command dbus-run-session
    require_command mutter

    local runtime_dir
    runtime_dir="$(mktemp -d)"
    (
        export XDG_RUNTIME_DIR="$runtime_dir"
        export GDK_BACKEND=wayland
        dbus-run-session -- \
            mutter --headless --wayland --no-x11 --virtual-monitor "$MONITOR" -- \
            cargo test -p lushtext --test widget -- "${TEST_ARGS[@]}"
    )
    local status=$?
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
    fi

    status=$?
    if (( attempt == max_attempts )); then
        exit "$status"
    fi

    echo "Widget test run failed on attempt $attempt/$max_attempts; retrying..." >&2
    attempt=$((attempt + 1))
done
