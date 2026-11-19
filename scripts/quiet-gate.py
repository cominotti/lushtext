#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Drain-first quiet gate for timing verdicts.

The rule is `.agents/rules/parallel-agent-work.md`, "Measurements"; the
reasons are in `docs/memorials/2026-11-18-change-flow-time.md`.

    quiet-gate.py busy [--window S]
        Busy CPUs over the window, from two /proc/stat reads.
    quiet-gate.py outsiders [--window S]
        Host CPU use outside our own work, in CPUs: the root cgroup's
        usage minus `user-lushtext.slice` (the parent of
        `scripts/with-memory-cap.sh --print-slice`), the legacy
        `lushtext-heavy.slice`, and the `cst-sccache-*.scope` siblings,
        which can run our compiles.
    quiet-gate.py wait-outsiders [--below 3.5] [--window 30] [--timeout 540]
        Holding nothing, sample until the outside use is below the bar
        (exit 0) or the timeout passes (exit 124).
    quiet-gate.py run --label L --log FILE [--mode gate|calib] [--below 4] [--window 30] -- <command...>
        The in-lock wrapper, run as measure.sh's command. It samples busy
        CPUs for the window, runs the command, measures busy CPUs over the
        whole command (the round's own use included), and samples again
        after it, logging every reading as one line. In `gate` mode it exits
        76 without running the command when the first sample is not below
        the bar, and 77 when the command exited 0 but the run or the after
        sample was not below it (the round ran and is not kept). Otherwise
        it returns the command's status. `calib` records and never gates.
    quiet-gate.py --self-test

Exit 2 means a reading could not be taken (an unreadable /proc/stat or
cpu.stat, no user manager, a usage error); no reading is printed then, so a
missing measurement never admits a round. 76 and 77 are distinct from
measure.sh's 75 (lock timeout) and 78 (reference CPUs unavailable).

Overrides, for the self-test and for hosts with another layout:
LUSHTEXT_QUIET_PROC_STAT (default /proc/stat), LUSHTEXT_QUIET_CGROUP_ROOT
(default /sys/fs/cgroup), and LUSHTEXT_QUIET_OWN_CGROUPS, a colon-separated
list of cgroup paths relative to that root which replaces the systemd lookup
of our own slices (sccache scopes are then globbed beside the first entry).
"""

from __future__ import annotations

import argparse
import datetime
import glob
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
EXIT_UNREADABLE = 2
EXIT_TIMEOUT = 124
EXIT_NOT_QUIET = 76
EXIT_NOT_KEPT = 77
ECHO_LOG = True


class ReadingError(Exception):
    """A counter could not be read; the caller exits 2 with no reading."""


class Host:
    """Reads the host's CPU counters. The self-test substitutes a fake."""

    def __init__(self) -> None:
        self.proc_stat = os.environ.get("LUSHTEXT_QUIET_PROC_STAT", "/proc/stat")
        self.cgroup_root = os.environ.get("LUSHTEXT_QUIET_CGROUP_ROOT", "/sys/fs/cgroup")
        self._own: list[str] | None = None

    def stat(self) -> tuple[int, int, int]:
        """Return (total jiffies, idle+iowait jiffies, CPU count)."""
        try:
            lines = Path(self.proc_stat).read_text(encoding="ascii").splitlines()
        except OSError as error:
            raise ReadingError(f"cannot read {self.proc_stat}: {error}") from error
        total = idle = None
        cpus = 0
        for line in lines:
            fields = line.split()
            if not fields:
                continue
            if fields[0] == "cpu":
                try:
                    values = [int(value) for value in fields[1:]]
                except ValueError as error:
                    raise ReadingError(f"malformed cpu line in {self.proc_stat}") from error
                if len(values) < 5:
                    raise ReadingError(f"short cpu line in {self.proc_stat}")
                # user, nice, system, idle, iowait, irq, softirq, steal; guest and
                # guest_nice are already inside user and nice.
                total, idle = sum(values[:8]), values[3] + values[4]
            elif fields[0].startswith("cpu") and fields[0][3:].isdigit():
                cpus += 1
        if total is None or idle is None or cpus == 0:
            raise ReadingError(f"no cpu lines in {self.proc_stat}")
        return total, idle, cpus

    def usage_usec(self, relative: str) -> int | None:
        """usage_usec of one cgroup, or None when the cgroup does not exist."""
        directory = Path(self.cgroup_root) / relative.lstrip("/")
        path = directory / "cpu.stat"
        if not directory.is_dir():
            return None
        try:
            for line in path.read_text(encoding="ascii").splitlines():
                if line.startswith("usage_usec "):
                    return int(line.split()[1])
        except (OSError, ValueError) as error:
            raise ReadingError(f"cannot read {path}: {error}") from error
        raise ReadingError(f"{path} has no usage_usec")

    def own_cgroups(self) -> list[str]:
        """Our own slices and the sccache scopes beside them, as relative paths."""
        if self._own is None:
            self._own = self._resolve_own()
        base = self._own
        scopes: list[str] = []
        if base:
            parent = (Path(self.cgroup_root) / base[0].lstrip("/")).parent
            for scope in sorted(glob.glob(str(parent / "cst-sccache-*.scope"))):
                scopes.append(os.path.relpath(scope, self.cgroup_root))
        return [*base, *scopes]

    def _resolve_own(self) -> list[str]:
        override = os.environ.get("LUSHTEXT_QUIET_OWN_CGROUPS")
        if override is not None:
            return [entry for entry in override.split(":") if entry]
        try:
            heavy = subprocess.run(
                [str(SCRIPT_DIR / "with-memory-cap.sh"), "--print-slice"],
                check=True, text=True, capture_output=True,
            ).stdout.strip()
        except (OSError, subprocess.CalledProcessError) as error:
            raise ReadingError(f"with-memory-cap.sh --print-slice failed: {error}") from error
        if not heavy.endswith(".slice") or "-" not in heavy:
            raise ReadingError(f"unexpected heavy slice name {heavy!r}")
        parent = heavy[: -len(".slice")].rsplit("-", 1)[0] + ".slice"
        legacy = heavy.removeprefix("user-")
        units = [parent, legacy, "user.slice"]
        paths: list[str] = []
        for unit in units:
            try:
                value = subprocess.run(
                    ["systemctl", "--user", "show", "-p", "ControlGroup", "--value", unit],
                    check=True, text=True, capture_output=True,
                ).stdout.strip()
            except (OSError, subprocess.CalledProcessError) as error:
                raise ReadingError(f"systemctl --user show {unit} failed: {error}") from error
            paths.append(value)
        parent_path, legacy_path, user_slice = paths
        if not user_slice:
            raise ReadingError("the user manager's user.slice has no control group")
        # An inactive slice has no control group yet; its place under user.slice is known.
        if not parent_path:
            parent_path = f"{user_slice}/{parent}"
        own = [parent_path]
        if legacy_path:
            own.append(legacy_path)
        return own


def busy_cpus(host: Host, window: float) -> float:
    total0, idle0, cpus = host.stat()
    time.sleep(window)
    total1, idle1, _ = host.stat()
    return busy_between((total0, idle0, cpus), (total1, idle1, cpus))


def busy_between(before: tuple[int, int, int], after: tuple[int, int, int]) -> float:
    total = after[0] - before[0]
    idle = after[1] - before[1]
    if total <= 0:
        # No time passed on the counters: that is no reading, and a missing
        # reading never admits a round.
        raise ReadingError("the /proc/stat counters did not advance")
    return max(0.0, after[2] * (1.0 - idle / total))


def outside_cpus(host: Host, window: float) -> float:
    own = host.own_cgroups()

    def read() -> tuple[int, dict[str, int]]:
        root = host.usage_usec("")
        if root is None:
            raise ReadingError(f"no root cgroup under {host.cgroup_root}")
        values = {}
        for path in own:
            value = host.usage_usec(path)
            if value is not None:
                values[path] = value
        return root, values

    start = time.monotonic()
    root0, own0 = read()
    time.sleep(window)
    root1, own1 = read()
    elapsed = max(time.monotonic() - start, 1e-6)
    ours = 0
    for path, value in own1.items():
        # A cgroup that appeared during the window started from zero; one that
        # vanished is not subtracted, which can only overstate outside use.
        ours += value - own0.get(path, 0)
    return max(0.0, (root1 - root0 - ours) / (elapsed * 1e6))


def load_text() -> str:
    try:
        return " ".join(Path("/proc/loadavg").read_text(encoding="ascii").split()[:3])
    except OSError:
        return "unknown"


def log_line(path: str, text: str) -> None:
    line = f"{datetime.datetime.now().astimezone().isoformat(timespec='seconds')} {text} loadavg={load_text()}\n"
    with open(path, "a", encoding="utf-8") as handle:
        handle.write(line)
    if ECHO_LOG:
        sys.stderr.write("quiet-gate: " + line)


def run_round(host: Host, label: str, log: str, mode: str, below: float, window: float,
              command: list[str]) -> int:
    pre = busy_cpus(host, window)
    log_line(log, f"quiet-gate label={label} mode={mode} phase=pre pre_busy={pre:.2f} bar={below}")
    if mode == "gate" and not pre < below:
        log_line(log, f"quiet-gate label={label} mode={mode} NOT-QUIET pre_busy={pre:.2f}; command not run")
        return EXIT_NOT_QUIET
    before = host.stat()
    try:
        rc = subprocess.run(command, check=False).returncode
    except OSError as error:
        log_line(log, f"quiet-gate label={label} mode={mode} cannot start the command: {error}")
        return 127
    after = host.stat()
    run = busy_between(before, after)
    post = busy_cpus(host, window)
    kept = rc == 0 and run < below and post < below
    log_line(log, f"quiet-gate label={label} mode={mode} rc={rc} run_busy={run:.2f} "
                  f"post_busy={post:.2f} pre_busy={pre:.2f} kept={'yes' if kept else 'no'}")
    if mode == "gate" and rc == 0 and not kept:
        return EXIT_NOT_KEPT
    return rc


def wait_outsiders(host: Host, below: float, window: float, timeout: float) -> int:
    deadline = time.monotonic() + timeout
    while True:
        value = outside_cpus(host, window)
        if value < below:
            print(f"{value:.2f}")
            return 0
        sys.stderr.write(f"quiet-gate: outside CPU use {value:.2f} >= {below}; waiting, holding nothing\n")
        if time.monotonic() >= deadline:
            sys.stderr.write(f"quiet-gate: outside load stayed >= {below} for {timeout:.0f} s; queue the rounds\n")
            return EXIT_TIMEOUT


class FakeHost(Host):
    """Counters from scripted sequences, read in order."""

    def __init__(self, stats: list[tuple[int, int, int]], usage: dict[str, list[int | None]],
                 own: list[str]) -> None:
        self.stats = list(stats)
        self.usage = {key: list(values) for key, values in usage.items()}
        self._own = own
        self.cgroup_root = "/nonexistent-fake"

    def stat(self) -> tuple[int, int, int]:
        if not self.stats:
            raise ReadingError("fake /proc/stat exhausted")
        return self.stats.pop(0)

    def usage_usec(self, relative: str) -> int | None:
        values = self.usage.get(relative)
        if not values:
            raise ReadingError(f"fake cpu.stat {relative!r} exhausted")
        return values.pop(0)

    def own_cgroups(self) -> list[str]:
        return list(self._own)


def stat_pair(busy: float, cpus: int = 8, jiffies: int = 1000) -> list[tuple[int, int, int]]:
    """Two /proc/stat readings whose difference shows `busy` busy CPUs."""
    idle = int(jiffies * (1 - busy / cpus))
    return [(10_000, 5_000, cpus), (10_000 + jiffies, 5_000 + idle, cpus)]


def self_test() -> int:
    global ECHO_LOG
    ECHO_LOG = False
    failures: list[str] = []

    def check(condition: bool, message: str) -> None:
        if not condition:
            failures.append(message)

    check(abs(busy_cpus(FakeHost(stat_pair(2.0), {}, []), 0) - 2.0) < 1e-6, "busy CPUs from two stat reads")
    check(busy_cpus(FakeHost(stat_pair(0.0), {}, []), 0) == 0.0, "an idle host reads 0 busy CPUs")
    # Counters that did not advance are no reading, never a quiet one.
    try:
        busy_cpus(FakeHost([(10_000, 5_000, 8), (10_000, 5_000, 8)], {}, []), 0)
        check(False, "stat counters that did not advance must not read as 0 busy CPUs")
    except ReadingError:
        pass

    # The total is the first eight fields: user already includes guest and
    # nice includes guest_nice, so summing all ten counts a VM's time twice
    # (Fable review N3).
    with tempfile.TemporaryDirectory() as tmp:
        stat_file = os.path.join(tmp, "stat")
        Path(stat_file).write_text(
            "cpu  100 10 50 800 20 5 5 10 60 4\ncpu0 100 10 50 800 20 5 5 10 60 4\n", encoding="ascii")
        saved = os.environ.get("LUSHTEXT_QUIET_PROC_STAT")
        os.environ["LUSHTEXT_QUIET_PROC_STAT"] = stat_file
        try:
            total, idle, cpus = Host().stat()
            check((total, idle, cpus) == (1000, 820, 1),
                  f"the stat total must exclude guest and guest_nice: got {(total, idle, cpus)}")
        finally:
            if saved is None:
                os.environ.pop("LUSHTEXT_QUIET_PROC_STAT", None)
            else:
                os.environ["LUSHTEXT_QUIET_PROC_STAT"] = saved

    # Outside use: root minus our slices; a scope that appears mid-window counts from zero.
    usage = {"": [0, 1_000_000], "ours": [100, 400_100], "sccache": [None, 100_000]}
    value = outside_cpus(FakeHost([], usage, ["ours", "sccache"]), 0)
    check(value > 0, "outside use is positive when the root grows more than our slices")
    usage = {"": [0, 1_000_000], "ours": [0, 1_000_000]}
    check(outside_cpus(FakeHost([], usage, ["ours"]), 0) == 0.0, "our own use is not outside use")

    with tempfile.TemporaryDirectory() as tmp:
        log = os.path.join(tmp, "gate.log")
        marker = os.path.join(tmp, "ran")
        touch = [sys.executable, "-c", f"open({marker!r}, 'w').close()"]

        rc = run_round(FakeHost(stat_pair(6.0), {}, []), "busy-pre", log, "gate", 4, 0, touch)
        check(rc == EXIT_NOT_QUIET, f"a busy pre-sample exits 76, got {rc}")
        check(not os.path.exists(marker), "a busy pre-sample does not run the command")

        quiet = stat_pair(1.0) + stat_pair(1.0) + stat_pair(1.0)
        rc = run_round(FakeHost(quiet, {}, []), "quiet", log, "gate", 4, 0, touch)
        check(rc == 0, f"a quiet round returns the command's 0, got {rc}")
        check(os.path.exists(marker), "a quiet round runs the command")

        busy_post = stat_pair(1.0) + stat_pair(1.0) + stat_pair(5.0)
        rc = run_round(FakeHost(busy_post, {}, []), "busy-post", log, "gate", 4, 0, touch)
        check(rc == EXIT_NOT_KEPT, f"a busy post sample after exit 0 exits 77, got {rc}")
        busy_run = stat_pair(1.0) + stat_pair(4.5) + stat_pair(1.0)
        rc = run_round(FakeHost(busy_run, {}, []), "busy-run", log, "gate", 4, 0, touch)
        check(rc == EXIT_NOT_KEPT, f"a busy run after exit 0 exits 77, got {rc}")

        fail = [sys.executable, "-c", "raise SystemExit(3)"]
        rc = run_round(FakeHost(quiet[:] , {}, []), "fails", log, "gate", 4, 0, fail)
        check(rc == 3, f"a failing command's status is returned, got {rc}")

        calib = stat_pair(6.0) + stat_pair(6.0) + stat_pair(6.0)
        rc = run_round(FakeHost(calib, {}, []), "calib", log, "calib", 4, 0, touch)
        check(rc == 0, f"calib mode never gates, got {rc}")

        text = Path(log).read_text(encoding="utf-8")
        for label in ("busy-pre", "quiet", "busy-post", "busy-run", "fails", "calib"):
            check(f"label={label}" in text, f"round {label} was logged")
        check("NOT-QUIET" in text and "kept=no" in text and "kept=yes" in text, "log records the decisions")
        check("loadavg=" in text, "log records the load")

        # Fail closed: an unreadable counter exits 2 and prints no reading.
        env = dict(os.environ, LUSHTEXT_QUIET_PROC_STAT=os.path.join(tmp, "missing"))
        result = subprocess.run([sys.executable, "-I", __file__, "busy", "--window", "0"],
                                env=env, text=True, capture_output=True)
        check(result.returncode == EXIT_UNREADABLE and result.stdout == "",
              f"an unreadable /proc/stat exits 2 with no reading (rc {result.returncode}, out {result.stdout!r})")
        cg = os.path.join(tmp, "cg")
        os.makedirs(os.path.join(cg, "ours"))
        Path(cg, "cpu.stat").write_text("user_usec 1\n", encoding="ascii")
        env = dict(os.environ, LUSHTEXT_QUIET_CGROUP_ROOT=cg, LUSHTEXT_QUIET_OWN_CGROUPS="ours")
        result = subprocess.run([sys.executable, "-I", __file__, "outsiders", "--window", "0"],
                                env=env, text=True, capture_output=True)
        check(result.returncode == EXIT_UNREADABLE and result.stdout == "",
              f"a cpu.stat without usage_usec exits 2 (rc {result.returncode})")
        Path(cg, "cpu.stat").write_text("usage_usec 100\n", encoding="ascii")
        Path(cg, "ours", "cpu.stat").write_text("usage_usec 50\n", encoding="ascii")
        result = subprocess.run([sys.executable, "-I", __file__, "wait-outsiders", "--window", "0",
                                 "--timeout", "0", "--below", "1"],
                                env=env, text=True, capture_output=True)
        check(result.returncode == 0, f"wait-outsiders succeeds below the bar (rc {result.returncode}, {result.stderr})")
        result = subprocess.run([sys.executable, "-I", __file__, "wait-outsiders", "--window", "0",
                                 "--timeout", "0", "--below", "0"],
                                env=env, text=True, capture_output=True)
        check(result.returncode == EXIT_TIMEOUT, f"wait-outsiders exits 124 at the timeout (rc {result.returncode})")
        Path(cg, "ours", "cpu.stat").unlink()
        result = subprocess.run([sys.executable, "-I", __file__, "outsiders", "--window", "0"],
                                env=env, text=True, capture_output=True)
        check(result.returncode == EXIT_UNREADABLE, "an own cgroup without cpu.stat exits 2")

        # The real /proc/stat gives a reading between 0 and the CPU count.
        result = subprocess.run([sys.executable, "-I", __file__, "busy", "--window", "0.05"],
                                text=True, capture_output=True)
        try:
            reading = float(result.stdout)
            check(result.returncode == 0 and 0 <= reading <= (os.cpu_count() or 1024),
                  f"real busy reading {reading}")
        except ValueError:
            failures.append(f"real busy reading unparsable: {result.stdout!r} {result.stderr!r}")

    if failures:
        for failure in failures:
            print(f"quiet-gate self-test FAILED: {failure}", file=sys.stderr)
        return 1
    print("quiet-gate self-test: ok (busy CPUs, outside use minus our slices, 76 before, 77 after, "
          "command status, calib never gates, readings logged, unreadable counters exit 2, "
          "wait-outsiders 0 and 124)")
    return 0


def main(argv: list[str] | None = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    if argv == ["--self-test"]:
        return self_test()
    command: list[str] = []
    if "--" in argv:
        split = argv.index("--")
        argv, command = argv[:split], argv[split + 1:]
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="action", required=True)
    for name in ("busy", "outsiders"):
        child = sub.add_parser(name)
        child.add_argument("--window", type=float, default=30.0)
    child = sub.add_parser("wait-outsiders")
    child.add_argument("--below", type=float, default=3.5)
    child.add_argument("--window", type=float, default=30.0)
    child.add_argument("--timeout", type=float, default=540.0)
    child = sub.add_parser("run")
    child.add_argument("--label", required=True)
    child.add_argument("--log", required=True)
    child.add_argument("--mode", choices=("gate", "calib"), default="gate")
    child.add_argument("--below", type=float, default=4.0)
    child.add_argument("--window", type=float, default=30.0)
    try:
        args = parser.parse_args(argv)
    except SystemExit as error:
        return EXIT_UNREADABLE if error.code else 0
    host = Host()
    try:
        if args.action == "busy":
            print(f"{busy_cpus(host, args.window):.2f}")
            return 0
        if args.action == "outsiders":
            print(f"{outside_cpus(host, args.window):.2f}")
            return 0
        if args.action == "wait-outsiders":
            return wait_outsiders(host, args.below, args.window, args.timeout)
        if not command:
            print("quiet-gate: run needs a command after --", file=sys.stderr)
            return EXIT_UNREADABLE
        return run_round(host, args.label, args.log, args.mode, args.below, args.window, command)
    except ReadingError as error:
        print(f"quiet-gate: {error}; no reading", file=sys.stderr)
        return EXIT_UNREADABLE


if __name__ == "__main__":
    raise SystemExit(main())
