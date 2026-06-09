#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Require visual geometry proof when local visual-sensitive files changed."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_ARTIFACT_DIR = REPO_ROOT / "build/smoke/visual-geometry"
VISUAL_SENSITIVE_PREFIXES = (
    "crates/lushtext-core/src/ui/",
    "crates/lushtext/tests/widget/",
    "resources/ui/",
    "resources/style/",
    "scripts/visual-geometry-scenarios/",
)
VISUAL_SENSITIVE_EXACT = (
    "scripts/visual-geometry-smoke.py",
    "scripts/visual_geometry_png.py",
)
VISUAL_SENSITIVE_SUFFIXES = (
    ".blp",
    ".css",
    ".ui",
)


def run_git(args: list[str]) -> list[str]:
    result = subprocess.run(
        ["git", *args],
        cwd=REPO_ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return []
    return [line.rstrip() for line in result.stdout.splitlines() if line.strip()]


def status_changed_files() -> list[str]:
    files: list[str] = []
    for line in run_git(["status", "--porcelain=v1", "--untracked-files=all"]):
        path = line[3:]
        if " -> " in path:
            _old, path = path.split(" -> ", 1)
        if path:
            files.append(path)
    return sorted(set(files))


def base_ref_changed_files(base_ref: str) -> list[str]:
    files = run_git(["diff", "--name-only", f"{base_ref}...HEAD"])
    if files:
        return sorted(set(files))
    files = run_git(["diff", "--name-only", base_ref, "HEAD"])
    return sorted(set(files))


def changed_files(base_ref: str | None) -> list[str]:
    if base_ref:
        files = base_ref_changed_files(base_ref)
        if files:
            return files

    github_base = os.environ.get("GITHUB_BASE_REF")
    if github_base:
        for candidate in (f"origin/{github_base}", github_base):
            files = base_ref_changed_files(candidate)
            if files:
                return files

    return status_changed_files()


def is_visual_sensitive(path: str) -> bool:
    normalized = path.replace("\\", "/")
    return (
        normalized in VISUAL_SENSITIVE_EXACT
        or normalized.startswith(VISUAL_SENSITIVE_PREFIXES)
        or normalized.endswith(VISUAL_SENSITIVE_SUFFIXES)
    )


def visual_change_fingerprint(paths: list[str]) -> dict[str, object]:
    entries = []
    for path in sorted(set(path.replace("\\", "/") for path in paths)):
        absolute = REPO_ROOT / path
        entry: dict[str, object] = {"path": path}
        try:
            if absolute.is_file():
                data = absolute.read_bytes()
                entry.update(
                    {
                        "state": "file",
                        "size": len(data),
                        "sha256": hashlib.sha256(data).hexdigest(),
                    }
                )
            elif absolute.exists():
                entry["state"] = "non-file"
            else:
                entry["state"] = "missing"
        except OSError as exc:
            entry.update({"state": "error", "error": type(exc).__name__})
        entries.append(entry)

    encoded = json.dumps(entries, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return {
        "schema_version": 1,
        "digest": hashlib.sha256(encoded).hexdigest(),
        "files": entries,
    }


def visual_proof_policy_metadata(base_ref: str | None = None) -> dict[str, object]:
    visual_changes = [path for path in changed_files(base_ref) if is_visual_sensitive(path)]
    fingerprint = visual_change_fingerprint(visual_changes)
    return {
        "schema_version": 1,
        "changed_file_count": len(visual_changes),
        "changed_files": [
            {"path": item["path"], "state": item["state"]} for item in fingerprint["files"]
        ],
        "changed_files_digest": fingerprint["digest"],
    }


def read_summary(artifact_dir: Path) -> tuple[dict[str, object] | None, str | None]:
    path = artifact_dir / "summary.json"
    if not path.is_file():
        return None, f"missing visual geometry summary: {path}"
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        return None, f"malformed visual geometry summary {path}: {exc}"
    if not isinstance(payload, dict):
        return None, f"visual geometry summary is not an object: {path}"
    return payload, None


def proof_is_verified(summary: dict[str, object]) -> tuple[bool, str]:
    status = summary.get("status")
    case_count = summary.get("case_count")
    passed = summary.get("passed")
    failed = summary.get("failed")
    skipped = summary.get("skipped")
    if status != "passed":
        return False, f"summary status is {status!r}, not 'passed'"
    if not isinstance(case_count, int) or case_count <= 0:
        return False, "summary has no executed visual geometry cases"
    if failed not in (0, None):
        return False, f"summary reports failed cases: {failed}"
    if skipped not in (0, None):
        return False, f"summary reports skipped cases: {skipped}; skipped coverage is not proof"
    if not isinstance(passed, int) or passed <= 0:
        return False, "summary reports no passing visual geometry cases"
    if summary.get("case_filter"):
        return False, "filtered visual geometry runs do not satisfy visual proof policy"
    return True, "visual geometry proof summary passed"


def proof_matches_current_changes(
    summary: dict[str, object], visual_changes: list[str]
) -> tuple[bool, str]:
    metadata = summary.get("visual_proof_policy")
    if not isinstance(metadata, dict):
        return False, "summary has no current-diff fingerprint; rerun visual geometry smoke"
    recorded_digest = metadata.get("changed_files_digest")
    if not isinstance(recorded_digest, str):
        return False, "summary has no changed-files digest; rerun visual geometry smoke"
    current = visual_change_fingerprint(visual_changes)
    if current["digest"] != recorded_digest:
        return (
            False,
            "summary changed-files digest does not match current visual-sensitive diff; "
            "rerun visual geometry smoke",
        )
    return True, "summary matches current visual-sensitive diff"


def check_policy(artifact_dir: Path, base_ref: str | None) -> tuple[bool, str]:
    changed = changed_files(base_ref)
    visual_changes = [path for path in changed if is_visual_sensitive(path)]
    if not visual_changes:
        return True, "No local visual-sensitive changes require visual geometry proof."

    summary, error = read_summary(artifact_dir)
    if error:
        return (
            False,
            "\n".join(
                [
                    "Visual-sensitive changes require same-session visual geometry proof.",
                    "Changed files:",
                    *(f"  - {path}" for path in visual_changes),
                    error,
                    "Run `make visual-geometry-smoke` and rerun this check.",
                ]
            ),
        )
    assert summary is not None
    ok, detail = proof_is_verified(summary)
    if ok:
        matched, match_detail = proof_matches_current_changes(summary, visual_changes)
        if matched:
            return True, f"{detail}; {match_detail}: {artifact_dir / 'summary.json'}"
        detail = match_detail
    return (
        False,
        "\n".join(
            [
                "Visual-sensitive changes require a passing visual geometry proof.",
                "Changed files:",
                *(f"  - {path}" for path in visual_changes),
                detail,
                f"Artifact summary: {artifact_dir / 'summary.json'}",
            ]
        ),
    )


def run_self_tests() -> None:
    assert is_visual_sensitive("crates/lushtext-core/src/ui/window/imp.rs")
    assert is_visual_sensitive("resources/ui/window.blp")
    assert is_visual_sensitive("resources/style/style.css")
    assert is_visual_sensitive("scripts/visual-geometry-scenarios/example.json")
    assert is_visual_sensitive("scripts/visual-geometry-smoke.py")
    assert not is_visual_sensitive("docs/automation.md")
    assert visual_change_fingerprint(["docs/automation.md"])["digest"] != visual_change_fingerprint(
        ["missing-visual-proof-file.rs"]
    )["digest"]

    with tempfile.TemporaryDirectory() as directory:
        artifact_dir = Path(directory)
        artifact_dir.mkdir(exist_ok=True)
        summary_path = artifact_dir / "summary.json"

        summary_path.write_text(
            json.dumps(
                {
                    "status": "passed",
                    "case_count": 1,
                    "passed": 1,
                    "failed": 0,
                    "skipped": 0,
                    "cases": [],
                }
            ),
            encoding="utf-8",
        )
        summary, error = read_summary(artifact_dir)
        assert error is None
        assert summary is not None
        ok, _detail = proof_is_verified(summary)
        assert ok
        ok, detail = proof_matches_current_changes(summary, ["docs/automation.md"])
        assert not ok
        assert "fingerprint" in detail

        policy = visual_change_fingerprint(["docs/automation.md"])
        summary["visual_proof_policy"] = {"changed_files_digest": policy["digest"]}
        ok, _detail = proof_matches_current_changes(summary, ["docs/automation.md"])
        assert ok

        summary_path.write_text(
            json.dumps(
                {
                    "status": "passed",
                    "case_count": 1,
                    "passed": 0,
                    "failed": 0,
                    "skipped": 1,
                }
            ),
            encoding="utf-8",
        )
        summary, error = read_summary(artifact_dir)
        assert error is None
        assert summary is not None
        ok, detail = proof_is_verified(summary)
        assert not ok
        assert "skipped" in detail


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--artifact-dir",
        type=Path,
        default=DEFAULT_ARTIFACT_DIR,
        help="Visual geometry smoke artifact directory.",
    )
    parser.add_argument(
        "--base-ref",
        help="Optional git base ref for committed-change checks, for example origin/main.",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="Run parser and policy self-tests.",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    if args.self_test:
        run_self_tests()

    ok, detail = check_policy(args.artifact_dir.resolve(), args.base_ref)
    print(detail)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
