#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/smoke-common.sh
source "$REPO_ROOT/scripts/smoke-common.sh"

ARTIFACT_DIR="${LUSHTEXT_SMOKE_ARTIFACT_DIR:-build/smoke/portal-sandbox}"
APP_ID="${LUSHTEXT_FLATPAK_APP_ID:-dev.cominotti.lushtext}"
FLATPAK_HOST_DIR=""

cleanup_flatpak_host_dir() {
    if [[ -n "$FLATPAK_HOST_DIR" && -d "$FLATPAK_HOST_DIR" ]]; then
        rm -rf "$FLATPAK_HOST_DIR"
    fi
}
trap cleanup_flatpak_host_dir EXIT

usage() {
    cat <<'EOF'
Usage: scripts/run-portal-sandbox-smoke.sh [--artifact-dir DIR]

Record Flatpak/Snap confinement state and run available confined smoke checks.
Missing packaging runtimes skip clearly instead of pretending confinement was
verified.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --artifact-dir)
            [[ $# -lt 2 ]] && smoke_fail "--artifact-dir requires a value"
            ARTIFACT_DIR="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            smoke_fail "unknown argument: $1"
            ;;
    esac
done

ARTIFACT_DIR="$(smoke_artifact_dir "$ARTIFACT_DIR")"
smoke_write_environment_report "$ARTIFACT_DIR/environment.txt"
mkdir -p "$ARTIFACT_DIR/fixtures"

collect_runtime_denials() {
    local output="$1"
    if command -v journalctl >/dev/null 2>&1; then
        journalctl --user --since "5 min ago" 2>/dev/null \
            | grep -iE "xdg-desktop-portal|flatpak|snap|apparmor|seccomp|denied|denial" \
            >"$output" || true
    else
        echo "SKIP: journalctl is not installed." >"$output"
    fi
}

run_chooser_widget_smoke() {
    local log="$ARTIFACT_DIR/chooser-widget-tests.log"

    if [[ "${LUSHTEXT_PORTAL_SMOKE_RUN_CHOOSER_TESTS:-1}" != "1" ]]; then
        echo "SKIP: LUSHTEXT_PORTAL_SMOKE_RUN_CHOOSER_TESTS disabled chooser tests." >"$log"
        return 2
    fi
    for command_name in dbus-run-session mutter; do
        if ! command -v "$command_name" >/dev/null 2>&1; then
            echo "SKIP: ${command_name} is not installed, cannot run chooser widget smoke." >"$log"
            return 2
        fi
    done
    if "$REPO_ROOT/scripts/run-widget-tests.sh" \
        --headless \
        --retries 0 \
        -- window::test_file_chooser \
        >"$log" 2>&1; then
        return 0
    fi

    tail -n 120 "$log" >&2 || true
    smoke_fail "file chooser outcome smoke failed. Artifacts: $ARTIFACT_DIR"
}

flatpak_hard_failure_regex() {
    printf '%s\n' 'failed to load installed GResource|failed to load GResource|Settings schema .* is not installed|GLib-GIO-ERROR|Gtk-ERROR|thread .* panicked|panic|Segmentation fault|Trace/breakpoint trap'
}

run_flatpak_launch_case() {
    local name="$1"
    local file_path="$2"
    local log="$ARTIFACT_DIR/flatpak-${name}.log"
    local runtime_dir="$ARTIFACT_DIR/flatpak-${name}-runtime"

    for command_name in dbus-run-session mutter timeout; do
        if ! command -v "$command_name" >/dev/null 2>&1; then
            echo "SKIP: ${command_name} is not installed, cannot run Flatpak ${name} smoke." >"$log"
            return 2
        fi
    done

    rm -rf "$runtime_dir"
    mkdir -p "$runtime_dir"
    chmod 700 "$runtime_dir"

    set +e
    env XDG_RUNTIME_DIR="$runtime_dir" GDK_BACKEND=wayland \
        dbus-run-session -- \
        mutter --headless --wayland --no-x11 --virtual-monitor 1280x800 -- \
        timeout 12s flatpak run \
        --no-a11y-bus \
        --env=LUSHTEXT_DATA_DIR="$FLATPAK_HOST_DIR/data" \
        --env=NO_AT_BRIDGE=1 \
        --unset-env=AT_SPI_BUS_ADDRESS \
        "$APP_ID" "$file_path" \
        >"$log" 2>&1
    local status=$?
    set -e

    local expected_mutter_timeout=0
    if [[ "$status" == "1" ]] && grep -q "nonzero status: 31744" "$log"; then
        expected_mutter_timeout=1
    fi
    if [[ "$status" != "0" && "$status" != "124" && "$expected_mutter_timeout" != "1" ]]; then
        tail -n 120 "$log" >&2 || true
        smoke_fail "Flatpak ${name} launch exited with status ${status}. Artifacts: $ARTIFACT_DIR"
    fi
    if grep -qiE "$(flatpak_hard_failure_regex)" "$log"; then
        tail -n 120 "$log" >&2 || true
        smoke_fail "Flatpak ${name} launch emitted resource/schema/crash failures. Artifacts: $ARTIFACT_DIR"
    fi
    if [[ "$name" == "accessible-open" ]] \
        && grep -qiE "Could not open|Permission denied|No such file|Failed to read" "$log"; then
        tail -n 120 "$log" >&2 || true
        smoke_fail "Flatpak accessible-open did not open the supported fixture cleanly. Artifacts: $ARTIFACT_DIR"
    fi
    return 0
}

chooser_status="skipped"
if run_chooser_widget_smoke; then
    chooser_status="passed"
fi

portal_status="skipped"
if command -v gdbus >/dev/null 2>&1; then
    if gdbus call \
        --session \
        --dest org.freedesktop.DBus \
        --object-path /org/freedesktop/DBus \
        --method org.freedesktop.DBus.ListNames \
        >"$ARTIFACT_DIR/session-bus-names.txt" 2>&1; then
        grep -o "org\.freedesktop\.[^']*portal[^']*" "$ARTIFACT_DIR/session-bus-names.txt" \
            | sort -u >"$ARTIFACT_DIR/portal-names.txt" || true
        if [[ -s "$ARTIFACT_DIR/portal-names.txt" ]]; then
            portal_status="detected"
        else
            echo "SKIP: no portal bus names found on the session bus." >"$ARTIFACT_DIR/portal-names.txt"
        fi
    else
        echo "SKIP: could not inspect the session bus for portal names." >"$ARTIFACT_DIR/portal-names.txt"
    fi
else
    echo "SKIP: gdbus is not installed." >"$ARTIFACT_DIR/portal-names.txt"
fi

collect_runtime_denials "$ARTIFACT_DIR/recent-runtime-denials-before.txt"

flatpak_status="skipped"
if command -v flatpak >/dev/null 2>&1; then
    if flatpak info "$APP_ID" >"$ARTIFACT_DIR/flatpak-info.txt" 2>&1; then
        flatpak info --show-permissions "$APP_ID" >"$ARTIFACT_DIR/flatpak-permissions.txt" 2>&1 || true
        flatpak info --show-runtime "$APP_ID" >"$ARTIFACT_DIR/flatpak-runtime.txt" 2>&1 || true
        FLATPAK_HOST_DIR="$(mktemp -d "${HOME}/.lushtext-flatpak-smoke.XXXXXX")"
        mkdir -p "$FLATPAK_HOST_DIR/data"
        echo "$FLATPAK_HOST_DIR" >"$ARTIFACT_DIR/flatpak-host-fixture-dir.txt"
        ACCESSIBLE_FILE="$FLATPAK_HOST_DIR/flatpak-accessible.txt"
        INACCESSIBLE_FILE="$FLATPAK_HOST_DIR/flatpak-inaccessible.txt"
        smoke_create_text_fixture "$ACCESSIBLE_FILE"
        smoke_create_text_fixture "$INACCESSIBLE_FILE"
        chmod 000 "$INACCESSIBLE_FILE"
        set +e
        run_flatpak_launch_case "accessible-open" "$ACCESSIBLE_FILE"
        accessible_status=$?
        run_flatpak_launch_case "inaccessible-open" "$INACCESSIBLE_FILE"
        inaccessible_status=$?
        set -e
        chmod 600 "$INACCESSIBLE_FILE" || true
        if [[ "$accessible_status" == "0" && "$inaccessible_status" == "0" ]]; then
            flatpak_status="passed"
        elif [[ "$accessible_status" == "2" || "$inaccessible_status" == "2" ]]; then
            flatpak_status="skipped"
        fi
    else
        echo "SKIP: Flatpak app '$APP_ID' is not installed." >"$ARTIFACT_DIR/flatpak-info.txt"
    fi
else
    echo "SKIP: flatpak is not installed." >"$ARTIFACT_DIR/flatpak-info.txt"
fi

snap_status="skipped"
if [[ -x "$REPO_ROOT/scripts/run-snap-smoke.sh" ]]; then
    if "$REPO_ROOT/scripts/run-snap-smoke.sh" >"$ARTIFACT_DIR/snap-smoke.log" 2>&1; then
        if grep -q '^PASS:' "$ARTIFACT_DIR/snap-smoke.log"; then
            snap_status="passed"
        else
            snap_status="skipped"
        fi
    else
        tail -n 120 "$ARTIFACT_DIR/snap-smoke.log" >&2 || true
        smoke_fail "snap confined smoke failed. Artifacts: $ARTIFACT_DIR"
    fi
else
    echo "SKIP: scripts/run-snap-smoke.sh is missing." >"$ARTIFACT_DIR/snap-smoke.log"
fi

collect_runtime_denials "$ARTIFACT_DIR/recent-runtime-denials-after.txt"

{
    echo "chooser=$chooser_status"
    echo "flatpak=$flatpak_status"
    echo "snap=$snap_status"
    echo "portal=$portal_status"
    echo "artifacts=$ARTIFACT_DIR"
} >"$ARTIFACT_DIR/summary.txt"

if [[ "$chooser_status" == "skipped" && "$flatpak_status" == "skipped" && "$snap_status" == "skipped" && "$portal_status" == "skipped" ]]; then
    smoke_skip "no confined runtime was available. Artifacts: $ARTIFACT_DIR"
fi

echo "PASS: portal/sandbox smoke recorded available runtime state in $ARTIFACT_DIR"
