#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run the Kani proof harnesses in named shards, and check the shard table.

Every `#[kani::proof]` harness belongs to exactly one shard. `make kani` runs
every shard in turn; the scheduled CI lane runs one shard per matrix job, so
each job stays inside the repository's 30-minute job cap even though the whole
lane takes longer than that. `check` (part of `make check-policy`, no Kani
needed) fails when a harness anywhere under `crates/` matches no shard or more
than one, so a new harness can never silently drop out of CI.

This table is the one source of truth for the shards, and the Makefile's
`KANI_VERSION` for the pinned Kani: `github-outputs` hands both to
`.github/workflows/kani.yml`, which builds its matrix from them.

Usage:
  scripts/kani-shards.py list
  scripts/kani-shards.py check [--self-test]
  scripts/kani-shards.py github-outputs
  scripts/kani-shards.py run all|<shard> [--target-dir DIR] [--measure JSON] [--self-test]

`run --measure JSON` is the measurement mode. For each shard it records the
wall time, the peak resident memory of the largest descendant process (CBMC,
from the kernel's per-child rusage), the exit status, and every harness's own
`Verification Time:`. It writes the figures to JSON and, when
`GITHUB_STEP_SUMMARY` is set, appends them to the job summary as a table.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import time
from dataclasses import dataclass, replace
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CRATES = REPO_ROOT / "crates"
MAKEFILE = REPO_ROOT / "Makefile"
KANI_VERSION_RE = re.compile(r"^KANI_VERSION \?= (\S+)$", re.M)

# The crates that own checked code, and each crate's source root. A harness's
# Kani name is its module path below the crate root plus its function name.
PACKAGES = {
    "gtk-lush-widgets": REPO_ROOT / "crates/gtk-lush/widgets/src",
    "lushtext-core": REPO_ROOT / "crates/lushtext-core/src",
}

# Budget margins, enforced by `check` against each shard's recorded runner
# measurement. The job cap is 30 minutes and a public `ubuntu-latest` runner has
# 16 GB: 25 minutes leaves room for runner variance and a cold Kani install,
# 12 GiB for the runner agent, the container, and kernel memory, and a
# pull-request shard must stay under 15 minutes so it never becomes the slowest
# required check. A shard over a margin is split first (design D3 of
# openspec/changes/harden-kani-lane-and-draft-token): split, then tune the
# solver, and only as a last resort reduce a proof bound.
MAX_SHARD_MINUTES = 25.0
MAX_SHARD_PEAK_GIB = 12.0
MAX_PULL_REQUEST_SHARD_MINUTES = 15.0
GATES = ("pull-request", "scheduled")


@dataclass(frozen=True)
class Shard:
    """One CI job's worth of harnesses.

    `filters` are matched by Kani as substrings of the fully qualified harness
    name, as `check` does. `gate` is `pull-request` (runs on pull requests and
    pushes to main as well as on schedule and dispatch) or `scheduled` (schedule
    and dispatch only). `ci_minutes` and `ci_peak_gib` are the shard's measured
    wall time and peak resident memory on the CI runner, the larger of the
    measured runs, and `measured_in` names the runs they came from. Timing from
    a developer machine does not count as a measurement.
    """

    package: str
    filters: tuple[str, ...]
    gate: str
    ci_minutes: float | None
    ci_peak_gib: float | None
    measured_in: str

    def owns(self, package: str, harness: str) -> bool:
        return self.package == package and any(f in harness for f in self.filters)


# The runs every budget below comes from: five `workflow_dispatch` runs of
# kani.yml on `ubuntu-latest` (Fedora 44 container), cold and warm Kani install
# cache, with each shard recording the largest wall time and peak memory seen
# across them, rounded up. Per-run figures and local CBMC times (Kani 0.68.0,
# one toolbox) are in docs/next/formal-verification.md, phase 2.
MEASURED_IN = "runs 35920992670, 35923346671, 35925626107, 35927734943, 35930757261 (max of the five)"
# The two pure-policy shards (extend-kani-to-pure-policies) come from four
# dispatched runs: 36052583128 and 36054671084 on the first form of the change,
# and 36059814296 and 36061831286 on the final, whole-pixel form. The memory
# harnesses did not change between the forms, so all four count for
# core-memory-policy; core-geometry-policies uses the last two, because the
# whole-pixel rewrite changed its harnesses (the first two took 15.2 and 15.7
# minutes). Runs 36059814296 and 36061831286 also re-measured every older
# shard; where they exceeded its recorded figure (core-journal-and-write 21.1,
# core-second-writer 17.4) the figure was raised to the larger value.
POLICY_MEASURED_IN = "runs 36052583128, 36054671084, 36059814296, 36061831286 (max of the four)"
GEOMETRY_POLICY_MEASURED_IN = "runs 36059814296, 36061831286 (max of the two, final whole-pixel form)"
RESAMPLED_IN = MEASURED_IN.replace(" (max of the five)", ", 36059814296, 36061831286 (max of the seven)")
SHARDS: dict[str, Shard] = {
    "widgets-geometry": Shard(
        "gtk-lush-widgets",
        (
            "kani_proofs::no_input_panics",
            "kani_proofs::whole_pixel_band_",
            "kani_proofs::slice_lies_",
            "kani_proofs::slice_covers_",
            "kani_proofs::slice_containment_",
            "kani_proofs::resting_bin_",
            "kani_proofs::requests_are_",
            "kani_proofs::request_lands_",
            "kani_proofs::request_landing_",
        ),
        gate="pull-request",
        ci_minutes=8.2,
        ci_peak_gib=1.8,
        measured_in=MEASURED_IN,
    ),
    "widgets-slice-loop-rest": Shard(
        "gtk-lush-widgets",
        ("kani_proofs::slice_loop_rests_",),
        gate="scheduled",
        ci_minutes=16.2,
        ci_peak_gib=1.8,
        measured_in=MEASURED_IN,
    ),
    "widgets-slice-loop-requests": Shard(
        "gtk-lush-widgets",
        (
            "kani_proofs::slice_loop_honours_",
            "kani_proofs::slice_loop_learning_frame_",
            "kani_proofs::slice_loop_one_reconfiguring_",
            "kani_proofs::slice_loop_two_reconfiguring_",
        ),
        gate="scheduled",
        ci_minutes=16.8,
        ci_peak_gib=1.8,
        measured_in=MEASURED_IN,
    ),
    "core-journal-and-write": Shard(
        "lushtext-core",
        (
            "services::draft_service::kani_proofs::journal_",
            "services::draft_service::kani_proofs::a_dirty_editor_",
            "services::draft_service::set_aside_retention::kani_proofs::",
            "services::filesystem::write_protocol::kani_proofs::",
        ),
        gate="scheduled",
        ci_minutes=21.2,
        ci_peak_gib=8.3,
        measured_in=RESAMPLED_IN,
    ),
    "core-second-writer": Shard(
        "lushtext-core",
        ("services::draft_service::kani_proofs::a_second_writer_",),
        gate="scheduled",
        ci_minutes=17.4,
        ci_peak_gib=8.9,
        measured_in=RESAMPLED_IN,
    ),
    "core-memory-policy": Shard(
        "lushtext-core",
        ("model::editor_memory::kani_proofs::",),
        gate="scheduled",
        ci_minutes=16.2,
        ci_peak_gib=9.0,
        measured_in=POLICY_MEASURED_IN,
    ),
    "core-geometry-policies": Shard(
        "lushtext-core",
        (
            "ui::editor_page::minimap::policy::kani_proofs::",
            "ui::window::geometry::policy::kani_proofs::",
            "ui::sidebar::width_preset::kani_proofs::",
            "ui::markdown_preview::policy::kani_proofs::",
        ),
        gate="pull-request",
        ci_minutes=13.6,
        ci_peak_gib=2.2,
        measured_in=GEOMETRY_POLICY_MEASURED_IN,
    ),
}

HARNESS_START_RE = re.compile(r"^Checking harness (\S+?)\.\.\.$")
HARNESS_VERDICT_RE = re.compile(r"^VERIFICATION:- (\S+)")
HARNESS_TIME_RE = re.compile(r"^Verification Time: ([0-9.]+)s$")

PROOF_RE = re.compile(r"#\[kani::proof\][^\n]*\n(?:\s*#\[[^\n]*\n)*\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+(\w+)")


def module_path(source_root: Path, path: Path) -> str:
    """The module path of a source file, relative to its crate root."""
    parts = list(path.relative_to(source_root).with_suffix("").parts)
    if parts[-1] in ("mod", "lib"):
        parts.pop()
    return "::".join(parts)


def discover() -> tuple[dict[str, list[str]], list[Path]]:
    """Every harness in the tree, as `package -> [kani names]`, plus the files
    holding harnesses outside every listed package (which no shard can run)."""
    found: dict[str, list[str]] = {package: [] for package in PACKAGES}
    unowned = []
    for path in sorted(CRATES.rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        if "kani::proof" not in text or not PROOF_RE.search(text):
            continue
        owner = next(
            ((package, root) for package, root in PACKAGES.items() if path.is_relative_to(root)),
            None,
        )
        if owner is None:
            unowned.append(path)
            continue
        package, root = owner
        prefix = module_path(root, path)
        for match in PROOF_RE.finditer(text):
            found[package].append(f"{prefix}::{match.group(1)}" if prefix else match.group(1))
    return found, unowned


def kani_version() -> str:
    """The pinned Kani version, from the Makefile."""
    match = KANI_VERSION_RE.search(MAKEFILE.read_text(encoding="utf-8"))
    if match is None:
        raise SystemExit("kani-shards: no `KANI_VERSION ?=` line in the Makefile")
    return match.group(1)


def check(found: dict[str, list[str]], unowned: list[Path]) -> list[str]:
    """Every harness matches exactly one shard of its own package."""
    problems = [
        f"{path.relative_to(REPO_ROOT)} holds Kani harnesses outside every package in PACKAGES"
        for path in unowned
    ]
    for package, names in found.items():
        for name in names:
            owners = [shard_name for shard_name, shard in SHARDS.items() if shard.owns(package, name)]
            if len(owners) != 1:
                problems.append(f"{package} harness {name} matches shards {owners or 'none'}")
    for shard_name, shard in SHARDS.items():
        for prefix in shard.filters:
            if not any(prefix in name for name in found.get(shard.package, [])):
                problems.append(f"shard {shard_name} prefix {prefix} matches no harness")
    return problems


def budget_problems(shards: dict[str, Shard]) -> list[str]:
    """Every shard has a runner measurement inside the margins for its gate."""
    problems = []
    for shard_name, shard in shards.items():
        if shard.gate not in GATES:
            problems.append(f"shard {shard_name} gate {shard.gate!r} is not one of {GATES}")
        if shard.ci_minutes is None or shard.ci_peak_gib is None or not shard.measured_in:
            problems.append(
                f"shard {shard_name} has no recorded CI runner measurement "
                "(ci_minutes, ci_peak_gib, measured_in); dispatch kani.yml and record it"
            )
            continue
        if shard.ci_minutes > MAX_SHARD_MINUTES:
            problems.append(
                f"shard {shard_name} takes {shard.ci_minutes} min on the runner, over the "
                f"{MAX_SHARD_MINUTES:g}-minute margin; split it"
            )
        if shard.ci_peak_gib > MAX_SHARD_PEAK_GIB:
            problems.append(
                f"shard {shard_name} peaks at {shard.ci_peak_gib} GiB on the runner, over the "
                f"{MAX_SHARD_PEAK_GIB:g} GiB margin; split it"
            )
        if shard.gate == "pull-request" and shard.ci_minutes > MAX_PULL_REQUEST_SHARD_MINUTES:
            problems.append(
                f"shard {shard_name} is gated pull-request but takes {shard.ci_minutes} min, over "
                f"the {MAX_PULL_REQUEST_SHARD_MINUTES:g}-minute pull-request margin; gate it scheduled"
            )
    return problems


class HarnessLog:
    """Parses Kani's streamed output into one record per harness: the name
    from `Checking harness NAME...`, then that harness's verdict and its own
    `Verification Time:` line."""

    def __init__(self) -> None:
        self.harnesses: list[dict[str, object]] = []

    def feed(self, line: str) -> None:
        line = line.strip()
        if match := HARNESS_START_RE.match(line):
            self.harnesses.append({"name": match.group(1), "verdict": None, "seconds": None})
        elif self.harnesses and (match := HARNESS_VERDICT_RE.match(line)):
            self.harnesses[-1]["verdict"] = match.group(1)
        elif self.harnesses and (match := HARNESS_TIME_RE.match(line)):
            self.harnesses[-1]["seconds"] = float(match.group(1))


def run_measured(command: list[str]) -> tuple[int, float, int, HarnessLog]:
    """Runs `command`, streaming and parsing its output. Returns the exit
    status, the wall time in seconds, and the peak resident set in KiB of the
    largest process in the child's tree: on Linux the child's `wait4` rusage
    folds in every descendant it waited for (cargo-kani waits on kani-driver,
    which waits on CBMC), so the figure is CBMC's peak and is per shard, not
    cumulative across shards like `RUSAGE_CHILDREN`."""
    log = HarnessLog()
    start = time.monotonic()
    child = subprocess.Popen(
        command, cwd=REPO_ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace"
    )
    assert child.stdout is not None
    for line in child.stdout:
        sys.stdout.write(line)
        log.feed(line)
    sys.stdout.flush()
    _, wait_status, usage = os.wait4(child.pid, 0)
    return os.waitstatus_to_exitcode(wait_status), time.monotonic() - start, usage.ru_maxrss, log


def shard_record(shard: str, status: int, seconds: float, peak_kib: int, log: HarnessLog) -> dict[str, object]:
    return {
        "shard": shard,
        "package": SHARDS[shard].package,
        "status": status,
        "wall_seconds": round(seconds, 1),
        "wall_minutes": round(seconds / 60, 2),
        "peak_rss_kib": peak_kib,
        "peak_rss_gib": round(peak_kib / (1024 * 1024), 2),
        "harnesses": log.harnesses,
    }


def summary_markdown(records: list[dict[str, object]]) -> str:
    lines = [
        "| Shard | Status | Wall time (min) | Peak RSS (GiB) | Harnesses |",
        "|---|---|---|---|---|",
    ]
    for record in records:
        lines.append(
            f"| `{record['shard']}` | {record['status']} | {record['wall_minutes']} "
            f"| {record['peak_rss_gib']} | {len(record['harnesses'])} |"
        )
    lines += ["", "| Harness | Verdict | Verification time (s) |", "|---|---|---|"]
    for record in records:
        for harness in record["harnesses"]:
            lines.append(f"| `{harness['name']}` | {harness['verdict']} | {harness['seconds']} |")
    return "\n".join(lines) + "\n"


def run(shard_names: list[str], target_dir: str, measure: str | None = None) -> int:
    records: list[dict[str, object]] = []
    result = 0
    for shard in shard_names:
        command = ["cargo", "kani", "-p", SHARDS[shard].package, "--target-dir", target_dir]
        for prefix in SHARDS[shard].filters:
            command += ["--harness", prefix]
        print(f"Running Kani shard {shard}: {' '.join(command)}", flush=True)
        if measure is None:
            status = subprocess.run(command, cwd=REPO_ROOT, check=False).returncode
        else:
            record = shard_record(shard, *run_measured(command))
            records.append(record)
            status = record["status"]
            print(
                f"Kani shard {shard}: {record['wall_minutes']} min, peak RSS {record['peak_rss_gib']} GiB, "
                f"{len(record['harnesses'])} harnesses",
                flush=True,
            )
        if status != 0:
            print(f"Kani shard {shard} failed with status {status}", file=sys.stderr)
            result = status
            break
    if measure is not None:
        Path(measure).write_text(
            json.dumps({"kani_version": kani_version(), "shards": records}, indent=2) + "\n", encoding="utf-8"
        )
        if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
            with open(summary, "a", encoding="utf-8") as handle:
                handle.write(summary_markdown(records))
    return result


def self_test() -> None:
    root = Path("/crate/src")
    assert module_path(root, root / "kani_proofs.rs") == "kani_proofs"
    assert module_path(root, root / "outer/inner/mod.rs") == "outer::inner"
    text = "#[kani::proof]\n#[kani::should_panic]\n#[kani::unwind(4)]\nfn one() {}\n#[kani::proof]\npub fn two() {}\n"
    assert [m.group(1) for m in PROOF_RE.finditer(text)] == ["one", "two"]
    assert KANI_VERSION_RE.search("X ?= 1\nKANI_VERSION ?= 0.68.0\n").group(1) == "0.68.0"

    # Kani's streamed output, abridged: one record per harness, with its own
    # verdict and time, and nothing picked up from lines between harnesses.
    log = HarnessLog()
    for line in (
        "Checking harness kani_proofs::no_input_panics...\n",
        "VERIFICATION:- SUCCESSFUL\n",
        "Verification Time: 0.4419652s\n",
        "Checking harness services::x::kani_proofs::a_second_writer_loses...\n",
        "Check 1: foo\n",
        "VERIFICATION:- SUCCESSFUL (encountered one or more panics as expected)\n",
        "Verification Time: 500.8s\n",
        "Complete - 2 successfully verified harnesses, 0 failures, 2 total.\n",
    ):
        log.feed(line)
    assert log.harnesses == [
        {"name": "kani_proofs::no_input_panics", "verdict": "SUCCESSFUL", "seconds": 0.4419652},
        {"name": "services::x::kani_proofs::a_second_writer_loses", "verdict": "SUCCESSFUL", "seconds": 500.8},
    ], log.harnesses

    # The JSON shape the workflow uploads, and its summary table.
    record = shard_record("widgets-geometry", 0, 90.0, 3 * 1024 * 1024, log)
    assert json.loads(json.dumps(record)) == {
        "shard": "widgets-geometry",
        "package": "gtk-lush-widgets",
        "status": 0,
        "wall_seconds": 90.0,
        "wall_minutes": 1.5,
        "peak_rss_kib": 3 * 1024 * 1024,
        "peak_rss_gib": 3.0,
        "harnesses": log.harnesses,
    }
    table = summary_markdown([record])
    assert "| `widgets-geometry` | 0 | 1.5 | 3.0 | 2 |" in table, table
    assert "| `kani_proofs::no_input_panics` | SUCCESSFUL | 0.4419652 |" in table, table

    # Budget rules: an unmeasured shard, one over 25 minutes, one over 12 GiB,
    # and a pull-request shard over 15 minutes each fail; a measured shard
    # inside every margin passes.
    good = Shard("p", ("f",), "pull-request", 14.0, 11.5, "run 1")
    assert budget_problems({"ok": good}) == [], budget_problems({"ok": good})
    assert budget_problems({"s": replace(good, gate="scheduled", ci_minutes=24.9)}) == []
    cases = {
        "unmeasured": replace(good, ci_minutes=None, ci_peak_gib=None, measured_in=""),
        "no-run-id": replace(good, measured_in=""),
        "slow": replace(good, gate="scheduled", ci_minutes=25.5),
        "memory": replace(good, ci_peak_gib=12.5),
        "slow-pull-request": replace(good, ci_minutes=15.5),
        "unknown-gate": replace(good, gate="nightly"),
    }
    for label, shard in cases.items():
        assert budget_problems({label: shard}), f"budget self-test {label} should fail"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=["list", "check", "github-outputs", "run"])
    parser.add_argument("shard", nargs="?", default="all")
    parser.add_argument("--target-dir", default="target/kani")
    parser.add_argument("--measure", metavar="JSON", help="run: record per-shard budgets to this JSON file")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    found, unowned = discover()
    if args.command == "list":
        for shard_name, shard in SHARDS.items():
            names = [n for n in found[shard.package] if shard.owns(shard.package, n)]
            budget = (
                "unmeasured"
                if shard.ci_minutes is None
                else f"{shard.ci_minutes} min, {shard.ci_peak_gib} GiB on the runner"
            )
            print(f"{shard_name} ({shard.package}, {shard.gate}, {budget}): {len(names)} harnesses")
            for name in names:
                print(f"  {name}")
        return 0
    problems = check(found, unowned)
    # Budgets gate the table (`check`) and the CI matrix (`github-outputs`), not
    # `run`: a shard over a margin must stay runnable so it can be re-measured.
    if args.command in ("check", "github-outputs"):
        problems += budget_problems(SHARDS)
    if problems:
        for problem in problems:
            print(f"kani-shards: {problem}", file=sys.stderr)
        return 1
    if args.command == "check":
        total = sum(len(names) for names in found.values())
        print(f"kani shard table passed: {total} harnesses, each in exactly one of {len(SHARDS)} shards")
        return 0
    if args.command == "github-outputs":
        print(f"shards={json.dumps(list(SHARDS))}")
        pr_shards = [shard_name for shard_name, shard in SHARDS.items() if shard.gate == "pull-request"]
        print(f"pr-shards={json.dumps(pr_shards)}")
        print(f"kani-version={kani_version()}")
        return 0
    if args.shard == "all":
        return run(list(SHARDS), args.target_dir, args.measure)
    if args.shard not in SHARDS:
        print(f"unknown shard {args.shard}; known: {', '.join(SHARDS)}", file=sys.stderr)
        return 2
    return run([args.shard], args.target_dir, args.measure)


if __name__ == "__main__":
    sys.exit(main())
