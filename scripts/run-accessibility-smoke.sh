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

Run an accessibility-enabled smoke check. This lane keeps NO_AT_BRIDGE unset and
uses the headless Mutter capture helper's AT-SPI path to prove the accessibility
stack is available before deeper accessibility assertions are added.
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
rm -rf "$ARTIFACT_DIR/fixtures" "$ARTIFACT_DIR/capture" "$ARTIFACT_DIR/assertions"
mkdir -p "$ARTIFACT_DIR/fixtures" "$ARTIFACT_DIR/capture" "$ARTIFACT_DIR/assertions"

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
        "$ARTIFACT_DIR/session.log"
        "$ARTIFACT_DIR/capture"/*.log
        "$ARTIFACT_DIR/capture"/lushtext.stdout
        "$ARTIFACT_DIR/capture"/lushtext.stderr
    )
    shopt -u nullglob

    for log_path in "${log_paths[@]}"; do
        [[ -f "$log_path" ]] || continue
        grep -E -i \
            '(Gtk|Gdk|GSK|Adwaita|Libadwaita|AT-SPI|accessibility).*(warning|critical|error)|GLib-GObject-CRITICAL|gtk_[a-z0-9_]+.*assertion|gdk_[a-z0-9_]+.*assertion' \
            "$log_path" >>"$warnings_path" || true
    done
}

FIXTURE="$ARTIFACT_DIR/fixtures/accessibility-smoke.txt"
OUTPUT="$ARTIFACT_DIR/accessibility-search.png"
TREE_OUTPUT="$ARTIFACT_DIR/atspi-tree.txt"
FOCUS_OUTPUT="$ARTIFACT_DIR/atspi-focus.txt"
WARNINGS_OUTPUT="$ARTIFACT_DIR/warnings.txt"
smoke_create_text_fixture "$FIXTURE"

unset NO_AT_BRIDGE
if ! /usr/bin/python3 \
    "$REPO_ROOT/.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py" \
    --file "$FIXTURE" \
    --output "$OUTPUT" \
    --binary "$BINARY" \
    --search needle \
    --width 1400 \
    --height 900 \
    --capture-artifact-dir "$ARTIFACT_DIR/capture" \
    --enable-atspi \
    --atspi-tree-output "$TREE_OUTPUT" \
    --atspi-focus-output "$FOCUS_OUTPUT" \
    --keep-artifacts \
    >"$ARTIFACT_DIR/session.log" 2>&1; then
    tail -n 120 "$ARTIFACT_DIR/session.log" >&2 || true
    collect_accessibility_warnings "$WARNINGS_OUTPUT"
    smoke_fail "accessibility smoke failed. Artifacts: $ARTIFACT_DIR"
fi

[[ -s "$OUTPUT" ]] || smoke_fail "accessibility smoke screenshot is empty: $OUTPUT"
[[ -s "$TREE_OUTPUT" ]] || smoke_fail "accessibility tree artifact is empty: $TREE_OUTPUT"
[[ -s "$FOCUS_OUTPUT" ]] || smoke_fail "accessibility focus artifact is empty: $FOCUS_OUTPUT"
collect_accessibility_warnings "$WARNINGS_OUTPUT"
{
    echo "status=passed"
    echo "screenshot=$OUTPUT"
    echo "atspi_tree=$TREE_OUTPUT"
    echo "atspi_focus=$FOCUS_OUTPUT"
    echo "warnings=$WARNINGS_OUTPUT"
    echo "session_log=$ARTIFACT_DIR/session.log"
    echo "capture_artifacts=$ARTIFACT_DIR/capture"
    echo "environment=$ARTIFACT_DIR/environment.txt"
} >"$ARTIFACT_DIR/summary.txt"
echo "PASS: accessibility-enabled smoke used AT-SPI and captured artifacts under $ARTIFACT_DIR"
