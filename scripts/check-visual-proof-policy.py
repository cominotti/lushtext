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
    "crates/lushtext-core/src/model/automation.rs",
    "scripts/check-visual-proof-policy.py",
    "scripts/lushtext-automation.py",
    "scripts/test-visual-geometry.py",
    "scripts/visual-geometry-smoke.py",
    "scripts/visual_geometry_png.py",
)
VISUAL_SENSITIVE_SUFFIXES = (
    ".blp",
    ".css",
    ".ui",
)
NATIVE_MINIMAP_HIGHLIGHT_INVARIANT = "native-minimap-highlight-anchors"
NATIVE_MINIMAP_ANIMATION_INVARIANT = "native-minimap-animation-highlight-anchors"


def delegate_cli_to_rust(argv: list[str]) -> int:
    """Run the Rust policy checker without masking its status or output."""

    command = [
        "cargo",
        "run",
        "-q",
        "-p",
        "cargo-gtk-proof",
        "--",
        "policy",
        *argv,
    ]
    try:
        return subprocess.run(command, cwd=REPO_ROOT, check=False).returncode
    except FileNotFoundError as exc:
        print(f"missing Rust proof tooling command: {exc.filename}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(delegate_cli_to_rust(sys.argv[1:]))


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


def required_invariants_for_changes(paths: list[str]) -> list[str]:
    # Keep this mapping narrow: these paths can change the native minimap
    # highlight, its Automation1 crop hints, or the pixel detector. Other
    # visual-sensitive files still need proof, but should not require this
    # specific invariant unless they touch the minimap effect.
    required: set[str] = set()
    for path in (item.replace("\\", "/") for item in paths):
        if (
            path == "crates/lushtext-core/src/ui/editor_page/minimap.rs"
            or path == "crates/lushtext-core/src/ui/automation.rs"
            or path == "crates/lushtext-core/src/model/automation.rs"
            or path == "resources/style/style.css"
            or path == "scripts/check-visual-proof-policy.py"
            or path == "scripts/lushtext-automation.py"
            or path == "scripts/test-visual-geometry.py"
            or path == "scripts/visual-geometry-smoke.py"
            or path == "scripts/visual_geometry_png.py"
            or path.startswith("scripts/visual-geometry-scenarios/minimap-sidebar-")
        ):
            required.add(NATIVE_MINIMAP_HIGHLIGHT_INVARIANT)
    return sorted(required)


def required_animation_invariants_for_changes(paths: list[str]) -> list[str]:
    # Animation coverage is narrower than general pixel coverage: require it
    # only when the diff can affect source-map rendering during editor/sidebar
    # width reflow or the animation proof lane itself.
    required: set[str] = set()
    for path in (item.replace("\\", "/") for item in paths):
        if (
            path == "crates/lushtext-core/src/ui/editor_page/imp.rs"
            or path == "crates/lushtext-core/src/ui/editor_page/minimap.rs"
            or path == "crates/lushtext-core/src/ui/editor_page/overscroll.rs"
            or path == "crates/lushtext-core/src/ui/automation.rs"
            or path == "crates/lushtext-core/src/model/automation.rs"
            or path == "scripts/check-visual-proof-policy.py"
            or path == "scripts/lushtext-automation.py"
            or path == "scripts/test-visual-geometry.py"
            or path == "scripts/visual-geometry-smoke.py"
            or path == "scripts/visual_geometry_png.py"
            or path.startswith("scripts/visual-geometry-scenarios/minimap-sidebar-")
        ):
            required.add(NATIVE_MINIMAP_ANIMATION_INVARIANT)
    return sorted(required)


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
        "required_invariant_ids": required_invariants_for_changes(visual_changes),
        "required_animation_invariant_ids": required_animation_invariants_for_changes(
            visual_changes
        ),
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


def proof_covers_required_invariants(
    summary: dict[str, object], visual_changes: list[str]
) -> tuple[bool, str]:
    required = required_invariants_for_changes(visual_changes)
    if not required:
        return proof_covers_required_animation_invariants(summary, visual_changes)
    # Require the pixel-specific field, not generic `verified_invariant_ids`, so
    # rectangle-only smoke runs cannot satisfy a pixel-anchor policy gate.
    verified = summary.get("pixel_verified_invariant_ids")
    if not isinstance(verified, list):
        return (
            False,
            "summary has no pixel_verified_invariant_ids; rerun visual geometry smoke",
        )
    missing = sorted(set(required) - {str(item) for item in verified})
    if missing:
        return False, (
            "summary did not pixel-verify required visual invariant ids: "
            f"{', '.join(missing)}"
        )
    evidence_ok, evidence_detail = proof_has_required_case_evidence(summary, required)
    if not evidence_ok:
        return False, evidence_detail
    animation_ok, animation_detail = proof_covers_required_animation_invariants(
        summary,
        visual_changes,
    )
    if not animation_ok:
        return False, animation_detail
    return True, (
        f"summary pixel-verified required visual invariant ids: {', '.join(required)}; "
        f"{animation_detail}"
    )


def proof_covers_required_animation_invariants(
    summary: dict[str, object],
    visual_changes: list[str],
) -> tuple[bool, str]:
    required = required_animation_invariants_for_changes(visual_changes)
    if not required:
        return True, "no animation-frame invariants required by current diff"
    verified = summary.get("animation_verified_invariant_ids")
    if not isinstance(verified, list):
        return (
            False,
            "summary has no animation_verified_invariant_ids; rerun visual geometry smoke with animation sampling",
        )
    missing = sorted(set(required) - {str(item) for item in verified})
    if missing:
        return False, (
            "summary did not animation-verify required visual invariant ids: "
            f"{', '.join(missing)}"
        )
    evidence_ok, evidence_detail = proof_has_required_animation_case_evidence(summary, required)
    if not evidence_ok:
        return False, evidence_detail
    return True, f"summary animation-verified required visual invariant ids: {', '.join(required)}"


def proof_has_required_case_evidence(
    summary: dict[str, object],
    required: list[str],
) -> tuple[bool, str]:
    cases = summary.get("cases")
    if not isinstance(cases, list):
        return False, "summary has no case rows with pixel evidence; rerun visual geometry smoke"
    for invariant_id in required:
        matching_cases = [
            case
            for case in cases
            if isinstance(case, dict)
            and case.get("status") == "passed"
            and invariant_id in [str(item) for item in case.get("pixel_verified_invariant_ids", [])]
        ]
        if not matching_cases:
            return False, f"summary has no passing pixel-evidence case for {invariant_id}"
        if not any(case_has_actionable_pixel_evidence(case) for case in matching_cases):
            return False, f"summary case for {invariant_id} lacks pixel rows or final geometry"
    return True, "required visual invariant cases include pixel rows and final geometry"


def proof_has_required_animation_case_evidence(
    summary: dict[str, object],
    required: list[str],
) -> tuple[bool, str]:
    cases = summary.get("cases")
    if not isinstance(cases, list):
        return False, "summary has no case rows with animation evidence; rerun visual geometry smoke"
    for invariant_id in required:
        matching_cases = [
            case
            for case in cases
            if isinstance(case, dict)
            and case.get("status") == "passed"
            and invariant_id
            in [str(item) for item in case.get("animation_verified_invariant_ids", [])]
        ]
        if not matching_cases:
            return False, f"summary has no passing animation-evidence case for {invariant_id}"
        if not any(case_has_actionable_animation_evidence(case) for case in matching_cases):
            return False, f"summary case for {invariant_id} lacks animation frame rows"
    return True, "required visual animation cases include sampled frame rows"


def case_has_actionable_pixel_evidence(case: dict[str, object]) -> bool:
    evidence = case.get("pixel_anchor_evidence")
    final_geometry = case.get("final_geometry")
    if not isinstance(evidence, list) or not evidence:
        return False
    if not isinstance(final_geometry, dict):
        return False
    return any(
        isinstance(row, dict)
        and row.get("before_row_y") is not None
        and row.get("after_row_y") is not None
        for row in evidence
    )


def case_has_actionable_animation_evidence(case: dict[str, object]) -> bool:
    evidence = case.get("animation_frame_evidence")
    if not isinstance(evidence, dict):
        return False
    if evidence.get("status") != "passed":
        return False
    if evidence.get("capture_mode") != "stream":
        return False
    if not isinstance(evidence.get("sampled_frame_count"), int) or int(evidence["sampled_frame_count"]) <= 0:
        return False
    if (
        not isinstance(evidence.get("mapped_intermediate_frame_count"), int)
        or int(evidence["mapped_intermediate_frame_count"]) <= 0
    ):
        return False
    if not isinstance(evidence.get("max_sample_skew_ms"), int):
        return False
    observed_skew = evidence.get("max_sample_skew_observed_ms")
    if observed_skew is None or not isinstance(observed_skew, int):
        return False
    if observed_skew > int(evidence["max_sample_skew_ms"]):
        return False
    frames = evidence.get("frames")
    if not isinstance(frames, list) or not frames:
        return False
    has_mapped_intermediate_anchor = False
    for frame in frames:
        if not isinstance(frame, dict):
            continue
        if frame.get("status") != "passed":
            return False
        if frame.get("mapped_sample_elapsed_ms") is None:
            return False
        if not isinstance(frame.get("sample_skew_ms"), int):
            return False
        if int(frame["sample_skew_ms"]) > int(evidence["max_sample_skew_ms"]):
            return False
        anchors = frame.get("anchors")
        if not isinstance(anchors, list):
            continue
        has_passed_anchor = any(
            isinstance(anchor, dict)
            and anchor.get("status") == "passed"
            and anchor.get("baseline_row_y") is not None
            and anchor.get("frame_row_y") is not None
            for anchor in anchors
        )
        if frame.get("sidebar_phase") == "intermediate" and has_passed_anchor:
            has_mapped_intermediate_anchor = True
    return has_mapped_intermediate_anchor


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
            covered, coverage_detail = proof_covers_required_invariants(summary, visual_changes)
            if covered:
                return True, (
                    f"{detail}; {match_detail}; {coverage_detail}: "
                    f"{artifact_dir / 'summary.json'}"
                )
            detail = coverage_detail
        else:
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
    assert is_visual_sensitive("crates/lushtext-core/src/model/automation.rs")
    assert is_visual_sensitive("resources/ui/window.blp")
    assert is_visual_sensitive("resources/style/style.css")
    assert is_visual_sensitive("scripts/visual-geometry-scenarios/example.json")
    assert is_visual_sensitive("scripts/check-visual-proof-policy.py")
    assert is_visual_sensitive("scripts/lushtext-automation.py")
    assert is_visual_sensitive("scripts/test-visual-geometry.py")
    assert is_visual_sensitive("scripts/visual-geometry-smoke.py")
    assert not is_visual_sensitive("docs/automation.md")
    assert required_invariants_for_changes(["resources/style/style.css"]) == [
        NATIVE_MINIMAP_HIGHLIGHT_INVARIANT
    ]
    assert required_animation_invariants_for_changes(
        ["crates/lushtext-core/src/ui/editor_page/overscroll.rs"]
    ) == [NATIVE_MINIMAP_ANIMATION_INVARIANT]
    assert required_invariants_for_changes(["resources/ui/window.blp"]) == []
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
        ok, detail = proof_covers_required_invariants(summary, ["resources/style/style.css"])
        assert not ok
        assert "pixel_verified_invariant_ids" in detail
        summary["verified_invariant_ids"] = [NATIVE_MINIMAP_HIGHLIGHT_INVARIANT]
        ok, detail = proof_covers_required_invariants(summary, ["resources/style/style.css"])
        assert not ok
        assert "pixel_verified_invariant_ids" in detail
        summary["pixel_verified_invariant_ids"] = [NATIVE_MINIMAP_HIGHLIGHT_INVARIANT]
        ok, detail = proof_covers_required_invariants(summary, ["resources/style/style.css"])
        assert not ok
        assert "pixel-evidence case" in detail
        summary["cases"] = [
            {
                "status": "passed",
                "pixel_verified_invariant_ids": [NATIVE_MINIMAP_HIGHLIGHT_INVARIANT],
                "pixel_anchor_evidence": [
                    {
                        "name": "minimap-native-viewport-top-edge",
                        "before_row_y": 10,
                        "after_row_y": 10,
                    }
                ],
                "final_geometry": {
                    "before": [{"name": "workspace-sidebar"}],
                    "after": [{"name": "workspace-sidebar"}],
                },
            }
        ]
        ok, detail = proof_covers_required_invariants(summary, ["resources/style/style.css"])
        assert ok
        assert NATIVE_MINIMAP_HIGHLIGHT_INVARIANT in detail
        ok, detail = proof_covers_required_invariants(
            summary,
            ["crates/lushtext-core/src/ui/editor_page/overscroll.rs"],
        )
        assert not ok
        assert "animation_verified_invariant_ids" in detail
        summary["animation_verified_invariant_ids"] = [NATIVE_MINIMAP_ANIMATION_INVARIANT]
        ok, detail = proof_covers_required_invariants(
            summary,
            ["crates/lushtext-core/src/ui/editor_page/overscroll.rs"],
        )
        assert not ok
        assert "animation-evidence case" in detail
        summary["cases"][0]["animation_verified_invariant_ids"] = [
            NATIVE_MINIMAP_ANIMATION_INVARIANT
        ]
        summary["cases"][0]["animation_frame_evidence"] = {
            "status": "passed",
            "sampled_frame_count": 2,
            "frames": [
                {
                    "frame_index": 0,
                    "status": "passed",
                    "anchors": [
                        {
                            "name": "minimap-native-viewport-top-edge",
                            "status": "passed",
                            "baseline_row_y": 10,
                            "frame_row_y": 10,
                        }
                    ],
                }
            ],
        }
        ok, detail = proof_covers_required_invariants(
            summary,
            ["crates/lushtext-core/src/ui/editor_page/overscroll.rs"],
        )
        assert not ok
        assert "lacks animation frame rows" in detail
        summary["cases"][0]["animation_frame_evidence"].update(
            {
                "capture_mode": "stream",
                "mapped_intermediate_frame_count": 1,
                "max_sample_skew_ms": 80,
                "max_sample_skew_observed_ms": 12,
            }
        )
        summary["cases"][0]["animation_frame_evidence"]["frames"][0].update(
            {
                "mapped_sample_elapsed_ms": 48,
                "sample_skew_ms": 12,
                "sidebar_phase": "intermediate",
            }
        )

        valid_animation_evidence = json.loads(
            json.dumps(summary["cases"][0]["animation_frame_evidence"])
        )
        for mutation in (
            lambda evidence: evidence.update({"capture_mode": "screenshot"}),
            lambda evidence: evidence.update({"mapped_intermediate_frame_count": 0}),
            lambda evidence: evidence.update({"max_sample_skew_observed_ms": 120}),
            lambda evidence: evidence["frames"][0].update({"mapped_sample_elapsed_ms": None}),
            lambda evidence: evidence["frames"][0].update({"anchors": []}),
        ):
            invalid = json.loads(json.dumps(valid_animation_evidence))
            mutation(invalid)
            summary["cases"][0]["animation_frame_evidence"] = invalid
            ok, detail = proof_covers_required_invariants(
                summary,
                ["crates/lushtext-core/src/ui/editor_page/overscroll.rs"],
            )
            assert not ok
            assert "lacks animation frame rows" in detail

        summary["cases"][0]["animation_frame_evidence"] = valid_animation_evidence
        ok, detail = proof_covers_required_invariants(
            summary,
            ["crates/lushtext-core/src/ui/editor_page/overscroll.rs"],
        )
        assert ok
        assert NATIVE_MINIMAP_ANIMATION_INVARIANT in detail

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
        print("PASS: visual proof policy self-tests")
        return 0

    ok, detail = check_policy(args.artifact_dir.resolve(), args.base_ref)
    print(detail)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
