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

usage() {
    cat <<'EOF'
Usage: scripts/run-visual-smoke.sh [--artifact-dir DIR] [--binary PATH]

Launch LushText in isolated headless Mutter sessions, capture representative
geometry-sensitive desktop states, and preserve screenshots/log artifacts.
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
smoke_write_environment_report "$ARTIFACT_DIR/environment.txt"

rm -rf "$ARTIFACT_DIR/screenshots" "$ARTIFACT_DIR/captures" "$ARTIFACT_DIR/assertions"
mkdir -p "$ARTIFACT_DIR/fixtures" "$ARTIFACT_DIR/screenshots" "$ARTIFACT_DIR/captures" "$ARTIFACT_DIR/assertions"

TEXT_FIXTURE="$ARTIFACT_DIR/fixtures/visual-smoke.txt"
MARKDOWN_FIXTURE="$ARTIFACT_DIR/fixtures/visual-smoke.md"
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
            | grep -E -v '^Gdk-Message: .*Error reading events from display: Broken pipe$' \
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
    few_names = {"notes-few", "notes-constrained", "bookmarks-few", "bookmarks-constrained"}
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
    local snapshot_path="$capture_dir/automation-snapshot.json"
    local summary_path="$ARTIFACT_DIR/assertions/${name}-surface-snapshot.txt"

    [[ -s "$snapshot_path" ]] || smoke_fail "visual smoke '${name}' did not write automation-snapshot.json"
    /usr/bin/python3 - "$snapshot_path" "$expected_properties" "$expected_preview_mode" >"$summary_path" <<'PY'
import json
import sys

snapshot_path, expected_properties, expected_preview_mode = sys.argv[1:]

def parse_expected(value):
    if value == "any":
        return None
    return value == "true"

with open(snapshot_path, encoding="utf-8") as handle:
    snapshot = json.load(handle)

window = snapshot["window"]
assert snapshot["enabled"] is True, snapshot
assert snapshot["idle"] is True, snapshot
assert window is not None, snapshot
surfaces = window["surfaces"]
properties = parse_expected(expected_properties)
preview_mode = parse_expected(expected_preview_mode)
if properties is not None:
    assert surfaces["document_properties_visible"] is properties, surfaces
    assert surfaces["document_properties_requested"] is properties, surfaces
if preview_mode is not None:
    assert surfaces["preview_mode"] is preview_mode, surfaces
print(f"document_properties_visible={surfaces['document_properties_visible']}")
print(f"document_properties_requested={surfaces['document_properties_requested']}")
print(f"preview_mode={surfaces['preview_mode']}")
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

    mkdir -p "$capture_dir"
    if [[ "$name" == "recovery-startup" ]]; then
        prepare_recovery_capture_state "$capture_dir"
    fi
    if [[ "$name" == workspace-* ]]; then
        prepare_workspace_capture_state "$name" "$capture_dir"
    fi
    if [[ "$name" == notes-* || "$name" == bookmarks-* ]]; then
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
    if [[ "$minimap" == "1" ]]; then
        capture_args+=(--enable-minimap)
    fi
    if [[ "$color_scheme" != "default" ]]; then
        capture_args+=(--color-scheme "$color_scheme")
    fi
    if [[ "$name" == "recovery-startup" ]]; then
        capture_args+=(
            --atspi-tree-output "$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
            --atspi-focus-output "$ARTIFACT_DIR/assertions/${name}-atspi-focus.txt"
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
                    --wait-atspi-text "Browse Bookmarks"
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
        capture_args+=(--window-action "$action")
    done

    if ! /usr/bin/python3 "${capture_args[@]}" >"$session_log" 2>&1; then
        if grep -qE 'AT-SPI registry did not register|Missing at-spi2-registryd|PipeWire did not become ready|Missing required command' "$session_log"; then
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
        normal-properties|compact-properties|constrained-properties)
            assert_surface_capture_artifacts "$name" "$capture_dir" "true" "false"
            ;;
        markdown-preview|constrained-preview)
            assert_surface_capture_artifacts "$name" "$capture_dir" "false" "true"
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

run_capture "main-search-minimap" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/main-search-minimap.png" "$WIDTH" "$HEIGHT" "needle" "1" "default"
run_capture "normal-properties" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/normal-properties.png" "1280" "860" "" "0" "default" "toggle-properties"
run_capture "compact-properties" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/compact-properties.png" "760" "720" "" "0" "default" "toggle-properties"
run_capture "constrained-properties" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/constrained-properties.png" "760" "520" "" "0" "default" "toggle-properties"
run_capture "short-layout" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/short-layout.png" "1200" "420" "" "0" "default"
run_capture "markdown-preview" "$MARKDOWN_FIXTURE" "$ARTIFACT_DIR/screenshots/markdown-preview.png" "1280" "860" "" "0" "default" "toggle-preview-mode"
run_capture "constrained-preview" "$MARKDOWN_FIXTURE" "$ARTIFACT_DIR/screenshots/constrained-preview.png" "760" "520" "" "0" "default" "toggle-preview-mode"
run_capture "workspace-empty" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/workspace-empty.png" "1280" "860" "" "0" "default"
run_capture "workspace-representative" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/workspace-representative.png" "1280" "860" "" "0" "default"
run_capture "workspace-dense-awkward" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/workspace-dense-awkward.png" "1280" "860" "" "0" "default"
run_capture "workspace-constrained" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/workspace-constrained.png" "760" "520" "" "0" "default"
run_capture "workspace-refresh" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/workspace-refresh.png" "1280" "860" "" "0" "default"
run_capture "notes-empty" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/notes-empty.png" "1280" "860" "" "0" "default" "show-notes"
run_capture "notes-few" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/notes-few.png" "1280" "860" "" "0" "default" "show-notes"
run_capture "bookmarks-few" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/bookmarks-few.png" "1280" "860" "" "0" "default" "show-notes"
run_capture "notes-dense" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/notes-dense.png" "1280" "860" "" "0" "default" "show-notes"
run_capture "bookmarks-dense" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/bookmarks-dense.png" "1280" "860" "" "0" "default" "show-notes"
run_capture "notes-constrained" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/notes-constrained.png" "760" "520" "" "0" "default" "show-notes"
run_capture "bookmarks-constrained" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/bookmarks-constrained.png" "760" "520" "" "0" "default" "show-notes"
run_capture "command-palette-files" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/command-palette-files.png" "1280" "860" "" "0" "default" "toggle-command-palette"
run_capture "command-palette-commands" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/command-palette-commands.png" "1280" "860" "" "0" "default" "toggle-command-palette"
run_capture "command-palette-notes" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/command-palette-notes.png" "1280" "860" "" "0" "default" "toggle-command-palette"
run_capture "command-palette-no-results" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/command-palette-no-results.png" "1280" "860" "" "0" "default" "toggle-command-palette"
run_capture "command-palette-dense-files" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/command-palette-dense-files.png" "1280" "860" "" "0" "default" "toggle-command-palette"
run_capture "command-palette-dismissed" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/command-palette-dismissed.png" "1280" "860" "" "0" "default" "toggle-command-palette" "toggle-command-palette"
run_capture "dark-style" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/dark-style.png" "$WIDTH" "$HEIGHT" "" "0" "force-dark"
run_capture "recovery-startup" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/recovery-startup.png" "1280" "860" "" "0" "default"

{
    echo "screenshots=$ARTIFACT_DIR/screenshots"
    echo "captures=$ARTIFACT_DIR/captures"
    echo "assertions=$ARTIFACT_DIR/assertions"
    find "$ARTIFACT_DIR/screenshots" -maxdepth 1 -type f -name '*.png' -print | sort
    find "$ARTIFACT_DIR/assertions" -maxdepth 1 -type f -name '*-manifest.json' -print | sort
} >"$ARTIFACT_DIR/summary.txt"

echo "PASS: visual smoke screenshots and artifacts captured under $ARTIFACT_DIR"
