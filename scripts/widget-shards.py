#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run the widget tests in named CI shards, and check the shard table.

Every widget test belongs to exactly one shard. CI runs one shard per matrix
job so each job stays inside the repository's 30-minute job cap even though the
whole suite, run serially in one headless session, takes longer than that on
the shared runner. `make test` and `make test-widget` still run every test in
one session; `make test-widget-shard WIDGET_SHARD=<name>` reproduces one CI
shard locally.

`check` (part of `make check-policy`, no build needed) discovers every widget
test from `crates/lushtext/tests/widget/*.rs` with the rule
`crates/lushtext/build.rs` registers them by, and fails when a test matches no
shard, when a module or test is claimed by more than one shard, or when a table
entry matches nothing, so a new test can never silently drop out of CI. `run`
cross-checks that static discovery against the compiled binary's `--list`, then
runs exactly the shard's tests and fails unless the harness selected exactly
that many.

This table is the one source of truth for the shards: `github-outputs` hands it
to `.github/workflows/ci.yml`, which builds its widget matrix from it.

Usage:
  scripts/widget-shards.py list
  scripts/widget-shards.py check [--self-test]
  scripts/widget-shards.py github-outputs
  scripts/widget-shards.py run all|<shard> [--measure JSON] [--self-test]

`run --measure JSON` is the measurement mode every CI shard job uses. It
records the step wall time (test-binary build, `--list` cross-check, and the
headless run), the exit status, the selected count, and each test's duration
(the gap between consecutive `test NAME ... ok` completions). It writes them to
JSON and, when `GITHUB_STEP_SUMMARY` is set, appends them to the job summary.
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
WIDGET_DIR = REPO_ROOT / "crates/lushtext/tests/widget"
RUNNER = REPO_ROOT / "scripts/run-widget-tests.sh"

# Budget margin, enforced by `check` against each shard's recorded runner
# measurement. The job cap is 30 minutes; the container, dependency install,
# and toolchain setup before the widget step measured under 3 minutes, and the
# same test has varied tenfold between runs of the shared runner, so a shard's
# step must stay at or under 20 minutes. A shard over the margin is split (move
# a module, or one slow test by name, to another shard); the cap is not raised
# and threshold-sized tests are not shrunk.
MAX_SHARD_MINUTES = 20.0


@dataclass(frozen=True)
class Shard:
    """One CI job's worth of widget tests.

    `modules` are widget-test file stems (`window` owns every `window::*`
    test). `tests` are explicit `module::test_name` names; an explicit name
    takes precedence over its module's shard, which is how a slow test is moved
    to balance the shards. `ci_minutes` is the shard's measured widget-step wall
    time on the CI runner, the largest of the measured runs, and `measured_in`
    names those runs. Timing from a developer machine does not count.
    """

    modules: tuple[str, ...]
    tests: tuple[str, ...]
    ci_minutes: float | None
    measured_in: str


# The three threshold-sized tests (their document sizes are the policy limits
# under test) take about 13 of the suite's runner minutes between them, so each
# lands in a different shard: the local-history one with `window`, the minimap
# mid-scan one with `editor-page`, and the minimap long-line one, by name, with
# `surfaces`. See openspec/changes/shard-widget-test-ci-job/design.md.
# The runs every budget below comes from: the first three runs of the sharded
# matrix on `ubuntu-latest` (Fedora 44 container), each shard recording the
# largest widget-step wall time of the three, rounded up to 0.1 minute. Run 1:
# window 10.23, editor-page 8.75, surfaces 9.03; run 2: 11.10, 7.01, 6.55;
# run 3: 11.03, 6.71, 10.42.
MEASURED_IN = "runs 35946276130, 35947171706, 35948158637 (max of the three)"
SHARDS: dict[str, Shard] = {
    "window": Shard(
        modules=("window",),
        tests=(),
        ci_minutes=11.1,
        measured_in=MEASURED_IN,
    ),
    "editor-page": Shard(
        modules=("editor_page",),
        tests=(),
        ci_minutes=8.8,
        measured_in=MEASURED_IN,
    ),
    "surfaces": Shard(
        modules=(
            "accessibility",
            "app",
            "attention_refresh",
            "command_palette",
            "editor_memory_eviction",
            "file_tree_item",
            "focus_mode",
            "gtk_axioms",
            "gtk_lush_adoption",
            "markdown_preview",
            "open_popover",
            "plain_disposal",
            "preferences",
            "properties_panel",
            "search_bar",
            "search_panel",
            "shell_geometry",
            "sidebar",
            "status_bar",
            "tab_strip",
            "transient_dismissal",
            "window_preview",
            "workspace_entry_visibility",
            "workspace_section",
            "workspace_tree_virtualization",
        ),
        tests=("editor_page::test_minimap_long_line_warning_scan_slices_large_many_short_buffer",),
        ci_minutes=10.5,
        measured_in=MEASURED_IN,
    ),
}

RUNNING_RE = re.compile(r"^running (\d+) tests$")
# A test's result closes its line: `test NAME ... ok`, or, when the child's
# own output interleaved (or the runner's benign-noise filter dropped the
# `test NAME ... ` prefix it shared a line with), a bare `ok` / `FAILED` line.
TEST_RESULT_RE = re.compile(r"(?:^|^test \S+ \.\.\. )(ok(?: \(FLAKY: passed on attempt \d+\))?|FAILED)$")


def extract_test_functions(source: str) -> list[str]:
    """The test names `crates/lushtext/build.rs::extract_test_functions`
    registers: for every line that is exactly `#[test]` once trimmed, the name
    of the first later line starting `fn `. Keep the two in step; `run`
    fails when they disagree."""
    lines = source.splitlines()
    names = []
    for index, line in enumerate(lines):
        if line.strip() != "#[test]":
            continue
        fn_line = next((later.strip() for later in lines[index + 1 :] if later.strip().startswith("fn ")), None)
        if fn_line is None:
            continue
        names.append(fn_line.removeprefix("fn ").split("(")[0])
    return names


def discover(widget_dir: Path = WIDGET_DIR) -> list[str]:
    """Every registered widget test as `module::name`, in registry order."""
    found = []
    for path in sorted(widget_dir.glob("*.rs")):
        if path.stem == "common":
            continue
        found += [f"{path.stem}::{name}" for name in extract_test_functions(path.read_text(encoding="utf-8"))]
    return found


def module_of(test: str) -> str:
    """The widget-test file stem a `module::name` test lives in."""
    return test.split("::", 1)[0]


def owners(shards: dict[str, Shard], test: str) -> list[str]:
    """The shards owning `test`: those naming it explicitly, else those listing
    its module."""
    explicit = [name for name, shard in shards.items() if test in shard.tests]
    if explicit:
        return explicit
    return [name for name, shard in shards.items() if module_of(test) in shard.modules]


def assignment(shards: dict[str, Shard], tests: list[str]) -> dict[str, list[str]]:
    """Each shard's tests, in registry order, for tests with exactly one owner."""
    result: dict[str, list[str]] = {name: [] for name in shards}
    for test in tests:
        owned_by = owners(shards, test)
        if len(owned_by) == 1:
            result[owned_by[0]].append(test)
    return result


def check(shards: dict[str, Shard], tests: list[str]) -> list[str]:
    """Every test has exactly one shard, and every table entry is live."""
    problems = []
    for test in tests:
        owned_by = owners(shards, test)
        if len(owned_by) != 1:
            problems.append(f"widget test {test} matches shards {owned_by or 'none'}")
    modules = {module_of(test) for test in tests}
    seen_modules: dict[str, str] = {}
    for shard_name, shard in shards.items():
        for module in shard.modules:
            if module in seen_modules:
                problems.append(f"module {module} is listed by shards {seen_modules[module]} and {shard_name}")
            seen_modules.setdefault(module, shard_name)
            if module not in modules:
                problems.append(f"shard {shard_name} module {module} has no widget tests")
        for test in shard.tests:
            if test not in tests:
                problems.append(f"shard {shard_name} test {test} is not a widget test")
            elif module_of(test) in shard.modules:
                problems.append(f"shard {shard_name} names {test} explicitly but already owns its module")
    return problems


def budget_problems(shards: dict[str, Shard]) -> list[str]:
    """Every shard has a runner measurement inside the margin."""
    problems = []
    for shard_name, shard in shards.items():
        if shard.ci_minutes is None or not shard.measured_in or shard.measured_in == "pending":
            problems.append(
                f"shard {shard_name} has no recorded CI runner measurement "
                "(ci_minutes, measured_in); run the CI widget matrix and record it"
            )
            continue
        if shard.ci_minutes > MAX_SHARD_MINUTES:
            problems.append(
                f"shard {shard_name} takes {shard.ci_minutes} min on the runner, over the "
                f"{MAX_SHARD_MINUTES:g}-minute margin; split it"
            )
    return problems


class WidgetLog:
    """Parses the harness's streamed output: every `running N tests` count
    (a whole-run retry prints it again and restarts the durations) and each
    test's duration, measured as the gap between consecutive result lines.

    Results are attributed by position, not by the printed name: the harness
    runs the selected tests in `names` order, one at a time, and a test's
    printed name can be lost to interleaved child output. When the number of
    results does not match `names`, the durations are marked unattributed."""

    def __init__(self, names: list[str]) -> None:
        self.names = names
        self.running_counts: list[int] = []
        self.results: list[tuple[str, float]] = []
        self._last: float | None = None

    def feed(self, line: str, now: float) -> None:
        line = line.strip()
        if match := RUNNING_RE.match(line):
            self.running_counts.append(int(match.group(1)))
            self.results = []
            self._last = now
        elif self._last is not None and (match := TEST_RESULT_RE.search(line)):
            self.results.append((match.group(1), round(now - self._last, 1)))
            self._last = now

    @property
    def attributed(self) -> bool:
        return len(self.results) == len(self.names)

    @property
    def tests(self) -> list[dict[str, object]]:
        names = self.names if self.attributed else [f"#{index + 1}" for index in range(len(self.results))]
        return [
            {"name": name, "result": result, "seconds": seconds}
            for name, (result, seconds) in zip(names, self.results)
        ]


def listed_tests() -> list[str]:
    """The compiled widget binary's own test list (builds it if needed; list
    mode never starts a compositor)."""
    output = subprocess.run(
        ["cargo", "test", "-p", "lushtext", "--test", "widget", "--", "--list", "--format", "terse"],
        cwd=REPO_ROOT,
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    ).stdout
    return [line.removesuffix(": test") for line in output.splitlines() if line.endswith(": test")]


def count_problems(expected: int, log: WidgetLog) -> list[str]:
    if not log.running_counts:
        return ["the harness never reported how many tests it selected"]
    return [
        f"the harness selected {count} tests, but the shard has {expected}"
        for count in log.running_counts
        if count != expected
    ]


def run_shard(shard: str, names: list[str], total: int, started: float) -> dict[str, object]:
    """Run one shard's tests and return its measurement record; `started` is
    when this shard's step began (the first shard's includes the build and
    `--list` cross-check)."""
    command = [str(RUNNER), "--headless", "--retries", "1", "--", "--exact", *names]
    print(f"Running widget shard {shard}: {len(names)} of {total} widget tests", flush=True)
    log = WidgetLog(names)
    child = subprocess.Popen(
        command, cwd=REPO_ROOT, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace"
    )
    assert child.stdout is not None
    for line in child.stdout:
        # Relay line by line: CI's stdout is a pipe, so without a flush a
        # cancelled job's log would lose how far the shard got.
        sys.stdout.write(line)
        sys.stdout.flush()
        log.feed(line, time.monotonic())
    status = child.wait()
    seconds = time.monotonic() - started
    problems = count_problems(len(names), log)
    for problem in problems:
        print(f"widget-shards: shard {shard}: {problem}", file=sys.stderr)
    if status == 0 and problems:
        status = 1
    if not log.attributed:
        print(
            f"widget-shards: shard {shard}: saw {len(log.results)} test results for {len(names)} tests; "
            "per-test durations are recorded unattributed",
            file=sys.stderr,
        )
    return {
        "shard": shard,
        "status": status,
        "expected": len(names),
        "suite_total": total,
        "selected": log.running_counts,
        "durations_attributed": log.attributed,
        "wall_seconds": round(seconds, 1),
        "wall_minutes": round(seconds / 60, 2),
        "tests": log.tests,
    }


def summary_markdown(records: list[dict[str, object]], slowest: int = 20) -> str:
    lines = [
        "| Shard | Status | Wall time (min) | Tests (selected / suite) |",
        "|---|---|---|---|",
    ]
    for record in records:
        lines.append(
            f"| `{record['shard']}` | {record['status']} | {record['wall_minutes']} "
            f"| {record['expected']} / {record['suite_total']} |"
        )
    lines += ["", f"Slowest {slowest} tests per shard:", "", "| Test | Result | Seconds |", "|---|---|---|"]
    for record in records:
        ranked = sorted(record["tests"], key=lambda test: test["seconds"], reverse=True)[:slowest]
        for test in ranked:
            lines.append(f"| `{test['name']}` | {test['result']} | {test['seconds']} |")
    return "\n".join(lines) + "\n"


def run(shard_names: list[str], static_tests: list[str], measure: str | None) -> int:
    start = time.monotonic()
    binary_tests = listed_tests()
    missing = sorted(set(static_tests) - set(binary_tests))
    extra = sorted(set(binary_tests) - set(static_tests))
    if missing or extra or len(binary_tests) != len(static_tests):
        for name in missing:
            print(f"widget-shards: {name} is discovered from source but not in the binary's --list", file=sys.stderr)
        for name in extra:
            print(f"widget-shards: {name} is in the binary's --list but not discovered from source", file=sys.stderr)
        print("widget-shards: the static discovery and the compiled widget binary disagree", file=sys.stderr)
        return 1
    # Run each shard's tests in the binary's own order, which is the order the
    # harness runs them in and so the order results are attributed in.
    order = {name: index for index, name in enumerate(binary_tests)}
    by_shard = {name: sorted(tests, key=order.__getitem__) for name, tests in assignment(SHARDS, static_tests).items()}
    records = []
    result_status = 0
    for shard in shard_names:
        record = run_shard(shard, by_shard[shard], len(static_tests), start)
        records.append(record)
        start = time.monotonic()
        print(
            f"Widget shard {shard}: status {record['status']}, {record['wall_minutes']} min, "
            f"{record['expected']} of {len(static_tests)} tests",
            flush=True,
        )
        if record["status"] != 0:
            result_status = int(record["status"])
            break
    if measure is not None:
        Path(measure).write_text(json.dumps({"shards": records}, indent=2) + "\n", encoding="utf-8")
        if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
            with open(summary, "a", encoding="utf-8") as handle:
                handle.write(summary_markdown(records))
    return result_status


def self_test() -> None:
    # The registry rule: exact `#[test]` lines, then the first `fn ` line,
    # skipping other attributes; indented `#[test]` counts, commented-out and
    # `#[test_case]`-style lines do not.
    source = (
        "#[test]\n#[ignore]\nfn test_one() {}\n"
        "    #[test]\n    fn test_two() -> () {}\n"
        "// #[test]\nfn not_a_test() {}\n"
        "#[test_case]\nfn also_not() {}\n"
    )
    assert extract_test_functions(source) == ["test_one", "test_two"], extract_test_functions(source)

    table = {
        "a": Shard(("alpha",), ("beta::test_moved",), 1.0, "run 1"),
        "b": Shard(("beta",), (), 1.0, "run 1"),
    }
    tests = ["alpha::test_x", "beta::test_y", "beta::test_moved"]
    assert check(table, tests) == [], check(table, tests)
    assert assignment(table, tests) == {"a": ["alpha::test_x", "beta::test_moved"], "b": ["beta::test_y"]}

    failing = {
        "unassigned": (table, tests + ["gamma::test_new"]),
        "module in two shards": ({**table, "c": Shard(("beta",), (), 1.0, "run 1")}, tests),
        "test named by two shards": ({**table, "c": Shard((), ("beta::test_moved",), 1.0, "run 1")}, tests),
        "stale module": ({**table, "c": Shard(("gone",), (), 1.0, "run 1")}, tests),
        "stale test": ({**table, "c": Shard((), ("beta::test_gone",), 1.0, "run 1")}, tests),
        "redundant name": ({"a": replace(table["a"], tests=("alpha::test_x",)), "b": table["b"]}, tests),
    }
    for label, (shards, names) in failing.items():
        assert check(shards, names), f"shard-table self-test {label} should fail"

    good = Shard(("m",), (), 19.9, "run 1")
    assert budget_problems({"ok": good}) == []
    for label, shard in {
        "unmeasured": replace(good, ci_minutes=None),
        "pending": replace(good, measured_in="pending"),
        "no run": replace(good, measured_in=""),
        "slow": replace(good, ci_minutes=20.5),
    }.items():
        assert budget_problems({label: shard}), f"budget self-test {label} should fail"

    # The harness's streamed output, abridged: counts and per-test gaps. The
    # second test's prefix shared a line with its own output, which the
    # runner's noise filter dropped, so only its bare result line remains.
    log = WidgetLog(["window::test_a", "window::test_b", "window::test_c"])
    for line, now in (
        ("   Compiling lushtext v0.8.3\n", 0.0),
        ("running 3 tests\n", 10.0),
        ("test window::test_a ... ok\n", 12.5),
        ("evidence line from test_b\n", 13.0),
        ("ok\n", 20.0),
        ("test window::test_c ... ok (FLAKY: passed on attempt 2)\n", 21.0),
        ("test result: ok. all tests passed (1 flaky on retry)\n", 21.1),
    ):
        log.feed(line, now)
    assert log.running_counts == [3]
    assert log.tests == [
        {"name": "window::test_a", "result": "ok", "seconds": 2.5},
        {"name": "window::test_b", "result": "ok", "seconds": 7.5},
        {"name": "window::test_c", "result": "ok (FLAKY: passed on attempt 2)", "seconds": 1.0},
    ], log.tests
    assert count_problems(3, log) == []
    assert count_problems(4, log)
    assert count_problems(3, WidgetLog([]))
    # A whole-run retry restarts the durations; a short count is unattributed.
    log.feed("running 3 tests\n", 30.0)
    log.feed("test window::test_a ... FAILED\n", 31.0)
    assert log.running_counts == [3, 3] and not log.attributed
    assert log.tests == [{"name": "#1", "result": "FAILED", "seconds": 1.0}], log.tests
    log = WidgetLog(["window::test_a", "window::test_b"])
    log.feed("running 2 tests\n", 0.0)
    log.feed("test window::test_a ... ok\n", 2.5)
    log.feed("test window::test_b ... ok\n", 10.0)

    record = {
        "shard": "window",
        "status": 0,
        "expected": 2,
        "suite_total": 5,
        "selected": [2],
        "durations_attributed": True,
        "wall_seconds": 90.0,
        "wall_minutes": 1.5,
        "tests": log.tests,
    }
    table_text = summary_markdown([record])
    assert "| `window` | 0 | 1.5 | 2 / 5 |" in table_text, table_text
    assert "| `window::test_b` | ok | 7.5 |" in table_text, table_text


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=["list", "check", "github-outputs", "run"])
    parser.add_argument("shard", nargs="?", default="all", choices=["all", *SHARDS])
    parser.add_argument("--measure", metavar="JSON", help="run: record per-shard measurements to this JSON file")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    tests = discover()
    if args.command == "list":
        for shard_name, names in assignment(SHARDS, tests).items():
            shard = SHARDS[shard_name]
            budget = "unmeasured" if shard.ci_minutes is None else f"{shard.ci_minutes} min on the runner"
            print(f"{shard_name} ({budget}): {len(names)} tests")
            for name in names:
                print(f"  {name}")
        return 0
    problems = check(SHARDS, tests)
    # Budgets gate the table (`check`) and the CI matrix (`github-outputs`), not
    # `run`: a shard over the margin must stay runnable so it can be re-measured.
    if args.command in ("check", "github-outputs"):
        problems += budget_problems(SHARDS)
    if problems:
        for problem in problems:
            print(f"widget-shards: {problem}", file=sys.stderr)
        return 1
    if args.command == "check":
        print(f"widget shard table passed: {len(tests)} widget tests, each in exactly one of {len(SHARDS)} shards")
        return 0
    if args.command == "github-outputs":
        print(f"shards={json.dumps(list(SHARDS))}")
        return 0
    return run(list(SHARDS) if args.shard == "all" else [args.shard], tests, args.measure)


if __name__ == "__main__":
    sys.exit(main())
