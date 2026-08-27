#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/smoke-common.sh
source "$REPO_ROOT/scripts/smoke-common.sh"

ARTIFACT_DIR="${LUSHTEXT_SMOKE_ARTIFACT_DIR:-build/smoke/visual}"
BINARY="$REPO_ROOT/target/debug/lushtext"
WIDTH="${LUSHTEXT_VISUAL_SMOKE_WIDTH:-1600}"
HEIGHT="${LUSHTEXT_VISUAL_SMOKE_HEIGHT:-1000}"
VISUAL_CASES=(
    main-search-minimap
    modified-tab
    destructive-close-dialog
    file-health-properties
    local-history-restore
    normal-properties
    compact-properties
    constrained-properties
    short-layout
    markdown-preview
    constrained-preview
    markdown-preview-side-by-side
    constrained-preview-side-by-side
    workspace-empty
    workspace-representative
    workspace-dense-awkward
    workspace-constrained
    workspace-refresh
    workspace-tree-context-menu
    workspace-header-context-menu
    notes-empty
    notes-few
    bookmarks-few
    notes-dense
    bookmarks-dense
    notes-constrained
    bookmarks-constrained
    command-palette-files
    command-palette-commands
    command-palette-notes
    command-palette-no-results
    command-palette-dense-files
    command-palette-dismissed
    dark-style
    high-contrast-style
    large-text-constrained
    reduced-motion-command-palette
    transparency-readability
    recovery-startup
)
SELECTED_CASES=()
LIST_CASES=false
VISUAL_CASES_RUN=0

usage() {
    cat <<'EOF'
Usage: scripts/run-visual-smoke.sh [--artifact-dir DIR] [--binary PATH] [--case PATTERN] [--list-cases]

Launch LushText in isolated headless Mutter sessions, capture representative
geometry-sensitive desktop states, and preserve screenshots/log artifacts.

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
    printf '%s\n' "${VISUAL_CASES[@]}"
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

smoke_require_command dbus-run-session
smoke_require_command mutter
smoke_require_command gdbus
smoke_require_command gsettings
smoke_require_command gst-launch-1.0
smoke_require_command pipewire
smoke_require_command pw-dump
smoke_require_command wireplumber

[[ -x /usr/bin/python3 ]] || smoke_skip "/usr/bin/python3 is not available."
[[ -x "$BINARY" ]] || smoke_skip "LushText debug binary is missing. Run 'make build-debug' first."

ARTIFACT_DIR="$(smoke_artifact_dir "$ARTIFACT_DIR")"
VISUAL_CASE_FILTERS="$(selected_case_description)"
export VISUAL_CASE_FILTERS
smoke_write_environment_report "$ARTIFACT_DIR/environment.txt"

rm -rf "$ARTIFACT_DIR/screenshots" "$ARTIFACT_DIR/captures" "$ARTIFACT_DIR/assertions"
mkdir -p "$ARTIFACT_DIR/fixtures" "$ARTIFACT_DIR/screenshots" "$ARTIFACT_DIR/captures" "$ARTIFACT_DIR/assertions"

TEXT_FIXTURE="$ARTIFACT_DIR/fixtures/visual-smoke.txt"
MARKDOWN_FIXTURE="$ARTIFACT_DIR/fixtures/visual-smoke.md"
FILE_HEALTH_FIXTURE="$ARTIFACT_DIR/fixtures/visual-file-health.txt"
smoke_create_text_fixture "$TEXT_FIXTURE"
cat >"$MARKDOWN_FIXTURE" <<'EOF'
# LushText visual smoke

This Markdown document exercises the rendered preview surface.

```rust
fn main() {
    println!("needle");
}
```

- narrow layout
- short layout
- preview geometry
EOF
printf 'alpha\r\nbeta\ngamma\rdelta\r\n' >"$FILE_HEALTH_FIXTURE"

scan_visual_logs() {
    local name="$1"
    local capture_dir="$2"
    local report="$ARTIFACT_DIR/assertions/${name}-logs.txt"
    local matches="$ARTIFACT_DIR/assertions/${name}-warnings.txt"

    : >"$report"
    : >"$matches"
    shopt -s nullglob
    local log_paths=(
        "$ARTIFACT_DIR/${name}.session.log"
        "$capture_dir"/*.log
        "$capture_dir"/lushtext.stdout
        "$capture_dir"/lushtext.stderr
    )
    shopt -u nullglob

    for log_path in "${log_paths[@]}"; do
        [[ -f "$log_path" ]] || continue
        printf 'scanned=%s\n' "$log_path" >>"$report"
        grep -E -i \
            '(Gtk|Gdk|GSK|Adwaita|Libadwaita|AT-SPI|accessibility).*(warning|critical|error)|GLib-GObject-CRITICAL|gtk_[a-z0-9_]+.*assertion|gdk_[a-z0-9_]+.*assertion' \
            "$log_path" \
            | /usr/bin/python3 "$REPO_ROOT/scripts/smoke_warning_classifiers.py" --drop-gdk-broken-pipe \
            >>"$matches" || true
    done

    if [[ -s "$matches" ]]; then
        cat "$matches" >&2
        smoke_fail "visual smoke '${name}' emitted unexpected GTK/Adwaita/GDK/accessibility warnings"
    fi
    echo "PASS: no unexpected GTK/Adwaita/GDK/accessibility warnings for ${name}" >>"$report"
}

write_visual_manifest() {
    local name="$1"
    local status="$2"
    local reason="$3"
    local fixture="$4"
    local output="$5"
    local width="$6"
    local height="$7"
    local search="$8"
    local minimap="$9"
    local color_scheme="${10}"
    local capture_dir="${11}"
    local session_log="${12}"
    shift 12

    /usr/bin/python3 - "$ARTIFACT_DIR" "$BINARY" \
        "$name" "$status" "$reason" "$fixture" "$output" "$width" "$height" \
        "$search" "$minimap" "$color_scheme" "$capture_dir" "$session_log" "$@" <<'PY'
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

TEXT_LIMIT = 600
MATRIX_ROWS_BY_CASE = {
    "main-search-minimap": ["A11Y-SHELL-REPRESENTATIVE", "A11Y-EDITOR-MINIMAP"],
    "modified-tab": ["A11Y-SHELL-REPRESENTATIVE"],
    "destructive-close-dialog": ["A11Y-DIALOG-SAVE-CLOSE"],
    "file-health-properties": ["A11Y-SHELL-ERROR-STATUS", "A11Y-PROPERTIES-NORMAL"],
    "local-history-restore": ["A11Y-LOCAL-HISTORY-POPULATED"],
    "normal-properties": ["A11Y-PROPERTIES-NORMAL"],
    "compact-properties": ["A11Y-PROPERTIES-COMPACT"],
    "constrained-properties": ["A11Y-PROPERTIES-COMPACT"],
    "short-layout": ["A11Y-SHELL-DENSE-CONSTRAINED"],
    "markdown-preview": ["A11Y-MARKDOWN-REPRESENTATIVE"],
    "constrained-preview": ["A11Y-MARKDOWN-CONSTRAINED"],
    "markdown-preview-side-by-side": ["A11Y-MARKDOWN-REPRESENTATIVE"],
    "constrained-preview-side-by-side": ["A11Y-MARKDOWN-CONSTRAINED"],
    "workspace-empty": ["A11Y-WORKSPACE-NO-CONTEXT"],
    "workspace-representative": ["A11Y-WORKSPACE-REPRESENTATIVE"],
    "workspace-dense-awkward": ["A11Y-WORKSPACE-DENSE-DEEP"],
    "workspace-constrained": ["A11Y-WORKSPACE-DENSE-DEEP"],
    "workspace-refresh": ["A11Y-WORKSPACE-BUSY-ERROR"],
    "workspace-tree-context-menu": ["A11Y-WORKSPACE-CONTEXT", "A11Y-WORKSPACE-DRAG-DROP", "A11Y-CONTEXT-MENUS-GENERAL"],
    "workspace-header-context-menu": ["A11Y-WORKSPACE-CONTEXT", "A11Y-CONTEXT-MENUS-GENERAL"],
    "notes-empty": ["A11Y-NOTES-EMPTY"],
    "notes-few": ["A11Y-NOTES-POPULATED"],
    "bookmarks-few": ["A11Y-BOOKMARKS"],
    "notes-dense": ["A11Y-NOTES-POPULATED"],
    "bookmarks-dense": ["A11Y-BOOKMARKS"],
    "notes-constrained": ["A11Y-NOTES-POPULATED"],
    "bookmarks-constrained": ["A11Y-BOOKMARKS"],
    "command-palette-files": ["A11Y-PALETTE-FILES"],
    "command-palette-commands": ["A11Y-PALETTE-COMMANDS"],
    "command-palette-notes": ["A11Y-PALETTE-NOTES"],
    "command-palette-no-results": ["A11Y-PALETTE-NO-RESULTS"],
    "command-palette-dense-files": ["A11Y-PALETTE-FILES"],
    "command-palette-dismissed": ["A11Y-PALETTE-DISMISS"],
    "dark-style": ["A11Y-EDITOR-REPRESENTATIVE"],
    "high-contrast-style": ["A11Y-EDITOR-REPRESENTATIVE"],
    "large-text-constrained": ["A11Y-SHELL-DENSE-CONSTRAINED", "A11Y-EDITOR-REPRESENTATIVE"],
    "reduced-motion-command-palette": ["A11Y-PALETTE-DISMISS", "A11Y-EDITOR-FOCUS-PREVIEW"],
    "transparency-readability": ["A11Y-EDITOR-REPRESENTATIVE", "A11Y-MARKDOWN-REPRESENTATIVE"],
    "recovery-startup": ["A11Y-RECOVERY-STARTUP"],
}


def bounded(text: str) -> str:
    return text if len(text) <= TEXT_LIMIT else text[:TEXT_LIMIT] + " [truncated]"


def rel(root: Path, path: str | Path) -> str:
    candidate = Path(path)
    try:
        return str(candidate.resolve().relative_to(root))
    except Exception:
        return str(candidate)


def artifact(root: Path, path: str | Path) -> str | None:
    candidate = Path(path)
    return rel(root, candidate) if candidate.exists() else None


def artifact_rows(root: Path, paths: list[Path], *, status: str = "passed") -> list[dict[str, str]]:
    rows = []
    for path in sorted({p for p in paths if p.exists() and p.is_file()}):
        rows.append({"name": path.stem, "status": status, "artifact": rel(root, path)})
    return rows


(
    artifact_dir,
    binary,
    name,
    status,
    reason,
    fixture,
    output,
    width,
    height,
    search,
    minimap,
    color_scheme,
    capture_dir,
    session_log,
    *actions,
) = sys.argv[1:]

artifact_root = Path(artifact_dir).resolve()
assertions = artifact_root / "assertions"
capture_root = Path(capture_dir)
manifest_path = assertions / f"{name}-manifest.json"
now = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")

state_paths = [
    p
    for p in assertions.glob(f"{name}-*")
    if p.name != manifest_path.name and p.suffix in {".txt", ".json"}
]
snapshot_paths = [
    *assertions.glob(f"{name}-*snapshot*.txt"),
    *assertions.glob(f"{name}-*snapshot*.json"),
    *capture_root.glob("*snapshot*.json"),
]
atspi_paths = [
    *assertions.glob(f"{name}-atspi-*.txt"),
    *assertions.glob(f"{name}-*atspi*.txt"),
]
warning_matches = assertions / f"{name}-warnings.txt"
warning_report = assertions / f"{name}-logs.txt"
unexpected_count = 0
if warning_matches.exists():
    unexpected_count = len([line for line in warning_matches.read_text(encoding="utf-8", errors="replace").splitlines() if line])

failure_reason = bounded(reason) if status == "failed" and reason else None
skip_reason = bounded(reason) if status == "skipped" and reason else None
manifest = {
    "schema_version": 1,
    "scenario_id": f"visual-smoke/{name}",
    "description": "Headless Mutter visual smoke capture for a representative LushText UI state.",
    "status": status,
    "matrix_rows": MATRIX_ROWS_BY_CASE.get(name, []),
    "started_at": now,
    "updated_at": now,
    "finished_at": now,
    "failure_reason": failure_reason,
    "skip_reason": skip_reason,
    "launch_mode": "dbus-run-session+headless-mutter-per-capture",
    "helper_arguments": {
        "artifact_dir": str(artifact_root),
        "binary": str(Path(binary).resolve()),
        "width": int(width),
        "height": int(height),
        "search": search or None,
        "enable_minimap": minimap == "1",
        "color_scheme": color_scheme,
    },
    "fixture_setup": [
        {
            "name": "opened-file",
            "kind": "text-file",
            "artifact": artifact(artifact_root, fixture) or str(fixture),
            "detail": "Primary file passed to the capture helper.",
        },
        {
            "name": "capture-state",
            "kind": "isolated-xdg-state",
            "artifact": artifact(artifact_root, capture_root) or rel(artifact_root, capture_root),
            "detail": "Per-capture data/config/cache and runtime artifacts.",
        },
    ],
    "actions": [
        {
            "name": action,
            "scope": "window",
            "status": "requested",
            "detail": "Activated through capture helper before screenshot.",
        }
        for action in actions
    ],
    "waits": artifact_rows(artifact_root, [*capture_root.glob("*wait*.txt")]),
    "state_assertions": artifact_rows(artifact_root, state_paths),
    "screenshots": artifact_rows(artifact_root, [Path(output)]),
    "at_spi_assertions": artifact_rows(artifact_root, atspi_paths)
    or [{"name": "at-spi-visible-ui-assertions", "status": "not-run"}],
    "dbus_summaries": artifact_rows(artifact_root, snapshot_paths),
    "warnings": {
        "status": "passed" if status == "passed" and unexpected_count == 0 else ("failed" if unexpected_count else "not-run"),
        "artifact": artifact(artifact_root, warning_report),
        "matches_artifact": artifact(artifact_root, warning_matches),
        "unexpected_count": unexpected_count,
        "detail": "Unexpected warning matches are stored in matches_artifact.",
    },
    "environment": {
        "environment_artifact": artifact(artifact_root, artifact_root / "environment.txt"),
        "session_log": artifact(artifact_root, session_log),
        "capture_dir": rel(artifact_root, capture_root),
    },
    "bounded_artifact_policy": {
        "embedded_text_limit": TEXT_LIMIT,
        "large_payload_strategy": "manifest stores relative artifact paths and bounded summaries",
    },
    "steps": [
        {"index": 1, "name": "prepare fixture", "kind": "fixture", "status": "passed"},
        {
            "index": 2,
            "name": "run capture helper",
            "kind": "launch",
            "status": "passed" if status == "passed" else status,
            "artifact": artifact(artifact_root, session_log),
            "detail": failure_reason or skip_reason,
        },
        {
            "index": 3,
            "name": "assert screenshot",
            "kind": "state-assertion",
            "status": "passed" if Path(output).exists() and status == "passed" else "not-run",
            "artifact": artifact(artifact_root, assertions / f"{name}-png.txt"),
        },
        {
            "index": 4,
            "name": "scan runtime warnings",
            "kind": "warning-scan",
            "status": "passed" if status == "passed" and unexpected_count == 0 else "not-run",
            "artifact": artifact(artifact_root, warning_report),
        },
        {
            "index": 5,
            "name": "record state artifacts",
            "kind": "state-assertion",
            "status": "passed" if status == "passed" else "not-run",
            "detail": f"{len(state_paths)} bounded assertion artifacts",
        },
    ],
}

manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY
}

prepare_recovery_capture_state() {
    local capture_dir="$1"
    local data_dir="$capture_dir/data/lushtext"
    mkdir -p "$data_dir/drafts"

    printf '{ malformed session metadata\n' >"$data_dir/session.json"
    printf '{ malformed draft manifest\n' >"$data_dir/drafts/manifest.json"
    printf 'Visual smoke recovered draft body\n' >"$data_dir/drafts/untitled-visual-smoke.draft"
}

assert_recovery_capture_artifacts() {
    local name="$1"
    local capture_dir="$2"
    local data_dir="$capture_dir/data/lushtext"
    local tree_path="$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
    local summary_path="$ARTIFACT_DIR/assertions/${name}-recovery-summary.txt"
    local quarantine_dir="$data_dir/recovery-quarantine"

    {
        echo "data_dir=$data_dir"
        if [[ -d "$quarantine_dir" ]]; then
            find "$quarantine_dir" -type f -printf '%P size=%s\n' | sort
        else
            echo "quarantine=<missing>"
        fi
    } >"$summary_path"

    if ! grep -q 'size=' "$summary_path"; then
        smoke_fail "recovery visual smoke did not preserve a quarantine summary"
    fi
    if ! grep -Eiq 'recovery|could not be loaded|draft|session' "$tree_path"; then
        smoke_fail "recovery visual smoke did not expose recovery diagnostics in the AT-SPI tree"
    fi
}

assert_modified_tab_capture_artifacts() {
    local name="$1"
    local capture_dir="$2"
    local require_dialog="$3"
    local tree_path="$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
    local snapshot_path="$capture_dir/automation-snapshot.json"
    local summary_path="$ARTIFACT_DIR/assertions/${name}-modified-tab.txt"

    [[ -s "$tree_path" ]] || smoke_fail "visual smoke '${name}' did not write an AT-SPI tree"
    [[ -s "$snapshot_path" ]] || smoke_fail "visual smoke '${name}' did not write automation-snapshot.json"
    /usr/bin/python3 - "$snapshot_path" "$tree_path" "$require_dialog" >"$summary_path" <<'PY'
import json
import sys

snapshot_path, tree_path, require_dialog = sys.argv[1:]
snapshot = json.load(open(snapshot_path, encoding="utf-8"))
tree = open(tree_path, encoding="utf-8", errors="replace").read()
window = snapshot["window"]
assert window is not None, snapshot
tabs = window["tabs"]
assert tabs, window
active = next((tab for tab in tabs if tab["active"]), tabs[0])
assert active["document_kind"] == "file", active
assert active["modified"] is True, active
if require_dialog == "true":
    assert "Save Changes?" in tree, tree[:2000]
    assert "unsaved changes" in tree, tree[:2000]
    assert "permanently lost" in tree, tree[:2000]
    assert "Cancel" in tree, tree[:2000]
    assert "Discard" in tree, tree[:2000]
    assert "Save" in tree, tree[:2000]
else:
    assert "Visual smoke modified buffer" in tree, tree[:2000]
print("active_tab_modified=true")
print(f"save_changes_dialog={require_dialog}")
print(f"snapshot={snapshot_path}")
print(f"tree={tree_path}")
PY
}

prepare_local_history_capture_state() {
    local capture_dir="$1"
    local fixture="$2"

    /usr/bin/python3 - "$capture_dir" "$fixture" <<'PY'
import json
import sys
from pathlib import Path

FNV_OFFSET = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3


def stable_hash(data: bytes) -> str:
    value = FNV_OFFSET
    for byte in data:
        value ^= byte
        value = (value * FNV_PRIME) & 0xFFFFFFFFFFFFFFFF
    return f"{value:016x}"


capture_dir = Path(sys.argv[1])
display_path = Path(sys.argv[2]).resolve()
data_dir = capture_dir / "data" / "lushtext"
sidecar_id = stable_hash(str(display_path).encode("utf-8"))
document_dir = data_dir / "local-history" / sidecar_id
document_dir.mkdir(parents=True, exist_ok=True)

snapshot_id = "history-00000000000000000000018bcff4aa00-0000000000000001"
text = "local history saved snapshot\nwith visible restore proof\n"
(document_dir / f"{snapshot_id}.txt").write_text(text, encoding="utf-8")
body = text.encode("utf-8")
index = {
    "kind": "dev.cominotti.lushtext.local-history-index",
    "version": 1,
    "data": {
        "identity": {
            "display_path": str(display_path),
            "canonical_path": str(display_path),
            "sidecar_id": sidecar_id,
        },
        "snapshots": [
            {
                "snapshot_id": snapshot_id,
                "captured_at_millis": 1_700_000_001_000,
                "origin": "Save",
                "byte_len": len(body),
                "content_hash": stable_hash(body),
            }
        ],
    },
}
(document_dir / "index.json").write_text(
    json.dumps(index, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
print(f"local_history_sidecar={sidecar_id}")
PY
}

assert_local_history_restore_capture_artifacts() {
    local name="$1"
    local capture_dir="$2"
    local tree_path="$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
    local snapshot_path="$capture_dir/automation-snapshot.json"
    local summary_path="$ARTIFACT_DIR/assertions/${name}-local-history-restore.txt"

    [[ -s "$tree_path" ]] || smoke_fail "visual smoke '${name}' did not write an AT-SPI tree"
    [[ -s "$snapshot_path" ]] || smoke_fail "visual smoke '${name}' did not write automation-snapshot.json"
    /usr/bin/python3 - "$snapshot_path" "$tree_path" "$capture_dir" >"$summary_path" <<'PY'
import json
import sys
from pathlib import Path

snapshot_path, tree_path, capture_dir = sys.argv[1:]
snapshot = json.load(open(snapshot_path, encoding="utf-8"))
tree = open(tree_path, encoding="utf-8", errors="replace").read()
window = snapshot["window"]
assert window is not None, snapshot
tabs = window["tabs"]
assert tabs, window
active = next((tab for tab in tabs if tab["active"]), tabs[0])
assert active["modified"] is True, active
assert "Restored from Local History" in tree, tree[:2000]
assert "Undo Restore" in tree, tree[:2000]
lineages = list((Path(capture_dir) / "data" / "lushtext" / "local-history").glob("*/index.json"))
assert lineages, capture_dir
print("local_history_restore_notification=true")
print("active_tab_modified=true")
print(f"lineage_indexes={len(lineages)}")
print(f"snapshot={snapshot_path}")
print(f"tree={tree_path}")
PY
}

assert_file_health_capture_artifacts() {
    local name="$1"
    local capture_dir="$2"
    local tree_path="$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
    local snapshot_path="$capture_dir/automation-snapshot.json"
    local summary_path="$ARTIFACT_DIR/assertions/${name}-file-health.txt"

    [[ -s "$tree_path" ]] || smoke_fail "visual smoke '${name}' did not write an AT-SPI tree"
    [[ -s "$snapshot_path" ]] || smoke_fail "visual smoke '${name}' did not write automation-snapshot.json"
    /usr/bin/python3 - "$snapshot_path" "$tree_path" >"$summary_path" <<'PY'
import json
import sys

snapshot_path, tree_path = sys.argv[1:]
snapshot = json.load(open(snapshot_path, encoding="utf-8"))
tree = open(tree_path, encoding="utf-8", errors="replace").read()
window = snapshot["window"]
assert window is not None, snapshot
assert window["surfaces"]["document_properties_visible"] is True, window["surfaces"]
assert "Health" in tree, tree[:2000]
assert "Mixed line endings" in tree, tree[:2000]
assert "Review" in tree, tree[:2000]
print("document_properties_visible=true")
print("file_health_finding=Mixed line endings")
print(f"snapshot={snapshot_path}")
print(f"tree={tree_path}")
PY
}

assert_search_minimap_capture_artifacts() {
    local name="$1"
    local capture_dir="$2"
    local fixture="$3"
    local query="$4"
    local snapshot_path="$capture_dir/automation-snapshot.json"
    local summary_path="$ARTIFACT_DIR/assertions/${name}-automation-snapshot.txt"

    [[ -s "$snapshot_path" ]] || smoke_fail "visual smoke '${name}' did not write automation-snapshot.json"
    /usr/bin/python3 - "$snapshot_path" "$fixture" "$query" >"$summary_path" <<'PY'
import json
import sys

snapshot_path, fixture, query = sys.argv[1:]
with open(snapshot_path, encoding="utf-8") as handle:
    snapshot = json.load(handle)

assert snapshot["enabled"] is True, snapshot
assert snapshot["idle"] is True, snapshot
window = snapshot["window"]
assert window is not None, snapshot
assert window["tabs"], window
assert window["tabs"][0]["path"] == fixture, window["tabs"][0]
assert window["search"]["editor_search_visible"] is True, window["search"]
assert window["search"]["editor_query"] == query, window["search"]
assert window["search"]["editor_match_count"] == 3, window["search"]
assert window["surfaces"]["minimap_requested"] is True, window["surfaces"]
print(f"query={query}")
print("editor_match_count=3")
print("minimap_requested=true")
print(f"snapshot={snapshot_path}")
PY
}

prepare_workspace_capture_state() {
    local name="$1"
    local capture_dir="$2"

    /usr/bin/python3 - "$name" "$capture_dir" <<'PY'
import json
import sys
from pathlib import Path

name, capture_dir = sys.argv[1:]
capture_dir = Path(capture_dir)
data_dir = capture_dir / "data" / "lushtext"
fixtures_dir = capture_dir / "workspace-fixtures"
data_dir.mkdir(parents=True, exist_ok=True)
fixtures_dir.mkdir(parents=True, exist_ok=True)

def write_file(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")

def folder(path_name: str, folder_id: str) -> dict[str, str]:
    path = fixtures_dir / path_name
    path.mkdir(parents=True, exist_ok=True)
    write_file(path / "README.md", f"# {path_name}\n")
    write_file(path / "src" / "main.rs", f"fn main() {{ println!(\"{path_name}\"); }}\n")
    return {"id": folder_id, "path": str(path)}

if name == "workspace-empty":
    workspace_data = {
        "current_scope": {"kind": "workspace", "workspace_id": "ws-empty"},
        "workspaces": [{"id": "ws-empty", "name": "Zero Folder Workspace", "folders": []}],
    }
elif name == "workspace-dense-awkward":
    folders = [
        folder(f"Very Long Folder Name {index:02d} With Spaces And Brackets [{index}]", f"folder-dense-{index:02d}")
        for index in range(8)
    ]
    workspace_data = {
        "current_scope": {"kind": "all"},
        "workspaces": [
            {
                "id": "ws-dense",
                "name": "Dense Workspace With A Very Long Display Name",
                "folders": folders,
            }
        ],
    }
else:
    alpha = folder("alpha-project", "folder-alpha")
    beta = folder("beta project with spaces", "folder-beta")
    gamma = folder("gamma-nested", "folder-gamma")
    workspace_data = {
        "current_scope": {"kind": "all"},
        "workspaces": [
            {"id": "ws-alpha", "name": "Alpha Project", "folders": [alpha, beta]},
            {"id": "ws-gamma", "name": "Gamma Utilities", "folders": [gamma]},
        ],
    }

envelope = {
    "kind": "dev.cominotti.lushtext.workspace-state",
    "version": 1,
    "data": workspace_data,
}
(data_dir / "workspaces.json").write_text(
    json.dumps(envelope, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY
}

prepare_notes_capture_state() {
    local name="$1"
    local capture_dir="$2"

    /usr/bin/python3 - "$name" "$capture_dir" <<'PY'
import json
import sys
from pathlib import Path

name, capture_dir = sys.argv[1:]
capture_dir = Path(capture_dir)
data_dir = capture_dir / "data" / "lushtext"
workspace = capture_dir / "notes-fixtures" / "workspace"
data_dir.mkdir(parents=True, exist_ok=True)
workspace.mkdir(parents=True, exist_ok=True)

FNV_OFFSET = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3

def stable_path_hash(path: Path) -> str:
    value = FNV_OFFSET
    for byte in str(path.resolve()).encode("utf-8"):
        value ^= byte
        value = (value * FNV_PRIME) & 0xFFFFFFFFFFFFFFFF
    return f"{value:016x}"

def write_file(path: Path, text: str) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    return path

def identity(path: Path) -> dict[str, str]:
    canonical = path.resolve()
    return {
        "display_path": str(path),
        "canonical_path": str(canonical),
        "sidecar_id": stable_path_hash(canonical),
    }

def write_envelope(path: Path, kind: str, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps({"kind": kind, "version": 1, "data": data}, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )

def save_document_note(path: Path, text: str) -> None:
    doc_identity = identity(path)
    write_envelope(
        data_dir / "document-notes" / f"{doc_identity['sidecar_id']}.json",
        "dev.cominotti.lushtext.document-note-sidecar",
        {
            "identity": doc_identity,
            "note": {"text": text, "created_at_secs": 1, "updated_at_secs": 1},
        },
    )

def save_bookmarks(path: Path, labels: list[str]) -> None:
    doc_identity = identity(path)
    write_envelope(
        data_dir / "bookmarks" / f"{doc_identity['sidecar_id']}.json",
        "dev.cominotti.lushtext.bookmark-sidecar",
        {
            "identity": doc_identity,
            "bookmarks": [
                {
                    "id": f"bookmark-visual-{index}",
                    "line": index + 1,
                    "label": label,
                    "created_at_secs": 1,
                    "updated_at_secs": 1,
                }
                for index, label in enumerate(labels)
            ],
        },
    )

workspace_data = {
    "current_scope": {"kind": "all"},
    "workspaces": [
        {
            "id": "ws-notes",
            "name": "Notes Workspace",
            "folders": [{"id": "folder-notes", "path": str(workspace)}],
        }
    ],
}
write_envelope(data_dir / "workspaces.json", "dev.cominotti.lushtext.workspace-state", workspace_data)

if name != "notes-empty":
    few_names = {"notes-few", "notes-constrained", "bookmarks-few", "bookmarks-constrained", "command-palette-notes"}
    few_count = 2 if name in few_names else 8
    bookmark_count = 2 if name in few_names else 8
    for index in range(few_count):
        note_file = write_file(
            workspace / f"note-target-{index:02d}.md",
            f"# Note target {index}\n\nbody\n",
        )
        save_document_note(note_file, f"# Visual note {index}\n\nSeeded by visual smoke.")
    bookmark_file = write_file(
        workspace / "bookmark-target.rs",
        "\n".join(f"line {index}" for index in range(1, 24)) + "\n",
    )
    save_bookmarks(bookmark_file, [f"Visual bookmark {index}" for index in range(bookmark_count)])
else:
    write_file(workspace / "empty-notes.md", "# No persisted notes\n")
PY
}

prepare_command_palette_capture_state() {
    local name="$1"
    local capture_dir="$2"

    /usr/bin/python3 - "$name" "$capture_dir" <<'PY'
import json
import sys
from pathlib import Path

name, capture_dir = sys.argv[1:]
if name != "command-palette-dense-files":
    raise SystemExit(0)

capture_dir = Path(capture_dir)
data_dir = capture_dir / "data" / "lushtext"
workspace = capture_dir / "palette-fixtures" / "workspace"
data_dir.mkdir(parents=True, exist_ok=True)
workspace.mkdir(parents=True, exist_ok=True)

for index in range(64):
    path = workspace / "src" / f"palette-dense-{index:02d}.rs"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(f"pub fn palette_dense_{index:02d}() {{}}\n", encoding="utf-8")

workspace_data = {
    "current_scope": {"kind": "all"},
    "workspaces": [
        {
            "id": "ws-palette",
            "name": "Palette Dense Workspace",
            "folders": [{"id": "folder-palette", "path": str(workspace)}],
        }
    ],
}
envelope = {
    "kind": "dev.cominotti.lushtext.workspace-state",
    "version": 1,
    "data": workspace_data,
}
(data_dir / "workspaces.json").write_text(
    json.dumps(envelope, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY
}

assert_surface_capture_artifacts() {
    local name="$1"
    local capture_dir="$2"
    local expected_properties="$3"
    local expected_preview_mode="$4"
    local expected_preview_pane="$5"
    local snapshot_path="$capture_dir/automation-snapshot.json"
    local summary_path="$ARTIFACT_DIR/assertions/${name}-surface-snapshot.txt"

    [[ -s "$snapshot_path" ]] || smoke_fail "visual smoke '${name}' did not write automation-snapshot.json"
    /usr/bin/python3 - "$snapshot_path" "$expected_properties" "$expected_preview_mode" "$expected_preview_pane" >"$summary_path" <<'PY'
import json
import sys

snapshot_path, expected_properties, expected_preview_mode, expected_preview_pane = sys.argv[1:]

def parse_expected(value):
    if value == "any":
        return None
    return value == "true"

def surface_by_name(snapshot, name):
    for surface in snapshot["window"]["visual_geometry"]["surfaces"]:
        if surface["name"] == name:
            return surface
    raise AssertionError(f"missing visual surface {name!r}")

with open(snapshot_path, encoding="utf-8") as handle:
    snapshot = json.load(handle)

window = snapshot["window"]
assert snapshot["enabled"] is True, snapshot
assert snapshot["idle"] is True, snapshot
assert window is not None, snapshot
surfaces = window["surfaces"]
properties = parse_expected(expected_properties)
preview_mode = parse_expected(expected_preview_mode)
preview_pane = parse_expected(expected_preview_pane)
if properties is not None:
    assert surfaces["document_properties_visible"] is properties, surfaces
    assert surfaces["document_properties_requested"] is properties, surfaces
if preview_mode is not None:
    assert surfaces["preview_mode"] is preview_mode, surfaces
if preview_pane is not None:
    assert surfaces["preview_pane_visible"] is preview_pane, surfaces
expected_preview_visible = (preview_mode is True) or (preview_pane is True)
if expected_preview_visible:
    preview_surface = surface_by_name(snapshot, "preview")
    assert preview_surface["visible"] is True, preview_surface
    rect = preview_surface["rect"]
    assert rect and rect["width"] > 0 and rect["height"] > 0, preview_surface
elif preview_mode is False and preview_pane is False:
    assert surface_by_name(snapshot, "preview")["visible"] is False, surfaces
print(f"document_properties_visible={surfaces['document_properties_visible']}")
print(f"document_properties_requested={surfaces['document_properties_requested']}")
print(f"preview_mode={surfaces['preview_mode']}")
print(f"preview_pane_visible={surfaces['preview_pane_visible']}")
print(f"preview_visual_visible={surface_by_name(snapshot, 'preview')['visible']}")
print(f"compact_surface={surfaces['compact_surface']}")
print(f"snapshot={snapshot_path}")
PY
}

assert_notes_capture_artifacts() {
    local name="$1"
    local capture_dir="$2"
    local expected_document_notes="$3"
    local expected_bookmarks="$4"
    local required_text="$5"
    local tree_path="$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
    local summary_path="$ARTIFACT_DIR/assertions/${name}-notes-tree.txt"

    [[ -s "$tree_path" ]] || smoke_fail "visual smoke '${name}' did not write an AT-SPI tree"
    /usr/bin/python3 - "$tree_path" "$capture_dir" "$expected_document_notes" "$expected_bookmarks" "$required_text" >"$summary_path" <<'PY'
import json
import sys
from pathlib import Path

tree_path, capture_dir, expected_document_notes, expected_bookmarks, required_text = sys.argv[1:]
text = open(tree_path, encoding="utf-8", errors="replace").read()
data_dir = Path(capture_dir) / "data" / "lushtext"
expected_document_notes = int(expected_document_notes)
expected_bookmarks = int(expected_bookmarks)
document_sidecars = list((data_dir / "document-notes").glob("*.json"))
bookmark_records = 0
for path in (data_dir / "bookmarks").glob("*.json"):
    with open(path, encoding="utf-8") as handle:
        bookmark_records += len(json.load(handle)["data"]["bookmarks"])
assert len(document_sidecars) == expected_document_notes, document_sidecars
assert bookmark_records == expected_bookmarks, bookmark_records
assert required_text in text, text[:2000]
print(f"document_note_sidecars={len(document_sidecars)}")
print(f"bookmark_records={bookmark_records}")
print(f"required_text={required_text}")
print(f"tree={tree_path}")
PY
}

assert_command_palette_capture_artifacts() {
    local name="$1"
    local capture_dir="$2"
    local expected_visible="$3"
    local expected_mode="$4"
    local expected_query="$5"
    local min_results="$6"
    local exact_results="$7"
    local min_file_index="$8"
    local snapshot_path="$capture_dir/automation-snapshot.json"
    local summary_path="$ARTIFACT_DIR/assertions/${name}-command-palette-snapshot.txt"

    [[ -s "$snapshot_path" ]] || smoke_fail "visual smoke '${name}' did not write automation-snapshot.json"
    /usr/bin/python3 - "$snapshot_path" "$expected_visible" "$expected_mode" "$expected_query" "$min_results" "$exact_results" "$min_file_index" >"$summary_path" <<'PY'
import json
import sys

(
    snapshot_path,
    expected_visible,
    expected_mode,
    expected_query,
    min_results,
    exact_results,
    min_file_index,
) = sys.argv[1:]
with open(snapshot_path, encoding="utf-8") as handle:
    snapshot = json.load(handle)

window = snapshot["window"]
assert snapshot["enabled"] is True, snapshot
assert snapshot["idle"] is True, snapshot
assert window is not None, snapshot
palette = window["command_palette"]
assert palette["visible"] is (expected_visible == "true"), palette
assert palette["mode"] == expected_mode, palette
assert palette["query"] == expected_query, palette
assert palette["pending_index_update_count"] == 0, palette
if exact_results != "any":
    assert palette["result_count"] == int(exact_results), palette
else:
    assert palette["result_count"] >= int(min_results), palette
assert palette["file_index_count"] >= int(min_file_index), palette
print(f"visible={palette['visible']}")
print(f"mode={palette['mode']}")
print(f"query={palette['query']}")
print(f"result_count={palette['result_count']}")
print(f"file_index_count={palette['file_index_count']}")
print(f"open_tab_source_count={palette['open_tab_source_count']}")
print(f"snapshot={snapshot_path}")
PY
}

assert_workspace_capture_artifacts() {
    local name="$1"
    local capture_dir="$2"
    local expected_workspace_count="$3"
    local expected_folder_count="$4"
    local expected_scoped_folder_count="$5"
    local expected_no_workspaces="$6"
    local expected_scope_kind="$7"
    local snapshot_path="$capture_dir/automation-snapshot.json"
    local summary_path="$ARTIFACT_DIR/assertions/${name}-workspace-snapshot.txt"

    [[ -s "$snapshot_path" ]] || smoke_fail "visual smoke '${name}' did not write automation-snapshot.json"
    /usr/bin/python3 - "$snapshot_path" "$expected_workspace_count" "$expected_folder_count" "$expected_scoped_folder_count" "$expected_no_workspaces" "$expected_scope_kind" >"$summary_path" <<'PY'
import json
import sys

(
    snapshot_path,
    expected_workspace_count,
    expected_folder_count,
    expected_scoped_folder_count,
    expected_no_workspaces,
    expected_scope_kind,
) = sys.argv[1:]
with open(snapshot_path, encoding="utf-8") as handle:
    snapshot = json.load(handle)

window = snapshot["window"]
assert snapshot["enabled"] is True, snapshot
assert snapshot["idle"] is True, snapshot
assert window is not None, snapshot
workspace = window["workspace"]
assert workspace["workspace_count"] == int(expected_workspace_count), workspace
assert workspace["folder_count"] == int(expected_folder_count), workspace
assert workspace["scoped_folder_count"] == int(expected_scoped_folder_count), workspace
assert workspace["no_workspaces"] is (expected_no_workspaces == "true"), workspace
assert workspace["scope_kind"] == expected_scope_kind, workspace
assert workspace["persistence_inflight"] is False, workspace
assert workspace["persistence_dirty"] is False, workspace
assert workspace["filter_animation_active"] is False, workspace
assert window["command_palette"]["pending_index_update_count"] == 0, window["command_palette"]
print(f"workspace_count={workspace['workspace_count']}")
print(f"folder_count={workspace['folder_count']}")
print(f"scoped_folder_count={workspace['scoped_folder_count']}")
print(f"scope_kind={workspace['scope_kind']}")
print(f"scope_workspace_name={workspace['scope_workspace_name']}")
print(f"snapshot={snapshot_path}")
PY
}

assert_workspace_context_menu_capture_artifacts() {
    local name="$1"
    local required_text="$2"
    local tree_path="$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
    local snapshot_path="$ARTIFACT_DIR/captures/${name}/automation-snapshot.json"
    local summary_path="$ARTIFACT_DIR/assertions/${name}-context-menu.txt"

    [[ -s "$tree_path" ]] || smoke_fail "visual smoke '${name}' did not write an AT-SPI tree"
    [[ -s "$snapshot_path" ]] || smoke_fail "visual smoke '${name}' did not write automation-snapshot.json"
    /usr/bin/python3 - "$tree_path" "$snapshot_path" "$required_text" >"$summary_path" <<'PY'
import json
import sys

tree_path, snapshot_path, required_text = sys.argv[1:]
tree = open(tree_path, encoding="utf-8", errors="replace").read()
snapshot = json.load(open(snapshot_path, encoding="utf-8"))
assert required_text in tree, tree[:2500]
window = snapshot["window"]
assert window is not None, snapshot
assert window["surfaces"]["workspace_sidebar_visible"] is True, window["surfaces"]
print(f"required_text={required_text}")
print("workspace_sidebar_visible=true")
print(f"tree={tree_path}")
print(f"snapshot={snapshot_path}")
PY
}

run_capture() {
    local name="$1"
    local fixture="$2"
    local output="$3"
    local width="$4"
    local height="$5"
    local search="$6"
    local minimap="$7"
    local color_scheme="$8"
    shift 8
    local actions=("$@")
    local capture_dir="$ARTIFACT_DIR/captures/$name"
    local session_log="$ARTIFACT_DIR/${name}.session.log"
    local manifest="$ARTIFACT_DIR/assertions/${name}-state.txt"
    local variant_notes=()

    mkdir -p "$capture_dir"
    if [[ "$name" == "recovery-startup" ]]; then
        prepare_recovery_capture_state "$capture_dir"
    fi
    if [[ "$name" == "local-history-restore" ]]; then
        prepare_local_history_capture_state "$capture_dir" "$fixture"
    fi
    if [[ "$name" == workspace-* ]]; then
        prepare_workspace_capture_state "$name" "$capture_dir"
    fi
    if [[ "$name" == notes-* || "$name" == bookmarks-* || "$name" == "command-palette-notes" ]]; then
        prepare_notes_capture_state "$name" "$capture_dir"
    fi
    if [[ "$name" == command-palette-* ]]; then
        prepare_command_palette_capture_state "$name" "$capture_dir"
    fi
    {
        echo "name=$name"
        echo "fixture=$fixture"
        echo "output=$output"
        echo "width=$width"
        echo "height=$height"
        echo "search=$search"
        echo "minimap=$minimap"
        echo "color_scheme=$color_scheme"
        printf 'actions='
        printf '%s ' "${actions[@]}"
        printf '\n'
    } >"$manifest"

    local capture_args=(
        "$REPO_ROOT/.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py"
        --file "$fixture"
        --output "$output"
        --binary "$BINARY"
        --width "$width"
        --height "$height"
        --capture-artifact-dir "$capture_dir"
        --keep-artifacts
        --enable-atspi
    )
    if [[ -n "$search" ]]; then
        capture_args+=(--search "$search")
    fi
    if [[ "$name" == "main-search-minimap" ]]; then
        capture_args+=(--expected-search-matches 3)
    fi
    if [[ "$name" == workspace-* ]]; then
        capture_args+=(--wait-predicate workspace-refresh-complete)
    fi
    # Every scenario in this lane asserts `workspace_sidebar_visible is True`, but
    # the constrained widths run the workspace sidebar's adaptive collapse and
    # restore. Gating only on `idle` lets the snapshot land mid-transition, with
    # `workspace_sidebar_requested: true` and `workspace_sidebar_visible: false` —
    # observed once in four runs. `visual-geometry-settled` includes the
    # `workspace-sidebar-animation` blocker (plus the breakpoint-driven shell
    # settle), which is exactly the transition being raced.
    case "$name" in
        constrained-*|compact-*|short-layout|large-text-constrained)
            capture_args+=(--wait-predicate visual-geometry-settled)
            ;;
    esac
    if [[ "$minimap" == "1" ]]; then
        capture_args+=(--enable-minimap)
    fi
    if [[ "$color_scheme" != "default" ]]; then
        capture_args+=(--color-scheme "$color_scheme")
    fi
    case "$name" in
        high-contrast-style)
            capture_args+=(--high-contrast --show-status-shapes)
            variant_notes+=("high_contrast=true" "show_status_shapes=true")
            ;;
        large-text-constrained)
            capture_args+=(--text-scale 1.45)
            variant_notes+=("text_scale=1.45")
            ;;
        reduced-motion-command-palette)
            capture_args+=(--reduced-motion)
            variant_notes+=("reduced_motion=true" "interface_enable_animations=false")
            ;;
        transparency-readability)
            capture_args+=(--tab-content-opacity 0.65 --enable-minimap)
            variant_notes+=("tab_content_opacity=0.65" "minimap_requested=true")
            ;;
    esac
    if [[ "$name" == "recovery-startup" ]]; then
        capture_args+=(
            --atspi-tree-output "$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
            --atspi-focus-output "$ARTIFACT_DIR/assertions/${name}-atspi-focus.txt"
        )
    fi
    if [[ "$name" == "modified-tab" ]]; then
        capture_args+=(
            --atspi-tree-output "$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
            --atspi-focus-output "$ARTIFACT_DIR/assertions/${name}-atspi-focus.txt"
            --step "atspi-set-editor-text:Visual smoke modified buffer with unsaved changes"
            --wait-atspi-text "Visual smoke modified buffer"
        )
    fi
    if [[ "$name" == "destructive-close-dialog" ]]; then
        capture_args+=(
            --atspi-tree-output "$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
            --atspi-focus-output "$ARTIFACT_DIR/assertions/${name}-atspi-focus.txt"
            --step "atspi-set-editor-text:Visual smoke modified buffer with unsaved changes"
            --step "window-action:close-tab"
            --step "wait-atspi-text:Save Changes?"
        )
    fi
    if [[ "$name" == "file-health-properties" ]]; then
        capture_args+=(
            --atspi-tree-output "$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
            --atspi-focus-output "$ARTIFACT_DIR/assertions/${name}-atspi-focus.txt"
            --wait-atspi-text "Mixed line endings"
        )
    fi
    if [[ "$name" == "workspace-tree-context-menu" ]]; then
        capture_args+=(
            --atspi-tree-output "$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
            --atspi-focus-output "$ARTIFACT_DIR/assertions/${name}-atspi-focus.txt"
            --step "wait-atspi-text:Folder alpha-project"
            --step "window-action:focus-workspace-tree"
            --step "window-action:show-workspace-tree-context-menu"
            --step "wait-atspi-text:New File"
        )
    fi
    if [[ "$name" == "workspace-header-context-menu" ]]; then
        capture_args+=(
            --atspi-tree-output "$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
            --atspi-focus-output "$ARTIFACT_DIR/assertions/${name}-atspi-focus.txt"
            --step "wait-atspi-text:Workspace Alpha Project"
            --step "window-action:focus-workspace-header"
            --step "window-action:show-workspace-header-context-menu"
            --step "wait-atspi-text:Rename Workspace"
        )
    fi
    if [[ "$name" == "local-history-restore" ]]; then
        capture_args+=(
            --atspi-tree-output "$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
            --atspi-focus-output "$ARTIFACT_DIR/assertions/${name}-atspi-focus.txt"
            --step "wait-window-action:show-local-history"
            --step "window-action:show-local-history"
            --step "wait-atspi-text:local history saved snapshot"
            --step "atspi-click-button:^Restore$"
            --step "wait-atspi-text:Restored from Local History"
        )
    fi
    if [[ "$name" == notes-* || "$name" == bookmarks-* ]]; then
        capture_args+=(
            --wait-predicate workspace-refresh-complete
            --atspi-tree-output "$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
            --atspi-focus-output "$ARTIFACT_DIR/assertions/${name}-atspi-focus.txt"
        )
        if [[ "$name" == "notes-empty" ]]; then
            capture_args+=(--wait-atspi-text "No notes yet")
        elif [[ "$name" == bookmarks-* ]]; then
            capture_args+=(
                --wait-window-action set-notes-browser-query
                --window-string-action "set-notes-browser-query=Visual bookmark"
                --wait-atspi-text "Visual bookmark"
            )
        else
            capture_args+=(
                --wait-window-action set-notes-browser-query
                --window-string-action "set-notes-browser-query=Visual note"
                --wait-atspi-text "note-target-00.md"
            )
        fi
    fi
    if [[ "$name" == command-palette-* ]]; then
        capture_args+=(
            --atspi-tree-output "$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
            --atspi-focus-output "$ARTIFACT_DIR/assertions/${name}-atspi-focus.txt"
        )
        if [[ "$name" == "command-palette-dense-files" ]]; then
            capture_args+=(--wait-predicate workspace-refresh-complete)
        fi
        if [[ "$name" != "command-palette-dismissed" ]]; then
            capture_args+=(--wait-window-action set-command-palette-query)
        fi
        case "$name" in
            command-palette-files)
                capture_args+=(
                    --window-string-action "set-command-palette-mode=files"
                    --window-string-action "set-command-palette-query=visual-smoke"
                    --wait-atspi-text "visual-smoke.txt"
                )
                ;;
            command-palette-commands)
                capture_args+=(
                    --window-string-action "set-command-palette-mode=commands"
                    --window-string-action "set-command-palette-query=Save"
                    --wait-atspi-text "Save"
                )
                ;;
            command-palette-notes)
                capture_args+=(
                    --window-string-action "set-command-palette-mode=notes"
                    --window-string-action "set-command-palette-query=bookmark"
                    --wait-atspi-text "Visual bookmark"
                )
                ;;
            command-palette-no-results)
                capture_args+=(
                    --window-string-action "set-command-palette-mode=files"
                    --window-string-action "set-command-palette-query=missing-palette-file"
                    --wait-atspi-text "Command palette no results"
                )
                ;;
            command-palette-dense-files)
                capture_args+=(
                    --window-string-action "set-command-palette-mode=files"
                    --window-string-action "set-command-palette-query=palette-dense"
                    --wait-atspi-text "palette-dense-00.rs"
                )
                ;;
        esac
    fi
    for action in "${actions[@]}"; do
        if [[ "$action" =~ =(true|false)$ ]]; then
            capture_args+=(--window-bool-action "$action")
        else
            capture_args+=(--window-action "$action")
        fi
    done
    if ((${#variant_notes[@]} > 0)); then
        printf '%s\n' "${variant_notes[@]}" >"$ARTIFACT_DIR/assertions/${name}-variant.txt"
    fi

    if ! /usr/bin/python3 "${capture_args[@]}" >"$session_log" 2>&1; then
        if grep -qE 'AT-SPI registry did not register|Missing at-spi2-registryd|PipeWire did not become ready|Missing required command|No such schema|No such key|not writable|outside of the valid range' "$session_log"; then
            tail -n 120 "$session_log" >&2 || true
            write_visual_manifest "$name" "skipped" "host support unavailable" \
                "$fixture" "$output" "$width" "$height" "$search" "$minimap" \
                "$color_scheme" "$capture_dir" "$session_log" "${actions[@]}" || true
            smoke_skip "visual smoke host support unavailable during '${name}'. Artifacts: $ARTIFACT_DIR"
        fi
        tail -n 120 "$session_log" >&2 || true
        write_visual_manifest "$name" "failed" "capture helper failed" \
            "$fixture" "$output" "$width" "$height" "$search" "$minimap" \
            "$color_scheme" "$capture_dir" "$session_log" "${actions[@]}" || true
        smoke_fail "visual smoke capture '${name}' failed. Artifacts: $ARTIFACT_DIR"
    fi

    [[ -s "$output" ]] || smoke_fail "visual smoke screenshot is empty: $output"
    /usr/bin/python3 "$REPO_ROOT/scripts/assert-png-smoke.py" \
        "$output" \
        --max-width "$width" \
        --max-height "$height" \
        --require-top-band-detail \
        --require-bottom-band-detail \
        >"$ARTIFACT_DIR/assertions/${name}-png.txt"
    scan_visual_logs "$name" "$capture_dir"
    if [[ "$name" == "main-search-minimap" ]]; then
        assert_search_minimap_capture_artifacts "$name" "$capture_dir" "$fixture" "$search"
    fi
    case "$name" in
        modified-tab)
            assert_modified_tab_capture_artifacts "$name" "$capture_dir" "false"
            ;;
        destructive-close-dialog)
            assert_modified_tab_capture_artifacts "$name" "$capture_dir" "true"
            ;;
        file-health-properties)
            assert_file_health_capture_artifacts "$name" "$capture_dir"
            ;;
        local-history-restore)
            assert_local_history_restore_capture_artifacts "$name" "$capture_dir"
            ;;
    esac
    case "$name" in
        normal-properties|compact-properties|constrained-properties)
            assert_surface_capture_artifacts "$name" "$capture_dir" "true" "false" "false"
            ;;
        markdown-preview|constrained-preview)
            assert_surface_capture_artifacts "$name" "$capture_dir" "false" "true" "false"
            ;;
        markdown-preview-side-by-side|constrained-preview-side-by-side)
            assert_surface_capture_artifacts "$name" "$capture_dir" "false" "false" "true"
            ;;
    esac
    case "$name" in
        workspace-empty)
            assert_workspace_capture_artifacts "$name" "$capture_dir" 1 0 0 "false" "workspace"
            ;;
        workspace-representative|workspace-constrained|workspace-refresh)
            assert_workspace_capture_artifacts "$name" "$capture_dir" 2 3 3 "false" "all"
            ;;
        workspace-dense-awkward)
            assert_workspace_capture_artifacts "$name" "$capture_dir" 1 8 8 "false" "all"
            ;;
        workspace-tree-context-menu)
            assert_workspace_context_menu_capture_artifacts "$name" "New File"
            ;;
        workspace-header-context-menu)
            assert_workspace_context_menu_capture_artifacts "$name" "Rename Workspace"
            ;;
    esac
    case "$name" in
        notes-empty)
            assert_notes_capture_artifacts "$name" "$capture_dir" 0 0 "No notes yet"
            ;;
        notes-few|notes-constrained)
            assert_notes_capture_artifacts "$name" "$capture_dir" 2 2 "note-target-00.md"
            ;;
        notes-dense)
            assert_notes_capture_artifacts "$name" "$capture_dir" 8 8 "note-target-00.md"
            ;;
        bookmarks-few|bookmarks-constrained)
            assert_notes_capture_artifacts "$name" "$capture_dir" 2 2 "Visual bookmark"
            ;;
        bookmarks-dense)
            assert_notes_capture_artifacts "$name" "$capture_dir" 8 8 "Visual bookmark"
            ;;
    esac
    case "$name" in
        command-palette-files)
            assert_command_palette_capture_artifacts "$name" "$capture_dir" "true" "files" "visual-smoke" 1 "any" 0
            ;;
        command-palette-commands)
            assert_command_palette_capture_artifacts "$name" "$capture_dir" "true" "commands" "Save" 1 "any" 0
            ;;
        command-palette-notes)
            assert_command_palette_capture_artifacts "$name" "$capture_dir" "true" "notes" "bookmark" 1 "any" 0
            ;;
        command-palette-no-results)
            assert_command_palette_capture_artifacts "$name" "$capture_dir" "true" "files" "missing-palette-file" 0 0 0
            ;;
        command-palette-dense-files)
            assert_command_palette_capture_artifacts "$name" "$capture_dir" "true" "files" "palette-dense" 50 "any" 64
            ;;
        command-palette-dismissed)
            assert_command_palette_capture_artifacts "$name" "$capture_dir" "false" "all" "" 0 0 0
            ;;
    esac
    if [[ "$name" == "recovery-startup" ]]; then
        assert_recovery_capture_artifacts "$name" "$capture_dir"
    fi
    if command -v file >/dev/null 2>&1; then
        file "$output" >"$ARTIFACT_DIR/assertions/${name}-file.txt" || true
    fi
    write_visual_manifest "$name" "passed" "" \
        "$fixture" "$output" "$width" "$height" "$search" "$minimap" \
        "$color_scheme" "$capture_dir" "$session_log" "${actions[@]}"
    echo "PASS: visual smoke '${name}' captured at $output"
}

run_selected_capture() {
    local name="$1"
    if ! case_selected "$name"; then
        return
    fi
    VISUAL_CASES_RUN=$((VISUAL_CASES_RUN + 1))
    run_capture "$@"
}

run_selected_capture "main-search-minimap" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/main-search-minimap.png" "$WIDTH" "$HEIGHT" "needle" "1" "default"
run_selected_capture "modified-tab" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/modified-tab.png" "1280" "860" "" "1" "default"
run_selected_capture "destructive-close-dialog" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/destructive-close-dialog.png" "1280" "860" "" "0" "default"
run_selected_capture "file-health-properties" "$FILE_HEALTH_FIXTURE" "$ARTIFACT_DIR/screenshots/file-health-properties.png" "1280" "860" "" "0" "default" "toggle-properties"
run_selected_capture "local-history-restore" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/local-history-restore.png" "1280" "860" "" "0" "default"
run_selected_capture "normal-properties" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/normal-properties.png" "1280" "860" "" "0" "default" "toggle-properties"
run_selected_capture "compact-properties" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/compact-properties.png" "760" "720" "" "0" "default" "toggle-properties"
run_selected_capture "constrained-properties" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/constrained-properties.png" "760" "520" "" "0" "default" "toggle-properties"
run_selected_capture "short-layout" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/short-layout.png" "1200" "420" "" "0" "default"
run_selected_capture "markdown-preview" "$MARKDOWN_FIXTURE" "$ARTIFACT_DIR/screenshots/markdown-preview.png" "1280" "860" "" "0" "default" "toggle-preview-mode"
run_selected_capture "constrained-preview" "$MARKDOWN_FIXTURE" "$ARTIFACT_DIR/screenshots/constrained-preview.png" "760" "520" "" "0" "default" "toggle-preview-mode"
run_selected_capture "markdown-preview-side-by-side" "$MARKDOWN_FIXTURE" "$ARTIFACT_DIR/screenshots/markdown-preview-side-by-side.png" "1280" "860" "" "0" "default" "set-preview-pane-visible=true"
run_selected_capture "constrained-preview-side-by-side" "$MARKDOWN_FIXTURE" "$ARTIFACT_DIR/screenshots/constrained-preview-side-by-side.png" "760" "520" "" "0" "default" "set-preview-pane-visible=true"
run_selected_capture "workspace-empty" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/workspace-empty.png" "1280" "860" "" "0" "default"
run_selected_capture "workspace-representative" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/workspace-representative.png" "1280" "860" "" "0" "default"
run_selected_capture "workspace-dense-awkward" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/workspace-dense-awkward.png" "1280" "860" "" "0" "default"
run_selected_capture "workspace-constrained" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/workspace-constrained.png" "760" "520" "" "0" "default"
run_selected_capture "workspace-refresh" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/workspace-refresh.png" "1280" "860" "" "0" "default"
run_selected_capture "workspace-tree-context-menu" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/workspace-tree-context-menu.png" "1280" "860" "" "0" "default"
run_selected_capture "workspace-header-context-menu" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/workspace-header-context-menu.png" "1280" "860" "" "0" "default"
run_selected_capture "notes-empty" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/notes-empty.png" "1280" "860" "" "0" "default" "show-notes"
run_selected_capture "notes-few" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/notes-few.png" "1280" "860" "" "0" "default" "show-notes"
run_selected_capture "bookmarks-few" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/bookmarks-few.png" "1280" "860" "" "0" "default" "show-notes"
run_selected_capture "notes-dense" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/notes-dense.png" "1280" "860" "" "0" "default" "show-notes"
run_selected_capture "bookmarks-dense" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/bookmarks-dense.png" "1280" "860" "" "0" "default" "show-notes"
run_selected_capture "notes-constrained" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/notes-constrained.png" "760" "520" "" "0" "default" "show-notes"
run_selected_capture "bookmarks-constrained" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/bookmarks-constrained.png" "760" "520" "" "0" "default" "show-notes"
run_selected_capture "command-palette-files" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/command-palette-files.png" "1280" "860" "" "0" "default" "toggle-command-palette"
run_selected_capture "command-palette-commands" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/command-palette-commands.png" "1280" "860" "" "0" "default" "toggle-command-palette"
run_selected_capture "command-palette-notes" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/command-palette-notes.png" "1280" "860" "" "0" "default" "toggle-command-palette"
run_selected_capture "command-palette-no-results" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/command-palette-no-results.png" "1280" "860" "" "0" "default" "toggle-command-palette"
run_selected_capture "command-palette-dense-files" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/command-palette-dense-files.png" "1280" "860" "" "0" "default" "toggle-command-palette"
run_selected_capture "command-palette-dismissed" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/command-palette-dismissed.png" "1280" "860" "" "0" "default" "toggle-command-palette" "toggle-command-palette"
run_selected_capture "dark-style" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/dark-style.png" "$WIDTH" "$HEIGHT" "" "0" "force-dark"
run_selected_capture "high-contrast-style" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/high-contrast-style.png" "$WIDTH" "$HEIGHT" "" "0" "default"
run_selected_capture "large-text-constrained" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/large-text-constrained.png" "760" "520" "" "0" "default"
run_selected_capture "reduced-motion-command-palette" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/reduced-motion-command-palette.png" "1280" "860" "" "0" "default" "toggle-command-palette"
run_selected_capture "transparency-readability" "$MARKDOWN_FIXTURE" "$ARTIFACT_DIR/screenshots/transparency-readability.png" "1280" "860" "" "0" "default" "set-preview-pane-visible=true"
run_selected_capture "recovery-startup" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/recovery-startup.png" "1280" "860" "" "0" "default"

if ((VISUAL_CASES_RUN == 0)); then
    smoke_fail "no visual smoke cases matched filter: $VISUAL_CASE_FILTERS"
fi

{
    echo "screenshots=$ARTIFACT_DIR/screenshots"
    echo "captures=$ARTIFACT_DIR/captures"
    echo "assertions=$ARTIFACT_DIR/assertions"
    find "$ARTIFACT_DIR/screenshots" -maxdepth 1 -type f -name '*.png' -print | sort
    find "$ARTIFACT_DIR/assertions" -maxdepth 1 -type f -name '*-manifest.json' -print | sort
} >"$ARTIFACT_DIR/summary.txt"

/usr/bin/python3 - "$ARTIFACT_DIR" "$VISUAL_CASE_FILTERS" "$REPO_ROOT" <<'PY'
import json
import sys
from pathlib import Path

root = Path(sys.argv[1]).resolve()
case_filters = [item for item in sys.argv[2].split(",") if item]
repo_root = Path(sys.argv[3]).resolve()
assertions = root / "assertions"
screenshots = root / "screenshots"
sys.path.insert(0, str(repo_root / "scripts"))
from accessibility_source_fingerprint import source_fingerprint

def rel(path: Path) -> str:
    try:
        return path.resolve().relative_to(root).as_posix()
    except ValueError:
        return path.as_posix()

manifests = sorted(assertions.glob("*-manifest.json"))
scenario_ids = [path.name.removesuffix("-manifest.json") for path in manifests]
matrix_rows = []
for manifest_path in manifests:
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        continue
    matrix_rows.extend(manifest.get("matrix_rows", []))
matrix_rows = sorted(set(matrix_rows))
warning_files = sorted(assertions.glob("*-warnings.txt"))
unexpected_warning_count = 0
for path in warning_files:
    if path.is_file():
        unexpected_warning_count += len(
            [line for line in path.read_text(encoding="utf-8", errors="replace").splitlines() if line]
        )

coverage = {
    "focus_indication_cases": [
        name
        for name in scenario_ids
        if name
        in {
            "main-search-minimap",
            "modified-tab",
            "destructive-close-dialog",
            "file-health-properties",
            "local-history-restore",
            "normal-properties",
            "compact-properties",
            "constrained-properties",
            "markdown-preview",
            "constrained-preview",
            "markdown-preview-side-by-side",
            "constrained-preview-side-by-side",
            "workspace-representative",
            "workspace-dense-awkward",
            "workspace-constrained",
            "notes-few",
            "bookmarks-few",
            "notes-constrained",
            "bookmarks-constrained",
            "command-palette-files",
            "command-palette-no-results",
            "reduced-motion-command-palette",
            "recovery-startup",
        }
    ],
    "variant_cases": {
        "dark": [name for name in scenario_ids if name == "dark-style"],
        "high_contrast": [name for name in scenario_ids if name == "high-contrast-style"],
        "large_text": [name for name in scenario_ids if name == "large-text-constrained"],
        "reduced_motion": [name for name in scenario_ids if name == "reduced-motion-command-palette"],
        "transparency_readability": [name for name in scenario_ids if name == "transparency-readability"],
    },
    "color_not_only_cases": [
        name
        for name in scenario_ids
        if name
        in {
            "main-search-minimap",
            "modified-tab",
            "destructive-close-dialog",
            "file-health-properties",
            "local-history-restore",
            "bookmarks-few",
            "bookmarks-dense",
            "recovery-startup",
            "high-contrast-style",
            "transparency-readability",
        }
    ],
    "constrained_geometry_cases": [
        name
        for name in scenario_ids
        if "constrained" in name or name in {"short-layout", "large-text-constrained"}
    ],
    "unsupported_variants": [],
}

payload = {
    "schema_version": 1,
    "status": "passed" if unexpected_warning_count == 0 else "failed",
    "lane": "visual-smoke",
    "case_filters": case_filters or ["all"],
    "scenario_source": {
        "manifest_count": len(manifests),
        "manifests": [rel(path) for path in manifests],
    },
    "matrix_coverage": {
        "row_count": len(matrix_rows),
        "rows": matrix_rows,
        "focused_run": (case_filters or ["all"]) != ["all"],
    },
    "screenshots": [rel(path) for path in sorted(screenshots.glob("*.png"))],
    "warnings": {
        "status": "passed" if unexpected_warning_count == 0 else "found",
        "unexpected_count": unexpected_warning_count,
        "artifacts": [rel(path) for path in warning_files],
    },
    "visual_accessibility_coverage": coverage,
    "environment_report": {"artifact": "environment.txt"},
    "source_fingerprint": source_fingerprint(repo_root),
}
(root / "summary.json").write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

echo "PASS: visual smoke screenshots and artifacts captured under $ARTIFACT_DIR"
