#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Keep every CI Flatpak build on the one shared composite action.

The container image `ghcr.io/flathub-infra/flatpak-github-actions` keeps its
system Flatpak installation on overlayfs, where a dependency update fails with
`Invalid cross-device link` on runners whose overlay cannot rename lower-layer
directories. `.github/actions/flatpak-build` relocates that installation before
`flatpak-builder` runs, so every job that builds against it must go through the
action. Rules, reported once per job with every rule the job breaks:

1. no workflow names `flatpak/flatpak-github-actions/flatpak-builder@` (only the
   composite action may);
2. a job whose container image contains `flatpak-github-actions` has a
   `uses: ./.github/actions/flatpak-build` step, unless it is exempt;
3. that step comes after the job's `actions/checkout` step (a local action
   exists only once the repository is checked out);
4. an exempt job's reason still holds.
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
WORKFLOW_DIR = REPO_ROOT / ".github/workflows"
COMINOTTI_SCRIPT = REPO_ROOT / "scripts/generate-cominotti-flatpak-repo.sh"

FLATPAK_BUILDER_ACTION = "flatpak/flatpak-github-actions/flatpak-builder@"
FLATPAK_IMAGE_MARKER = "flatpak-github-actions"
SHARED_ACTION = "./.github/actions/flatpak-build"
CHECKOUT_ACTION = "actions/checkout@"

JOB_RE = re.compile(r"^  ([A-Za-z0-9_-]+):\s*(?:#.*)?$")
CONTAINER_RE = re.compile(r"^    container:\s*(.*?)\s*(?:#.*)?$")
IMAGE_RE = re.compile(r"^      image:\s*(.*?)\s*(?:#.*)?$")
STEPS_RE = re.compile(r"^    steps:\s*(?:#.*)?$")
STEP_START_RE = re.compile(r"^      -(?:\s|$)")
USES_RE = re.compile(r"^      (?:-\s+|  )uses:\s*['\"]?([^'\"\s#]+)")

RULE_DIRECT_BUILDER = 1
RULE_MISSING_SHARED_ACTION = 2
RULE_SHARED_BEFORE_CHECKOUT = 3
RULE_EXEMPTION_STALE = 4


def cominotti_script_installs_user(script: Path) -> bool:
    """True while the Cominotti script's flatpak-builder arguments include `--user`."""
    try:
        lines = script.read_text(encoding="utf-8").splitlines()
    except OSError:
        return False
    in_args = False
    for line in lines:
        stripped = line.strip()
        if not in_args:
            if stripped.startswith("builder_args=("):
                in_args = True
                rest = stripped.removeprefix("builder_args=(")
                if "--user" in rest.split():
                    return True
                if rest.endswith(")"):
                    return False
            continue
        if stripped.startswith(")"):
            return False
        if stripped.split("#", 1)[0].split() == ["--user"]:
            return True
    return False


@dataclass(frozen=True)
class Exemption:
    """A job allowed to bypass the shared action, and the check of its reason."""

    workflow: str
    job_id: str
    reason: str


EXEMPTIONS = (
    Exemption(
        workflow="release.yml",
        job_id="cominotti-flatpak",
        reason=(
            "scripts/generate-cominotti-flatpak-repo.sh passes `--user` to "
            "flatpak-builder, so it installs into a fresh user installation "
            "on the host-bound $HOME, never the image's overlay system one"
        ),
    ),
)


@dataclass
class Job:
    """One parsed workflow job: its container image and its ordered steps."""

    path: Path
    job_id: str
    line_number: int
    image: str | None = None
    # (step index, action reference, line number) for every `uses:` step.
    uses: list[tuple[int, str, int]] = field(default_factory=list)
    # Line numbers anywhere in the job that name the flatpak-builder action.
    builder_lines: list[int] = field(default_factory=list)


def parse_jobs(path: Path) -> tuple[list[Job], list[int]]:
    """Parse jobs, returning them and the out-of-job lines naming flatpak-builder."""
    lines = path.read_text(encoding="utf-8").splitlines()
    jobs: list[Job] = []
    stray_builder_lines: list[int] = []
    in_jobs = False
    current: Job | None = None
    in_container = False
    in_steps = False
    step_index = -1

    for line_number, line in enumerate(lines, start=1):
        names_builder = FLATPAK_BUILDER_ACTION in line
        if not in_jobs:
            if names_builder:
                stray_builder_lines.append(line_number)
            if line == "jobs:" or line.startswith("jobs: "):
                in_jobs = True
            continue

        if line and not line.startswith((" ", "#")):
            in_jobs = False
            current = None
            if names_builder:
                stray_builder_lines.append(line_number)
            continue

        job_match = JOB_RE.match(line)
        if job_match:
            current = Job(path=path, job_id=job_match.group(1), line_number=line_number)
            jobs.append(current)
            in_container = False
            in_steps = False
            step_index = -1
            continue

        if current is None:
            if names_builder:
                stray_builder_lines.append(line_number)
            continue

        if names_builder:
            current.builder_lines.append(line_number)

        if line.startswith("    ") and not line.startswith("     ") and line.strip():
            # A job-level key ends any container or steps block.
            in_container = False
            in_steps = False
            container_match = CONTAINER_RE.match(line)
            if container_match:
                value = container_match.group(1)
                if value:
                    current.image = value.strip("'\"")
                else:
                    in_container = True
                continue
            if STEPS_RE.match(line):
                in_steps = True
                continue
            continue

        if in_container:
            image_match = IMAGE_RE.match(line)
            if image_match:
                current.image = image_match.group(1).strip("'\"")
            continue

        if in_steps:
            if STEP_START_RE.match(line):
                step_index += 1
            uses_match = USES_RE.match(line)
            if uses_match:
                current.uses.append((step_index, uses_match.group(1), line_number))

    return jobs, stray_builder_lines


def display(path: Path) -> str:
    try:
        return str(path.relative_to(REPO_ROOT))
    except ValueError:
        return str(path)


def first_step(job: Job, prefix: str) -> tuple[int, int] | None:
    for index, reference, line_number in job.uses:
        if reference == prefix or reference.startswith(prefix):
            return index, line_number
    return None


def check_workflows(
    paths: list[Path],
    exemptions: tuple[Exemption, ...] = EXEMPTIONS,
    cominotti_script: Path = COMINOTTI_SCRIPT,
) -> list[str]:
    """Return one finding per offending job (or stray line), naming every broken rule."""
    findings: list[str] = []
    seen_exemptions: set[tuple[str, str]] = set()

    for path in paths:
        jobs, stray = parse_jobs(path)
        for line_number in stray:
            findings.append(
                f"{display(path)}:{line_number}: breaks rule {RULE_DIRECT_BUILDER} "
                f"(names `{FLATPAK_BUILDER_ACTION}` outside any job; only "
                f"`{SHARED_ACTION}` may use it)"
            )
        for job in jobs:
            broken: list[str] = []
            if job.builder_lines:
                lines = ", ".join(str(n) for n in job.builder_lines)
                broken.append(
                    f"rule {RULE_DIRECT_BUILDER}: uses `{FLATPAK_BUILDER_ACTION}` directly "
                    f"(line {lines}); build through `uses: {SHARED_ACTION}`"
                )
            in_flatpak_container = (
                job.image is not None and FLATPAK_IMAGE_MARKER in job.image
            )
            exemption = next(
                (
                    e
                    for e in exemptions
                    if e.workflow == path.name and e.job_id == job.job_id
                ),
                None,
            )
            if exemption is not None:
                seen_exemptions.add((exemption.workflow, exemption.job_id))
            shared = first_step(job, SHARED_ACTION)
            if in_flatpak_container:
                if exemption is not None:
                    if not cominotti_script_installs_user(cominotti_script):
                        broken.append(
                            f"rule {RULE_EXEMPTION_STALE}: no longer exempt: its reason "
                            f"({exemption.reason}) does not hold, because "
                            f"`{display(cominotti_script)}` no longer passes `--user` "
                            f"to flatpak-builder"
                        )
                elif shared is None:
                    broken.append(
                        f"rule {RULE_MISSING_SHARED_ACTION}: runs in container image "
                        f"`{job.image}` without a `uses: {SHARED_ACTION}` step"
                    )
            if shared is not None:
                checkout = first_step(job, CHECKOUT_ACTION)
                if checkout is None or checkout[0] > shared[0]:
                    broken.append(
                        f"rule {RULE_SHARED_BEFORE_CHECKOUT}: `uses: {SHARED_ACTION}` at "
                        f"line {shared[1]} comes before the job's `actions/checkout` step"
                    )
            if broken:
                findings.append(
                    f"{display(path)}:{job.line_number} job `{job.job_id}` breaks "
                    + "; ".join(broken)
                )

    for exemption in exemptions:
        if (exemption.workflow, exemption.job_id) not in seen_exemptions and any(
            p.name == exemption.workflow for p in paths
        ):
            findings.append(
                f"{exemption.workflow}: exempt job `{exemption.job_id}` no longer exists; "
                f"remove its exemption from scripts/check-flatpak-ci-builds.py"
            )
    return findings


def workflow_paths() -> list[Path]:
    return sorted({*WORKFLOW_DIR.glob("*.yml"), *WORKFLOW_DIR.glob("*.yaml")})


CHECKOUT = "      - uses: actions/checkout@v6\n"
SHARED = (
    "      - uses: ./.github/actions/flatpak-build\n"
    "        with:\n"
    "          manifest-path: build-aux/app.json\n"
)
DIRECT = (
    "      - uses: flatpak/flatpak-github-actions/flatpak-builder@v6\n"
    "        with:\n"
    "          manifest-path: build-aux/app.json\n"
)


def container_job(job_id: str, steps: str) -> str:
    return (
        f"  {job_id}:\n"
        "    runs-on: ubuntu-latest\n"
        "    timeout-minutes: 30\n"
        "    container:\n"
        "      image: ghcr.io/flathub-infra/flatpak-github-actions:gnome-51\n"
        "      options: --privileged\n"
        "    steps:\n"
        f"{steps}"
    )


def run_self_test() -> None:
    """Exercise every rule against synthetic workflows."""
    user_script = (
        "builder_args=(\n    --disable-rofiles-fuse\n    --user\n"
        "    --install-deps-from=flathub\n)\n"
    )
    system_script = "builder_args=(\n    --disable-rofiles-fuse\n    --force-clean\n)\n"
    exemptions = (Exemption("release.yml", "cominotti", "installs with --user"),)

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        user = root / "user.sh"
        user.write_text(user_script, encoding="utf-8")
        system = root / "system.sh"
        system.write_text(system_script, encoding="utf-8")

        def workflow(name: str, jobs: str) -> Path:
            path = root / name
            path.write_text(f"name: T\non:\n  push:\njobs:\n{jobs}", encoding="utf-8")
            return path

        def expect(findings: list[str], count: int, terms: list[str]) -> None:
            if len(findings) != count:
                raise AssertionError(f"expected {count} finding(s), got {findings}")
            for term in terms:
                if not any(term in f for f in findings):
                    raise AssertionError(f"expected a finding containing {term!r}, got {findings}")

        # Direct flatpak-builder use in a container job: rules 1 and 2, one finding.
        direct = workflow("direct.yml", container_job("flatpak", CHECKOUT + DIRECT))
        expect(
            check_workflows([direct], exemptions, user),
            1,
            ["job `flatpak`", "rule 1:", "rule 2:"],
        )

        # Direct use outside a Flatpak container still breaks rule 1 alone.
        plain = workflow(
            "plain.yml",
            "  lint:\n    runs-on: ubuntu-latest\n    steps:\n" + CHECKOUT + DIRECT,
        )
        findings = check_workflows([plain], exemptions, user)
        expect(findings, 1, ["job `lint`", "rule 1:"])
        if "rule 2:" in findings[0]:
            raise AssertionError(f"a non-container job cannot break rule 2: {findings}")

        # A Flatpak container job without the shared action: rule 2.
        missing = workflow(
            "missing.yml",
            container_job("build", CHECKOUT + "      - run: flatpak-builder app app.json\n"),
        )
        expect(check_workflows([missing], exemptions, user), 1, ["job `build`", "rule 2:"])

        # The shared action before checkout: rule 3.
        before = workflow("before.yml", container_job("flatpak", SHARED + CHECKOUT))
        expect(check_workflows([before], exemptions, user), 1, ["job `flatpak`", "rule 3:"])

        # An exempt job: passes while its script passes --user, fails once it stops.
        exempt = workflow(
            "release.yml",
            container_job("cominotti", CHECKOUT + "      - run: ./scripts/gen.sh\n"),
        )
        expect(check_workflows([exempt], exemptions, user), 0, [])
        expect(
            check_workflows([exempt], exemptions, system),
            1,
            ["job `cominotti`", "rule 4:", "no longer exempt"],
        )

        # An exemption whose job vanished is reported.
        vanished = root / "release.yml"
        vanished.write_text(
            "name: T\njobs:\n" + container_job("flatpak", CHECKOUT + SHARED),
            encoding="utf-8",
        )
        expect(check_workflows([vanished], exemptions, user), 1, ["no longer exists"])

        # A clean pass: shared action after checkout, inline container image form too.
        clean = workflow(
            "clean.yml",
            container_job("flatpak", CHECKOUT + SHARED)
            + "  inline:\n    runs-on: ubuntu-latest\n"
            "    container: ghcr.io/flathub-infra/flatpak-github-actions:gnome-51\n"
            "    steps:\n" + CHECKOUT + SHARED,
        )
        expect(check_workflows([clean], exemptions, user), 0, [])

        # An inline container image without the action is still caught.
        inline = workflow(
            "inline.yml",
            "  inline:\n    runs-on: ubuntu-latest\n"
            "    container: ghcr.io/flathub-infra/flatpak-github-actions:gnome-51\n"
            "    steps:\n" + CHECKOUT,
        )
        expect(check_workflows([inline], exemptions, user), 1, ["job `inline`", "rule 2:"])

        # The real Cominotti script still passes --user (the exemption's live reason).
        if not cominotti_script_installs_user(COMINOTTI_SCRIPT):
            raise AssertionError(
                f"{display(COMINOTTI_SCRIPT)} no longer passes --user; revisit the exemption"
            )
    print("check-flatpak-ci-builds self-test passed")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="run the built-in rule tests before checking the workflows",
    )
    args = parser.parse_args()
    if args.self_test:
        run_self_test()

    findings = check_workflows(workflow_paths())
    if findings:
        print("Flatpak CI build policy violations:")
        for finding in findings:
            print(f"  - {finding}")
        return 1
    print(
        "Flatpak CI build policy passed: every flatpak-github-actions container job "
        f"builds through `{SHARED_ACTION}` after checkout"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
