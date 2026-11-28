#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Regression guard for the workspace folder-set rename.

Compatibility bridges still need to mention old sidecar and payload names,
but ordinary code, resources, fixtures, and docs must speak in folder terms.
Zero-Cargo replacement for crates/lushtext-ui/tests/workspace_terminology.rs so
both `make check-policy` and `make check-policy-docs` enforce it without
compiling Rust.
"""

from __future__ import annotations

import os
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]

FORBIDDEN_TERMS: tuple[tuple[str, str], ...] = (
    ("WorkspaceRoot", "workspaceroot"),
    ("workspace_root", "workspace_root"),
    ("workspace-root", "workspace-root"),
    ("workspace root", "workspace root"),
    ("root_paths", "root_paths"),
    ("v1_root_payload", "v1_root_payload"),
    ("search(query, roots, ...)", "search(query, roots"),
    ("Open Workspace Note", "open workspace note"),
    ("WorkspaceNote", "workspacenote"),
    ("workspace_note", "workspace_note"),
    ("workspace-note", "workspace-note"),
    ("workspace note", "workspace note"),
    ("display_root", "display_root"),
    ("canonical_root", "canonical_root"),
)

SCAN_TARGETS: tuple[str, ...] = (
    "AGENTS.md",
    "README.md",
    "data",
    "docs",
    "openspec/specs",
    "resources/ui",
    "crates/lushtext-model/benches",
    "crates/lushtext-model/src",
    "crates/lushtext-model/tests",
    "crates/lushtext-services/benches",
    "crates/lushtext-services/src",
    "crates/lushtext-services/tests",
    "crates/lushtext-ui/benches",
    "crates/lushtext-ui/src",
    "crates/lushtext-ui/tests",
    "crates/lushtext/src",
    "crates/lushtext/tests",
)

PATH_SCAN_TARGETS: tuple[str, ...] = (
    "data",
    "resources/ui",
    "crates/lushtext-model/benches",
    "crates/lushtext-model/src",
    "crates/lushtext-model/tests",
    "crates/lushtext-services/benches",
    "crates/lushtext-services/src",
    "crates/lushtext-services/tests",
    "crates/lushtext-ui/benches",
    "crates/lushtext-ui/src",
    "crates/lushtext-ui/tests",
    "crates/lushtext/src",
    "crates/lushtext/tests",
)

SCANNABLE_EXTENSIONS: frozenset[str] = frozenset({".json", ".md", ".rs", ".ui", ".xml", ".in"})
SELF = "scripts/check-terminology.py"

LEGACY_FOLDER_NOTE_FILES: frozenset[str] = frozenset(
    {
        "README.md",
        "docs/recovery-reliability.md",
        "openspec/specs/workspace-notes/spec.md",
        "crates/lushtext-model/src/persistence/migration_ledger.rs",
        "crates/lushtext-services/src/persistence/app_data_layout.rs",
        "crates/lushtext-services/src/persistence/app_data_leftovers.rs",
        "crates/lushtext-services/src/notes/folder_note_service.rs",
        "crates/lushtext-services/src/persistence/format_upgrade/diagnostics.rs",
        "crates/lushtext-services/src/persistence/format_upgrade/inventory.rs",
        "crates/lushtext-services/src/persistence/json_format.rs",
        "crates/lushtext-services/src/notes/note_storage.rs",
        "crates/lushtext-services/tests/persistent_json_format.rs",
        "crates/lushtext-services/tests/fixtures/persistent_json/legacy-folder-note-sidecar-v1.json",
    }
)

LEGACY_FOLDER_NOTE_IDENTITY_FILES: frozenset[str] = frozenset(
    {
        "crates/lushtext-model/src/persistence/folder_note.rs",
        "crates/lushtext-services/src/notes/folder_note_service.rs",
        "crates/lushtext-services/tests/fixtures/persistent_json/legacy-folder-note-sidecar-v1.json",
    }
)

# Current workspace-search traversal API surface.
# A search "display root" is one configured folder's traversal and presentation
# anchor, not the retired folder-note identity field this audit guards against.
SEARCH_TRAVERSAL_DISPLAY_ROOT_FILES: frozenset[str] = frozenset(
    {
        "crates/lushtext-model/src/workspace/workspace_search.rs",
        "crates/lushtext-services/benches/benchmarks.rs",
    }
)

WORKSPACE_NOTE_NEEDLES: frozenset[str] = frozenset(
    {"workspace-note", "workspace_note", "workspace note", "workspacenote"}
)

LEGACY_LINE_MARKERS: tuple[str, ...] = (
    "legacy",
    "older",
    "workspace-note-sidecar",
    "workspace-notes",
    "workspacenotes",
    "compatibility",
)


def is_legacy_folder_note_compatibility(relative: str, line: str) -> bool:
    lower = line.lower()
    return relative in LEGACY_FOLDER_NOTE_FILES and any(
        marker in lower for marker in LEGACY_LINE_MARKERS
    )


def is_allowed_compatibility_reference(relative: str, needle: str, line: str) -> bool:
    if needle in WORKSPACE_NOTE_NEEDLES:
        return is_legacy_folder_note_compatibility(relative, line)
    if needle == "display_root":
        return (
            relative in LEGACY_FOLDER_NOTE_IDENTITY_FILES
            or relative in SEARCH_TRAVERSAL_DISPLAY_ROOT_FILES
        )
    if needle == "canonical_root":
        return relative in LEGACY_FOLDER_NOTE_IDENTITY_FILES
    return False


def is_allowed_compatibility_path(relative: str, needle: str) -> bool:
    return (
        relative
        == "crates/lushtext-services/tests/fixtures/persistent_json/legacy-folder-note-sidecar-v1.json"
        and needle in WORKSPACE_NOTE_NEEDLES
    )


def _collect_scannable_files(target: Path, out: list[Path]) -> None:
    if not target.exists():
        return
    if target.is_file():
        if target.suffix in SCANNABLE_EXTENSIONS:
            out.append(target)
        return
    if target.is_dir():
        for root, dirs, files in os.walk(target, followlinks=False):
            dirs.sort()
            files.sort()
            root_path = Path(root)
            for name in files:
                p = root_path / name
                if p.suffix in SCANNABLE_EXTENSIONS and p.is_file():
                    out.append(p)


def _collect_all_paths(target: Path, out: list[Path]) -> None:
    if not target.exists() and not target.is_symlink():
        return
    if target.is_dir():
        out.append(target)
        for root, dirs, files in os.walk(target, followlinks=False):
            dirs.sort()
            files.sort()
            root_path = Path(root)
            for d in dirs:
                out.append(root_path / d)
            for name in files:
                out.append(root_path / name)
        return
    out.append(target)


def check_terminology(repo_root: Path = REPO_ROOT) -> list[str]:
    findings: list[str] = []

    content_files: list[Path] = []
    for target in SCAN_TARGETS:
        _collect_scannable_files(repo_root / target, content_files)
    for file_path in content_files:
        relative = file_path.relative_to(repo_root).as_posix()
        if relative == SELF:
            continue
        text = file_path.read_text(encoding="utf-8")
        for line_index, line in enumerate(text.splitlines(), start=1):
            lower = line.lower()
            for label, needle in FORBIDDEN_TERMS:
                if needle in lower and not is_allowed_compatibility_reference(
                    relative, needle, line
                ):
                    findings.append(
                        f"{relative}:{line_index} contains {label}: {line.strip()}"
                    )

    all_paths: list[Path] = []
    for target in PATH_SCAN_TARGETS:
        _collect_all_paths(repo_root / target, all_paths)
    for path in all_paths:
        relative = path.relative_to(repo_root).as_posix()
        if relative == SELF:
            continue
        lower_path = relative.lower()
        for label, needle in FORBIDDEN_TERMS:
            if needle in lower_path and not is_allowed_compatibility_path(relative, needle):
                findings.append(f"{relative} path contains {label}")

    return findings


def run_self_test() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        (root / "docs").mkdir(parents=True)
        (root / "crates/lushtext-model/src/workspace").mkdir(parents=True)

        bad_doc = root / "docs/bad.md"
        bad_doc.write_text("Open WorkspaceRoot here\n", encoding="utf-8")
        findings = check_terminology(root)
        assert any("docs/bad.md:1 contains WorkspaceRoot" in f for f in findings), findings
        bad_doc.unlink()

        bad_path = root / "crates/lushtext-model/src/workspace_root.rs"
        bad_path.write_text("pub fn ok() {}\n", encoding="utf-8")
        findings = check_terminology(root)
        assert any("workspace_root.rs path contains workspace_root" in f for f in findings), findings
        bad_path.unlink()

        readme = root / "README.md"
        readme.write_text("compatibility bridge for workspace-note\n", encoding="utf-8")
        assert not check_terminology(root)
        readme.write_text("use workspace-note directly\n", encoding="utf-8")
        findings = check_terminology(root)
        assert any("README.md:1 contains workspace-note" in f for f in findings), findings
        readme.unlink()

        ws_search = root / "crates/lushtext-model/src/workspace/workspace_search.rs"
        ws_search.write_text("pub struct S { display_root: usize }\n", encoding="utf-8")
        assert not check_terminology(root)
        other_rs = root / "crates/lushtext-model/src/other.rs"
        other_rs.write_text("let display_root = 1;\n", encoding="utf-8")
        findings = check_terminology(root)
        assert any("other.rs:1 contains display_root" in f for f in findings), findings

    print("check-terminology self-test: ok")


def main(argv: list[str] | None = None) -> int:
    args = sys.argv[1:] if argv is None else argv
    if args == ["--self-test"]:
        run_self_test()
        return 0
    if args:
        print("usage: scripts/check-terminology.py [--self-test]", file=sys.stderr)
        return 64
    findings = check_terminology(REPO_ROOT)
    if findings:
        print(
            "old workspace-root/workspace-note terminology escaped the compatibility boundary:",
            file=sys.stderr,
        )
        for finding in findings:
            print(finding, file=sys.stderr)
        return 1
    print("check-terminology passed (zero forbidden workspace-root/workspace-note terms)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
