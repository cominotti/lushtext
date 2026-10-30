#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run the widget tests in named CI shards, and check the shard table.

Every widget test belongs to exactly one shard. CI runs one shard per matrix
job so each job stays inside the repository's 30-minute job cap even though the
whole suite, run serially in one headless session, takes longer than that on
the shared runner. `make test-widget-shard WIDGET_SHARD=<name>` reproduces one
CI shard locally.

`parallel` is the local full run behind `make test`, `make test-widget`, and
`make test-widget-headless`: one shared build of the widget binary in a
memory-capped scope, the same `--list` cross-check as `run`, then every shard
at once, each through `scripts/with-memory-cap.sh <cap>
scripts/run-widget-tests.sh --scope-label wN`, so each has its own memory scope,
temp scope, D-Bus session, Mutter, and runtime directory. It relays each
shard's output prefixed `[<shard>]`, keeps each shard's whole output in
`target/widget-parallel/<shard>.log`, lets every shard finish, and prints one
summary. `--retries N` is passed to the runner, which reruns a failed shard in
a new session; a shard that needed a rerun is printed as `FLAKY-SHARD:`.

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
  scripts/widget-shards.py parallel [--retries N] [--jobs N] [--measure JSON]
      [--shard-memory-max CAP] [--build-memory-max CAP] [--build-jobs N]

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
import queue
import re
import signal
import subprocess
import sys
import threading
import time
from dataclasses import dataclass, field, replace
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
# The runs every budget below comes from: CI runs 39448450712, 39464816359,
# and 40770877859 on `main` (Fedora 44 container), the three most recent with
# a `widget-measure-<shard>` artifact for every shard. After the 2026-10-28
# rebalance (window was 10.2-12.2 min clean, and 26.27 min in 39464816359)
# each budget is recomputed, not re-measured: per run and shard, the shard's
# own overhead (its widget-step wall time minus its per-test durations, i.e.
# build, `--list`, and session setup) plus that run's per-test durations of the
# tests in the NEW membership (a moved test or module is charged at its
# measured duration, against the receiving shard's overhead), the largest of
# the runs, rounded up to 0.1 minute. Run 39448450712: window 11.42,
# editor-page 10.14, surfaces 10.81; run 39464816359: editor-page 10.64,
# surfaces 11.28; run 40770877859: window 11.11, editor-page 10.95, surfaces
# 11.15. Run 39464816359's window step (26.27 min) is excluded from window's
# budget: its overhead was a failed first suite attempt plus the runner's
# whole-suite `--retries 1` rerun, after
# `window::test_document_sized_local_history_restore_and_undo_are_bounded_and_exact`
# timed out twice in a 5 s wait. That was a flaky test, not a shard size no
# membership could absorb (a full retry doubles any shard); the same change
# fixed it (its preview wait copied the whole 11 MiB preview buffer on every
# poll, starving the sliced install it waited for, on a 5 s budget).
MEASURED_IN = "runs 39448450712, 39464816359, 40770877859 (recomputed for the 2026-10-28 membership; max of the runs)"
SHARDS: dict[str, Shard] = {
    "window": Shard(
        modules=("window",),
        tests=(),
        ci_minutes=11.5,
        measured_in=MEASURED_IN,
    ),
    "editor-page": Shard(
        # Rebalanced 2026-10-28 from `window` (multi_window_drafts and two
        # slow local-history/restore tests) and `surfaces` (attention_refresh,
        # sidebar, workspace_entry_visibility).
        modules=("attention_refresh", "editor_page", "multi_window_drafts", "sidebar", "workspace_entry_visibility"),
        tests=(
            "window::test_local_history_restore_defers_compactly_until_progress_capacity_clears",
            "window::test_selected_restore_tab_gets_bounded_priority_after_capacity_releases",
        ),
        ci_minutes=11.0,
        measured_in=MEASURED_IN,
    ),
    "surfaces": Shard(
        modules=(
            "accessibility",
            "app",
            "command_palette",
            "editor_memory_eviction",
            "file_tree_item",
            "focus_mode",
            "gtk_lush_adoption",
            "markdown_preview",
            "open_popover",
            "plain_disposal",
            "preferences",
            "properties_panel",
            "search_bar",
            "search_panel",
            "shell_geometry",
            "status_bar",
            "tab_strip",
            "transient_dismissal",
            "window_composition",
            "window_construction",
            "window_preview",
            "workspace_section",
            "workspace_tree_virtualization",
        ),
        tests=("editor_page::test_minimap_long_line_warning_scan_slices_large_many_short_buffer",),
        ci_minutes=11.3,
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
    if not cross_check(static_tests, binary_tests):
        return 1
    by_shard = shard_order(static_tests, binary_tests)
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


# --- `parallel`: every shard at once, locally -------------------------------

MEMORY_CAP = REPO_ROOT / "scripts/with-memory-cap.sh"
# Per-shard memory scope (dbus-daemon, Mutter, the harness parent, and its
# current test child). Measured 2026-10-30: see `.agents/rules/build.md`.
DEFAULT_SHARD_MEMORY_MAX = "6G"
DEFAULT_BUILD_MEMORY_MAX = "16G"
DEFAULT_BUILD_JOBS = 4
# `sun_path` holds 108 bytes; the margin covers the other sockets a session
# opens next to `wayland-0`.
SOCKET_PATH_LIMIT = 100
RERUN_MARKER = "Widget test run failed on attempt"
FLAKY_NAME_RE = re.compile(r"^FLAKY: (\S+)$")
MEMORY_RE = re.compile(r"^widget-memory: peak (\d+) MiB, oom_kill (\d+)$")
# Live lines are shortened past this (the per-shard log keeps them whole):
# `with-memory-cap.sh` echoes its command, several hundred `--exact` names.
LIVE_LINE_LIMIT = 500


def scope_label(index: int) -> str:
    """A shard's temp-scope label: short, so its socket path stays short."""
    return f"w{index}"


def socket_path_problem(base: str, labels: list[str], pid_digits: int) -> str | None:
    """Why the longest Wayland socket path a shard's session would get, under
    `with-temp-scope.sh --session-runtime`, is too long, or None."""
    longest = max(
        f"{base.rstrip('/')}/lt-scope-{'0' * pid_digits}-{label}-XXXXXX/.session-runtime/wayland-0" for label in labels
    )
    if len(longest) <= SOCKET_PATH_LIMIT:
        return None
    return (
        f"a shard's Wayland socket path would be {len(longest)} bytes (limit {SOCKET_PATH_LIMIT}): {longest}; "
        f"the temporary directory {base} (TMPDIR) is too deep, so point TMPDIR at a shorter one"
    )


def max_pid_digits() -> int:
    try:
        return len(str(int(Path("/proc/sys/kernel/pid_max").read_text()) - 1))
    except (OSError, ValueError):
        return 7


@dataclass
class ShardJob:
    shard: str
    names: list[str]
    command: list[str]


@dataclass
class ShardOutcome:
    """What one shard's stream and exit said."""

    shard: str
    expected: int
    log: WidgetLog
    status: int | None = None
    attempts: int = 1
    failed: list[str] = field(default_factory=list)
    earlier_failed: list[str] = field(default_factory=list)
    flaky: list[str] = field(default_factory=list)
    peak_mib: int | None = None
    oom_kills: int = 0
    wall_seconds: float = 0.0
    _in_failures: bool = False

    def feed(self, line: str, now: float) -> None:
        self.log.feed(line, now)
        text = line.strip()
        if RERUN_MARKER in text:
            # The runner reruns the whole shard in a new scope and session;
            # what the failed attempt reported moves aside.
            self.attempts += 1
            self.earlier_failed += self.failed
            self.failed, self.flaky = [], []
        elif text == "failures:":
            self._in_failures = True
        elif self._in_failures:
            if text and not text.startswith("test result:"):
                self.failed.append(text)
            else:
                self._in_failures = False
        elif match := FLAKY_NAME_RE.match(text):
            self.flaky.append(match.group(1))
        elif match := MEMORY_RE.match(text):
            self.peak_mib, self.oom_kills = int(match.group(1)), int(match.group(2))

    def problems(self) -> list[str]:
        problems = count_problems(self.expected, self.log)
        if self.status is None:
            problems.append("never started")
        elif self.status != 0:
            problems.append(f"exited with status {self.status}")
        if self.oom_kills:
            problems.append(f"{self.oom_kills} process(es) OOM-killed in its memory scope")
        return problems


class ParallelRun:
    """Run shard jobs, at most `max_jobs` at once, each in its own session
    (`setsid`) so the terminal's Ctrl-C reaches only this driver, which
    forwards SIGINT/SIGTERM to every running shard's process group, starts no
    queued shard, and waits for the running ones so each scope's traps release
    their directories."""

    def __init__(self, jobs: list[ShardJob], max_jobs: int, log_dir: Path, out=sys.stdout, err=sys.stderr) -> None:
        self.jobs = jobs
        self.max_jobs = max(1, max_jobs)
        self.log_dir = log_dir
        self.out = out
        self.err = err
        self.lock = threading.Lock()
        self.finished: queue.Queue[str] = queue.Queue()
        self.running: dict[str, subprocess.Popen[str]] = {}
        self.stop_signal: int | None = None

    def _relay(self, job: ShardJob, child: subprocess.Popen[str], outcome: ShardOutcome, started: float) -> None:
        assert child.stdout is not None
        with open(self.log_dir / f"{job.shard}.log", "w", encoding="utf-8") as log:
            for line in child.stdout:
                log.write(line)
                log.flush()
                live = line.rstrip("\n")
                if len(live) > LIVE_LINE_LIMIT:
                    live = live[:LIVE_LINE_LIMIT] + f" ... [{len(live) - LIVE_LINE_LIMIT} more characters in the log]"
                with self.lock:
                    self.out.write(f"[{job.shard}] {live}\n")
                    self.out.flush()
                outcome.feed(line, time.monotonic())
        status = child.wait()
        outcome.status = 128 - status if status < 0 else status
        outcome.wall_seconds = round(time.monotonic() - started, 1)
        self.finished.put(job.shard)

    def _forward(self, signum: int, _frame: object) -> None:
        self.stop_signal = self.stop_signal or signum
        for child in list(self.running.values()):
            try:
                os.killpg(child.pid, signum)
            except ProcessLookupError:
                pass

    def run(self) -> tuple[list[ShardOutcome], int]:
        """Each job's outcome, in job order, and 0, or 128 + the signal that
        stopped the run."""
        self.log_dir.mkdir(parents=True, exist_ok=True)
        outcomes = {job.shard: ShardOutcome(job.shard, len(job.names), WidgetLog(job.names)) for job in self.jobs}
        pending = list(self.jobs)
        threads = []
        previous = {sig: signal.signal(sig, self._forward) for sig in (signal.SIGINT, signal.SIGTERM)}
        try:
            while pending or self.running:
                while pending and len(self.running) < self.max_jobs and self.stop_signal is None:
                    job = pending.pop(0)
                    child = subprocess.Popen(
                        job.command,
                        cwd=REPO_ROOT,
                        stdout=subprocess.PIPE,
                        stderr=subprocess.STDOUT,
                        text=True,
                        errors="replace",
                        start_new_session=True,
                    )
                    self.running[job.shard] = child
                    thread = threading.Thread(
                        target=self._relay, args=(job, child, outcomes[job.shard], time.monotonic()), daemon=True
                    )
                    thread.start()
                    threads.append(thread)
                if self.stop_signal is not None:
                    pending = []
                if not self.running:
                    break
                try:
                    # A timeout, so a signal handler runs promptly.
                    self.running.pop(self.finished.get(timeout=0.2))
                except queue.Empty:
                    pass
            for thread in threads:
                thread.join()
        finally:
            for sig, handler in previous.items():
                signal.signal(sig, handler)
        status = 0 if self.stop_signal is None else 128 + self.stop_signal
        return [outcomes[job.shard] for job in self.jobs], status


def summary_text(outcomes: list[ShardOutcome], total: int) -> str:
    rows = [("Shard", "Status", "Attempts", "Selected/expected", "Minutes", "Peak MiB", "Failed", "FLAKY")]
    for outcome in outcomes:
        selected = "/".join(str(count) for count in outcome.log.running_counts) or "-"
        rows.append(
            (
                outcome.shard,
                "not run" if outcome.status is None else ("ok" if not outcome.problems() else f"FAILED ({outcome.status})"),
                str(outcome.attempts),
                f"{selected}/{outcome.expected}",
                f"{outcome.wall_seconds / 60:.1f}",
                "-" if outcome.peak_mib is None else str(outcome.peak_mib),
                ", ".join(outcome.failed) or "-",
                ", ".join(outcome.flaky) or "-",
            )
        )
    widths = [max(len(row[column]) for row in rows) for column in range(len(rows[0]))]
    lines = ["", f"Widget shards ({total} widget tests):"]
    lines += ["  " + "  ".join(cell.ljust(width) for cell, width in zip(row, widths)).rstrip() for row in rows]
    return "\n".join(lines) + "\n"


def evaluate(outcomes: list[ShardOutcome], total: int, err=sys.stderr) -> int:
    """Repeat every FLAKY test and shard rerun on stderr, and return 1 unless
    every shard passed and their selected counts sum to the binary's total."""
    failed = False
    for outcome in outcomes:
        for name in outcome.flaky:
            print(f"FLAKY: [{outcome.shard}] {name}", file=err)
        if outcome.attempts > 1:
            earlier = ", ".join(outcome.earlier_failed) or "no test result"
            print(
                f"FLAKY-SHARD: {outcome.shard} needed {outcome.attempts} attempts (earlier attempts failed: {earlier})",
                file=err,
            )
        for problem in outcome.problems():
            print(f"widget-shards: shard {outcome.shard}: {problem}", file=err)
            failed = True
    selected = sum(outcome.log.running_counts[-1] for outcome in outcomes if outcome.log.running_counts)
    if selected != total:
        print(f"widget-shards: the shards selected {selected} tests, but the binary has {total}", file=err)
        failed = True
    return 1 if failed else 0


def cross_check(static_tests: list[str], binary_tests: list[str]) -> bool:
    """Whether static discovery and the compiled binary's --list agree."""
    missing = sorted(set(static_tests) - set(binary_tests))
    extra = sorted(set(binary_tests) - set(static_tests))
    if not missing and not extra and len(binary_tests) == len(static_tests):
        return True
    for name in missing:
        print(f"widget-shards: {name} is discovered from source but not in the binary's --list", file=sys.stderr)
    for name in extra:
        print(f"widget-shards: {name} is in the binary's --list but not discovered from source", file=sys.stderr)
    print("widget-shards: the static discovery and the compiled widget binary disagree", file=sys.stderr)
    return False


def shard_order(static_tests: list[str], binary_tests: list[str]) -> dict[str, list[str]]:
    """Each shard's tests in the binary's own order, which is the order the
    harness runs them in and so the order results are attributed in."""
    order = {name: index for index, name in enumerate(binary_tests)}
    return {name: sorted(tests, key=order.__getitem__) for name, tests in assignment(SHARDS, static_tests).items()}


def parallel(static_tests: list[str], args: argparse.Namespace) -> int:
    base = os.environ.get("TMPDIR") or "/tmp"
    labels = [scope_label(index) for index in range(len(SHARDS))]
    if problem := socket_path_problem(base, labels, max_pid_digits()):
        print(f"widget-shards: {problem}", file=sys.stderr)
        return 2
    if os.environ.get("LUSHTEXT_MEMORY_CAP_KIB"):
        print(
            "widget-shards: already inside a memory-capped scope, so every shard shares that one cap "
            "instead of its own; start the parallel run unwrapped to give each shard its own scope",
            file=sys.stderr,
        )
    build = [
        str(MEMORY_CAP), args.build_memory_max,
        "cargo", "test", "-j", str(args.build_jobs), "-p", "lushtext", "--test", "widget", "--no-run",
    ]  # fmt: skip
    print(f"Building the widget test binary once: {' '.join(build[2:])}", flush=True)
    if status := subprocess.run(build, cwd=REPO_ROOT).returncode:
        print(
            "widget-shards: the shared build failed; without a user systemd manager the parallel run "
            "cannot cap its shards, so use WIDGET_PARALLEL=0 for one serial session",
            file=sys.stderr,
        )
        return status
    binary_tests = listed_tests()
    if not cross_check(static_tests, binary_tests):
        return 1
    by_shard = shard_order(static_tests, binary_tests)
    jobs = [
        ShardJob(
            shard,
            names,
            [
                str(MEMORY_CAP), args.shard_memory_max,
                str(RUNNER), "--headless", "--scope-label", label, "--retries", str(args.retries),
                "--", "--exact", *names,
            ],  # fmt: skip
        )
        for (shard, names), label in zip(by_shard.items(), labels)
    ]
    max_jobs = args.jobs or len(jobs)
    log_dir = REPO_ROOT / "target/widget-parallel"
    for stale in log_dir.glob("*.log"):
        stale.unlink()
    print(
        f"Running {len(jobs)} widget shards, at most {max_jobs} at once, "
        f"each in its own {args.shard_memory_max} memory scope and headless session; logs in {log_dir}",
        flush=True,
    )
    outcomes, stopped = ParallelRun(jobs, max_jobs, log_dir).run()
    print(summary_text(outcomes, len(static_tests)), flush=True)
    status = evaluate(outcomes, len(static_tests))
    if args.measure is not None:
        records = [
            {
                "shard": outcome.shard,
                "status": outcome.status,
                "attempts": outcome.attempts,
                "expected": outcome.expected,
                "suite_total": len(static_tests),
                "selected": outcome.log.running_counts,
                "durations_attributed": outcome.log.attributed,
                "wall_seconds": outcome.wall_seconds,
                "wall_minutes": round(outcome.wall_seconds / 60, 2),
                "peak_mib": outcome.peak_mib,
                "tests": outcome.log.tests,
            }
            for outcome in outcomes
        ]
        Path(args.measure).write_text(json.dumps({"shards": records}, indent=2) + "\n", encoding="utf-8")
    return stopped or status


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

    self_test_parallel()


def self_test_parallel() -> None:
    """The parallel driver's scheduler, merge, and failure semantics, with
    shell stubs standing in for `run-widget-tests.sh` (no GTK, no session)."""
    import io
    import signal
    import tempfile
    import threading

    # The socket guard: `/tmp` fits; a base that pushes the socket past the
    # limit is refused and named.
    assert socket_path_problem("/tmp", ["w0", "w1", "w2"], 7) is None
    deep = "/tmp/" + "d" * 60
    problem = socket_path_problem(deep, ["w0"], 7)
    assert problem is not None and deep in problem and "TMPDIR" in problem, problem

    def stub(text: str, status: int = 0) -> list[str]:
        return ["sh", "-c", f"printf '%s\\n' {text}; exit {status}"]

    def quoted(*lines: str) -> str:
        return " ".join("'" + line.replace("'", "'\\''") + "'" for line in lines)

    with tempfile.TemporaryDirectory() as scratch:
        log_dir = Path(scratch) / "logs"
        jobs = [
            ShardJob("a", ["a::one", "a::two"], stub(quoted(
                "running 2 tests", "test a::one ... ok", "test a::two ... ok",
                "widget-memory: peak 812 MiB, oom_kill 0"))),
            ShardJob("b", ["b::bad"], stub(quoted(
                "running 1 tests", "test b::bad ... FAILED", "failures:", "    b::bad",
                "test result: FAILED. 1 failed"), 101)),
            ShardJob("c", ["c::x"], stub(quoted(
                "running 1 tests", "test c::x ... FAILED",
                "Widget test run failed on attempt 1/2; retrying...",
                "running 1 tests", "test c::x ... ok (FLAKY: passed on attempt 2)",
                "FLAKY SUMMARY: 1 widget test(s) passed only on retry - investigate and fix the root cause:",
                "    FLAKY: c::x"))),
        ]
        out, err = io.StringIO(), io.StringIO()
        outcomes, status = ParallelRun(jobs, 2, log_dir, out, err).run()
        assert status == 0, status  # the driver only signals stops; verdicts come from evaluate()
        by_name = {outcome.shard: outcome for outcome in outcomes}
        assert by_name["a"].status == 0 and by_name["a"].peak_mib == 812
        assert by_name["b"].status == 101 and by_name["b"].failed == ["b::bad"], by_name["b"]
        assert by_name["c"].status == 0 and by_name["c"].attempts == 2 and by_name["c"].flaky == ["c::x"]
        assert "[a] running 2 tests" in out.getvalue(), out.getvalue()
        assert (log_dir / "b.log").read_text().splitlines()[0] == "running 1 tests"
        verdict = evaluate(outcomes, total=4, err=err)
        assert verdict != 0
        text = summary_text(outcomes, total=4)
        assert "b::bad" in text and "FAILED" in text, text
        assert "FLAKY-SHARD: c" in err.getvalue() and "FLAKY: [c] c::x" in err.getvalue(), err.getvalue()

        # A green run passes; selected counts that miss the total fail it.
        green = [ShardJob("a", ["a::one"], stub(quoted("running 1 tests", "test a::one ... ok")))]
        outcomes, _ = ParallelRun(green, 1, log_dir, io.StringIO(), io.StringIO()).run()
        assert evaluate(outcomes, total=1, err=io.StringIO()) == 0
        assert evaluate(outcomes, total=2, err=io.StringIO()) != 0
        # A shard whose harness selected the wrong count fails.
        short = [ShardJob("a", ["a::one", "a::two"], stub(quoted("running 1 tests", "test a::one ... ok")))]
        outcomes, _ = ParallelRun(short, 1, log_dir, io.StringIO(), io.StringIO()).run()
        assert evaluate(outcomes, total=2, err=io.StringIO()) != 0
        # An OOM kill reported by the runner fails the shard even at status 0.
        oom = [ShardJob("a", ["a::one"], stub(quoted(
            "running 1 tests", "test a::one ... ok", "widget-memory: peak 6100 MiB, oom_kill 1")))]
        outcomes, _ = ParallelRun(oom, 1, log_dir, io.StringIO(), io.StringIO()).run()
        assert evaluate(outcomes, total=1, err=io.StringIO()) != 0

        # SIGTERM reaches every running shard's process group, which runs its
        # own trap, and no queued shard starts; the run exits 143.
        marker = Path(scratch) / "trapped"
        sleeper = ["sh", "-c", f"trap 'touch {marker}; exit 143' TERM; echo started; sleep 30 & wait"]
        never = Path(scratch) / "never"
        queued = ["sh", "-c", f"touch {never}"]
        timer = threading.Timer(1.0, os.kill, (os.getpid(), signal.SIGTERM))
        timer.start()
        started = time.monotonic()
        outcomes, status = ParallelRun(
            [ShardJob("s", [], sleeper), ShardJob("q", [], queued)], 1, log_dir, io.StringIO(), io.StringIO()
        ).run()
        timer.join()
        assert status == 143, status
        assert marker.exists(), "the running shard did not receive SIGTERM"
        assert not never.exists(), "a queued shard started after SIGTERM"
        assert time.monotonic() - started < 10


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=["list", "check", "github-outputs", "run", "parallel"])
    parser.add_argument("shard", nargs="?", default="all", choices=["all", *SHARDS])
    parser.add_argument(
        "--measure", metavar="JSON", help="run, parallel: record per-shard measurements to this JSON file"
    )
    parser.add_argument("--retries", type=int, default=0, help="parallel: rerun a failed shard up to N times")
    parser.add_argument("--jobs", type=int, default=0, help="parallel: at most N shards at once (default: all)")
    parser.add_argument("--shard-memory-max", default=DEFAULT_SHARD_MEMORY_MAX, help="parallel: per-shard scope cap")
    parser.add_argument("--build-memory-max", default=DEFAULT_BUILD_MEMORY_MAX, help="parallel: build scope cap")
    parser.add_argument("--build-jobs", type=int, default=DEFAULT_BUILD_JOBS, help="parallel: cargo -j for the build")
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
    if args.command == "parallel":
        return parallel(tests, args)
    return run(list(SHARDS) if args.shard == "all" else [args.shard], tests, args.measure)


if __name__ == "__main__":
    sys.exit(main())
