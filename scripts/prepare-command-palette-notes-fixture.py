#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Seed representative command-palette Notes fixtures in an isolated root."""

from __future__ import annotations

import json
import sys
from pathlib import Path

FNV_OFFSET = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3
STAMP = 1_720_000_000


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


def folder_identity(path: Path) -> dict[str, str]:
    canonical = path.resolve()
    return {
        "display_folder": str(path),
        "canonical_folder": str(canonical),
        "sidecar_id": stable_path_hash(canonical),
    }


def note(text: str) -> dict[str, object]:
    return {
        "text": text.strip(),
        "created_at_secs": STAMP,
        "updated_at_secs": STAMP,
    }


def envelope(path: Path, kind: str, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps({"kind": kind, "version": 1, "data": data}, indent=2, sort_keys=True)
        + "\n",
        encoding="utf-8",
    )


def prepare_fixture(root: Path) -> None:
    data_dir = root / "data" / "lushtext"
    workspace = root / "fixtures" / "palette-notes-workspace"
    outside = root / "fixtures" / "outside-open-tab"
    data_dir.mkdir(parents=True, exist_ok=True)
    workspace.mkdir(parents=True, exist_ok=True)
    outside.mkdir(parents=True, exist_ok=True)

    bookmark_file = write_file(
        workspace / "src" / "palette-bookmark.rs",
        "\n".join(
            [
                "// palette bookmark fixture",
                "fn bookmark_anchor() {",
                "    println!(\"palette bookmark line\");",
                "}",
                "",
            ]
        ),
    )
    document_file = write_file(
        workspace / "docs" / "palette-document.md",
        "# Palette document fixture\n\nDocument body used by the Notes palette smoke.\n",
    )
    open_tab_file = write_file(
        outside / "palette-open-tab.md",
        "# Palette open tab fixture\n\nThis saved file is intentionally outside the workspace.\n",
    )

    workspace_data = {
        "current_scope": {"kind": "all"},
        "workspaces": [
            {
                "id": "ws-palette-notes",
                "name": "Palette Notes Workspace",
                "folders": [{"id": "folder-palette-notes", "path": str(workspace)}],
            }
        ],
    }
    envelope(data_dir / "workspaces.json", "dev.cominotti.lushtext.workspace-state", workspace_data)

    bookmark_identity = identity(bookmark_file)
    envelope(
        data_dir / "bookmarks" / f"{bookmark_identity['sidecar_id']}.json",
        "dev.cominotti.lushtext.bookmark-sidecar",
        {
            "identity": bookmark_identity,
            "bookmarks": [
                {
                    "id": "bookmark-palette-notes-smoke",
                    "line": 1,
                    "label": "Palette bookmark marker",
                    "created_at_secs": STAMP,
                    "updated_at_secs": STAMP,
                }
            ],
        },
    )

    folder_note_identity = folder_identity(workspace)
    envelope(
        data_dir / "folder-notes" / f"{folder_note_identity['sidecar_id']}.json",
        "dev.cominotti.lushtext.folder-note-sidecar",
        {
            "identity": folder_note_identity,
            "note": note("# Palette folder note\n\nFolder note body for palette search."),
        },
    )

    document_identity = identity(document_file)
    envelope(
        data_dir / "document-notes" / f"{document_identity['sidecar_id']}.json",
        "dev.cominotti.lushtext.document-note-sidecar",
        {
            "identity": document_identity,
            "note": note("# Palette document note\n\nDocument note body for palette search."),
        },
    )

    open_tab_identity = identity(open_tab_file)
    envelope(
        data_dir / "document-notes" / f"{open_tab_identity['sidecar_id']}.json",
        "dev.cominotti.lushtext.document-note-sidecar",
        {
            "identity": open_tab_identity,
            "note": note("# Palette open tab note\n\nOpen tab note body for palette search."),
        },
    )

    (root / "fixture-summary.json").write_text(
        json.dumps(
            {
                "workspace": str(workspace),
                "open_tab_file": str(open_tab_file),
                "workspace_bookmark_file": str(bookmark_file),
                "workspace_document_note_file": str(document_file),
                "data_dir": str(data_dir),
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: prepare-command-palette-notes-fixture.py ROOT", file=sys.stderr)
        return 2
    prepare_fixture(Path(argv[1]))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
