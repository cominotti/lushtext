#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Enforce LushText's hard 30-minute GitHub Actions job budget."""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
WORKFLOW_DIR = REPO_ROOT / ".github/workflows"
MAX_JOB_TIMEOUT_MINUTES = 30
JOB_RE = re.compile(r"^  ([A-Za-z0-9_-]+):\s*(?:#.*)?$")
TIMEOUT_PREFIX = "    timeout-minutes:"


@dataclass(frozen=True)
class WorkflowJob:
    """One parsed GitHub Actions job and its declared timeout value."""

    path: Path
    job_id: str
    line_number: int
    timeout_line: int | None
    timeout_value: str | None


def strip_inline_comment(value: str) -> str:
    """Remove YAML comments from the simple scalar timeout values we support."""
    in_single = False
    in_double = False
    escaped = False
    for index, char in enumerate(value):
        if escaped:
            escaped = False
            continue
        if char == "\\" and in_double:
            escaped = True
            continue
        if char == "'" and not in_double:
            in_single = not in_single
            continue
        if char == '"' and not in_single:
            in_double = not in_double
            continue
        if char == "#" and not in_single and not in_double:
            return value[:index].strip()
    return value.strip()


def parse_workflow_jobs(path: Path) -> list[WorkflowJob]:
    """Parse top-level workflow jobs without depending on YAML 1.1 semantics."""
    lines = path.read_text(encoding="utf-8").splitlines()
    in_jobs = False
    current_job: WorkflowJob | None = None
    jobs: list[WorkflowJob] = []

    def flush_current() -> None:
        nonlocal current_job
        if current_job is not None:
            jobs.append(current_job)
            current_job = None

    for line_number, line in enumerate(lines, start=1):
        if not in_jobs:
            if line == "jobs:" or line.startswith("jobs: "):
                in_jobs = True
            continue

        if line and not line.startswith((" ", "#")):
            flush_current()
            break

        job_match = JOB_RE.match(line)
        if job_match:
            flush_current()
            current_job = WorkflowJob(
                path=path,
                job_id=job_match.group(1),
                line_number=line_number,
                timeout_line=None,
                timeout_value=None,
            )
            continue

        if current_job is None:
            continue

        if line.startswith(TIMEOUT_PREFIX):
            current_job = WorkflowJob(
                path=current_job.path,
                job_id=current_job.job_id,
                line_number=current_job.line_number,
                timeout_line=line_number,
                timeout_value=strip_inline_comment(line.removeprefix(TIMEOUT_PREFIX)),
            )

    flush_current()
    return jobs


def timeout_findings_for_job(job: WorkflowJob) -> list[str]:
    """Return policy findings for one workflow job."""
    try:
        display_path = job.path.relative_to(REPO_ROOT)
    except ValueError:
        display_path = job.path
    location = f"{display_path}:{job.line_number} job `{job.job_id}`"
    if job.timeout_value is None:
        return [f"{location} is missing timeout-minutes (implicit default exceeds 30)"]

    try:
        timeout = int(job.timeout_value)
    except ValueError:
        return [
            f"{location} declares non-integer timeout-minutes at line {job.timeout_line}: "
            f"{job.timeout_value!r}"
        ]

    if timeout > MAX_JOB_TIMEOUT_MINUTES:
        return [
            f"{location} declares timeout-minutes={timeout} at line {job.timeout_line}, "
            f"above the {MAX_JOB_TIMEOUT_MINUTES}-minute limit"
        ]
    return []


def check_workflows(paths: list[Path]) -> list[str]:
    """Return all workflow timeout policy findings."""
    findings: list[str] = []
    for path in paths:
        jobs = parse_workflow_jobs(path)
        if not jobs:
            findings.append(f"{path.relative_to(REPO_ROOT)}: no jobs were parsed")
            continue
        for job in jobs:
            findings.extend(timeout_findings_for_job(job))
    return findings


def workflow_paths() -> list[Path]:
    """List checked GitHub Actions workflows in stable order."""
    return sorted({*WORKFLOW_DIR.glob("*.yml"), *WORKFLOW_DIR.glob("*.yaml")})


def run_self_test() -> None:
    """Exercise the policy checker against representative workflow shapes."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        ok = root / "ok.yml"
        ok.write_text(
            """
name: OK
jobs:
  lint:
    runs-on: ubuntu-latest
    timeout-minutes: 30
  quick:
    runs-on: ubuntu-latest
    timeout-minutes: 5 # small policy lane
""".strip()
            + "\n",
            encoding="utf-8",
        )
        excessive = root / "excessive.yml"
        excessive.write_text(
            """
name: Excessive
jobs:
  slow:
    runs-on: ubuntu-latest
    timeout-minutes: 31
""".strip()
            + "\n",
            encoding="utf-8",
        )
        missing = root / "missing.yml"
        missing.write_text(
            """
name: Missing
jobs:
  implicit:
    runs-on: ubuntu-latest
""".strip()
            + "\n",
            encoding="utf-8",
        )
        expression = root / "expression.yml"
        expression.write_text(
            """
name: Expression
jobs:
  dynamic:
    runs-on: ubuntu-latest
    timeout-minutes: ${{ vars.TIMEOUT }}
""".strip()
            + "\n",
            encoding="utf-8",
        )

        ok_findings = check_workflows([ok])
        if ok_findings:
            raise AssertionError(f"expected ok workflow to pass, got {ok_findings}")

        findings = check_workflows([excessive, missing, expression])
        expected_terms = [
            "timeout-minutes=31",
            "missing timeout-minutes",
            "non-integer timeout-minutes",
        ]
        for term in expected_terms:
            if not any(term in finding for finding in findings):
                raise AssertionError(f"expected finding containing {term!r}, got {findings}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="run built-in policy parser tests before checking workflows",
    )
    args = parser.parse_args()

    if args.self_test:
        run_self_test()

    findings = check_workflows(workflow_paths())
    if findings:
        print("workflow timeout policy violations:")
        for finding in findings:
            print(f"  - {finding}")
        return 1

    print(
        "workflow timeout policy passed: all jobs declare "
        f"timeout-minutes <= {MAX_JOB_TIMEOUT_MINUTES}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
