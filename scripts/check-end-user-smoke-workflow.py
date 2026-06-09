#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Check that scheduled/manual end-user smoke lanes stay wired correctly."""

from __future__ import annotations

import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = REPO_ROOT / ".github/workflows/end-user-smoke.yml"

EXPECTED_LANES: dict[str, tuple[str, str]] = {
    "automation": (
        "make automation-smoke SMOKE_ARTIFACT_DIR=build/smoke",
        "build/smoke/automation",
    ),
    "visual": (
        "make visual-smoke SMOKE_ARTIFACT_DIR=build/smoke",
        "build/smoke/visual",
    ),
    "crash-recovery": (
        "make crash-recovery-smoke SMOKE_ARTIFACT_DIR=build/smoke",
        "build/smoke/crash-recovery",
    ),
    "portal-sandbox": (
        "make portal-sandbox-smoke SMOKE_ARTIFACT_DIR=build/smoke",
        "build/smoke/portal-sandbox",
    ),
    "accessibility": (
        "make accessibility-smoke SMOKE_ARTIFACT_DIR=build/smoke",
        "build/smoke/accessibility",
    ),
    "performance": (
        "make performance-smoke SMOKE_ARTIFACT_DIR=build/smoke",
        "build/smoke/performance",
    ),
}


def lane_blocks(workflow: str) -> dict[str, str]:
    """Extract matrix lane blocks with enough structure for this policy check."""
    blocks: dict[str, str] = {}
    current_lane: str | None = None
    current_lines: list[str] = []

    def flush_current() -> None:
        if current_lane is not None:
            blocks[current_lane] = "\n".join(current_lines)

    for line in workflow.splitlines():
        stripped = line.strip()
        if stripped.startswith("- lane: "):
            flush_current()
            current_lane = stripped.removeprefix("- lane: ").strip()
            current_lines = [stripped]
            continue
        if current_lane is not None:
            if stripped.startswith("- ") and not stripped.startswith("- lane: "):
                flush_current()
                current_lane = None
                current_lines = []
            else:
                current_lines.append(stripped)
    flush_current()
    return blocks


def check_workflow(workflow: str) -> list[str]:
    """Return human-readable drift findings for the scheduled smoke matrix."""
    blocks = lane_blocks(workflow)
    findings: list[str] = []

    for lane, (command, artifact_path) in EXPECTED_LANES.items():
        block = blocks.get(lane)
        if block is None:
            findings.append(f"missing lane: {lane}")
            continue
        if f"command: {command}" not in block:
            findings.append(f"{lane}: expected command `{command}`")
        if f"artifact_path: {artifact_path}" not in block:
            findings.append(f"{lane}: expected artifact path `{artifact_path}`")

    required_terms = [
        "workflow_dispatch:",
        "schedule:",
        "if: always()",
        "actions/upload-artifact",
    ]
    findings.extend(
        f"workflow is missing required term `{term}`"
        for term in required_terms
        if term not in workflow
    )
    return findings


def main() -> int:
    workflow = WORKFLOW.read_text(encoding="utf-8")
    findings = check_workflow(workflow)
    if findings:
        print("end-user smoke workflow drift detected:")
        for finding in findings:
            print(f"  - {finding}")
        return 1
    print("end-user smoke workflow matrix is current")
    return 0


if __name__ == "__main__":
    sys.exit(main())
