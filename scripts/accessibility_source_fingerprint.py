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
    "crates/lushtext-ui/src/ui/",
    "resources/style/",
    "resources/ui/",
)


def relevant_diff(repo_root: Path, base_ref: str) -> tuple[list[str], bool]:
    """Return sorted `is_relevant` paths changed between `base_ref` and HEAD."""
    if not base_ref:
        return [], False
    verify = git_output(repo_root, ["rev-parse", "--verify", "--quiet", f"{base_ref}^{{commit}}"])
    if not verify:
        return [], False
    lines, available = git_lines_checked(
        repo_root, ["diff", "--name-only", "--no-renames", f"{base_ref}...HEAD"]
    )
    if not available and not git_output(repo_root, ["merge-base", base_ref, "HEAD"]):
        lines, available = git_lines_checked(
            repo_root, ["diff", "--name-only", "--no-renames", base_ref, "HEAD"]
        )
    if not available:
        return [], False
    relevant = sorted({p.replace("\\", "/") for p in lines if is_relevant(p.replace("\\", "/"))})
    return relevant, True


def source_fingerprint(repo_root: Path, base_ref: str | None = None) -> dict[str, object]:
    repo_root = repo_root.resolve()
    entries = list(source_entries(repo_root))
    status, status_available = relevant_status(repo_root)
    head = git_output(repo_root, ["rev-parse", "HEAD"]) or None
    payload: dict[str, object] = {
        "schema_version": 2,
        "sha256": entries_digest(entries),
        "git_head": head,
        # Fail safe when git could not answer. `git_output` returns "" on a
        # non-zero exit, which is indistinguishable from "clean" — so a broken or
        # absent git made `dirty` false, which made smoke freshness *not required*,
        # which let the accessibility gate pass with no proof at all. An
        # unanswerable question is treated as dirty.
        "dirty": bool(status) or not status_available,
        "status_available": status_available,
        "path_count": len(entries),
        "relevant_status": status,
    }
    if base_ref is not None:
        branch_changes, branch_available = relevant_diff(repo_root, base_ref)
        payload["branch_relevant_changes"] = branch_changes
        payload["branch_status_available"] = branch_available
    return payload


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


def relevant_status(repo_root: Path) -> tuple[list[str], bool]:
    """Return relevant porcelain rows, and whether git actually answered.

    The second element is what keeps a git failure from reading as a clean tree.
    """
    lines, available = git_lines_checked(
        repo_root, ["status", "--porcelain=v1", "--untracked-files=all"]
    )
    rows = []
    for line in lines:
        path = line[3:]
        if " -> " in path:
            old, new = path.split(" -> ", 1)
            candidates = (old, new)
        else:
            candidates = (path,)
        if any(is_relevant(p.replace("\\", "/")) for p in candidates if p):
            rows.append(line)
    return sorted(rows), available


def is_relevant(path: str) -> bool:
    return path in RELEVANT_EXACT_PATHS or path.startswith(RELEVANT_PREFIXES)


def git_lines_checked(repo_root: Path, args: list[str]) -> tuple[list[str], bool]:
    """Return git output lines and whether the command succeeded."""
    result = subprocess.run(
        ["git", *args],
        cwd=repo_root,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return [], False
    lines = [line.rstrip() for line in result.stdout.splitlines() if line.strip()]
    return lines, True


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
