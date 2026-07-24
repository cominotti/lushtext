#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Fingerprint the source/docs/tooling that define accessibility proof.

The compared digest covers only the *contents* of the relevant files (tracked
or not), so smoke proof stays valid across git bookkeeping transitions —
staging, committing, or branch motion with identical bytes — and is voided
exactly when a relevant file's bytes change. `git_head` and `relevant_status`
are recorded as informational forensics only and MUST stay out of the digest.
"""

from __future__ import annotations

import hashlib
import json
import subprocess
from pathlib import Path


RELEVANT_EXACT_PATHS = (
    ".agents/skills/gtk-agentic-debugging/scripts/atspi-accessible-action.py",
    ".agents/skills/gtk-agentic-debugging/scripts/atspi-click-button.py",
    ".agents/skills/gtk-agentic-debugging/scripts/atspi-dump-tree.py",
    ".agents/skills/gtk-agentic-debugging/scripts/atspi-set-text.py",
    ".agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py",
    "scripts/accessibility_source_fingerprint.py",
    "scripts/accessibility_warning_allowlist.py",
    "scripts/run-accessibility-smoke.sh",
    "scripts/run-visual-smoke.sh",
    "scripts/smoke_warning_classifiers.py",
    "docs/accessibility.md",
    "docs/accessibility-matrix.md",
    "docs/accessibility-orca-checklist.md",
    "docs/automation.md",
    "docs/automation-reference.md",
    "docs/end-user-coverage.md",
)

RELEVANT_PREFIXES = (
    "crates/lushtext-core/src/ui/",
    "crates/lushtext/tests/widget/",
    "resources/style/",
    "resources/ui/",
)


def source_fingerprint(repo_root: Path) -> dict[str, object]:
    repo_root = repo_root.resolve()
    entries = list(source_entries(repo_root))
    status = relevant_status(repo_root)
    head = git_output(repo_root, ["rev-parse", "HEAD"]) or None
    return {
        "schema_version": 2,
        "sha256": entries_digest(entries),
        "git_head": head,
        "dirty": bool(status),
        "path_count": len(entries),
        "relevant_status": status,
    }


def entries_digest(entries: list[dict[str, object]]) -> str:
    """Digest relevant-file contents only, never git bookkeeping state.

    Including `git_head` or porcelain status lines here would void live smoke
    proof on staging or committing byte-identical trees, forcing pointless
    lane reruns without catching any additional drift; `source_entries`
    already hashes tracked and untracked relevant files from disk.
    """
    encoded = json.dumps(
        {"entries": entries},
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def source_entries(repo_root: Path):
    for rel_path in relevant_files(repo_root):
        path = repo_root / rel_path
        entry: dict[str, object] = {"path": rel_path}
        try:
            if path.is_file():
                data = path.read_bytes()
                entry.update(
                    {
                        "state": "file",
                        "size": len(data),
                        "sha256": hashlib.sha256(data).hexdigest(),
                    }
                )
            elif path.exists():
                entry["state"] = "non-file"
            else:
                entry["state"] = "missing"
        except OSError as exc:
            entry.update({"state": "error", "error": type(exc).__name__})
        yield entry


def relevant_files(repo_root: Path) -> list[str]:
    files = set(RELEVANT_EXACT_PATHS)
    for prefix in RELEVANT_PREFIXES:
        root = repo_root / prefix
        if root.is_dir():
            for path in root.rglob("*"):
                if path.is_file():
                    files.add(path.relative_to(repo_root).as_posix())
    return sorted(files)


def relevant_status(repo_root: Path) -> list[str]:
    rows = []
    for line in git_lines(repo_root, ["status", "--porcelain=v1", "--untracked-files=all"]):
        path = line[3:]
        if " -> " in path:
            _old, path = path.split(" -> ", 1)
        normalized = path.replace("\\", "/")
        if is_relevant(normalized):
            rows.append(line)
    return sorted(rows)


def is_relevant(path: str) -> bool:
    return path in RELEVANT_EXACT_PATHS or path.startswith(RELEVANT_PREFIXES)


def git_lines(repo_root: Path, args: list[str]) -> list[str]:
    text = git_output(repo_root, args)
    return [line.rstrip() for line in text.splitlines() if line.strip()]


def git_output(repo_root: Path, args: list[str]) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=repo_root,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return ""
    return result.stdout.strip()


def main() -> int:
    import argparse
    import sys

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("repo_root", nargs="?", default=Path.cwd(), type=Path)
    args = parser.parse_args()
    json.dump(source_fingerprint(args.repo_root), sys.stdout, indent=2, sort_keys=True)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
