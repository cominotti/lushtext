#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/smoke-common.sh
source "$REPO_ROOT/scripts/smoke-common.sh"

ARTIFACT_DIR="${LUSHTEXT_SMOKE_ARTIFACT_DIR:-build/smoke/accessibility}"
BINARY="$REPO_ROOT/target/debug/lushtext"

usage() {
    cat <<'EOF'
Usage: scripts/run-accessibility-smoke.sh [--artifact-dir DIR] [--binary PATH]

Run accessibility-enabled smoke checks. This lane keeps NO_AT_BRIDGE unset and
uses the headless Mutter capture helper's AT-SPI path to prove stable anchors,
focus paths, and no-context overlay states through the real accessibility bus.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --artifact-dir)
            [[ $# -lt 2 ]] && smoke_fail "--artifact-dir requires a value"
            ARTIFACT_DIR="$2"
            shift 2
            ;;
        --binary)
            [[ $# -lt 2 ]] && smoke_fail "--binary requires a value"
            BINARY="$2"
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
rm -rf \
    "$ARTIFACT_DIR/fixtures" \
    "$ARTIFACT_DIR/captures" \
    "$ARTIFACT_DIR/assertions" \
    "$ARTIFACT_DIR/capture" \
    "$ARTIFACT_DIR/search-capture-test"
rm -f "$ARTIFACT_DIR"/*.png "$ARTIFACT_DIR"/*.session.log "$ARTIFACT_DIR/session.log"
rm -f "$ARTIFACT_DIR/atspi-tree.txt" "$ARTIFACT_DIR/atspi-focus.txt" "$ARTIFACT_DIR/skip-reason.txt"
mkdir -p "$ARTIFACT_DIR/fixtures" "$ARTIFACT_DIR/captures" "$ARTIFACT_DIR/assertions"

accessibility_skip() {
    local reason="$*"
    echo "SKIP: $reason"
    printf '%s\n' "$reason" >"$ARTIFACT_DIR/skip-reason.txt"
    {
        echo "status=skipped"
        echo "reason=$reason"
        echo "environment=$ARTIFACT_DIR/environment.txt"
    } >"$ARTIFACT_DIR/summary.txt"
    exit 0
}

accessibility_require_command() {
    local command_name="$1"
    if ! command -v "$command_name" >/dev/null 2>&1; then
        accessibility_skip "'${command_name}' is not installed."
    fi
}

accessibility_require_command dbus-run-session
accessibility_require_command gdbus
accessibility_require_command mutter
accessibility_require_command pipewire
accessibility_require_command pw-dump
accessibility_require_command wireplumber

[[ -x /usr/bin/python3 ]] || accessibility_skip "/usr/bin/python3 is not available."
/usr/bin/python3 -c 'import gi, pyatspi' >/dev/null 2>&1 || accessibility_skip "system Python lacks gi/pyatspi."
[[ -x /usr/libexec/at-spi2-registryd ]] || accessibility_skip "at-spi2-registryd is not available."
[[ -x "$BINARY" ]] || accessibility_skip "LushText debug binary is missing. Run 'make build-debug' first."

collect_accessibility_warnings() {
    local warnings_path="$1"
    : >"$warnings_path"
    shopt -s nullglob
    local log_paths=(
        "$ARTIFACT_DIR"/*.session.log
        "$ARTIFACT_DIR/captures"/*/*.log
        "$ARTIFACT_DIR/captures"/*/lushtext.stdout
        "$ARTIFACT_DIR/captures"/*/lushtext.stderr
    )
    shopt -u nullglob

    for log_path in "${log_paths[@]}"; do
        [[ -f "$log_path" ]] || continue
        grep -E -i \
            '(Gtk|Gdk|GSK|Adwaita|Libadwaita|AT-SPI|accessibility).*(warning|critical|error)|GLib-GObject-CRITICAL|gtk_[a-z0-9_]+.*assertion|gdk_[a-z0-9_]+.*assertion' \
            "$log_path" >>"$warnings_path" || true
    done
}

run_accessibility_capture() {
    local name="$1"
    shift
    local output="$ARTIFACT_DIR/${name}.png"
    local tree_output="$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
    local focus_output="$ARTIFACT_DIR/assertions/${name}-atspi-focus.txt"
    local capture_dir="$ARTIFACT_DIR/captures/$name"
    local session_log="$ARTIFACT_DIR/${name}.session.log"

    if ! /usr/bin/python3 \
        "$REPO_ROOT/.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py" \
        --file "$FIXTURE" \
        --output "$output" \
        --binary "$BINARY" \
        --width 1400 \
        --height 900 \
        --capture-artifact-dir "$capture_dir" \
        --enable-atspi \
        --atspi-tree-output "$tree_output" \
        --atspi-focus-output "$focus_output" \
        --keep-artifacts \
        "$@" \
        >"$session_log" 2>&1; then
        tail -n 120 "$session_log" >&2 || true
        collect_accessibility_warnings "$WARNINGS_OUTPUT"
        smoke_fail "accessibility smoke capture '${name}' failed. Artifacts: $ARTIFACT_DIR"
    fi

    [[ -s "$output" ]] || smoke_fail "accessibility smoke screenshot is empty: $output"
    [[ -s "$tree_output" ]] || smoke_fail "accessibility tree artifact is empty: $tree_output"
    [[ -s "$focus_output" ]] || smoke_fail "accessibility focus artifact is empty: $focus_output"
}

assert_anchor() {
    local capture="$1"
    local surface="$2"
    local role="$3"
    local name="$4"
    local tree="$ARTIFACT_DIR/assertions/${capture}-atspi-tree.txt"
    local report="$ARTIFACT_DIR/assertions/accessibility-anchors.txt"
    local pattern="role='$role' name='$name'"

    if grep -F "$pattern" "$tree" >"$ARTIFACT_DIR/assertions/${capture}-${role// /-}-${name// /-}.anchor.txt"; then
        printf 'PASS surface=%s role=%s name=%s tree=%s\n' "$surface" "$role" "$name" "$tree" >>"$report"
        return
    fi

    {
        echo "Missing accessibility anchor:"
        echo "surface=$surface"
        echo "role=$role"
        echo "name=$name"
        echo "tree=$tree"
    } >&2
    smoke_fail "accessibility anchor '${name}' missing from '${capture}'. Artifacts: $ARTIFACT_DIR"
}

record_focus_anchor() {
    local capture="$1"
    local expected_name="$2"
    local focus="$ARTIFACT_DIR/assertions/${capture}-atspi-focus.txt"
    local tree="$ARTIFACT_DIR/assertions/${capture}-atspi-tree.txt"
    local report="$ARTIFACT_DIR/assertions/accessibility-focus.txt"
    local focus_anchor="$ARTIFACT_DIR/assertions/${capture}-focus.anchor.txt"

    if grep -F "name='$expected_name'" "$focus" >"$focus_anchor"; then
        printf 'PASS capture=%s focused_name=%s focus=%s\n' "$capture" "$expected_name" "$focus" >>"$report"
        return
    fi
    rm -f "$focus_anchor"

    if grep -F "name='$expected_name'" "$tree" >"$ARTIFACT_DIR/assertions/${capture}-focus-fallback.anchor.txt"; then
        printf 'PASS capture=%s focused_name=<unreported> fallback_visible_name=%s focus=%s\n' "$capture" "$expected_name" "$focus" >>"$report"
        return
    fi

    smoke_fail "accessibility focus target '${expected_name}' missing from '${capture}'. Artifacts: $ARTIFACT_DIR"
}

FIXTURE="$ARTIFACT_DIR/fixtures/accessibility-smoke.txt"
WARNINGS_OUTPUT="$ARTIFACT_DIR/warnings.txt"
smoke_create_text_fixture "$FIXTURE"

unset NO_AT_BRIDGE
: >"$ARTIFACT_DIR/assertions/accessibility-anchors.txt"
: >"$ARTIFACT_DIR/assertions/accessibility-focus.txt"

run_accessibility_capture "shell" --search needle
assert_anchor "shell" "window shell" "page tab list" "Open document tabs"
assert_anchor "shell" "window shell" "toggle button" "Toggle workspace sidebar"
assert_anchor "shell" "window shell" "grouping" "Document metadata"
assert_anchor "shell" "window shell" "button" "New file"
assert_anchor "shell" "window shell" "button" "Open recent documents"
assert_anchor "shell" "window shell" "button" "Notes menu"
assert_anchor "shell" "window shell" "button" "Main menu"
assert_anchor "shell" "window shell" "toggle button" "Toggle document properties"
assert_anchor "shell" "workspace sidebar" "button" "New Workspace"

run_accessibility_capture "command-palette" \
    --window-action toggle-command-palette \
    --wait-window-action set-command-palette-query \
    --window-string-action set-command-palette-mode=files \
    --window-string-action set-command-palette-query=accessibility-smoke \
    --wait-atspi-text "Command palette query"
assert_anchor "command-palette" "command palette" "entry" "Command palette query"
assert_anchor "command-palette" "command palette" "list" "Command palette results"
assert_anchor "command-palette" "command palette" "combo box" "Files"
record_focus_anchor "command-palette" "Command palette query"

run_accessibility_capture "notes-empty" \
    --window-action show-notes \
    --wait-atspi-text "No notes yet"
assert_anchor "notes-empty" "notes browser" "dialog" "Notes"
assert_anchor "notes-empty" "notes browser" "grouping" "No notes yet"
assert_anchor "notes-empty" "notes browser" "button" "Close"

collect_accessibility_warnings "$WARNINGS_OUTPUT"
{
    echo "status=passed"
    echo "screenshots=$ARTIFACT_DIR/*.png"
    echo "atspi_trees=$ARTIFACT_DIR/assertions/*-atspi-tree.txt"
    echo "atspi_focus=$ARTIFACT_DIR/assertions/*-atspi-focus.txt"
    echo "anchors=$ARTIFACT_DIR/assertions/accessibility-anchors.txt"
    echo "focus_assertions=$ARTIFACT_DIR/assertions/accessibility-focus.txt"
    echo "warnings=$WARNINGS_OUTPUT"
    echo "session_logs=$ARTIFACT_DIR/*.session.log"
    echo "capture_artifacts=$ARTIFACT_DIR/captures"
    echo "environment=$ARTIFACT_DIR/environment.txt"
} >"$ARTIFACT_DIR/summary.txt"
echo "PASS: accessibility smoke verified AT-SPI anchors and focus artifacts under $ARTIFACT_DIR"
