#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/smoke-common.sh
source "$REPO_ROOT/scripts/smoke-common.sh"

ARTIFACT_DIR="${LUSHTEXT_COMMAND_PALETTE_NOTES_ARTIFACT_DIR:-build/smoke/command-palette-notes}"
BINARY="$REPO_ROOT/target/debug/lushtext"
QUERY="${COMMAND_PALETTE_NOTES_QUERY:-palette}"

usage() {
    cat <<'EOF'
Usage: scripts/run-command-palette-notes-smoke.sh [--artifact-dir DIR] [--binary PATH] [--query TEXT]

Launch one isolated headless LushText session with representative bookmark,
folder-note, document-note, and open-tab note fixtures, then capture the
command palette's Notes category.
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
        --query)
            [[ $# -lt 2 ]] && smoke_fail "--query requires a value"
            QUERY="$2"
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

smoke_require_command dbus-run-session
smoke_require_command gdbus
smoke_require_command gsettings
smoke_require_command gst-launch-1.0
smoke_require_command mutter
smoke_require_command pipewire
smoke_require_command pw-dump
smoke_require_command wireplumber

[[ -x /usr/bin/python3 ]] || smoke_skip "/usr/bin/python3 is not available."
[[ -x "$BINARY" ]] || smoke_skip "LushText debug binary is missing. Run 'make build-debug' first."

ARTIFACT_DIR="$(smoke_artifact_dir "$ARTIFACT_DIR")"
CAPTURE_NAME="command-palette-notes-all"
CAPTURE_DIR="$ARTIFACT_DIR/captures/$CAPTURE_NAME"
SCREENSHOT="$ARTIFACT_DIR/screenshots/$CAPTURE_NAME.png"
ATSPI_TREE="$ARTIFACT_DIR/assertions/$CAPTURE_NAME-atspi-tree.txt"
ATSPI_FOCUS="$ARTIFACT_DIR/assertions/$CAPTURE_NAME-atspi-focus.txt"
ASSERTION_SUMMARY="$ARTIFACT_DIR/assertions/$CAPTURE_NAME-summary.txt"

rm -rf "$ARTIFACT_DIR/captures" "$ARTIFACT_DIR/screenshots" "$ARTIFACT_DIR/assertions"
mkdir -p "$CAPTURE_DIR" "$ARTIFACT_DIR/screenshots" "$ARTIFACT_DIR/assertions"
smoke_write_environment_report "$ARTIFACT_DIR/environment.txt"

assert_capture() {
    [[ -s "$SCREENSHOT" ]] || smoke_fail "command palette Notes screenshot is empty: $SCREENSHOT"
    /usr/bin/python3 "$REPO_ROOT/scripts/assert-png-smoke.py" \
        "$SCREENSHOT" \
        --max-width 1280 \
        --max-height 860 \
        --require-top-band-detail \
        --require-bottom-band-detail \
        >"$ARTIFACT_DIR/assertions/$CAPTURE_NAME-png.txt"

    /usr/bin/python3 - "$CAPTURE_DIR/automation-snapshot.json" "$ATSPI_TREE" "$QUERY" "$ASSERTION_SUMMARY" <<'PY'
import json
import sys
from pathlib import Path

snapshot_path, tree_path, query, summary_path = sys.argv[1:]
snapshot = json.loads(Path(snapshot_path).read_text(encoding="utf-8"))
tree = Path(tree_path).read_text(encoding="utf-8", errors="replace")

palette = snapshot["window"]["command_palette"]
assert palette["visible"] is True, palette
assert palette["mode"] == "notes", palette
assert palette["query"] == query, palette
assert palette["pending_index_update_count"] == 0, palette
assert palette["result_count"] >= 8, palette

required_text = [
    "Bookmarks",
    "Folder Notes",
    "Document Notes",
    "Open Tabs",
    "Bookmark · Palette bookmark marker",
    "Folder Note · Palette Notes Workspace",
    "Document Note · palette-document.md",
    "Document Note · palette-open-tab.md",
]
missing = [text for text in required_text if text not in tree]
assert not missing, {"missing": missing, "tree_excerpt": tree[:3000]}

Path(summary_path).write_text(
    "\n".join(
        [
            f"visible={palette['visible']}",
            f"mode={palette['mode']}",
            f"query={palette['query']}",
            f"result_count={palette['result_count']}",
            f"screenshot={Path(snapshot_path).parents[2] / 'screenshots' / 'command-palette-notes-all.png'}",
            f"atspi_tree={tree_path}",
        ]
    )
    + "\n",
    encoding="utf-8",
)
PY
}

/usr/bin/python3 "$REPO_ROOT/scripts/prepare-command-palette-notes-fixture.py" "$CAPTURE_DIR"

OPEN_TAB_FILE="$(/usr/bin/python3 - "$CAPTURE_DIR/fixture-summary.json" <<'PY'
import json
import sys
print(json.load(open(sys.argv[1], encoding="utf-8"))["open_tab_file"])
PY
)"

if ! /usr/bin/python3 "$REPO_ROOT/.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py" \
    --file "$OPEN_TAB_FILE" \
    --output "$SCREENSHOT" \
    --binary "$BINARY" \
    --width 1280 \
    --height 860 \
    --capture-artifact-dir "$CAPTURE_DIR" \
    --keep-artifacts \
    --enable-atspi \
    --window-action toggle-command-palette \
    --wait-window-action set-command-palette-query \
    --window-string-action set-command-palette-mode=notes \
    --window-string-action "set-command-palette-query=$QUERY" \
    --wait-atspi-text "Bookmarks" \
    --wait-atspi-text "Folder Notes" \
    --wait-atspi-text "Document Notes" \
    --wait-atspi-text "Open Tabs" \
    --atspi-tree-output "$ATSPI_TREE" \
    --atspi-focus-output "$ATSPI_FOCUS" \
    >"$ARTIFACT_DIR/$CAPTURE_NAME.session.log" 2>&1; then
    if grep -qE 'AT-SPI registry did not register|Missing at-spi2-registryd|PipeWire did not become ready|Missing required command' "$ARTIFACT_DIR/$CAPTURE_NAME.session.log"; then
        tail -n 120 "$ARTIFACT_DIR/$CAPTURE_NAME.session.log" >&2 || true
        smoke_skip "command palette Notes smoke host support unavailable. Artifacts: $ARTIFACT_DIR"
    fi
    tail -n 120 "$ARTIFACT_DIR/$CAPTURE_NAME.session.log" >&2 || true
    smoke_fail "command palette Notes smoke failed. Artifacts: $ARTIFACT_DIR"
fi

assert_capture

{
    echo "screenshot=$SCREENSHOT"
    echo "snapshot=$CAPTURE_DIR/automation-snapshot.json"
    echo "atspi_tree=$ATSPI_TREE"
    echo "assertion_summary=$ASSERTION_SUMMARY"
    echo "query=$QUERY"
} >"$ARTIFACT_DIR/summary.txt"

echo "PASS: command palette Notes smoke captured all note kinds at $SCREENSHOT"
