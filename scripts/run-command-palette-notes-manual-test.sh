#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STATE_ROOT="${COMMAND_PALETTE_NOTES_MANUAL_HOME:-build/manual/command-palette-notes}"
QUERY="${COMMAND_PALETTE_NOTES_QUERY:-palette}"
MARKER=".lushtext-command-palette-notes-manual-test"
launcher_pid=""

usage() {
    cat <<'EOF'
Usage: scripts/run-command-palette-notes-manual-test.sh [--state-dir DIR] [--query TEXT]

Launch LushText on the live desktop with isolated app data containing bookmark,
folder-note, document-note, and open-tab note fixtures, then open the command
palette in Notes mode for manual inspection.
EOF
}

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

cleanup() {
    local status="${1:-$?}"

    trap - EXIT INT TERM
    if [[ -n "$launcher_pid" ]] && kill -0 "$launcher_pid" 2>/dev/null; then
        kill "$launcher_pid" 2>/dev/null || true
        wait "$launcher_pid" 2>/dev/null || true
    fi
    exit "$status"
}

trap 'cleanup $?' EXIT
trap 'cleanup 130' INT
trap 'cleanup 143' TERM

while [[ $# -gt 0 ]]; do
    case "$1" in
        --state-dir)
            [[ $# -lt 2 ]] && fail "--state-dir requires a value"
            STATE_ROOT="$2"
            shift 2
            ;;
        --query)
            [[ $# -lt 2 ]] && fail "--query requires a value"
            QUERY="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            fail "unknown argument: $1"
            ;;
    esac
done

[[ -x /usr/bin/python3 ]] || fail "/usr/bin/python3 is not available."
command -v gtk-launch >/dev/null 2>&1 || fail "gtk-launch is required for the live manual test."
command -v gdbus >/dev/null 2>&1 || fail "gdbus is required to pre-open the Notes palette."
[[ -x "$REPO_ROOT/target/debug/lushtext" ]] || fail "debug binary is missing. Run 'make build-debug' first."

STATE_ROOT="${STATE_ROOT%/}"
[[ -n "$STATE_ROOT" && "$STATE_ROOT" != "/" ]] || fail "unsafe state directory: $STATE_ROOT"
STATE_PARENT="$(dirname "$STATE_ROOT")"
STATE_BASENAME="$(basename "$STATE_ROOT")"
mkdir -p "$STATE_PARENT"
STATE_PARENT="$(cd "$STATE_PARENT" && pwd)"
STATE_ROOT="$STATE_PARENT/$STATE_BASENAME"

if [[ -e "$STATE_ROOT" && ! -f "$STATE_ROOT/$MARKER" ]]; then
    fail "$STATE_ROOT exists and was not created by this manual test. Set COMMAND_PALETTE_NOTES_MANUAL_HOME to another directory."
fi

rm -rf -- "$STATE_ROOT"
mkdir -p "$STATE_ROOT/cache" "$STATE_ROOT/config" "$STATE_ROOT/data"
: >"$STATE_ROOT/$MARKER"

/usr/bin/python3 "$REPO_ROOT/scripts/prepare-command-palette-notes-fixture.py" "$STATE_ROOT"

OPEN_TAB_FILE="$(/usr/bin/python3 - "$STATE_ROOT/fixture-summary.json" <<'PY'
import json
import sys

print(json.load(open(sys.argv[1], encoding="utf-8"))["open_tab_file"])
PY
)"

export XDG_CACHE_HOME="$STATE_ROOT/cache"
export XDG_CONFIG_HOME="$STATE_ROOT/config"
export XDG_DATA_HOME="$STATE_ROOT/data"
export GSETTINGS_BACKEND="${GSETTINGS_BACKEND:-keyfile}"
export GSETTINGS_SCHEMA_DIR="${GSETTINGS_SCHEMA_DIR:-$REPO_ROOT/data}"

drive_palette() {
    local automation="$REPO_ROOT/scripts/lushtext-automation.py"
    local snapshot="$STATE_ROOT/command-palette-snapshot.json"

    "$automation" wait window-actions-exported --timeout-ms 15000 >/dev/null
    "$automation" wait file-open-complete --timeout-ms 15000 >/dev/null
    "$automation" action win.toggle-command-palette --timeout-ms 10000 >/dev/null
    "$automation" wait idle --timeout-ms 10000 >/dev/null
    "$automation" action win.set-command-palette-mode --string notes --timeout-ms 10000 >/dev/null
    "$automation" action win.set-command-palette-query --string "$QUERY" --timeout-ms 10000 >/dev/null
    "$automation" wait idle --timeout-ms 10000 >/dev/null
    "$automation" snapshot --json >"$snapshot"

    /usr/bin/python3 - "$snapshot" "$QUERY" <<'PY'
import json
import sys
from pathlib import Path

envelope = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
palette = envelope["data"]["window"]["command_palette"]
assert palette["visible"] is True, palette
assert palette["mode"] == "notes", palette
assert palette["query"] == sys.argv[2], palette
assert palette["result_count"] >= 4, palette
PY
}

echo "Launching LushText with isolated command-palette Notes fixtures..."
echo "State: $STATE_ROOT"
echo "Query: $QUERY"
echo "Expected sections: Bookmarks, Folder Notes, Document Notes, Open Tabs"

LUSHTEXT_DEV_RUN_FORCE_RESTART=1 \
    "$REPO_ROOT/scripts/run-dev-app.sh" "$OPEN_TAB_FILE" &
launcher_pid=$!

if ! drive_palette; then
    echo "Warning: LushText launched, but the palette could not be prepared automatically." >&2
    echo "Open the command palette manually and switch to Notes to inspect the seeded fixture." >&2
fi

echo "Manual Notes palette fixture is ready. Close LushText to finish this make target."
wait "$launcher_pid"
trap - EXIT INT TERM
