#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/smoke-common.sh
source "$REPO_ROOT/scripts/smoke-common.sh"

ARTIFACT_DIR="${LUSHTEXT_SMOKE_ARTIFACT_DIR:-build/smoke/accessibility}"
BINARY="$REPO_ROOT/target/debug/lushtext"
ACCESSIBILITY_CASES=(
    shell
    preferences
    properties-panel
    compact-properties
    markdown-preview
    preview-mode-transition
    editor
    focus-mode
    minimap-transition
    editor-search
    editor-save-completion
    editor-failed-load
    editor-too-large-policy
    workspace-search-no-workspace
    workspace-search
    workspace-search-no-results
    workspace-search-dense-constrained
    workspace-search-replace-undo
    workspace-tree-no-workspace
    workspace-tree
    workspace-tree-zero-folder
    workspace-tree-dense-constrained
    workspace-tree-deep-expanded
    workspace-tree-file-peek
    workspace-tree-folder-context-menu
    workspace-header-context-menu
    open-popover-empty
    open-popover-dense
    open-popover-filtered
    open-popover-no-match
    open-popover-dismiss
    command-palette
    command-palette-commands
    command-palette-notes
    command-palette-dense-files
    command-palette-mode-changes
    command-palette-focus-restore
    command-palette-no-results
    notes-empty
    notes-populated
    notes-no-results
    bookmarks-populated
    local-history-empty
    local-history
    local-history-restore
    local-history-empty-snapshot
    unsaved-close-dialog
    discard-confirmation
)
SELECTED_CASES=()
LIST_CASES=false

usage() {
    cat <<'EOF'
Usage: scripts/run-accessibility-smoke.sh [--artifact-dir DIR] [--binary PATH] [--case PATTERN] [--list-cases]

Run accessibility-enabled smoke checks. This lane keeps NO_AT_BRIDGE unset and
uses the headless Mutter capture helper's AT-SPI path to prove stable anchors,
focus paths, and no-context overlay states through the real accessibility bus.

Options:
  --case PATTERN   Run only matching scenario names. Shell-style globs are accepted.
                   May be repeated.
  --list-cases     Print known scenario names and exit.
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
        --case)
            [[ $# -lt 2 ]] && smoke_fail "--case requires a value"
            SELECTED_CASES+=("$2")
            shift 2
            ;;
        --list-cases)
            LIST_CASES=true
            shift
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

if [[ "$LIST_CASES" == true ]]; then
    printf '%s\n' "${ACCESSIBILITY_CASES[@]}"
    exit 0
fi

case_selected() {
    local name="$1"
    if ((${#SELECTED_CASES[@]} == 0)); then
        return 0
    fi

    local pattern
    for pattern in "${SELECTED_CASES[@]}"; do
        if [[ "$name" == $pattern ]]; then
            return 0
        fi
    done
    return 1
}

selected_case_description() {
    if ((${#SELECTED_CASES[@]} == 0)); then
        printf 'all'
        return
    fi
    local joined=""
    local pattern
    for pattern in "${SELECTED_CASES[@]}"; do
        if [[ -n "$joined" ]]; then
            joined+=","
        fi
        joined+="$pattern"
    done
    printf '%s' "$joined"
}

ARTIFACT_DIR="$(smoke_artifact_dir "$ARTIFACT_DIR")"
WARNINGS_OUTPUT="$ARTIFACT_DIR/warnings.txt"
ASSERTION_EVENTS="$ARTIFACT_DIR/assertions/accessibility-assertions.jsonl"
ACCESSIBILITY_CASE_FILTERS="$(selected_case_description)"
export ACCESSIBILITY_CASE_FILTERS
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

write_accessibility_summary_json() {
    local status="$1"
    local reason="${2:-}"

    if [[ ! -x /usr/bin/python3 ]]; then
        cat >"$ARTIFACT_DIR/summary.json" <<JSON
{
  "schema_version": 1,
  "status": "unsupported-host",
  "skip_reason": "system Python is unavailable; JSON summary was written by shell fallback",
  "missing_capabilities": ["python3"],
  "lane": "accessibility-smoke"
}
JSON
        return
    fi

    /usr/bin/python3 - "$ARTIFACT_DIR" "$status" "$reason" "$REPO_ROOT" <<'PY'
import json
import os
import sys
from pathlib import Path

root = Path(sys.argv[1])
status = sys.argv[2]
reason = sys.argv[3]
repo_root = Path(sys.argv[4]).resolve()
assertions_dir = root / "assertions"
sys.path.insert(0, str(repo_root / "scripts"))
from accessibility_source_fingerprint import source_fingerprint

def rel(path: Path) -> str:
    try:
        return path.relative_to(root).as_posix()
    except ValueError:
        return path.as_posix()

def lines(path: Path) -> list[str]:
    if not path.is_file():
        return []
    return path.read_text(encoding="utf-8", errors="replace").splitlines()

manifests = sorted(assertions_dir.glob("*-manifest.json"))
screenshots = sorted(root.glob("*.png"))
warning_path = root / "warnings.txt"
warning_lines = lines(warning_path)

def warning_line_is_allowlisted(line: str) -> bool:
    if line.startswith("Gdk-Message: ") and line.endswith("Error reading events from display: Broken pipe"):
        return True
    return (
        "ERROR lushtext_core::ui::editor_page::load_save: Failed to read " in line
        and "unreadable-load-target.txt: Permission denied" in line
    )

matrix_rows = []
for manifest_path in manifests:
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        continue
    matrix_rows.extend(manifest.get("matrix_rows", []))
matrix_rows = sorted(set(matrix_rows))
unexpected_warnings = [
    line
    for line in warning_lines
    if not warning_line_is_allowlisted(line)
]
warning_status = "passed"
if warning_lines and not unexpected_warnings:
    warning_status = "allowlisted"
elif unexpected_warnings:
    warning_status = "found"
assertion_events = assertions_dir / "accessibility-assertions.jsonl"
payload = {
    "schema_version": 1,
    "status": status,
    "lane": "accessibility-smoke",
    "case_filters": [
        value
        for value in os.environ.get("ACCESSIBILITY_CASE_FILTERS", "all").split(",")
        if value
    ],
    "scenario_source": {
        "manifest_count": len(manifests),
        "manifests": [rel(path) for path in manifests],
    },
    "matrix_coverage": {
        "row_count": len(matrix_rows),
        "rows": matrix_rows,
        "focused_run": os.environ.get("ACCESSIBILITY_CASE_FILTERS", "all") != "all",
    },
    "screenshots": [rel(path) for path in screenshots],
    "environment_report": {"artifact": "environment.txt"},
    "missing_capabilities": [reason] if status in {"skipped", "unsupported-host"} and reason else [],
    "warnings": {
        "status": warning_status,
        "artifact": rel(warning_path),
        "line_count": len(warning_lines),
        "unexpected_count": len(unexpected_warnings),
    },
    "accessibility_assertions": {
        "artifact": rel(assertion_events),
        "line_count": len(lines(assertion_events)),
        "anchors": "assertions/accessibility-anchors.txt",
        "focus": "assertions/accessibility-focus.txt",
        "text": "assertions/accessibility-text.txt",
    },
    "source_fingerprint": source_fingerprint(repo_root),
}
if reason:
    key = "skip_reason" if status in {"skipped", "unsupported-host"} else "failure_reason"
    payload[key] = reason
(root / "summary.json").write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
}

write_accessibility_manifest() {
    local name="$1"
    local status="$2"
    local output="$3"
    local tree_output="$4"
    local focus_output="$5"
    local capture_dir="$6"
    local session_log="$7"
    local fixture="$8"
    shift 8

    /usr/bin/python3 - \
        "$ARTIFACT_DIR" \
        "$name" \
        "$status" \
        "$output" \
        "$tree_output" \
        "$focus_output" \
        "$capture_dir" \
        "$session_log" \
        "$fixture" \
        "$@" <<'PY'
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
name = sys.argv[2]
status = sys.argv[3]
output = Path(sys.argv[4])
tree_output = Path(sys.argv[5])
focus_output = Path(sys.argv[6])
capture_dir = Path(sys.argv[7])
session_log = Path(sys.argv[8])
fixture = Path(sys.argv[9])
capture_args = sys.argv[10:]

MATRIX_ROWS_BY_CASE = {
    "shell": ["A11Y-SHELL-NO-CONTEXT", "A11Y-SHELL-REPRESENTATIVE"],
    "preferences": ["A11Y-PREFERENCES-PAGES"],
    "properties-panel": ["A11Y-PROPERTIES-NORMAL"],
    "compact-properties": ["A11Y-PROPERTIES-COMPACT", "A11Y-SHELL-DENSE-CONSTRAINED"],
    "markdown-preview": ["A11Y-MARKDOWN-REPRESENTATIVE"],
    "preview-mode-transition": ["A11Y-EDITOR-FOCUS-PREVIEW", "A11Y-MARKDOWN-CONSTRAINED"],
    "editor": ["A11Y-EDITOR-REPRESENTATIVE"],
    "focus-mode": ["A11Y-EDITOR-FOCUS-PREVIEW"],
    "minimap-transition": ["A11Y-EDITOR-MINIMAP"],
    "editor-search": ["A11Y-EDITOR-SEARCH"],
    "editor-save-completion": ["A11Y-EDITOR-BUSY"],
    "editor-failed-load": ["A11Y-EDITOR-ERROR", "A11Y-ERROR-SURFACES"],
    "editor-too-large-policy": ["A11Y-EDITOR-LARGE-READONLY", "A11Y-EDITOR-ERROR"],
    "workspace-search-no-workspace": ["A11Y-WORKSPACE-SEARCH-NO-CONTEXT", "A11Y-WORKSPACE-NO-CONTEXT"],
    "workspace-search": ["A11Y-WORKSPACE-SEARCH-REPRESENTATIVE"],
    "workspace-search-no-results": ["A11Y-WORKSPACE-SEARCH-DENSE-NORESULTS"],
    "workspace-search-dense-constrained": ["A11Y-WORKSPACE-SEARCH-DENSE-NORESULTS"],
    "workspace-search-replace-undo": ["A11Y-WORKSPACE-SEARCH-REPLACE"],
    "workspace-tree-no-workspace": ["A11Y-WORKSPACE-NO-CONTEXT"],
    "workspace-tree": ["A11Y-WORKSPACE-REPRESENTATIVE"],
    "workspace-tree-zero-folder": ["A11Y-WORKSPACE-ZERO-FOLDER"],
    "workspace-tree-dense-constrained": ["A11Y-WORKSPACE-DENSE-DEEP"],
    "workspace-tree-deep-expanded": ["A11Y-WORKSPACE-DENSE-DEEP"],
    "workspace-tree-file-peek": ["A11Y-WORKSPACE-PEEK"],
    "workspace-tree-folder-context-menu": ["A11Y-WORKSPACE-CONTEXT", "A11Y-WORKSPACE-DRAG-DROP", "A11Y-CONTEXT-MENUS-GENERAL"],
    "workspace-header-context-menu": ["A11Y-WORKSPACE-CONTEXT", "A11Y-CONTEXT-MENUS-GENERAL"],
    "open-popover-empty": ["A11Y-OPEN-EMPTY"],
    "open-popover-dense": ["A11Y-OPEN-DENSE-FILTERED"],
    "open-popover-filtered": ["A11Y-OPEN-DENSE-FILTERED"],
    "open-popover-no-match": ["A11Y-OPEN-DENSE-FILTERED"],
    "open-popover-dismiss": ["A11Y-OPEN-HIDDEN"],
    "command-palette": ["A11Y-PALETTE-FILES"],
    "command-palette-commands": ["A11Y-PALETTE-COMMANDS"],
    "command-palette-notes": ["A11Y-PALETTE-NOTES"],
    "command-palette-dense-files": ["A11Y-PALETTE-FILES"],
    "command-palette-mode-changes": ["A11Y-PALETTE-NO-RESULTS"],
    "command-palette-focus-restore": ["A11Y-PALETTE-DISMISS"],
    "command-palette-no-results": ["A11Y-PALETTE-NO-RESULTS"],
    "notes-empty": ["A11Y-NOTES-EMPTY"],
    "notes-populated": ["A11Y-NOTES-POPULATED"],
    "notes-no-results": ["A11Y-NOTES-POPULATED"],
    "bookmarks-populated": ["A11Y-BOOKMARKS"],
    "local-history-empty": ["A11Y-LOCAL-HISTORY-EMPTY"],
    "local-history": ["A11Y-LOCAL-HISTORY-POPULATED"],
    "local-history-restore": ["A11Y-LOCAL-HISTORY-POPULATED", "A11Y-DIALOG-DESTRUCTIVE"],
    "local-history-empty-snapshot": ["A11Y-LOCAL-HISTORY-EMPTY"],
    "unsaved-close-dialog": ["A11Y-DIALOG-SAVE-CLOSE"],
    "discard-confirmation": ["A11Y-DIALOG-DESTRUCTIVE"],
}

def rel(path: Path) -> str:
    try:
        return path.relative_to(root).as_posix()
    except ValueError:
        return path.as_posix()

actions = []
waits = []
index = 0
while index < len(capture_args):
    arg = capture_args[index]
    value = None
    if "=" in arg:
        flag, value = arg.split("=", 1)
    else:
        flag = arg
        if index + 1 < len(capture_args) and not capture_args[index + 1].startswith("--"):
            value = capture_args[index + 1]
            index += 1
    row = {"flag": flag}
    if value is not None:
        row["value"] = value
    if flag in {"--app-action", "--window-action", "--window-string-action", "--window-bool-action"}:
        actions.append(row)
    elif flag == "--step":
        if value and value.startswith(("app-action:", "window-action:", "window-string-action:", "window-bool-action:", "atspi-click-button:", "atspi-focus-accessible:", "atspi-activate-accessible:", "atspi-context-click-accessible:", "atspi-key:")):
            actions.append(row)
        elif value and value.startswith(("wait-window-action:", "wait-predicate:", "wait-atspi-text:")):
            waits.append(row)
    elif flag.startswith("--wait-"):
        waits.append(row)
    index += 1

payload = {
    "schema_version": 1,
    "scenario_id": name,
    "scenario_type": "accessibility-smoke",
    "status": status,
    "matrix_rows": MATRIX_ROWS_BY_CASE.get(name, []),
    "fixture": rel(fixture),
    "capture_args": capture_args,
    "actions": actions,
    "waits": waits,
    "screenshots": [rel(output)],
    "atspi_tree": rel(tree_output),
    "atspi_focus": rel(focus_output),
    "assertions": [],
    "assertion_evidence": {
        "events": "assertions/accessibility-assertions.jsonl",
        "anchors": "assertions/accessibility-anchors.txt",
        "focus": "assertions/accessibility-focus.txt",
        "text": "assertions/accessibility-text.txt",
        "manifest_filter": name,
    },
    "anchor_scope": {
        "public": "Product control, role, and region names that should remain meaningful to assistive technology users.",
        "fixture_only": "Seeded row names, fixture file names, and synthetic paths under this smoke artifact directory.",
    },
    "artifact_boundary": {
        "fixture_data": "synthetic accessibility-smoke fixtures only",
        "private_user_data": False,
        "text_policy": "bounded AT-SPI tree excerpts and assertion summaries only",
    },
    "host_caveats": [
        "Requires a host with AT-SPI, D-Bus, Mutter, PipeWire, WirePlumber, and PyGObject.",
        "Headless AT-SPI may omit a focused node; focus assertions may accept a visible-tree fallback for the documented target.",
    ],
    "capture_artifacts": rel(capture_dir),
    "session_log": rel(session_log),
    "warning_scan": "warnings.txt",
    "assertion_events": "assertions/accessibility-assertions.jsonl",
}
(root / "assertions" / f"{name}-manifest.json").write_text(
    json.dumps(payload, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY
}

record_accessibility_assertion() {
    local capture="$1"
    local assertion_kind="$2"
    local surface="$3"
    local role="$4"
    local name="$5"
    local artifact="$6"

    /usr/bin/python3 - \
        "$ARTIFACT_DIR" \
        "$ASSERTION_EVENTS" \
        "$capture" \
        "$assertion_kind" \
        "$surface" \
        "$role" \
        "$name" \
        "$artifact" <<'PY'
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
events = Path(sys.argv[2])
artifact = Path(sys.argv[8])
try:
    artifact_value = artifact.relative_to(root).as_posix()
except ValueError:
    artifact_value = artifact.as_posix()
row = {
    "status": "passed",
    "capture": sys.argv[3],
    "kind": sys.argv[4],
    "surface": sys.argv[5],
    "role": sys.argv[6],
    "name": sys.argv[7],
    "artifact": artifact_value,
}
with events.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(row, sort_keys=True) + "\n")
manifest_path = root / "assertions" / f"{sys.argv[3]}-manifest.json"
if manifest_path.is_file():
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        manifest = None
    if isinstance(manifest, dict):
        manifest.setdefault("assertions", []).append(row)
        manifest_path.write_text(
            json.dumps(manifest, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
PY
}

accessibility_skip() {
    local reason="$*"
    echo "SKIP: $reason"
    printf '%s\n' "$reason" >"$ARTIFACT_DIR/skip-reason.txt"
    {
        echo "status=skipped"
        echo "reason=$reason"
        echo "environment=$ARTIFACT_DIR/environment.txt"
    } >"$ARTIFACT_DIR/summary.txt"
    write_accessibility_summary_json "unsupported-host" "$reason"
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
    RUN_CASE_COUNT=$((RUN_CASE_COUNT + 1))
    local output="$ARTIFACT_DIR/${name}.png"
    local tree_output="$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
    local focus_output="$ARTIFACT_DIR/assertions/${name}-atspi-focus.txt"
    local capture_dir="$ARTIFACT_DIR/captures/$name"
    local session_log="$ARTIFACT_DIR/${name}.session.log"
    local capture_args=(--wait-predicate accessibility-settled "$@")

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
        "${capture_args[@]}" \
        >"$session_log" 2>&1; then
        tail -n 120 "$session_log" >&2 || true
        collect_accessibility_warnings "$WARNINGS_OUTPUT"
        write_accessibility_manifest "$name" "failed" "$output" "$tree_output" "$focus_output" "$capture_dir" "$session_log" "$FIXTURE" "${capture_args[@]}"
        write_accessibility_summary_json "failed" "capture '${name}' failed"
        smoke_fail "accessibility smoke capture '${name}' failed. Artifacts: $ARTIFACT_DIR"
    fi

    [[ -s "$output" ]] || smoke_fail "accessibility smoke screenshot is empty: $output"
    [[ -s "$tree_output" ]] || smoke_fail "accessibility tree artifact is empty: $tree_output"
    [[ -s "$focus_output" ]] || smoke_fail "accessibility focus artifact is empty: $focus_output"
    write_accessibility_manifest "$name" "passed" "$output" "$tree_output" "$focus_output" "$capture_dir" "$session_log" "$FIXTURE" "${capture_args[@]}"
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
        record_accessibility_assertion "$capture" "anchor" "$surface" "$role" "$name" "$tree"
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

assert_anchor_prefix() {
    local capture="$1"
    local surface="$2"
    local role="$3"
    local name_prefix="$4"
    local tree="$ARTIFACT_DIR/assertions/${capture}-atspi-tree.txt"
    local report="$ARTIFACT_DIR/assertions/accessibility-anchors.txt"
    local pattern="role='$role' name='$name_prefix"

    if grep -F "$pattern" "$tree" >"$ARTIFACT_DIR/assertions/${capture}-${role// /-}-${name_prefix// /-}.anchor.txt"; then
        printf 'PASS surface=%s role=%s name_prefix=%s tree=%s\n' "$surface" "$role" "$name_prefix" "$tree" >>"$report"
        record_accessibility_assertion "$capture" "anchor-prefix" "$surface" "$role" "$name_prefix" "$tree"
        return
    fi

    {
        echo "Missing accessibility anchor prefix:"
        echo "surface=$surface"
        echo "role=$role"
        echo "name_prefix=$name_prefix"
        echo "tree=$tree"
    } >&2
    smoke_fail "accessibility anchor prefix '${name_prefix}' missing from '${capture}'. Artifacts: $ARTIFACT_DIR"
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
        record_accessibility_assertion "$capture" "focus" "focus path" "focus" "$expected_name" "$focus"
        return
    fi
    rm -f "$focus_anchor"

    if grep -F "name='$expected_name'" "$tree" >"$ARTIFACT_DIR/assertions/${capture}-focus-fallback.anchor.txt"; then
        printf 'PASS capture=%s focused_name=<unreported> fallback_visible_name=%s focus=%s\n' "$capture" "$expected_name" "$focus" >>"$report"
        record_accessibility_assertion "$capture" "focus-fallback" "focus path" "focus" "$expected_name" "$tree"
        return
    fi

    smoke_fail "accessibility focus target '${expected_name}' missing from '${capture}'. Artifacts: $ARTIFACT_DIR"
}

assert_text_interface() {
    local capture="$1"
    local surface="$2"
    local role="$3"
    local name="$4"
    local min_chars="$5"
    local tree="$ARTIFACT_DIR/assertions/${capture}-atspi-tree.txt"
    local report="$ARTIFACT_DIR/assertions/accessibility-text.txt"
    local pattern="role='$role' name='$name'"
    local line

    line="$(grep -F "$pattern" "$tree" | head -n 1 || true)"
    if [[ -z "$line" ]]; then
        smoke_fail "accessibility text target '${name}' missing from '${capture}'. Artifacts: $ARTIFACT_DIR"
    fi

    if [[ ! "$line" =~ text_chars=([0-9]+) ]]; then
        smoke_fail "accessibility text target '${name}' has no bounded text summary. Artifacts: $ARTIFACT_DIR"
    fi

    local char_count="${BASH_REMATCH[1]}"
    if (( char_count < min_chars )); then
        smoke_fail "accessibility text target '${name}' has only ${char_count} characters; expected at least ${min_chars}. Artifacts: $ARTIFACT_DIR"
    fi

    if [[ "$line" != *" caret="* || "$line" != *" selections="* ]]; then
        smoke_fail "accessibility text target '${name}' is missing caret or selection metadata. Artifacts: $ARTIFACT_DIR"
    fi

    printf 'PASS surface=%s role=%s name=%s text_chars=%s tree=%s\n' "$surface" "$role" "$name" "$char_count" "$tree" >>"$report"
    record_accessibility_assertion "$capture" "text" "$surface" "$role" "$name" "$tree"
}

seed_workspace_for_capture() {
    local capture="$1"
    local data_dir="$ARTIFACT_DIR/captures/$capture/data/lushtext"
    mkdir -p "$data_dir"
    cat >"$data_dir/workspaces.json" <<JSON
{
  "kind": "dev.cominotti.lushtext.workspace-state",
  "version": 1,
  "data": {
    "current_scope": {
      "kind": "all"
    },
    "workspaces": [
      {
        "id": "accessibility-smoke-workspace",
        "name": "Accessibility Smoke",
        "folders": [
          {
            "id": "accessibility-smoke-folder",
            "path": "$ARTIFACT_DIR/fixtures"
          }
        ]
      }
    ]
  }
}
JSON
}

seed_zero_folder_workspace_for_capture() {
    local capture="$1"
    local data_dir="$ARTIFACT_DIR/captures/$capture/data/lushtext"
    mkdir -p "$data_dir"
    cat >"$data_dir/workspaces.json" <<JSON
{
  "kind": "dev.cominotti.lushtext.workspace-state",
  "version": 1,
  "data": {
    "current_scope": {
      "kind": "all"
    },
    "workspaces": [
      {
        "id": "empty-workspace-smoke",
        "name": "Empty Workspace Smoke",
        "folders": []
      }
    ]
  }
}
JSON
}

seed_dense_tree_workspace_for_capture() {
    local capture="$1"
    local data_dir="$ARTIFACT_DIR/captures/$capture/data/lushtext"
    local dense_root="$ARTIFACT_DIR/fixtures/tree-dense"
    local folder_a="$dense_root/Folder 01 - Extremely Long Workspace Folder Name For Accessibility Smoke"
    local folder_b="$dense_root/Folder 02 - Symbols [Draft] And Spaces For Accessibility Smoke"
    local folder_c="$dense_root/Folder 03 - Another Deeply Named Workspace Folder"
    mkdir -p "$data_dir" "$folder_a" "$folder_b" "$folder_c"
    printf 'dense tree alpha\n' >"$folder_a/dense-alpha.txt"
    printf 'dense tree beta\n' >"$folder_b/dense-beta.txt"
    printf 'dense tree gamma\n' >"$folder_c/dense-gamma.txt"
    cat >"$data_dir/workspaces.json" <<JSON
{
  "kind": "dev.cominotti.lushtext.workspace-state",
  "version": 1,
  "data": {
    "current_scope": {
      "kind": "all"
    },
    "workspaces": [
      {
        "id": "dense-tree-smoke",
        "name": "Dense Tree Smoke",
        "folders": [
          {
            "id": "dense-folder-a",
            "path": "$folder_a"
          },
          {
            "id": "dense-folder-b",
            "path": "$folder_b"
          },
          {
            "id": "dense-folder-c",
            "path": "$folder_c"
          }
        ]
      }
    ]
  }
}
JSON
}

seed_deep_tree_workspace_for_capture() {
    local capture="$1"
    local data_dir="$ARTIFACT_DIR/captures/$capture/data/lushtext"
    local root="$ARTIFACT_DIR/fixtures/deep-tree-root"
    local level_one="$root/Level 1 Deep Folder"
    local level_two="$level_one/Level 2 Deep Folder"
    local level_three="$level_two/Level 3 Deep Folder"
    mkdir -p "$data_dir" "$level_three"
    printf 'deep tree leaf\n' >"$level_three/deep-leaf.txt"
    cat >"$data_dir/workspaces.json" <<JSON
{
  "kind": "dev.cominotti.lushtext.workspace-state",
  "version": 1,
  "data": {
    "current_scope": {
      "kind": "all"
    },
    "workspaces": [
      {
        "id": "deep-tree-smoke",
        "name": "Deep Tree Smoke",
        "folders": [
          {
            "id": "deep-tree-root",
            "path": "$root"
          }
        ]
      }
    ]
  }
}
JSON
}

seed_file_peek_workspace_for_capture() {
    local capture="$1"
    local data_dir="$ARTIFACT_DIR/captures/$capture/data/lushtext"
    local root="$ARTIFACT_DIR/fixtures/file-peek-root"
    local peek_file="$root/accessibility-peek.txt"
    mkdir -p "$data_dir" "$root"
    printf 'peek target body\n' >"$peek_file"
    cat >"$data_dir/workspaces.json" <<JSON
{
  "kind": "dev.cominotti.lushtext.workspace-state",
  "version": 1,
  "data": {
    "current_scope": {
      "kind": "all"
    },
    "workspaces": [
      {
        "id": "file-peek-smoke",
        "name": "File Peek Smoke",
        "folders": [
          {
            "id": "file-peek-root",
            "path": "$root"
          }
        ]
      }
    ]
  }
}
JSON
}

seed_dense_workspace_for_capture() {
    local capture="$1"
    local dense_root="$ARTIFACT_DIR/fixtures/dense-results/Deeply Nested Folder With A Long Accessible Path Segment"
    seed_workspace_for_capture "$capture"
    mkdir -p "$dense_root"

    local index
    for index in $(seq 1 16); do
        local padded
        printf -v padded '%02d' "$index"
        printf 'dense-needle match %s in an intentionally long workspace result path\n' "$padded" \
            >"$dense_root/dense-result-${padded}-with-long-name-for-accessibility-smoke.txt"
    done
}

seed_command_palette_dense_workspace_for_capture() {
    local capture="$1"
    local dense_root="$ARTIFACT_DIR/fixtures/palette-dense/Deeply Nested Folder With A Long Palette Path Segment"
    seed_workspace_for_capture "$capture"
    mkdir -p "$dense_root"

    local index
    for index in $(seq 1 40); do
        local padded
        printf -v padded '%02d' "$index"
        printf 'palette dense file %s for accessibility command palette coverage\n' "$padded" \
            >"$dense_root/palette-dense-file-${padded}-with-long-name-for-accessibility-smoke.txt"
    done
}

seed_replace_workspace_for_capture() {
    local capture="$1"
    seed_workspace_for_capture "$capture"
    cat >"$ARTIFACT_DIR/fixtures/replace-target.txt" <<'EOF'
replace-needle alpha
ordinary line between matches
replace-needle beta
EOF
}

seed_recent_documents_for_capture() {
    local capture="$1"
    local recent_root="$ARTIFACT_DIR/fixtures/recent-documents"
    local deep_root="$recent_root/Long Workspace Name With Spaces/Deeply Nested Folder With A Very Long Segment"
    local alpha="$recent_root/Accessibility Recent Report.txt"
    local mixed="$recent_root/Mixed CASE Notes.md"
    local brackets="$recent_root/brackets-[draft]-2026.txt"
    local deep="$deep_root/Long Recent Name With Spaces And Details.txt"
    local zeta="$recent_root/zeta-other-file.txt"
    local data_dir="$ARTIFACT_DIR/captures/$capture/data/lushtext"

    mkdir -p "$deep_root" "$data_dir"
    printf 'recent alpha\n' >"$alpha"
    printf 'recent mixed\n' >"$mixed"
    printf 'recent brackets\n' >"$brackets"
    printf 'recent deep\n' >"$deep"
    printf 'recent zeta\n' >"$zeta"

    cat >"$data_dir/recent-documents.json" <<JSON
{
  "entries": [
    {
      "path": "$alpha",
      "last_opened_secs": 2000000000
    },
    {
      "path": "$mixed",
      "last_opened_secs": 1999999900
    },
    {
      "path": "$brackets",
      "last_opened_secs": 1999999800
    },
    {
      "path": "$deep",
      "last_opened_secs": 1999999700
    },
    {
      "path": "$zeta",
      "last_opened_secs": 1999999600
    }
  ]
}
JSON
}

seed_bookmarks_for_capture() {
    local capture="$1"
    local fixture_path="$2"
    seed_workspace_for_capture "$capture"

    local data_dir="$ARTIFACT_DIR/captures/$capture/data/lushtext"
    mkdir -p "$data_dir"
    /usr/bin/python3 - "$data_dir" "$fixture_path" <<'PY'
import json
import sys
from pathlib import Path

FNV_OFFSET = 0xCBF29CE484222325
FNV_PRIME = 0x00000100000001B3


def stable_hash(data: bytes) -> str:
    value = FNV_OFFSET
    for byte in data:
        value ^= byte
        value = (value * FNV_PRIME) & 0xFFFFFFFFFFFFFFFF
    return f"{value:016x}"


data_dir = Path(sys.argv[1])
display_path = Path(sys.argv[2])
canonical_path = display_path.resolve()
sidecar_id = stable_hash(str(canonical_path).encode())
identity = {
    "display_path": str(display_path),
    "canonical_path": str(canonical_path),
    "sidecar_id": sidecar_id,
}
document = {
    "kind": "dev.cominotti.lushtext.bookmark-sidecar",
    "version": 1,
    "data": {
        "identity": identity,
        "bookmarks": [
            {
                "id": "bookmark-accessibility-smoke-1",
                "line": 0,
                "label": "Smoke Bookmark",
                "created_at_secs": 1,
                "updated_at_secs": 1,
            },
            {
                "id": "bookmark-accessibility-smoke-2",
                "line": 2,
                "label": "Second Smoke Bookmark",
                "created_at_secs": 1,
                "updated_at_secs": 1,
            },
        ],
    },
}
bookmarks_dir = data_dir / "bookmarks"
bookmarks_dir.mkdir(parents=True, exist_ok=True)
(bookmarks_dir / f"{sidecar_id}.json").write_text(
    json.dumps(document, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY
}

seed_local_history_for_capture() {
    local capture="$1"
    local fixture_path="$2"
    local mode="${3:-content}"
    local data_dir="$ARTIFACT_DIR/captures/$capture/data/lushtext"

    mkdir -p "$data_dir"
    /usr/bin/python3 - "$data_dir" "$fixture_path" "$mode" <<'PY'
import json
import sys
from pathlib import Path

FNV_OFFSET = 0xCBF29CE484222325
FNV_PRIME = 0x00000100000001B3


def stable_hash(data: bytes) -> str:
    value = FNV_OFFSET
    for byte in data:
        value ^= byte
        value = (value * FNV_PRIME) & 0xFFFFFFFFFFFFFFFF
    return f"{value:016x}"


data_dir = Path(sys.argv[1])
display_path = Path(sys.argv[2])
canonical_path = display_path.resolve()
mode = sys.argv[3]
sidecar_id = stable_hash(str(canonical_path).encode())
document_dir = data_dir / "local-history" / sidecar_id
document_dir.mkdir(parents=True, exist_ok=True)

if mode == "empty":
    rows = [
        (
            "history-00000000000000000000018bcfe56800-0000000000000001",
            1_700_000_000_000,
            "Baseline",
            "",
        )
    ]
else:
    rows = [
        (
            "history-00000000000000000000018bcff4aa00-0000000000000001",
            1_700_000_001_000,
            "Save",
            "local history saved snapshot\nwith accessible preview text\n",
        )
    ]

snapshots = []
for snapshot_id, captured_at_millis, origin, text in rows:
    (document_dir / f"{snapshot_id}.txt").write_text(text, encoding="utf-8")
    body = text.encode()
    snapshots.append(
        {
            "snapshot_id": snapshot_id,
            "captured_at_millis": captured_at_millis,
            "origin": origin,
            "byte_len": len(body),
            "content_hash": stable_hash(body),
        }
    )

index = {
    "kind": "dev.cominotti.lushtext.local-history-index",
    "version": 1,
    "data": {
        "identity": {
            "display_path": str(display_path),
            "canonical_path": str(canonical_path),
            "sidecar_id": sidecar_id,
        },
        "snapshots": snapshots,
    },
}
(document_dir / "index.json").write_text(json.dumps(index, indent=2) + "\n", encoding="utf-8")
PY
}

FIXTURE="$ARTIFACT_DIR/fixtures/accessibility-smoke.txt"
smoke_create_text_fixture "$FIXTURE"

unset NO_AT_BRIDGE
: >"$ARTIFACT_DIR/assertions/accessibility-anchors.txt"
: >"$ARTIFACT_DIR/assertions/accessibility-focus.txt"
: >"$ARTIFACT_DIR/assertions/accessibility-text.txt"
: >"$ASSERTION_EVENTS"
RUN_CASE_COUNT=0

if case_selected "shell"; then
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
assert_anchor "shell" "in-tab search" "entry" "Find text"
assert_anchor "shell" "in-tab search" "button" "Previous search match"
assert_anchor "shell" "in-tab search" "button" "Next search match"
assert_anchor "shell" "in-tab search" "button" "Close search"
fi

if case_selected "preferences"; then
run_accessibility_capture "preferences" \
    --app-action preferences \
    --wait-atspi-text "Preferences"
assert_anchor "preferences" "preferences" "dialog" "Preferences"
assert_anchor "preferences" "preferences" "grouping" "Preferences"
assert_anchor "preferences" "preferences" "page tab" "Editor"
assert_anchor "preferences" "preferences" "page tab" "Workspace"
assert_anchor "preferences" "preferences" "page tab" "Data"
assert_anchor "preferences" "preferences" "combo box" "Color Scheme"
assert_anchor "preferences" "preferences" "list item" "Background Opacity"
assert_anchor "preferences" "preferences" "grouping" "Tab Width"
assert_anchor "preferences" "preferences" "switch" "Word Wrap"
fi

if case_selected "properties-panel"; then
run_accessibility_capture "properties-panel" \
    --window-bool-action set-properties-visible=true \
    --wait-atspi-text "Document properties"
assert_anchor "properties-panel" "document properties" "grouping" "Document properties"
assert_anchor "properties-panel" "document properties" "list item" "Location"
assert_anchor "properties-panel" "document properties" "list item" "File Size"
assert_anchor "properties-panel" "document properties" "list item" "Statistics"
assert_anchor "properties-panel" "document properties" "list item" "Formatting Source"
assert_anchor "properties-panel" "document properties" "list item" "File Health"
fi

if case_selected "compact-properties"; then
run_accessibility_capture "compact-properties" \
    --width 760 \
    --height 640 \
    --step window-bool-action:set-properties-visible=true \
    --step wait-atspi-text:"Document properties"
assert_anchor "compact-properties" "compact document properties" "grouping" "Document properties"
assert_anchor "compact-properties" "compact document properties" "list item" "Location"
assert_anchor "compact-properties" "compact document properties" "list item" "File Size"
fi

if case_selected "markdown-preview"; then
DEFAULT_FIXTURE="$FIXTURE"
FIXTURE="$REPO_ROOT/samples/markdown-test.md"
run_accessibility_capture "markdown-preview" \
    --window-bool-action set-preview-mode=true \
    --wait-predicate visual-geometry-settled \
    --wait-atspi-text "Rendered Markdown content"
FIXTURE="$DEFAULT_FIXTURE"
assert_anchor "markdown-preview" "markdown preview" "document text" "Markdown preview"
assert_anchor "markdown-preview" "markdown preview" "scroll pane" "Markdown preview scroll area"
assert_anchor "markdown-preview" "markdown preview" "text" "Rendered Markdown content"
assert_text_interface "markdown-preview" "markdown preview" "text" "Rendered Markdown content" 100
assert_anchor "markdown-preview" "markdown preview" "grouping" "Markdown rust code block"
assert_anchor "markdown-preview" "markdown preview" "text" "Markdown rust code block"
assert_anchor "markdown-preview" "markdown preview" "table" "Markdown table"
assert_anchor "markdown-preview" "markdown preview" "table cell" "Table cell Headings"
assert_anchor "markdown-preview" "markdown preview" "image" "Markdown image: Image could not be loaded"
assert_anchor "markdown-preview" "markdown preview" "image" "Markdown image: Remote images are not supported"
fi

if case_selected "preview-mode-transition"; then
DEFAULT_FIXTURE="$FIXTURE"
FIXTURE="$REPO_ROOT/samples/markdown-test.md"
run_accessibility_capture "preview-mode-transition" \
    --step window-bool-action:set-preview-mode=true \
    --step wait-atspi-text:"Rendered Markdown content"
FIXTURE="$DEFAULT_FIXTURE"
assert_anchor "preview-mode-transition" "markdown preview" "document text" "Markdown preview"
assert_anchor "preview-mode-transition" "markdown preview" "scroll pane" "Markdown preview scroll area"
assert_anchor "preview-mode-transition" "markdown preview" "text" "Rendered Markdown content"
assert_text_interface "preview-mode-transition" "markdown preview" "text" "Rendered Markdown content" 100
fi

if case_selected "editor"; then
run_accessibility_capture "editor" \
    --wait-atspi-text "Editor for accessibility-smoke.txt"
assert_anchor "editor" "editor" "text" "Editor for accessibility-smoke.txt"
record_focus_anchor "editor" "Editor for accessibility-smoke.txt"
assert_text_interface "editor" "editor" "text" "Editor for accessibility-smoke.txt" 1
fi

if case_selected "focus-mode"; then
run_accessibility_capture "focus-mode" \
    --step window-bool-action:set-focus-mode=true \
    --step wait-predicate:visual-geometry-settled \
    --step wait-atspi-text:"Editor for accessibility-smoke.txt"
assert_anchor "focus-mode" "focus mode" "text" "Editor for accessibility-smoke.txt"
record_focus_anchor "focus-mode" "Editor for accessibility-smoke.txt"
fi

if case_selected "minimap-transition"; then
run_accessibility_capture "minimap-transition" \
    --enable-minimap \
    --step wait-window-action:cycle-invisible-characters \
    --step window-action:cycle-invisible-characters \
    --step window-bool-action:set-minimap-visible=true \
    --step wait-predicate:visual-geometry-settled \
    --step wait-atspi-text:"Editor for accessibility-smoke.txt"
assert_anchor "minimap-transition" "minimap transition" "text" "Editor for accessibility-smoke.txt"
assert_text_interface "minimap-transition" "minimap transition" "text" "Editor for accessibility-smoke.txt" 1
fi

if case_selected "editor-search"; then
run_accessibility_capture "editor-search" \
    --search needle \
    --expected-search-matches 3 \
    --step wait-atspi-text:"Search match count"
assert_anchor "editor-search" "in-tab search" "entry" "Find text"
assert_anchor "editor-search" "in-tab search" "status bar" "Search match count"
assert_anchor "editor-search" "in-tab search" "button" "Previous search match"
assert_anchor "editor-search" "in-tab search" "button" "Next search match"
assert_anchor "editor-search" "in-tab search" "button" "Close search"
assert_text_interface "editor-search" "in-tab search" "entry" "Find text" 6
fi

if case_selected "editor-save-completion"; then
run_accessibility_capture "editor-save-completion" \
    --step atspi-set-editor-text:"save completion body" \
    --step wait-window-action:save \
    --step window-action:save \
    --step wait-predicate:save-complete \
    --step wait-atspi-text:"File saved"
assert_anchor "editor-save-completion" "editor" "text" "Editor for accessibility-smoke.txt"
assert_anchor "editor-save-completion" "status bar" "label" "File saved"
assert_text_interface "editor-save-completion" "editor" "text" "Editor for accessibility-smoke.txt" 1
smoke_create_text_fixture "$FIXTURE"
fi

if case_selected "editor-failed-load"; then
DEFAULT_FIXTURE="$FIXTURE"
FIXTURE="$ARTIFACT_DIR/fixtures/unreadable-load-target.txt"
printf 'unreadable fixture\n' >"$FIXTURE"
chmod 000 "$FIXTURE"
run_accessibility_capture "editor-failed-load" \
    --allow-file-open-failure \
    --step wait-atspi-text:"Could Not Open File"
chmod 600 "$FIXTURE" || true
FIXTURE="$DEFAULT_FIXTURE"
assert_anchor_prefix "editor-failed-load" "editor error" "alert" "Could Not Open File:"
assert_anchor "editor-failed-load" "editor error" "button" "Retry"
assert_anchor "editor-failed-load" "editor" "text" "Editor for unreadable-load-target.txt"
fi

if case_selected "editor-too-large-policy"; then
DEFAULT_FIXTURE="$FIXTURE"
FIXTURE="$ARTIFACT_DIR/fixtures/too-large-accessibility-smoke.txt"
rm -f "$FIXTURE"
truncate -s 501M "$FIXTURE"
run_accessibility_capture "editor-too-large-policy" \
    --allow-file-open-failure \
    --step wait-atspi-text:"Could Not Open File" \
    --step wait-atspi-text:"too large to edit"
FIXTURE="$DEFAULT_FIXTURE"
assert_anchor_prefix "editor-too-large-policy" "editor error" "alert" "Could Not Open File:"
assert_anchor "editor-too-large-policy" "editor" "text" "Editor for too-large-accessibility-smoke.txt"
fi

if case_selected "workspace-search-no-workspace"; then
run_accessibility_capture "workspace-search-no-workspace" \
    --step window-action:toggle-search-panel \
    --step wait-window-action:set-search-panel-query \
    --step window-string-action:set-search-panel-query=orphan-needle \
    --step wait-predicate:search-complete \
    --step wait-atspi-text:"No workspace folders"
assert_anchor "workspace-search-no-workspace" "workspace search" "entry" "Workspace search query"
assert_anchor "workspace-search-no-workspace" "workspace search" "status bar" "No workspace folders"
fi

if case_selected "workspace-search"; then
seed_workspace_for_capture "workspace-search"
run_accessibility_capture "workspace-search" \
    --window-action toggle-search-panel \
    --wait-window-action set-search-panel-query \
    --window-string-action set-search-panel-query=needle \
    --wait-predicate search-complete \
    --wait-atspi-text "Workspace search query"
assert_anchor "workspace-search" "workspace search" "entry" "Workspace search query"
assert_anchor "workspace-search" "workspace search" "list" "Workspace search results"
assert_anchor "workspace-search" "workspace search" "status bar" "3 results in 1 files"
assert_anchor "workspace-search" "workspace search" "button" "Save search"
fi

if case_selected "workspace-search-no-results"; then
seed_workspace_for_capture "workspace-search-no-results"
run_accessibility_capture "workspace-search-no-results" \
    --window-action toggle-search-panel \
    --wait-window-action set-search-panel-query \
    --window-string-action set-search-panel-query=no-such-accessibility-term \
    --wait-predicate search-complete \
    --wait-atspi-text "No results found"
assert_anchor "workspace-search-no-results" "workspace search" "entry" "Workspace search query"
assert_anchor "workspace-search-no-results" "workspace search" "status bar" "No results found"
fi

if case_selected "workspace-search-dense-constrained"; then
seed_dense_workspace_for_capture "workspace-search-dense-constrained"
run_accessibility_capture "workspace-search-dense-constrained" \
    --width 760 \
    --height 640 \
    --step window-action:toggle-search-panel \
    --step wait-window-action:set-search-panel-query \
    --step window-string-action:set-search-panel-query=dense-needle \
    --step wait-predicate:search-complete \
    --step wait-atspi-text:"16 results in 16 files"
assert_anchor "workspace-search-dense-constrained" "workspace search" "entry" "Workspace search query"
assert_anchor "workspace-search-dense-constrained" "workspace search" "list" "Workspace search results"
assert_anchor "workspace-search-dense-constrained" "workspace search" "status bar" "16 results in 16 files"
fi

if case_selected "workspace-search-replace-undo"; then
seed_replace_workspace_for_capture "workspace-search-replace-undo"
run_accessibility_capture "workspace-search-replace-undo" \
    --step window-action:toggle-search-panel \
    --step wait-window-action:set-search-panel-query \
    --step window-string-action:set-search-panel-query=replace-needle \
    --step wait-predicate:search-complete \
    --step wait-atspi-text:"2 results in 1 files" \
    --step window-string-action:set-search-panel-replace-query=after-needle \
    --step wait-window-action:preview-search-panel-replacements \
    --step window-action:preview-search-panel-replacements \
    --step wait-predicate:search-complete \
    --step wait-atspi-text:"Include replacement at line 1" \
    --step window-action:confirm-search-panel-replacements \
    --step wait-atspi-text:"Replaced 2 of 2 matches in 1 files" \
    --step wait-atspi-text:"Undo replacements"
assert_anchor "workspace-search-replace-undo" "workspace search" "entry" "Workspace search query"
assert_anchor "workspace-search-replace-undo" "workspace search" "text" "Workspace replacement text"
assert_anchor "workspace-search-replace-undo" "workspace search" "button" "Undo replacements"
assert_anchor "workspace-search-replace-undo" "workspace search" "label" "Replaced 2 of 2 matches in 1 files"
fi

if case_selected "workspace-tree-no-workspace"; then
run_accessibility_capture "workspace-tree-no-workspace" \
    --wait-atspi-text "New Workspace"
assert_anchor "workspace-tree-no-workspace" "workspace sidebar" "combo box" "All workspaces"
assert_anchor "workspace-tree-no-workspace" "workspace sidebar" "button" "New Workspace"
fi

if case_selected "workspace-tree"; then
seed_workspace_for_capture "workspace-tree"
run_accessibility_capture "workspace-tree" \
    --wait-atspi-text "Workspace file tree"
assert_anchor "workspace-tree" "workspace sidebar" "grouping" "Workspace Accessibility Smoke"
assert_anchor "workspace-tree" "workspace sidebar" "button" "Add folder"
assert_anchor "workspace-tree" "workspace sidebar" "button" "Refresh Workspace Folders"
assert_anchor "workspace-tree" "workspace sidebar" "list" "Workspace file tree"
assert_anchor "workspace-tree" "workspace sidebar" "list item" "Folder fixtures"
fi

if case_selected "workspace-tree-zero-folder"; then
seed_zero_folder_workspace_for_capture "workspace-tree-zero-folder"
run_accessibility_capture "workspace-tree-zero-folder" \
    --wait-atspi-text "No folders in this workspace"
assert_anchor "workspace-tree-zero-folder" "workspace sidebar" "grouping" "Workspace Empty Workspace Smoke"
assert_anchor "workspace-tree-zero-folder" "workspace sidebar" "button" "Add folder"
assert_anchor "workspace-tree-zero-folder" "workspace sidebar" "button" "Refresh Workspace Folders"
assert_anchor "workspace-tree-zero-folder" "workspace sidebar" "status bar" "No folders in this workspace"
fi

if case_selected "workspace-tree-dense-constrained"; then
seed_dense_tree_workspace_for_capture "workspace-tree-dense-constrained"
run_accessibility_capture "workspace-tree-dense-constrained" \
    --width 900 \
    --height 640 \
    --wait-atspi-text "Folder 01 - Extremely Long Workspace Folder Name For Accessibility Smoke"
assert_anchor "workspace-tree-dense-constrained" "workspace sidebar" "grouping" "Workspace Dense Tree Smoke"
assert_anchor "workspace-tree-dense-constrained" "workspace sidebar" "list" "Workspace file tree"
assert_anchor "workspace-tree-dense-constrained" "workspace sidebar" "list item" "Folder Folder 01 - Extremely Long Workspace Folder Name For Accessibility Smoke"
assert_anchor "workspace-tree-dense-constrained" "workspace sidebar" "list item" "Folder Folder 02 - Symbols [Draft] And Spaces For Accessibility Smoke"
fi

if case_selected "workspace-tree-deep-expanded"; then
seed_deep_tree_workspace_for_capture "workspace-tree-deep-expanded"
run_accessibility_capture "workspace-tree-deep-expanded" \
    --step wait-atspi-text:"deep-tree-root" \
    --step atspi-click-button:"^deep-tree-root$" \
    --step wait-atspi-text:"Level 1 Deep Folder" \
    --step atspi-click-button:"^Level 1 Deep Folder$" \
    --step wait-atspi-text:"Level 2 Deep Folder"
assert_anchor "workspace-tree-deep-expanded" "workspace sidebar" "grouping" "Workspace Deep Tree Smoke"
assert_anchor "workspace-tree-deep-expanded" "workspace sidebar" "list" "Workspace file tree"
assert_anchor "workspace-tree-deep-expanded" "workspace sidebar" "list item" "Folder deep-tree-root"
assert_anchor "workspace-tree-deep-expanded" "workspace sidebar" "list item" "Folder Level 1 Deep Folder"
assert_anchor "workspace-tree-deep-expanded" "workspace sidebar" "list item" "Folder Level 2 Deep Folder"
fi

if case_selected "workspace-tree-file-peek"; then
seed_file_peek_workspace_for_capture "workspace-tree-file-peek"
run_accessibility_capture "workspace-tree-file-peek" \
    --step wait-atspi-text:"file-peek-root" \
    --step atspi-click-button:"^file-peek-root$" \
    --step wait-atspi-text:"accessibility-peek.txt"
assert_anchor "workspace-tree-file-peek" "workspace sidebar" "grouping" "Workspace File Peek Smoke"
assert_anchor "workspace-tree-file-peek" "workspace sidebar" "list" "Workspace file tree"
assert_anchor "workspace-tree-file-peek" "workspace sidebar" "list item" "Folder file-peek-root"
assert_anchor "workspace-tree-file-peek" "workspace sidebar" "list item" "File accessibility-peek.txt"
fi

if case_selected "workspace-tree-folder-context-menu"; then
seed_workspace_for_capture "workspace-tree-folder-context-menu"
run_accessibility_capture "workspace-tree-folder-context-menu" \
    --step wait-atspi-text:"Folder fixtures" \
    --step window-action:focus-workspace-tree \
    --step window-action:show-workspace-tree-context-menu \
    --step wait-atspi-text:"Move Up"
assert_anchor "workspace-tree-folder-context-menu" "workspace context menu" "menu" "Workspace folder actions for fixtures"
assert_anchor "workspace-tree-folder-context-menu" "workspace context menu" "menu item" "Open Folder Note…"
assert_anchor "workspace-tree-folder-context-menu" "workspace context menu" "menu item" "Move Up"
assert_anchor "workspace-tree-folder-context-menu" "workspace context menu" "menu item" "Move Down"
assert_anchor "workspace-tree-folder-context-menu" "workspace context menu" "menu item" "Remove from Workspace"
assert_anchor "workspace-tree-folder-context-menu" "workspace context menu" "menu item" "New File"
assert_anchor "workspace-tree-folder-context-menu" "workspace context menu" "menu item" "New Folder"
fi

if case_selected "workspace-header-context-menu"; then
seed_workspace_for_capture "workspace-header-context-menu"
run_accessibility_capture "workspace-header-context-menu" \
    --step wait-atspi-text:"Workspace Accessibility Smoke" \
    --step window-action:focus-workspace-header \
    --step window-action:show-workspace-header-context-menu \
    --step wait-atspi-text:"Rename Workspace"
assert_anchor "workspace-header-context-menu" "workspace context menu" "menu" "Workspace context menu"
assert_anchor "workspace-header-context-menu" "workspace context menu" "menu item" "Add Folder…"
assert_anchor "workspace-header-context-menu" "workspace context menu" "menu item" "Open Folder Note…"
assert_anchor "workspace-header-context-menu" "workspace context menu" "menu item" "Rename Workspace"
assert_anchor "workspace-header-context-menu" "workspace context menu" "menu item" "Remove Workspace"
fi

if case_selected "open-popover-empty"; then
run_accessibility_capture "open-popover-empty" \
    --window-action open-recent \
    --wait-atspi-text "Recent documents search"
assert_anchor "open-popover-empty" "open popover" "entry" "Recent documents search"
assert_anchor "open-popover-empty" "open popover" "button" "Open another file"
assert_anchor "open-popover-empty" "open popover" "status bar" "No recent documents"
fi

if case_selected "open-popover-dense"; then
seed_recent_documents_for_capture "open-popover-dense"
run_accessibility_capture "open-popover-dense" \
    --width 760 \
    --height 640 \
    --window-action open-recent \
    --wait-window-action set-open-popover-query \
    --wait-atspi-text "Accessibility Recent Report.txt"
assert_anchor "open-popover-dense" "open popover" "entry" "Recent documents search"
assert_anchor "open-popover-dense" "open popover" "list" "Recent documents"
assert_anchor "open-popover-dense" "open popover" "label" "Accessibility Recent Report.txt"
assert_anchor "open-popover-dense" "open popover" "button" "Remove Accessibility Recent Report.txt from recent documents"
fi

if case_selected "open-popover-filtered"; then
seed_recent_documents_for_capture "open-popover-filtered"
run_accessibility_capture "open-popover-filtered" \
    --width 760 \
    --height 640 \
    --window-action open-recent \
    --wait-window-action set-open-popover-query \
    --window-string-action set-open-popover-query=Long \
    --wait-atspi-text "Long Recent Name With Spaces And Details.txt"
assert_anchor "open-popover-filtered" "open popover" "entry" "Recent documents search"
assert_anchor "open-popover-filtered" "open popover" "list" "Recent documents"
assert_anchor "open-popover-filtered" "open popover" "label" "Long Recent Name With Spaces And Details.txt"
assert_anchor "open-popover-filtered" "open popover" "button" "Remove Long Recent Name With Spaces And Details.txt from recent documents"
fi

if case_selected "open-popover-no-match"; then
seed_recent_documents_for_capture "open-popover-no-match"
run_accessibility_capture "open-popover-no-match" \
    --window-action open-recent \
    --wait-window-action set-open-popover-query \
    --window-string-action set-open-popover-query=no-such-recent-document \
    --wait-atspi-text "No matching recent documents"
assert_anchor "open-popover-no-match" "open popover" "entry" "Recent documents search"
assert_anchor "open-popover-no-match" "open popover" "status bar" "No matching recent documents"
fi

if case_selected "open-popover-dismiss"; then
seed_recent_documents_for_capture "open-popover-dismiss"
run_accessibility_capture "open-popover-dismiss" \
    --step window-action:open-recent \
    --step wait-atspi-text:"Recent documents search" \
    --step atspi-key:Escape \
    --step wait-atspi-text:"Editor for accessibility-smoke.txt"
assert_anchor "open-popover-dismiss" "editor" "text" "Editor for accessibility-smoke.txt"
record_focus_anchor "open-popover-dismiss" "Editor for accessibility-smoke.txt"
fi

if case_selected "command-palette"; then
run_accessibility_capture "command-palette" \
    --window-action toggle-command-palette \
    --wait-window-action set-command-palette-query \
    --window-string-action set-command-palette-mode=files \
    --window-string-action set-command-palette-query=accessibility-smoke \
    --wait-atspi-text "Command palette query"
assert_anchor "command-palette" "command palette" "entry" "Command palette query"
assert_anchor "command-palette" "command palette" "list" "Command palette results"
assert_anchor "command-palette" "command palette" "combo box" "Files"
assert_anchor "command-palette" "command palette" "label" "Open Tabs"
assert_anchor "command-palette" "command palette" "label" "accessibility-smoke.txt"
record_focus_anchor "command-palette" "Command palette query"
fi

if case_selected "command-palette-commands"; then
run_accessibility_capture "command-palette-commands" \
    --step window-action:toggle-command-palette \
    --step wait-window-action:set-command-palette-query \
    --step window-string-action:set-command-palette-mode=commands \
    --step window-string-action:set-command-palette-query=focus \
    --step wait-atspi-text:"Focus Mode"
assert_anchor "command-palette-commands" "command palette" "entry" "Command palette query"
assert_anchor "command-palette-commands" "command palette" "list" "Command palette results"
assert_anchor "command-palette-commands" "command palette" "combo box" "Commands"
assert_anchor "command-palette-commands" "command palette" "label" "Commands"
assert_anchor "command-palette-commands" "command palette" "label" "Focus Mode"
record_focus_anchor "command-palette-commands" "Command palette query"
fi

if case_selected "command-palette-notes"; then
seed_workspace_for_capture "command-palette-notes"
run_accessibility_capture "command-palette-notes" \
    --step wait-window-action:toggle-bookmark \
    --step window-action:toggle-bookmark \
    --step window-action:toggle-command-palette \
    --step wait-window-action:set-command-palette-query \
    --step window-string-action:set-command-palette-mode=notes \
    --step window-string-action:set-command-palette-query=bookmark \
    --step wait-atspi-text:"Bookmark · Line 1"
assert_anchor "command-palette-notes" "command palette" "entry" "Command palette query"
assert_anchor "command-palette-notes" "command palette" "list" "Command palette results"
assert_anchor "command-palette-notes" "command palette" "combo box" "Notes"
assert_anchor "command-palette-notes" "command palette" "label" "Bookmarks"
assert_anchor "command-palette-notes" "command palette" "label" "Bookmark · Line 1"
record_focus_anchor "command-palette-notes" "Command palette query"
fi

if case_selected "command-palette-dense-files"; then
seed_command_palette_dense_workspace_for_capture "command-palette-dense-files"
run_accessibility_capture "command-palette-dense-files" \
    --width 760 \
    --height 640 \
    --step window-action:toggle-command-palette \
    --step wait-window-action:set-command-palette-query \
    --step window-string-action:set-command-palette-mode=files \
    --step window-string-action:set-command-palette-query=palette-dense \
    --step wait-atspi-text:"palette-dense-file-01-with-long-name-for-accessibility-smoke.txt"
assert_anchor "command-palette-dense-files" "command palette" "entry" "Command palette query"
assert_anchor "command-palette-dense-files" "command palette" "list" "Command palette results"
assert_anchor "command-palette-dense-files" "command palette" "combo box" "Files"
assert_anchor "command-palette-dense-files" "command palette" "label" "All Workspaces"
assert_anchor "command-palette-dense-files" "command palette" "label" "palette-dense-file-01-with-long-name-for-accessibility-smoke.txt"
record_focus_anchor "command-palette-dense-files" "Command palette query"
fi

if case_selected "command-palette-mode-changes"; then
run_accessibility_capture "command-palette-mode-changes" \
    --step window-action:toggle-command-palette \
    --step wait-window-action:set-command-palette-query \
    --step window-string-action:set-command-palette-mode=files \
    --step window-string-action:set-command-palette-query=accessibility-smoke \
    --step wait-atspi-text:"accessibility-smoke.txt" \
    --step window-string-action:set-command-palette-mode=all \
    --step window-string-action:set-command-palette-query=focus \
    --step wait-atspi-text:"Commands" \
    --step window-string-action:set-command-palette-mode=commands \
    --step wait-atspi-text:"Focus Mode"
assert_anchor "command-palette-mode-changes" "command palette" "entry" "Command palette query"
assert_anchor "command-palette-mode-changes" "command palette" "list" "Command palette results"
assert_anchor "command-palette-mode-changes" "command palette" "combo box" "Commands"
assert_anchor "command-palette-mode-changes" "command palette" "label" "Commands"
assert_anchor "command-palette-mode-changes" "command palette" "label" "Focus Mode"
record_focus_anchor "command-palette-mode-changes" "Command palette query"
fi

if case_selected "command-palette-focus-restore"; then
run_accessibility_capture "command-palette-focus-restore" \
    --step window-action:toggle-command-palette \
    --step wait-window-action:set-command-palette-query \
    --step window-string-action:set-command-palette-query=focus \
    --step wait-atspi-text:"Command palette query" \
    --step window-action:toggle-command-palette \
    --step wait-atspi-text:"Editor for accessibility-smoke.txt"
assert_anchor "command-palette-focus-restore" "editor" "text" "Editor for accessibility-smoke.txt"
record_focus_anchor "command-palette-focus-restore" "Editor for accessibility-smoke.txt"
fi

if case_selected "command-palette-no-results"; then
run_accessibility_capture "command-palette-no-results" \
    --window-action toggle-command-palette \
    --wait-window-action set-command-palette-query \
    --window-string-action set-command-palette-mode=commands \
    --window-string-action set-command-palette-query=no-such-command \
    --wait-atspi-text "Command palette no results"
assert_anchor "command-palette-no-results" "command palette" "entry" "Command palette query"
assert_anchor "command-palette-no-results" "command palette" "status bar" "Command palette no results"
record_focus_anchor "command-palette-no-results" "Command palette query"
fi

if case_selected "notes-empty"; then
run_accessibility_capture "notes-empty" \
    --window-action show-notes \
    --wait-atspi-text "No notes yet"
assert_anchor "notes-empty" "notes browser" "dialog" "Notes"
assert_anchor "notes-empty" "notes browser" "status bar" "No notes yet"
assert_anchor "notes-empty" "notes browser" "button" "Close"
fi

if case_selected "notes-populated"; then
seed_workspace_for_capture "notes-populated"
run_accessibility_capture "notes-populated" \
    --step wait-window-action:toggle-bookmark \
    --step window-action:toggle-bookmark \
    --step window-action:show-notes \
    --step wait-atspi-text:"Bookmark · Line 1"
assert_anchor "notes-populated" "notes browser" "dialog" "Notes"
assert_anchor "notes-populated" "notes browser" "entry" "Search notes"
assert_anchor "notes-populated" "notes browser" "list" "Notes results"
assert_anchor "notes-populated" "notes browser" "button" "Open"
assert_anchor "notes-populated" "notes browser" "button" "Close"
assert_anchor "notes-populated" "notes browser" "label" "Bookmark · Line 1"
assert_anchor "notes-populated" "notes browser" "grouping" "Notes preview"
assert_anchor "notes-populated" "notes browser" "text" "Bookmark source preview"
assert_text_interface "notes-populated" "notes browser" "text" "Bookmark source preview" 1
fi

if case_selected "notes-no-results"; then
seed_workspace_for_capture "notes-no-results"
run_accessibility_capture "notes-no-results" \
    --step wait-window-action:toggle-bookmark \
    --step window-action:toggle-bookmark \
    --step window-action:show-notes \
    --step wait-window-action:set-notes-browser-query \
    --step window-string-action:set-notes-browser-query=no-such-accessibility-note \
    --step wait-atspi-text:"No notes match that search"
assert_anchor "notes-no-results" "notes browser" "dialog" "Notes"
assert_anchor "notes-no-results" "notes browser" "entry" "Search notes"
assert_anchor "notes-no-results" "notes browser" "status bar" "No notes match that search"
assert_anchor "notes-no-results" "notes browser" "button" "Close"
fi

if case_selected "bookmarks-populated"; then
seed_bookmarks_for_capture "bookmarks-populated" "$FIXTURE"
run_accessibility_capture "bookmarks-populated" \
    --step wait-window-action:show-bookmarks \
    --step window-action:show-bookmarks \
    --step wait-atspi-text:"Smoke Bookmark"
assert_anchor "bookmarks-populated" "bookmarks browser" "dialog" "Bookmarks"
assert_anchor "bookmarks-populated" "bookmarks browser" "entry" "Search bookmarks"
assert_anchor "bookmarks-populated" "bookmarks browser" "list" "Bookmark results"
assert_anchor "bookmarks-populated" "bookmarks browser" "button" "Open bookmark Smoke Bookmark"
assert_anchor "bookmarks-populated" "bookmarks browser" "label" "Smoke Bookmark"
fi

if case_selected "local-history-empty"; then
run_accessibility_capture "local-history-empty" \
    --step wait-window-action:show-local-history \
    --step window-action:show-local-history \
    --step wait-atspi-text:"No local history yet"
assert_anchor "local-history-empty" "local history" "dialog" "Local History"
assert_anchor "local-history-empty" "local history" "status bar" "No local history yet"
fi

if case_selected "local-history"; then
seed_local_history_for_capture "local-history" "$FIXTURE"
run_accessibility_capture "local-history" \
    --step wait-window-action:show-local-history \
    --step window-action:show-local-history \
    --step wait-atspi-text:"local history saved snapshot"
assert_anchor "local-history" "local history" "dialog" "Local History"
assert_anchor "local-history" "local history" "list" "Local history snapshots"
assert_anchor "local-history" "local history" "grouping" "Local history preview"
assert_anchor "local-history" "local history" "text" "Snapshot text preview"
assert_text_interface "local-history" "local history" "text" "Snapshot text preview" 1
assert_anchor "local-history" "local history" "button" "Copy"
assert_anchor "local-history" "local history" "button" "Restore"
fi

if case_selected "local-history-restore"; then
seed_local_history_for_capture "local-history-restore" "$FIXTURE"
run_accessibility_capture "local-history-restore" \
    --step wait-window-action:show-local-history \
    --step window-action:show-local-history \
    --step wait-atspi-text:"local history saved snapshot" \
    --step atspi-click-button:"^Restore$" \
    --step wait-atspi-text:"Restored from Local History"
assert_anchor_prefix "local-history-restore" "local history restore" "alert" "Restored from Local History"
assert_anchor "local-history-restore" "local history restore" "button" "Undo Restore"
assert_anchor "local-history-restore" "local history restore" "button" "Dismiss"
assert_anchor "local-history-restore" "editor" "text" "Editor for accessibility-smoke.txt"
assert_text_interface "local-history-restore" "editor" "text" "Editor for accessibility-smoke.txt" 1
fi

if case_selected "local-history-empty-snapshot"; then
seed_local_history_for_capture "local-history-empty-snapshot" "$FIXTURE" empty
run_accessibility_capture "local-history-empty-snapshot" \
    --step wait-window-action:show-local-history \
    --step window-action:show-local-history \
    --step wait-atspi-text:"This snapshot was empty"
assert_anchor "local-history-empty-snapshot" "local history" "dialog" "Local History"
assert_anchor "local-history-empty-snapshot" "local history" "list" "Local history snapshots"
assert_anchor "local-history-empty-snapshot" "local history" "grouping" "Local history preview"
assert_anchor "local-history-empty-snapshot" "local history" "status bar" "This snapshot was empty"
assert_anchor "local-history-empty-snapshot" "local history" "button" "Copy"
assert_anchor "local-history-empty-snapshot" "local history" "button" "Restore"
fi

if case_selected "unsaved-close-dialog"; then
run_accessibility_capture "unsaved-close-dialog" \
    --step atspi-set-editor-text:"unsaved close body" \
    --step wait-window-action:close-tab \
    --step window-action:close-tab \
    --step wait-atspi-text:"Save Changes?"
assert_anchor "unsaved-close-dialog" "save changes dialog" "alert" "Save Changes?"
assert_anchor "unsaved-close-dialog" "save changes dialog" "label" "Open documents contain unsaved changes. Changes which are not saved will be permanently lost."
assert_anchor "unsaved-close-dialog" "save changes dialog" "check box" "Save accessibility-smoke.txt"
assert_anchor "unsaved-close-dialog" "save changes dialog" "button" "Cancel"
assert_anchor "unsaved-close-dialog" "save changes dialog" "button" "Discard"
assert_anchor "unsaved-close-dialog" "save changes dialog" "button" "Save"
fi

if case_selected "discard-confirmation"; then
run_accessibility_capture "discard-confirmation" \
    --step atspi-set-editor-text:"discard dialog body" \
    --step wait-window-action:discard-changes \
    --step window-action:discard-changes \
    --step wait-atspi-text:"Discard Changes"
assert_anchor "discard-confirmation" "discard confirmation" "alert" "Discard Changes to “accessibility-smoke.txt”?"
assert_anchor "discard-confirmation" "discard confirmation" "label" "Unsaved changes will be permanently lost."
assert_anchor "discard-confirmation" "discard confirmation" "button" "Cancel"
assert_anchor "discard-confirmation" "discard confirmation" "button" "Discard"
fi

if ((RUN_CASE_COUNT == 0)); then
    write_accessibility_summary_json "failed" "no accessibility smoke cases matched filter '${ACCESSIBILITY_CASE_FILTERS}'"
    smoke_fail "no accessibility smoke cases matched filter '${ACCESSIBILITY_CASE_FILTERS}'"
fi

collect_accessibility_warnings "$WARNINGS_OUTPUT"
if /usr/bin/python3 - "$WARNINGS_OUTPUT" "$ARTIFACT_DIR/assertions/unexpected-warnings.txt" <<'PY'
import sys
from pathlib import Path

warnings_path = Path(sys.argv[1])
unexpected_path = Path(sys.argv[2])

def warning_line_is_allowlisted(line: str) -> bool:
    if line.startswith("Gdk-Message: ") and line.endswith("Error reading events from display: Broken pipe"):
        return True
    return (
        "ERROR lushtext_core::ui::editor_page::load_save: Failed to read " in line
        and "unreadable-load-target.txt: Permission denied" in line
    )

lines = warnings_path.read_text(encoding="utf-8", errors="replace").splitlines()
unexpected = [line for line in lines if not warning_line_is_allowlisted(line)]
unexpected_path.write_text(
    "".join(f"{line}\n" for line in unexpected),
    encoding="utf-8",
)
sys.exit(0 if unexpected else 1)
PY
then
    write_accessibility_summary_json "failed" "unexpected accessibility warnings"
    smoke_fail "accessibility smoke found unexpected warnings. Artifacts: $ARTIFACT_DIR"
fi
{
    echo "status=passed"
    echo "screenshots=$ARTIFACT_DIR/*.png"
    echo "atspi_trees=$ARTIFACT_DIR/assertions/*-atspi-tree.txt"
    echo "atspi_focus=$ARTIFACT_DIR/assertions/*-atspi-focus.txt"
    echo "anchors=$ARTIFACT_DIR/assertions/accessibility-anchors.txt"
    echo "focus_assertions=$ARTIFACT_DIR/assertions/accessibility-focus.txt"
    echo "text_assertions=$ARTIFACT_DIR/assertions/accessibility-text.txt"
    echo "warnings=$WARNINGS_OUTPUT"
    echo "session_logs=$ARTIFACT_DIR/*.session.log"
    echo "capture_artifacts=$ARTIFACT_DIR/captures"
    echo "environment=$ARTIFACT_DIR/environment.txt"
} >"$ARTIFACT_DIR/summary.txt"
write_accessibility_summary_json "passed"
echo "PASS: accessibility smoke verified AT-SPI anchors and focus artifacts under $ARTIFACT_DIR"
