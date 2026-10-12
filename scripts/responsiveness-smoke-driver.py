#!/usr/bin/python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Long-line responsiveness lane driver (`make responsiveness-smoke`).

Drives the measurement build of LushText (built with the smoke-only
`responsiveness-probes` feature) inside a private headless session and turns
its in-process probe records into a report with per-case stall bounds, frame
gaps, input-to-presented-frame latency, IM and AT counts, and the derived
long-line thresholds and budget verdict that the document-scale and
protected-view changes consume.

Subcommands (the shell wrapper `scripts/run-responsiveness-smoke.sh` calls
them in order; nothing here runs against a live desktop):

``session``  inside ``dbus-run-session``: start the AT-SPI registry when an
             AT variant is selected, then run ``mutter --headless`` with
             ``run`` as its child.
``run``      inside the private compositor: preflight, calibration, the case
             sweep, bisection, integrity checks. Writes ``results.jsonl``.
``report``   after the session: storage evidence, warning findings, the D7
             derivation, ``report.json`` and ``report.md``.
``list-cases`` print the case ids of a tier.
``merge-runs`` join several runs' artifacts for one report.
``affinity-list`` the first N CPUs of the inherited affinity, as a
             ``taskset -c`` list (the wrapper's CPU cap).
``self-test`` exercise the parsing, classification, noise, contamination,
             derivation and data-integrity logic on synthetic records, without
             GTK or a compositor.

The probe JSONL contract is v1 (see ``KNOWN_KINDS``); every ``t_us`` is
``CLOCK_MONOTONIC`` in microseconds, the same clock as ``time.monotonic_ns``.
"""

from __future__ import annotations

import argparse
import fnmatch
import hashlib
import importlib.util
import json
import math
import os
import platform
import re
import shutil
import signal
import socket
import statistics
import subprocess
import sys
import tempfile
import time
from collections import Counter
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Callable, Iterable, Iterator

SCRIPT_PATH = Path(__file__).resolve()
REPO_ROOT = SCRIPT_PATH.parents[1]
SYSTEM_PYTHON = Path("/usr/bin/python3")
ATSPI_REGISTRYD = Path("/usr/libexec/at-spi2-registryd")
GENERATOR_PATH = REPO_ROOT / "scripts/generate-long-line-fixtures.py"

sys.path.insert(0, str(REPO_ROOT / "scripts"))
from smoke_warning_classifiers import is_gdk_broken_pipe_teardown  # noqa: E402

APP_ID = "dev.cominotti.lushtext"
APP_OBJECT_PATH = "/dev/cominotti/lushtext"
WINDOW_OBJECT_PATH = f"{APP_OBJECT_PATH}/window/1"
AUTOMATION_OBJECT_PATH = f"{APP_OBJECT_PATH}/Automation"
AUTOMATION_INTERFACE = "dev.cominotti.lushtext.Automation1"
SCHEMA_GROUP = "dev/cominotti/lushtext"
RD_BUS = "org.gnome.Mutter.RemoteDesktop"
RD_PATH = "/org/gnome/Mutter/RemoteDesktop"
RD_SESSION_IFACE = "org.gnome.Mutter.RemoteDesktop.Session"

# Budgets mirror crates/lushtext-core/src/model/responsiveness_budget.rs (the
# single source of truth; the report re-reads that file so a revised constant
# is picked up without editing this script).
BUDGET_SOURCE = REPO_ROOT / "crates/lushtext-core/src/model/responsiveness_budget.rs"
DEFAULT_BUDGETS = {
    "B_INTERACT_US": 8_000,
    "B_SETTLE_US": 16_000,
    "REFERENCE_FRAME_US": 8_333,
    "INPUT_TO_FRAME_FRAMES": 2,
    "SCROLL_P95_FRAME_GAP_US": 8_333,
}

MONITOR = os.environ.get("RESPONSIVENESS_MONITOR", "1280x800@120")
SCREEN_W, SCREEN_H = 1280, 800
KNOWN_KINDS = ("start", "late", "second", "frame", "input", "im", "dropped", "topk")

# Keysyms. Only these may be injected; the guard refuses any Control+letter so
# no save chord can ever reach the app.
KEY_RIGHT = 0xFF53
KEY_PAGE_DOWN = 0xFF56
KEY_HOME = 0xFF50
KEY_END = 0xFF57
KEY_CONTROL_L = 0xFFE3
KEY_SHIFT_L = 0xFFE1
BTN_LEFT = 0x110
ALLOWED_KEYSYMS = {KEY_RIGHT, KEY_PAGE_DOWN, KEY_HOME, KEY_END, KEY_CONTROL_L, KEY_SHIFT_L} | set(
    range(0x61, 0x7B)
)
TYPE_BURST = "abcdefghij"

# Sweep definition (design D7).
PARAGRAPH_LENGTHS = {"quick": (10_000, 250_000), "full": (2_000, 10_000, 50_000, 250_000, 1_900_000)}
DENSE_LINE_LENGTHS = {"quick": (2_000,), "full": (500, 2_000, 10_000, 50_000)}
DENSE_LINE_COUNT = 160
TOTAL_SIZES = {"quick": (1_000_000, 5_000_000), "full": (1_000_000, 2_000_000, 5_000_000, 10_000_000)}
DELIMITER_LONGEST = {"quick": 250_000, "full": 1_894_586}
SESSION_LENGTH = {"quick": 250_000, "full": 1_900_000}
INVISIBLES_LENGTHS = {"quick": (250_000,), "full": (2_000, 10_000, 50_000, 250_000, 1_900_000)}
INSTALL_SIZES = (100_000_000, 250_000_000, 500_000_000)
EVICTION_FILE_BYTES = 8 * 1024 * 1024
REPETITIONS = {"quick": 1, "full": 3}
GROUPS = ("baseline", "paragraph", "dense", "total-size", "delimiters", "session", "eviction", "reload", "install", "invisibles")
VARIANTS = ("plain", "at", "probe-im", "ibus")
BISECT_STEPS = 4

PHASE_KINDS = {
    "open": "settle",
    "background": "settle",
    "restore": "settle",
    "reload": "settle",
    "scroll": "interaction",
    "click": "interaction",
    "pagedown": "interaction",
    "type-start": "interaction",
    "type-middle": "interaction",
    "type-end": "interaction",
    "find": "interaction",
    "tab-switch": "interaction",
    "reselect": "interaction",
    "idle": "calibration",
}
CARET_PHASES = ("click", "type-start", "type-middle", "type-end")
# Window construction (open, restore) is owned by C4 and left out of stall verdicts.
STALL_EXCLUDED_PHASES = ("open", "restore")

KNOWN_DEFECTS = (
    {
        "id": "context-engine-cutoff",
        "pattern": re.compile(r"Highlighting a single line took too much time"),
        "owner": "document-scale-policy (C1)",
        "axiom": "A29",
    },
    {
        "id": "minimap-detach-critical",
        "pattern": re.compile(r"(gtk_source_map|GtkSourceMap|source_map)", re.I),
        "owner": "document-scale-policy (C1)",
        "axiom": "A32",
        # Any CRITICAL in a minimap-on case is this defect.
        "critical_in_case": "minimap-on",
    },
)

# Pre-existing blockers this programme has fixed. A match is a regression: it
# fails the lane instead of being reported as an owned finding.
FIXED_DEFECTS = (
    {
        # The trough of the editor's scrollbar, snapshotted without an
        # allocation: GtkTextView flushed the validation an in-frame edit or
        # caret move left pending from inside its snapshot (ledger A37). Fixed
        # by the editor page queueing its view's allocation on every edit and
        # caret move (ui/editor_page/frame_validation.rs).
        "id": "gizmo-snapshot-without-allocation",
        "pattern": re.compile(r"Trying to snapshot GtkGizmo .* without a current allocation"),
        "fixed_by": "editor page frame validation (probe-long-line-toolkit-costs)",
        "axiom": "A37",
    },
)


# --------------------------------------------------------------------------
# Pure helpers (self-tested)
# --------------------------------------------------------------------------


def classify(lateness_us: int, tick_us: int, slack_us: int, budget_us: int) -> str:
    """Within only when L + T <= B, Over only when L - eps > B (design D4/D5)."""
    if lateness_us + tick_us <= budget_us:
        return "within"
    if lateness_us - slack_us > budget_us:
        return "over"
    return "indeterminate"


def nearest_rank(values: list[int], percent: int) -> int | None:
    if not values:
        return None
    ordered = sorted(values)
    rank = max(1, math.ceil(percent * len(ordered) / 100))
    return ordered[min(rank, len(ordered)) - 1]


def read_budgets(source: Path = BUDGET_SOURCE) -> dict[str, int]:
    budgets = dict(DEFAULT_BUDGETS)
    budgets["source"] = "defaults"  # type: ignore[assignment]
    try:
        text = source.read_text(encoding="utf-8")
    except OSError:
        return budgets
    found = 0
    for name in DEFAULT_BUDGETS:
        match = re.search(rf"const\s+{name}\s*:\s*\w+\s*=\s*([0-9_]+)\s*;", text)
        if match:
            budgets[name] = int(match.group(1).replace("_", ""))
            found += 1
    if found:
        budgets["source"] = str(source.relative_to(REPO_ROOT))  # type: ignore[assignment]
    return budgets


def parse_probe_log(lines: Iterable[str]) -> dict[str, Any]:
    """Group probe records by kind; tolerate a truncated last line (SIGKILL)."""
    records: dict[str, list[dict]] = {kind: [] for kind in KNOWN_KINDS}
    unknown = 0
    malformed = 0
    for line in lines:
        line = line.strip()
        if not line:
            continue
        try:
            record = json.loads(line)
        except json.JSONDecodeError:
            malformed += 1
            continue
        kind = record.get("kind")
        if kind in records:
            records[kind].append(record)
        else:
            unknown += 1
    start = records["start"][0] if records["start"] else {}
    dropped = sum(int(r.get("count", 0)) for r in records["dropped"])
    return {
        "records": records,
        "tick_us": int(start.get("tick_us", 2000)),
        "late_threshold_us": int(start.get("late_threshold_us", 1000)),
        "started": bool(start),
        "unknown": unknown,
        "malformed": malformed,
        "dropped": dropped,
    }


def overlapping_lates(parsed: dict, start_us: int, end_us: int) -> Iterator[dict]:
    """The late records whose stall (ready time to dispatch) overlaps [start, end]."""
    for record in parsed["records"]["late"]:
        ready = int(record.get("ready_us", record["t_us"] - record["lateness_us"]))
        if record["t_us"] >= start_us and ready <= end_us:
            yield record


def phase_stall(parsed: dict, start_us: int, end_us: int) -> dict[str, Any]:
    """Worst main-loop stall overlapping [start, end] from late records."""
    worst = 0
    worst_at = None
    seen = 0
    for record in overlapping_lates(parsed, start_us, end_us):
        seen += 1
        if record["lateness_us"] > worst:
            worst = int(record["lateness_us"])
            worst_at = int(record["t_us"])
    below_threshold = seen == 0
    ticks = sum(int(r.get("ticks", 0)) for r in parsed["records"]["second"] if start_us <= r["t_us"] <= end_us + 1_000_000)
    return {
        "lateness_us": worst if seen else parsed["late_threshold_us"],
        "below_threshold": below_threshold,
        "late_records": seen,
        "worst_at_us": worst_at,
        "ticks": ticks,
    }


IDLE_GAP_US = 4 * 8_333


def frame_gap_p95(parsed: dict, start_us: int, end_us: int, reference_frame_us: int = 8_333) -> dict[str, Any]:
    """p95 gap between consecutive presented frames in [start, end].

    A frame-gap figure is about frames the app wanted and did not get, so
    two kinds of gap are left out and counted instead. A gap next to a frame
    the compositor never presented (headless Mutter reports no presentation
    for some frames, and GTK then paces on its 60 Hz fallback timer) says
    nothing about the app. A gap longer than four reference frames while the
    main loop was free is idle time: no frame was requested. A gap with a
    main-loop stall inside it always counts.
    """
    frames = sorted((r for r in parsed["records"]["frame"] if start_us <= int(r["frame_time_us"]) <= end_us), key=lambda r: int(r["frame_time_us"]))
    unpresented = sum(1 for r in frames if not r.get("presentation_us"))
    presented = [frame_presented(parsed, r, reference_frame_us)[0] if r.get("presentation_us") else None for r in frames]
    gaps: list[int] = []
    idle = 0
    for (a, at), (b, bt) in zip(zip(frames, presented), zip(frames[1:], presented[1:])):
        if at is None or bt is None:
            continue
        gap = bt - at
        if gap <= 0:
            continue
        if gap > IDLE_GAP_US and not loop_stalled(parsed, int(a["frame_time_us"]), int(b["frame_time_us"]), reference_frame_us):
            idle += 1
            continue
        gaps.append(gap)
    intervals = [int(r["refresh_interval_us"]) for r in parsed["records"]["frame"] if r.get("refresh_interval_us")]
    return {
        "frames": len(frames),
        "p95_gap_us": nearest_rank(gaps, 95),
        "max_gap_us": max(gaps) if gaps else None,
        "gaps": len(gaps),
        "source": "presentation",
        "unpresented_frames": unpresented,
        "idle_gaps": idle,
        "refresh_interval_us": statistics.median(intervals) if intervals else None,
    }


def loop_stalled(parsed: dict, start_us: int, end_us: int, threshold_us: int) -> bool:
    """Whether a main-loop stall of at least `threshold_us` overlapped [start, end]."""
    return any(int(record["lateness_us"]) >= threshold_us for record in overlapping_lates(parsed, start_us, end_us))


def frame_presented(parsed: dict, frame: dict, reference_frame_us: int) -> tuple[int, str]:
    """When a frame reached the screen, and how that time was obtained.

    Headless Mutter presents a lone frame after an idle period only when the
    next commit arrives, so its presentation time can trail the frame start
    by the time to the next input. A presentation more than two reference
    frames after the frame started while the main loop was free is that
    deferral, and the frame counts as presented one reference frame after it
    started (the earliest a 120 Hz compositor could show it). A presentation
    delayed by a main-loop stall (a long paint) keeps its real time. A frame
    with no presentation at all is likewise counted one reference frame
    after it started.
    """
    start = int(frame["frame_time_us"])
    presentation = frame.get("presentation_us")
    if not presentation:
        return start + reference_frame_us, "frame-start"
    presentation = int(presentation)
    if presentation - start > 2 * reference_frame_us and not loop_stalled(parsed, start, presentation, reference_frame_us):
        return start + reference_frame_us, "deferred-presentation"
    return presentation, "presentation"


def input_to_frame(parsed: dict, injections: list[dict], reference_frame_us: int, horizon_us: int | None = None) -> list[dict]:
    """For each measured injected event: origin -> first presentation of a frame started after its handler.

    Two attribution rules keep an unrelated frame from being charged to an
    input. An unmeasured injection (a modifier press) still consumes its
    input record, so it cannot stand in for the key it modifies. And when
    the first frame after an input's handler starts only after the NEXT
    input was handled, while the main loop was free in between, that input
    requested no frame (it changed nothing visible); it is reported as
    `no-visible-change`, never as the later input's latency. While the loop
    was stalled the late frame stays attributed, because the stall is what
    delayed it.
    """
    inputs = sorted(parsed["records"]["input"], key=lambda r: r["t_us"])
    frames = sorted(parsed["records"]["frame"], key=lambda r: int(r["frame_time_us"]))
    matched: list[tuple[dict, int | None]] = []
    used: set[int] = set()
    for injection in injections:
        origin = injection["inject_us"]
        wanted = injection["event"]
        handler = None
        for index, record in enumerate(inputs):
            if index in used or record["t_us"] < origin - 2_000:
                continue
            if record.get("event") == wanted:
                handler = int(record["t_us"])
                used.add(index)
                break
        matched.append((injection, handler))
    results = []
    for position, (injection, handler) in enumerate(matched):
        if not injection.get("measured", True):
            continue
        origin = injection["inject_us"]
        wanted = injection["event"]
        if handler is None:
            results.append({"event": wanted, "inject_us": origin, "status": "no-input-record"})
            continue
        frame = next((f for f in frames if int(f["frame_time_us"]) >= handler), None)
        if frame is not None and horizon_us is not None and int(frame["frame_time_us"]) > horizon_us:
            # The input changed nothing visible (for example Page Down at the
            # end of a short document); a later unrelated frame is not its frame.
            frame = None
        if frame is None:
            results.append({"event": wanted, "inject_us": origin, "handler_us": handler, "status": "no-frame"})
            continue
        next_handler = next((h for _inj, h in matched[position + 1 :] if h is not None and h > handler), None)
        if next_handler is not None and int(frame["frame_time_us"]) > next_handler and not loop_stalled(parsed, handler, next_handler, reference_frame_us):
            results.append({"event": wanted, "inject_us": origin, "handler_us": handler, "status": "no-visible-change"})
            continue
        presented, source = frame_presented(parsed, frame, reference_frame_us)
        latency = max(0, presented - origin)
        results.append(
            {
                "event": wanted,
                "inject_us": origin,
                "handler_us": handler,
                "presented_us": presented,
                "source": source,
                "latency_us": latency,
                "frames": math.ceil(latency / reference_frame_us) if latency else 0,
            }
        )
    return results


def im_summary(parsed: dict, start_us: int, end_us: int) -> dict[str, Any]:
    records = [r for r in parsed["records"]["im"] if start_us <= r["t_us"] <= end_us]
    sizes = [int(r["surrounding_bytes"]) for r in records if r.get("surrounding_bytes") is not None]
    triggers = Counter(record.get("trigger", "?") for record in records)
    return {"calls": len(records), "max_surrounding_bytes": max(sizes) if sizes else None, "triggers": dict(triggers)}


def at_summary(events: list[dict], start_us: int, end_us: int) -> dict[str, Any]:
    inside = [e for e in events if start_us <= e["t_us"] <= end_us]
    lengths = [int(e.get("payload_len", 0)) for e in inside]
    offsets = [int(e.get("detail1", 0)) for e in inside]
    return {
        "events": len(inside),
        "by_type": dict(Counter(event["type"] for event in inside)),
        "max_payload_len": max(lengths) if lengths else None,
        "max_offset": max(offsets) if offsets else None,
    }


def is_noisy(values: list[int], floor_us: int = 2_000) -> bool:
    if len(values) < 2:
        return False
    median = statistics.median(values)
    return max(values) > 2 * median and max(values) >= floor_us


def storage_status(report: dict | None) -> str:
    if report is None:
        return "unavailable"
    verdicts = " ".join(report.get("verdict", []))
    if "NO DATA" in verdicts:
        return "unavailable"
    if "INCOMPLETE" in verdicts:
        return "incomplete"
    if "NOT a bottleneck" in verdicts:
        return "clean"
    return "contaminated"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


class IntegrityLedger:
    """Digest of the last bytes the driver itself wrote to every fixture."""

    def __init__(self) -> None:
        self.expected: dict[str, str] = {}
        self.violations: list[dict] = []
        self.checks = 0

    def record(self, path: Path, digest: str | None = None) -> None:
        self.expected[str(path)] = digest or sha256_file(path)

    def verify(self, paths: Iterable[Path], context: str) -> bool:
        ok = True
        for path in paths:
            self.checks += 1
            key = str(path)
            actual = sha256_file(path) if path.exists() else "missing"
            if self.expected.get(key) != actual:
                ok = False
                self.violations.append({"path": key, "expected": self.expected.get(key), "actual": actual, "context": context})
        return ok

    def forget(self, paths: Iterable[Path]) -> None:
        for path in paths:
            self.expected.pop(str(path), None)


# --------------------------------------------------------------------------
# Case model
# --------------------------------------------------------------------------


@dataclass
class FixtureSpec:
    kind: str
    size: int
    seed: int
    ext: str = "txt"
    delimiter: str = "lf"
    charset: str = "ascii"
    count: int | None = None
    flavor: str = "css"


@dataclass
class Case:
    id: str
    group: str
    fixture: FixtureSpec | None
    settings: dict[str, Any] = field(default_factory=dict)
    flow: str = "standard"  # standard | session | eviction | reload | install | calibration
    length: int | None = None  # the swept paragraph length / line length / total size
    attrs: dict[str, Any] = field(default_factory=dict)


def seed_for(*parts: Any) -> int:
    return int(hashlib.sha256("/".join(map(str, parts)).encode()).hexdigest()[:8], 16)


def paragraph_case(length: int, ext: str, wrap: bool, minimap: bool, group: str = "paragraph", invisibles: str = "off", flavor: str = "css") -> Case:
    case_id = f"{group}/{length}/{flavor}/{ext}/wrap-{'on' if wrap else 'off'}/minimap-{'on' if minimap else 'off'}"
    if group == "invisibles":
        case_id = f"invisibles/{length}/{invisibles}"
    return Case(
        id=case_id,
        group=group,
        fixture=FixtureSpec("paragraph", length, seed_for("paragraph", length, flavor), ext=ext, flavor=flavor),
        settings={"word-wrap": wrap, "show-minimap": minimap, "invisible-characters-mode": invisibles},
        length=length,
        attrs={"ext": ext, "wrap": wrap, "minimap": minimap, "invisibles": invisibles, "flavor": flavor},
    )


def sweep_lengths(tier: str, overrides: str | None = None) -> dict[str, tuple[int, ...]]:
    """The tier's length grid, or the probe points a reduced sweep chose.

    `RESPONSIVENESS_LENGTHS` (`paragraph=2000,4500;invisibles=10000`)
    replaces a group's grid, so the reference sweep can bisect over length
    instead of running the whole grid (docs/next/long-line-documents.md).
    """
    lengths = {
        "paragraph": PARAGRAPH_LENGTHS[tier],
        "dense": DENSE_LINE_LENGTHS[tier],
        "total-size": TOTAL_SIZES[tier],
        "invisibles": INVISIBLES_LENGTHS[tier],
    }
    raw = os.environ.get("RESPONSIVENESS_LENGTHS", "") if overrides is None else overrides
    for item in filter(None, (part.strip() for part in raw.split(";"))):
        group, _, values = item.partition("=")
        if group.strip() not in lengths:
            raise SystemExit(f"RESPONSIVENESS_LENGTHS: unknown group {group!r}")
        lengths[group.strip()] = tuple(int(v) for v in values.split(",") if v.strip())
    return lengths


def build_cases(tier: str) -> list[Case]:
    grid = sweep_lengths(tier)
    cases: list[Case] = [
        Case("baseline/calibration", "baseline", None, flow="calibration"),
        Case("baseline/empty", "baseline", None, settings={"word-wrap": True}),
        Case(
            "baseline/small",
            "baseline",
            FixtureSpec("paragraph", 400, seed_for("small")),
            settings={"word-wrap": True},
            length=400,
        ),
    ]
    for length in grid["paragraph"]:
        for ext in ("html", "txt"):
            for wrap in (True, False):
                for minimap in (False, True):
                    cases.append(paragraph_case(length, ext, wrap, minimap))
                # The script flavour is the costliest for the context engine
                # (generate-long-line-fixtures.py script_chunk); L_syntax takes
                # the worst flavour, so it runs the syntax series too.
                cases.append(paragraph_case(length, ext, wrap, False, flavor="script"))
    for line_length in grid["dense"]:
        for wrap in (False, True):
            cases.append(
                Case(
                    f"dense/{line_length}/wrap-{'on' if wrap else 'off'}",
                    "dense",
                    FixtureSpec("dense", line_length, seed_for("dense", line_length), count=DENSE_LINE_COUNT),
                    settings={"word-wrap": wrap, "show-minimap": False},
                    length=line_length,
                    attrs={"wrap": wrap, "lines": DENSE_LINE_COUNT},
                )
            )
    for total in grid["total-size"]:
        # The same bytes as .html and as .txt: S_syntax is the difference.
        for ext in ("html", "txt"):
            cases.append(
                Case(
                    f"total-size/{total}" + ("/txt" if ext == "txt" else ""),
                    "total-size",
                    FixtureSpec("short-html", total, seed_for("short-html", total), ext=ext),
                    settings={"word-wrap": True, "show-minimap": False},
                    length=total,
                    attrs={"ext": ext},
                )
            )
    for delimiter in ("lf", "cr", "crlf", "u2029"):
        cases.append(
            Case(
                f"delimiters/{delimiter}",
                "delimiters",
                FixtureSpec("minified-html", DELIMITER_LONGEST[tier], seed_for("minified"), ext="html", delimiter=delimiter),
                settings={"word-wrap": True, "show-minimap": False},
                length=DELIMITER_LONGEST[tier],
                attrs={"delimiter": delimiter},
            )
        )
    cases.extend(
        [
            Case(
                "session/restore-3",
                "session",
                FixtureSpec("session", SESSION_LENGTH[tier], seed_for("session"), count=3),
                settings={"word-wrap": True, "show-minimap": False},
                flow="session",
                length=SESSION_LENGTH[tier],
            ),
            Case(
                "eviction/reselect",
                "eviction",
                FixtureSpec("eviction", EVICTION_FILE_BYTES, seed_for("eviction")),
                settings={"word-wrap": True, "show-minimap": False},
                flow="eviction",
            ),
            Case(
                "reload/250000",
                "reload",
                FixtureSpec("paragraph", 250_000, seed_for("reload")),
                settings={"word-wrap": True, "show-minimap": False},
                flow="reload",
                length=250_000,
            ),
        ]
    )
    if tier == "full":
        for size in INSTALL_SIZES:
            cases.append(
                Case(
                    f"install/{size}",
                    "install",
                    FixtureSpec("paragraph", size, seed_for("install", size)),
                    settings={"word-wrap": True, "show-minimap": False},
                    flow="install",
                    length=size,
                )
            )
    for length in grid["invisibles"]:
        for mode in ("off", "all"):
            cases.append(paragraph_case(length, "txt", True, False, group="invisibles", invisibles=mode))
    return cases


def select_cases(cases: list[Case], patterns: list[str]) -> list[Case]:
    if not patterns:
        return cases
    return [case for case in cases if any(fnmatch.fnmatchcase(case.id, p) or case.id.startswith(p.rstrip("/") + "/") for p in patterns)]


# --------------------------------------------------------------------------
# Runtime (inside the private compositor)
# --------------------------------------------------------------------------


def selected_variants() -> list[str]:
    """The variants RESPONSIVENESS_VARIANTS selects, in VARIANTS order (all when unset)."""
    wanted = {v.strip() for v in os.environ.get("RESPONSIVENESS_VARIANTS", ",".join(VARIANTS)).split(",") if v.strip()}
    return [v for v in VARIANTS if v in wanted]


def requested_renderer() -> str:
    return os.environ.get("RESPONSIVENESS_RENDERER", "cairo")


def now_us() -> int:
    return time.monotonic_ns() // 1000


def load_generator():
    spec = importlib.util.spec_from_file_location("long_line_fixtures", GENERATOR_PATH)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class Bus:
    def __init__(self) -> None:
        from gi.repository import Gio

        self.connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)

    def call(self, dest: str, path: str, iface: str, method: str, params=None, reply: str | None = None, timeout_ms: int = 10_000):
        from gi.repository import Gio, GLib

        return self.connection.call_sync(
            dest, path, iface, method, params, GLib.VariantType.new(reply) if reply else None, Gio.DBusCallFlags.NONE, timeout_ms, None
        )


class RemoteDesktop:
    """Mutter RemoteDesktop input, warmed up (the first notification is dropped)."""

    def __init__(self, bus: Bus) -> None:
        self.bus = bus
        self.session: str | None = None
        self.error: str | None = None
        try:
            (self.session,) = bus.call(RD_BUS, RD_PATH, RD_BUS, "CreateSession", None, "(o)").unpack()
            self._call("Start", None)
            time.sleep(1.0)
            self.warm_up()
        except Exception as exc:  # noqa: BLE001 - reported as unavailable
            self.error = f"{type(exc).__name__}: {exc}"
            self.session = None

    @property
    def available(self) -> bool:
        return self.session is not None

    def _call(self, method: str, params) -> None:
        self.bus.call(RD_BUS, self.session, RD_SESSION_IFACE, method, params)

    def warm_up(self) -> None:
        from gi.repository import GLib

        self._call("NotifyKeyboardKeysym", GLib.Variant("(ub)", (KEY_SHIFT_L, True)))
        self._call("NotifyKeyboardKeysym", GLib.Variant("(ub)", (KEY_SHIFT_L, False)))
        self._call("NotifyPointerMotionRelative", GLib.Variant("(dd)", (1.0, 1.0)))
        time.sleep(0.5)

    def move_to(self, x: int, y: int) -> None:
        from gi.repository import GLib

        self._call("NotifyPointerMotionRelative", GLib.Variant("(dd)", (-10_000.0, -10_000.0)))
        self._call("NotifyPointerMotionRelative", GLib.Variant("(dd)", (float(x), float(y))))

    def key(self, keysym: int, pressed: bool) -> int:
        if keysym not in ALLOWED_KEYSYMS:
            raise RuntimeError(f"refusing to inject keysym {keysym:#x}")
        stamp = now_us()
        self._notify_key(keysym, pressed)
        return stamp

    def _notify_key(self, keysym: int, pressed: bool) -> None:
        from gi.repository import GLib

        self._call("NotifyKeyboardKeysym", GLib.Variant("(ub)", (keysym, pressed)))

    def tap(self, keysym: int, injections: list[dict], control: bool = False) -> None:
        if control and 0x61 <= keysym <= 0x7A:
            raise RuntimeError("refusing Control+letter: no shortcut chord may be injected")
        if control:
            # The modifier press reaches the window as its own key-press
            # record; recording it (unmeasured) keeps it from standing in
            # for the key it modifies when inputs are matched to records.
            injections.append({"event": "key-press", "inject_us": self.key(KEY_CONTROL_L, True), "measured": False})
        injections.append({"event": "key-press", "inject_us": self.key(keysym, True)})
        self.key(keysym, False)
        if control:
            self.key(KEY_CONTROL_L, False)

    def wheel(self, steps: int, injections: list[dict]) -> None:
        from gi.repository import GLib

        stamp = now_us()
        self._call("NotifyPointerAxisDiscrete", GLib.Variant("(ui)", (0, steps)))
        injections.append({"event": "scroll", "inject_us": stamp})

    def click(self, x: int, y: int, injections: list[dict]) -> None:
        from gi.repository import GLib

        self.move_to(x, y)
        time.sleep(0.05)
        stamp = now_us()
        self._call("NotifyPointerButton", GLib.Variant("(ib)", (BTN_LEFT, True)))
        self._call("NotifyPointerButton", GLib.Variant("(ib)", (BTN_LEFT, False)))
        injections.append({"event": "button-press", "inject_us": stamp})

    def close(self) -> None:
        if self.session:
            try:
                self._call("Stop", None)
            except Exception:  # noqa: BLE001
                pass


class AtspiListener:
    """A pyatspi listener process recording text events (lengths and offsets only)."""

    def __init__(self, log_path: Path) -> None:
        self.log_path = log_path
        self.process = subprocess.Popen(
            [str(SYSTEM_PYTHON), str(SCRIPT_PATH), "atspi-listener", "--output", str(log_path)],
            stdout=subprocess.DEVNULL,
            stderr=(log_path.parent / "atspi-listener.stderr").open("wb"),
        )
        time.sleep(0.5)

    def stop(self) -> list[dict]:
        if self.process.poll() is None:
            self.process.send_signal(signal.SIGTERM)
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
        events = []
        if self.log_path.exists():
            for line in self.log_path.read_text(encoding="utf-8", errors="replace").splitlines():
                try:
                    events.append(json.loads(line))
                except json.JSONDecodeError:
                    pass
        return events


def atspi_listener_main(output: Path) -> int:
    import pyatspi  # type: ignore[import-not-found]

    handle = output.open("w", encoding="utf-8")

    def on_event(event) -> None:
        payload = event.any_data
        length = len(payload) if isinstance(payload, str) else 0
        handle.write(
            json.dumps(
                {"t_us": now_us(), "type": str(event.type), "detail1": int(event.detail1), "detail2": int(event.detail2), "payload_len": length}
            )
            + "\n"
        )
        handle.flush()

    def stop(*_args) -> None:
        handle.flush()
        pyatspi.Registry.stop()

    signal.signal(signal.SIGTERM, stop)
    for name in ("object:text-changed", "object:text-caret-moved", "object:text-selection-changed"):
        pyatspi.Registry.registerEventListener(on_event, name)
    pyatspi.Registry.start()
    handle.close()
    return 0


@dataclass
class Launch:
    process: subprocess.Popen
    spawn_us: int
    stderr_path: Path


class Runner:
    def __init__(self, args: argparse.Namespace) -> None:
        self.artifact_dir: Path = args.artifact_dir
        self.binary: Path = args.binary
        self.tier: str = args.tier
        self.cap_s = float(os.environ.get("RESPONSIVENESS_CASE_CAP_S", "20"))
        # A reduced sweep may measure only the interactions its thresholds
        # read (open and background always run); unset means all of them.
        only = os.environ.get("RESPONSIVENESS_PHASES", "").strip()
        self.only_phases: set[str] | None = {p.strip() for p in only.split(",") if p.strip()} or None
        self.install_cap_s = float(os.environ.get("RESPONSIVENESS_INSTALL_CAP_S", "180"))
        # GTK 4.22 renamed `ngl` to `gl` and warns on the old name.
        self.renderer = {"ngl": "gl"}.get(requested_renderer(), requested_renderer())
        self.repetitions = int(os.environ.get("RESPONSIVENESS_REPETITIONS", str(REPETITIONS[self.tier])))
        self.bisect = os.environ.get("RESPONSIVENESS_BISECT", "1" if self.tier == "full" else "0") == "1"
        self.variants = selected_variants()
        self.scope = Path(os.environ.get("TMPDIR", tempfile.gettempdir()))
        self.fixture_root = self.scope / "fixtures"
        self.generator = load_generator()
        self.ledger = IntegrityLedger()
        self.bus = Bus()
        self.rd = RemoteDesktop(self.bus)
        self.results_path = self.artifact_dir / "results.jsonl"
        self.fixture_manifests: list[dict] = []
        self.event_clock_ok = True
        self.slack_us: int | None = None
        self.failures: list[str] = []
        self.ibus_available = shutil.which("ibus-daemon") is not None

    # -- fixtures -----------------------------------------------------------

    def make_fixture(self, spec: FixtureSpec, label: str) -> list[Path]:
        out_dir = self.fixture_root / re.sub(r"[^A-Za-z0-9_.-]", "_", label)
        manifest = self.generator.generate(
            spec.kind, spec.size, spec.seed, self.scope, out_dir, ext=spec.ext, delimiter=spec.delimiter, charset=spec.charset, count=spec.count, flavor=spec.flavor
        )
        paths = []
        for entry in manifest["files"]:
            path = Path(entry["path"])
            self.ledger.record(path, entry["sha256"])
            paths.append(path)
        recorded = {k: v for k, v in manifest.items() if k != "files"}
        recorded["case"] = label
        recorded["files"] = [{k: v for k, v in entry.items() if k != "path"} for entry in manifest["files"]]
        self.fixture_manifests.append(recorded)
        return paths

    def drop_fixtures(self, paths: list[Path]) -> None:
        self.ledger.forget(paths)
        for path in paths:
            try:
                path.unlink()
            except FileNotFoundError:
                pass
        for parent in {path.parent for path in paths}:
            shutil.rmtree(parent, ignore_errors=True)

    # -- app lifecycle ------------------------------------------------------

    def fresh_state(self, case_dir: Path, settings: dict[str, Any]) -> dict[str, str]:
        state = Path(tempfile.mkdtemp(prefix="state-", dir=self.scope))
        for name in ("cache", "config", "data"):
            (state / name).mkdir()
        keyfile = state / "config/glib-2.0/settings/keyfile"
        keyfile.parent.mkdir(parents=True)
        values = {
            "window-maximized": True,
            "workspace-sidebar-visible": False,
            "properties-sidebar-visible": False,
            "search-panel-visible": False,
            **settings,
        }
        lines = [f"[{SCHEMA_GROUP}]"]
        for key, value in values.items():
            if isinstance(value, bool):
                lines.append(f"{key}={'true' if value else 'false'}")
            elif isinstance(value, int):
                lines.append(f"{key}=uint32 {value}")
            else:
                lines.append(f"{key}='{value}'")
        keyfile.write_text("\n".join(lines) + "\n", encoding="utf-8")
        # No caret blink. Under headless Mutter the blink's frames get no
        # presentation feedback, so GTK paces them on its 60 Hz fallback
        # timer and every input waits up to 16.7 ms for the next tick:
        # measured in rr exp-blink0/1 (Page Down input-to-frame median
        # 18.6 ms with blink, 10.8 ms without). That is the lane's
        # compositor, not the app, so the lane measures without it.
        gtk_settings = state / "config/gtk-4.0/settings.ini"
        gtk_settings.parent.mkdir(parents=True)
        gtk_settings.write_text("[Settings]\ngtk-cursor-blink=false\n", encoding="utf-8")
        (case_dir / "settings.json").write_text(json.dumps(values, indent=2) + "\n", encoding="utf-8")
        return {
            "XDG_CACHE_HOME": str(state / "cache"),
            "XDG_CONFIG_HOME": str(state / "config"),
            "XDG_DATA_HOME": str(state / "data"),
            "LUSHTEXT_DATA_DIR": str(state / "data/lushtext"),
            "STATE_ROOT": str(state),
        }

    def app_env(self, variant: str, state_env: dict[str, str], log_path: Path, tick_us: int | None = None) -> dict[str, str]:
        env = os.environ.copy()
        for key in ("NO_AT_BRIDGE", "GTK_A11Y", "LUSHTEXT_PROBE_IM", "GTK_IM_MODULE", "LUSHTEXT_RESPONSIVENESS_TICK_US"):
            env.pop(key, None)
        env.update({k: v for k, v in state_env.items() if k != "STATE_ROOT"})
        env.update(
            {
                "GDK_BACKEND": "wayland",
                "GSETTINGS_BACKEND": "keyfile",
                "GSETTINGS_SCHEMA_DIR": str(REPO_ROOT / "data"),
                "GSK_RENDERER": self.renderer,
                # Prints "Using renderer '...'": the renderer GTK actually got.
                "GSK_DEBUG": "renderer",
                "GTK_USE_PORTAL": "0",
                "ADW_DISABLE_PORTAL": "1",
                "GDK_DEBUG": "no-portals",
                "RUST_LOG": env.get("RUST_LOG", "warn"),
                "LUSHTEXT_RESPONSIVENESS_LOG": str(log_path),
            }
        )
        if tick_us:
            env["LUSHTEXT_RESPONSIVENESS_TICK_US"] = str(tick_us)
        # Every variant selects its IM explicitly: never GTK's Wayland
        # text-input default under headless Mutter (S7B-6 crash race).
        env["GTK_IM_MODULE"] = {"probe-im": "lushtext-probe", "ibus": "ibus"}.get(variant, "gtk-im-context-simple")
        if variant == "probe-im":
            env["LUSHTEXT_PROBE_IM"] = "1"
        if variant != "at":
            env["NO_AT_BRIDGE"] = "1"
            env["GTK_A11Y"] = "none"
        return env

    def launch(self, rep_dir: Path, env: dict[str, str], files: list[Path], tag: str) -> Launch:
        stderr_path = rep_dir / f"app-{tag}.stderr"
        spawn = now_us()
        process = subprocess.Popen(
            [str(self.binary), *map(str, files)],
            stdout=(rep_dir / f"app-{tag}.stdout").open("wb"),
            stderr=stderr_path.open("wb"),
            env=env,
            start_new_session=True,
        )
        return Launch(process, spawn, stderr_path)

    def kill(self, launch: Launch) -> None:
        if launch.process.poll() is None:
            launch.process.send_signal(signal.SIGKILL)
            launch.process.wait(timeout=10)

    def wait_bus(self, launch: Launch, deadline: float) -> bool:
        while time.monotonic() < deadline:
            if launch.process.poll() is not None:
                return False
            try:
                self.bus.call(APP_ID, AUTOMATION_OBJECT_PATH, "org.freedesktop.DBus.Introspectable", "Introspect", None, "(s)", 2_000)
                return True
            except Exception:  # noqa: BLE001
                time.sleep(0.05)
        return False

    def wait_ready(self, predicate: str, deadline: float) -> tuple[bool, str]:
        """A main-loop round trip: returns only once the app's main loop serviced it."""
        from gi.repository import GLib

        remaining_ms = max(1, int((deadline - time.monotonic()) * 1000))
        try:
            ok, status, _detail = self.bus.call(
                APP_ID,
                AUTOMATION_OBJECT_PATH,
                AUTOMATION_INTERFACE,
                "WaitForReady",
                GLib.Variant("(su)", (predicate, remaining_ms)),
                "(bss)",
                remaining_ms + 2_000,
            ).unpack()
            return bool(ok), status
        except Exception as exc:  # noqa: BLE001 - a timeout is a cap hit
            return False, f"dbus: {type(exc).__name__}"

    def action(self, scope: str, name: str, params: list | None, deadline: float) -> bool:
        from gi.repository import GLib

        path = APP_OBJECT_PATH if scope == "app" else WINDOW_OBJECT_PATH
        try:
            self.bus.call(
                APP_ID,
                path,
                "org.gtk.Actions",
                "Activate",
                GLib.Variant("(sava{sv})", (name, params or [], {})),
                None,
                max(1, int((deadline - time.monotonic()) * 1000)),
            )
            return True
        except Exception:  # noqa: BLE001
            return False

    # -- phases -------------------------------------------------------------

    def run_phase(self, phases: list[dict], name: str, cap_s: float, body: Callable[[list[dict], float], tuple[bool, str]]) -> bool:
        injections: list[dict] = []
        start = now_us()
        wall = time.time()
        deadline = time.monotonic() + cap_s
        try:
            ok, status = body(injections, deadline)
        except RuntimeError as exc:
            ok, status = False, f"error: {exc}"
        if ok:
            # Let follow-up idle work and frames run, then prove the loop is free.
            time.sleep(0.3)
            ok, status = self.wait_ready("app-startup", deadline)
        end = now_us()
        exceeded = not ok and time.monotonic() >= deadline - 0.05
        if ok:
            phase_status = "ok"
        elif exceeded:
            phase_status = "exceeded_cap"
        else:
            phase_status = status
        phases.append(
            {
                "name": name,
                "kind": PHASE_KINDS.get(name, "interaction"),
                "start_us": start,
                "end_us": end,
                "wall_start": wall,
                "wall_end": time.time(),
                "status": phase_status,
                "injections": injections,
                "input_available": self.rd.available,
            }
        )
        return ok

    def step(self, phases: list[dict], name: str, cap: float, body, recover: Callable[[], bool] | None) -> bool:
        """Run one phase; after a cap hit, relaunch the app and carry on.

        A hang is a result, not the end of the case: the remaining
        interactions still run against a fresh launch, so a stall in one
        phase does not hide the cost of the phases after it.
        """
        if self.run_phase(phases, name, cap, body):
            return True
        if phases[-1].get("status") == "exceeded_cap" and recover is not None:
            return recover()
        return False

    def settle_after(self, seconds: float) -> Callable[[Any, float], tuple[bool, str]]:
        """A background phase body: wait `seconds`, then for app startup readiness."""

        def body(_inj, deadline):
            time.sleep(seconds)
            return self.wait_ready("app-startup", deadline)

        return body

    def wanted(self, name: str) -> bool:
        """Whether this run measures the interaction (RESPONSIVENESS_PHASES).

        The click also gives the window keyboard focus, so it runs whenever a
        keyboard interaction does.
        """
        if self.only_phases is None:
            return True
        if name == "click":
            return bool(self.only_phases & {"click", "pagedown", "type-start", "type-middle", "type-end"})
        return name in self.only_phases

    def standard_interactions(self, phases: list[dict], tabs: int = 1, recover: Callable[[], bool] | None = None) -> bool:
        cap = self.cap_s

        if not self.step(phases, "background", cap, self.settle_after(2.0), recover):
            return False
        if not self.rd.available:
            for name in ("scroll", "click", "pagedown", "type-start", "type-middle", "type-end"):
                phases.append({"name": name, "kind": PHASE_KINDS[name], "status": "unavailable", "reason": self.rd.error or "RemoteDesktop unavailable"})
        else:

            def scroll(inj, deadline):
                self.rd.move_to(SCREEN_W // 2, SCREEN_H // 2)
                end = time.monotonic() + 2.0
                while time.monotonic() < end and time.monotonic() < deadline:
                    self.rd.wheel(1, inj)
                    time.sleep(0.05)
                return True, "ok"

            def click(inj, _deadline):
                # Inside the text area, away from the header bar: a header
                # click starts a compositor grab and the window never gets
                # keyboard focus (observed headless).
                self.rd.click(SCREEN_W // 3, SCREEN_H // 2, inj)
                return True, "ok"

            def pagedown(inj, _deadline):
                for _ in range(10):
                    self.rd.tap(KEY_PAGE_DOWN, inj)
                    time.sleep(0.15)
                return True, "ok"

            def type_at(where: str):
                def body(inj, _deadline):
                    if where == "start":
                        self.rd.tap(KEY_HOME, inj, control=True)
                    elif where == "middle":
                        self.rd.click(SCREEN_W // 2, SCREEN_H // 2, inj)
                    else:
                        self.rd.tap(KEY_END, inj, control=True)
                    time.sleep(0.1)
                    for _ in range(5):
                        self.rd.tap(KEY_RIGHT, inj)
                        time.sleep(0.05)
                    for char in TYPE_BURST:
                        self.rd.tap(ord(char), inj)
                        time.sleep(0.05)
                    return True, "ok"

                return body

            # The click comes first: it also gives the new window keyboard focus.
            for name, body in (
                ("click", click),
                ("scroll", scroll),
                ("pagedown", pagedown),
                ("type-start", type_at("start")),
                ("type-middle", type_at("middle")),
                ("type-end", type_at("end")),
            ):
                if not self.wanted(name):
                    continue
                if not self.step(phases, name, cap, body, recover):
                    return False

        def find(_inj, deadline):
            if not self.action("win", "set-search-query", [self._string(self.generator.FIND_MARKER)], deadline):
                return False, "action-failed"
            ok, status = self.wait_ready("search-complete", deadline)
            if not ok:
                return ok, status
            for _ in range(2):
                self.action("win", "next-match", None, deadline)
            return True, "ok"

        if self.wanted("find") and not self.step(phases, "find", cap, find, recover):
            return False
        if tabs > 1:

            def tab_switch(_inj, deadline):
                for index in list(range(tabs)) + [0]:
                    self.action("win", "select-tab", [self._uint32(index)], deadline)
                    ok, status = self.wait_ready("file-open-complete", deadline)
                    if not ok:
                        return ok, status
                return True, "ok"

            if not self.step(phases, "tab-switch", cap, tab_switch, recover):
                return False
        return True

    @staticmethod
    def _string(value: str):
        from gi.repository import GLib

        return GLib.Variant("s", value)

    @staticmethod
    def _uint32(value: int):
        from gi.repository import GLib

        return GLib.Variant("u", value)

    def open_phase(self, phases: list[dict], launch: Launch, cap_s: float, predicate: str = "file-open-complete") -> bool:
        start = launch.spawn_us
        wall = time.time()
        deadline = time.monotonic() + cap_s
        ok = self.wait_bus(launch, deadline)
        if ok:
            status = "ok"
        elif launch.process.poll() is not None:
            status = "crashed"
        else:
            status = "exceeded_cap"
        if ok:
            ok, status = self.wait_ready(predicate, deadline)
            if not ok and time.monotonic() >= deadline - 0.05:
                status = "exceeded_cap"
        phases.append(
            {"name": "open" if predicate != "session-restore-complete" else "restore", "kind": "settle", "start_us": start, "end_us": now_us(), "wall_start": wall, "wall_end": time.time(), "status": "ok" if ok else status, "injections": []}
        )
        return ok

    # -- one repetition -----------------------------------------------------

    def run_rep(self, case: Case, variant: str, rep: int, files: list[Path]) -> dict[str, Any]:
        rep_dir = self.artifact_dir / "cases" / case.id / variant / f"rep{rep}"
        rep_dir.mkdir(parents=True, exist_ok=True)
        state_env = self.fresh_state(rep_dir, case.settings)
        log_path = rep_dir / "probe.jsonl"
        env = self.app_env(variant, state_env, log_path)
        phases: list[dict] = []
        launches: list[Launch] = []
        listener = AtspiListener(rep_dir / "atspi.jsonl") if variant == "at" else None
        outcome = "ok"
        crashed = False
        wall_start = time.time()
        extra_states: list[str] = []
        recoveries = [0]

        def recover() -> bool:
            for live in launches:
                if live.process.poll() is None:
                    self.kill(live)
            recoveries[0] += 1
            if recoveries[0] > 6:
                return False
            fresh = self.fresh_state(rep_dir, case.settings)
            extra_states.append(fresh["STATE_ROOT"])
            renv = self.app_env(variant, fresh, rep_dir / f"probe-r{recoveries[0]}.jsonl")
            relaunched = self.launch(rep_dir, renv, files, f"r{recoveries[0]}")
            launches.append(relaunched)
            scratch: list[dict] = []
            ok = self.open_phase(scratch, relaunched, cap)
            phases.append({"name": "recovery", "kind": "recovery", "status": scratch[-1]["status"] if scratch else "failed", "after": phases[-1]["name"] if phases else None})
            if ok:
                time.sleep(1.0)
                if self.rd.available:
                    # Give the fresh window keyboard focus (unmeasured).
                    self.rd.click(SCREEN_W // 3, SCREEN_H // 2, [])
                    time.sleep(0.3)
                    self.wait_ready("app-startup", time.monotonic() + cap)
            return ok

        try:
            if not self.ledger.verify(files, f"{case.id}@{variant}#{rep} before launch"):
                self.failures.append(f"fixture digest changed before {case.id}")
            cap = self.install_cap_s if case.flow == "install" else self.cap_s
            if case.flow == "session":
                launch = self.launch(rep_dir, env, files, "first")
                launches.append(launch)
                ok = self.open_phase(phases, launch, cap) and self.run_phase(phases, "background", cap, self.settle_after(1.0))
                if ok:
                    # Graceful quit happens before any typing, so no save prompt can appear.
                    self.action("app", "quit", None, time.monotonic() + cap)
                    try:
                        launch.process.wait(timeout=cap)
                    except subprocess.TimeoutExpired:
                        self.kill(launch)
                        ok = False
                if ok:
                    env2 = dict(env)
                    env2["LUSHTEXT_RESPONSIVENESS_LOG"] = str(rep_dir / "probe-restore.jsonl")
                    relaunch = self.launch(rep_dir, env2, [], "restore")
                    launches.append(relaunch)
                    ok = self.open_phase(phases, relaunch, cap, predicate="session-restore-complete") and self.standard_interactions(phases, tabs=len(files), recover=recover)
            elif case.flow == "calibration":
                ok = self.run_calibration(rep_dir, state_env, phases, launches)
            else:
                launch = self.launch(rep_dir, env, files, "main")
                launches.append(launch)
                ok = self.open_phase(phases, launch, cap)
                if ok and case.flow == "install":
                    ok = self.run_phase(phases, "background", cap, self.settle_after(2.0))
                    if ok and self.rd.available:
                        self.run_phase(phases, "scroll", cap, lambda inj, d: (self.rd.move_to(SCREEN_W // 2, SCREEN_H // 2), [self.rd.wheel(1, inj) or time.sleep(0.05) for _ in range(40)], (True, "ok"))[2])
                elif ok and case.flow == "eviction":
                    ok = self.run_phase(phases, "background", cap, self.settle_after(2.0))

                    def reselect(_inj, deadline):
                        self.action("win", "select-tab", [self._uint32(0)], deadline)
                        return self.wait_ready("file-open-complete", deadline)

                    ok = ok and self.run_phase(phases, "reselect", cap, reselect)
                elif ok and case.flow == "reload":
                    ok = self.run_phase(phases, "background", cap, self.settle_after(1.0))

                    def reload(_inj, deadline):
                        self.rewrite_fixture(case, files[0])
                        time.sleep(1.5)
                        return self.wait_ready("file-open-complete", deadline)

                    ok = ok and self.run_phase(phases, "reload", cap, reload)
                    ok = ok and self.standard_interactions(phases, recover=recover)
                elif ok:
                    ok = self.standard_interactions(phases, tabs=max(1, len(files)), recover=recover)
            for launch in launches:
                if launch.process.poll() is not None and launch.process.returncode not in (0, -signal.SIGKILL):
                    crashed = True
            if any(p.get("status") == "exceeded_cap" for p in phases):
                outcome = "exceeded_cap"
            elif not ok:
                outcome = "failed"
        finally:
            for launch in launches:
                if launch.process.poll() is None:
                    # Typed edits are never saved: the app is always killed.
                    self.kill(launch)
            at_events = listener.stop() if listener else []
            shutil.rmtree(state_env["STATE_ROOT"], ignore_errors=True)
            for root in extra_states:
                shutil.rmtree(root, ignore_errors=True)
        if crashed and outcome != "exceeded_cap":
            outcome = "crashed"
            self.failures.append(f"app crashed in {case.id}@{variant}#{rep}")
        if not self.ledger.verify(files, f"{case.id}@{variant}#{rep} after case"):
            self.failures.append(f"fixture digest changed after {case.id}@{variant}#{rep}")
            outcome = "integrity-failure"
        row = self.analyse(case, variant, rep, rep_dir, phases, launches, at_events, outcome)
        row["wall_start"] = wall_start
        row["wall_end"] = time.time()
        (rep_dir / "phases.json").write_text(json.dumps(row, indent=2) + "\n", encoding="utf-8")
        self.append_result(row)
        return row

    def rewrite_fixture(self, case: Case, path: Path) -> None:
        spec = case.fixture
        assert spec is not None
        tmp_dir = Path(tempfile.mkdtemp(prefix="rewrite-", dir=self.scope))
        manifest = self.generator.generate(spec.kind, spec.size, spec.seed + 1, self.scope, tmp_dir, ext=spec.ext)
        data = Path(manifest["files"][0]["path"]).read_bytes()
        shutil.rmtree(tmp_dir, ignore_errors=True)
        with path.open("wb") as handle:
            handle.write(data)
        self.ledger.record(path, hashlib.sha256(data).hexdigest())

    def run_calibration(self, rep_dir: Path, state_env: dict[str, str], phases: list[dict], launches: list[Launch]) -> bool:
        result: dict[str, Any] = {"ticks": {}}
        for tick in (1000, 2000, 4000):
            log_path = rep_dir / f"probe-tick{tick}.jsonl"
            env = self.app_env("plain", state_env, log_path, tick_us=tick)
            launch = self.launch(rep_dir, env, [], f"tick{tick}")
            launches.append(launch)
            ok = self.wait_bus(launch, time.monotonic() + self.cap_s) and self.wait_ready("app-startup", time.monotonic() + self.cap_s)[0]
            if not ok:
                return False
            time.sleep(1.0)
            cpu0 = proc_cpu_ticks(launch.process.pid)
            idle_start = now_us()
            time.sleep(3.0)
            idle_end = now_us()
            cpu1 = proc_cpu_ticks(launch.process.pid)
            clock = None
            if self.rd.available and tick == 2000:
                injections: list[dict] = []
                self.rd.tap(KEY_RIGHT, injections)
                time.sleep(0.5)
                clock = {"injections": injections}
            self.wait_ready("app-startup", time.monotonic() + self.cap_s)
            time.sleep(0.3)
            self.kill(launch)
            parsed = parse_probe_log(log_path.read_text(encoding="utf-8", errors="replace").splitlines())
            idle_lates = [int(r["lateness_us"]) for r in parsed["records"]["late"] if idle_start <= r["t_us"] <= idle_end]
            seconds = [r for r in parsed["records"]["second"] if idle_start <= r["t_us"] <= idle_end]
            entry = {
                "started": parsed["started"],
                "idle_window_us": idle_end - idle_start,
                "max_idle_lateness_us": max([int(r.get("max_lateness_us", 0)) for r in seconds] + idle_lates + [0]),
                "late_records": len(idle_lates),
                "ticks": sum(int(r.get("ticks", 0)) for r in seconds),
                "cpu_ticks_per_s": round((cpu1 - cpu0) / ((idle_end - idle_start) / 1e6), 3) if cpu0 is not None and cpu1 is not None else None,
            }
            if clock:
                for injection in clock["injections"]:
                    record = next((r for r in parsed["records"]["input"] if r["t_us"] >= injection["inject_us"] - 2000 and r.get("event") == "key-press"), None)
                    if record:
                        event_us = int(record.get("event_time_ms", 0)) * 1000
                        clock.update(
                            {
                                "inject_us": injection["inject_us"],
                                "event_us": event_us,
                                "handler_us": record["t_us"],
                                "event_minus_inject_us": event_us - injection["inject_us"],
                                "handler_minus_event_us": record["t_us"] - event_us,
                            }
                        )
                        self.event_clock_ok = injection["inject_us"] - 2_000 <= event_us <= record["t_us"] + 1_000
                        break
                entry["clock_agreement"] = clock
            result["ticks"][str(tick)] = entry
        self.slack_us = int(result["ticks"]["2000"]["max_idle_lateness_us"])
        result["slack_us"] = self.slack_us
        result["event_clock_monotonic"] = self.event_clock_ok
        (self.artifact_dir / "calibration.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        phases.append({"name": "idle", "kind": "calibration", "status": "ok", "start_us": 0, "end_us": 0, "injections": []})
        return True

    # -- analysis -----------------------------------------------------------

    def analyse(self, case: Case, variant: str, rep: int, rep_dir: Path, phases: list[dict], launches: list[Launch], at_events: list[dict], outcome: str) -> dict:
        budgets = read_budgets()
        parsed_logs = []
        for path in sorted(rep_dir.glob("probe*.jsonl")):
            parsed_logs.append(parse_probe_log(path.read_text(encoding="utf-8", errors="replace").splitlines()))
        merged = {kind: [] for kind in KNOWN_KINDS}
        tick = 2000
        threshold = 1000
        dropped = 0
        for parsed in parsed_logs:
            for kind in KNOWN_KINDS:
                merged[kind].extend(parsed["records"][kind])
            tick = parsed["tick_us"]
            threshold = parsed["late_threshold_us"]
            dropped += parsed["dropped"]
        parsed = {"records": merged, "tick_us": tick, "late_threshold_us": threshold}
        probe_started = bool(merged["start"]) or case.flow == "calibration"
        phase_rows = []
        for phase in phases:
            row = {k: v for k, v in phase.items() if k != "injections"}
            if "start_us" in phase and phase.get("status") not in ("unavailable",) and phase["name"] != "idle":
                start, end = phase["start_us"], phase["end_us"]
                stall = phase_stall(parsed, start, end)
                if phase.get("status") == "exceeded_cap":
                    stall["lateness_us"] = max(stall["lateness_us"], end - start)
                    stall["lower_bound_only"] = True
                row["stall"] = stall
                row["frames"] = frame_gap_p95(parsed, start, end)
                row["input_to_frame"] = input_to_frame(parsed, phase.get("injections", []), budgets["REFERENCE_FRAME_US"], end)
                row["im"] = im_summary(parsed, start, end)
                if variant == "at":
                    row["at"] = at_summary(at_events, start, end)
            phase_rows.append(row)
        window_us = None
        if launches and merged["frame"]:
            first_frame = min(int(r["frame_time_us"]) for r in merged["frame"])
            window_us = first_frame - launches[0].spawn_us
        return {
            "case": case.id,
            "group": case.group,
            "length": case.length,
            "attrs": case.attrs,
            "variant": variant,
            "rep": rep,
            "outcome": outcome,
            "probe_started": probe_started,
            "tick_us": tick,
            "late_threshold_us": threshold,
            "dropped_records": dropped,
            "window_construction_us": window_us,
            "only_phases": sorted(self.only_phases) if self.only_phases else None,
            "exit_codes": [launch.process.returncode for launch in launches],
            "renderer": self.renderer,
            "renderer_actual": actual_renderer(launches),
            "phases": phase_rows,
        }

    # -- sweep --------------------------------------------------------------

    def append_result(self, row: dict) -> None:
        with self.results_path.open("a", encoding="utf-8") as results:
            results.write(json.dumps(row) + "\n")

    def record_skip(self, case: Case, variant: str, reason: str) -> dict:
        row = {"case": case.id, "group": case.group, "length": case.length, "variant": variant, "rep": 0, "outcome": "skipped", "reason": reason, "phases": []}
        self.append_result(row)
        return row

    def run_case(self, case: Case) -> list[dict]:
        files: list[Path] = []
        rows: list[dict] = []
        if case.flow == "install" and not memory_ok(case.length or 0):
            return [self.record_skip(case, "*", "not enough available memory")]
        if case.fixture is not None:
            files = self.make_fixture(case.fixture, case.id)
        try:
            variants = ["plain"] if case.flow == "calibration" else self.variants
            for variant in variants:
                if variant == "ibus" and not self.ibus_available:
                    rows.append(self.record_skip(case, variant, "ibus-daemon is not installed"))
                    continue
                reps = 1 if case.flow == "calibration" else self.repetitions
                variant_rows = [self.run_rep(case, variant, rep, files) for rep in range(reps)]
                worst = [worst_stall(row) for row in variant_rows if row["outcome"] in ("ok", "exceeded_cap")]
                if self.repetitions > 1 and is_noisy(worst):
                    variant_rows.append(self.run_rep(case, variant, reps, files))
                rows.extend(variant_rows)
                print(f"[{time.strftime('%H:%M:%S')}] {case.id}@{variant}: {[r['outcome'] for r in variant_rows]} worst={worst}", flush=True)
        finally:
            if files:
                self.drop_fixtures(files)
        return rows

    def run(self, cases: list[Case]) -> int:
        start_wall = time.time()
        rows: list[dict] = []
        try:
            for case in cases:
                rows.extend(self.run_case(case))
            if self.bisect:
                rows.extend(self.run_bisection(rows))
        finally:
            self.rd.close()
            shutil.rmtree(self.fixture_root, ignore_errors=True)
        (self.artifact_dir / "fixtures.json").write_text(json.dumps(self.fixture_manifests, indent=2) + "\n", encoding="utf-8")
        (self.artifact_dir / "integrity.json").write_text(
            json.dumps({"checks": self.ledger.checks, "violations": self.ledger.violations}, indent=2) + "\n", encoding="utf-8"
        )
        run_info = json.loads((self.artifact_dir / "run.json").read_text(encoding="utf-8"))
        run_info.update(
            {
                "wall_start": start_wall,
                "wall_end": time.time(),
                "remote_desktop": {"available": self.rd.available, "error": self.rd.error},
                "variants": self.variants,
                "repetitions": self.repetitions,
                "bisection": self.bisect,
                "case_cap_s": self.cap_s,
                "driver_failures": self.failures,
                "event_clock_monotonic": self.event_clock_ok,
            }
        )
        (self.artifact_dir / "run.json").write_text(json.dumps(run_info, indent=2) + "\n", encoding="utf-8")
        return 1 if self.failures else 0

    def run_bisection(self, rows: list[dict]) -> list[dict]:
        """Bounded bisection between the last passing and first failing sweep length."""
        budgets = read_budgets()
        slack = self.slack_us or 0
        extra: list[dict] = []
        series = {
            "syntax": lambda length: [paragraph_case(length, ext, True, False, flavor=flavor) for flavor in ("css", "script") for ext in ("html", "txt")],
            "plain": lambda length: [paragraph_case(length, "txt", True, False), paragraph_case(length, "txt", False, False)],
        }
        lengths = PARAGRAPH_LENGTHS[self.tier]
        for name, make in series.items():
            verdicts = {}
            for length in lengths:
                verdicts[length] = series_pass(name, rows, length, budgets, slack)
            flip = next(((lo, hi) for lo, hi in zip(lengths, lengths[1:]) if verdicts[lo] is True and verdicts[hi] is False), None)
            if not flip:
                continue
            lo, hi = flip
            all_rows = list(rows)
            for _ in range(BISECT_STEPS):
                mid = int(math.sqrt(lo * hi))
                if mid in (lo, hi):
                    break
                saved = self.repetitions
                self.repetitions = 1
                try:
                    for case in make(mid):
                        case.attrs["bisection"] = name
                        new_rows = self.run_case(case)
                        extra.extend(new_rows)
                        all_rows.extend(new_rows)
                finally:
                    self.repetitions = saved
                if series_pass(name, all_rows, mid, budgets, slack):
                    lo = mid
                else:
                    hi = mid
        return extra


def actual_renderer(launches: list) -> str | None:
    for launch in launches:
        try:
            text = launch.stderr_path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        match = re.search(r"Using renderer '([^']+)'", text)
        if match:
            return match.group(1)
    return None


def proc_cpu_ticks(pid: int) -> int | None:
    try:
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        return int(fields[11]) + int(fields[12])
    except (OSError, IndexError, ValueError):
        return None


def memory_ok(size: int) -> bool:
    try:
        for line in Path("/proc/meminfo").read_text().splitlines():
            if line.startswith("MemAvailable:"):
                available = int(line.split()[1]) * 1024
                return available >= 6 * size + 4 * 1024**3
    except OSError:
        return False
    return False


def worst_stall(row: dict) -> int:
    values = [p["stall"]["lateness_us"] for p in row.get("phases", []) if "stall" in p and p["name"] not in STALL_EXCLUDED_PHASES]
    return max(values) if values else 0


# --------------------------------------------------------------------------
# Derivation (report)
# --------------------------------------------------------------------------


def usable(row: dict) -> bool:
    return row.get("outcome") in ("ok", "exceeded_cap") and not row.get("storage_contaminated") and row.get("probe_started", True)


def phase_lateness(rows: list[dict], phase_names: Iterable[str]) -> int | None:
    names = set(phase_names)
    values = [p["stall"]["lateness_us"] for row in rows for p in row.get("phases", []) if p.get("name") in names and "stall" in p]
    return max(values) if values else None


def hung(rows: list[dict], phase_names: Iterable[str]) -> bool:
    """Whether any row hit the interaction cap in (or before reaching) one of these phases.

    A capped phase is a failure of the side that hung. Subtracting the
    other side's lateness would turn two capped phases into a ~0 difference
    and read as "within" (quick run #4 derived L_syntax = 250000 that way).
    """
    names = set(phase_names)
    for row in rows:
        present = [p for p in row.get("phases", []) if p.get("name") in names]
        if any(p.get("status") == "exceeded_cap" or p.get("stall", {}).get("lower_bound_only") for p in present):
            return True
        measured = row.get("only_phases")
        if not present and row.get("outcome") == "exceeded_cap" and (measured is None or names & set(measured)):
            return True
    return False


SETTLE_PHASES = ("open", "background")


def rows_for(rows: list[dict], group: str, length: int | None = None, **attrs: Any) -> list[dict]:
    selected = []
    for row in rows:
        if row.get("group") != group or (length is not None and row.get("length") != length):
            continue
        if all(row.get("attrs", {}).get(k) == v for k, v in attrs.items()):
            selected.append(row)
    return selected


INTERACTION_PHASES = ("scroll", "click", "pagedown", "type-start", "type-middle", "type-end", "find", "tab-switch", "reselect")


# The derived value each in-driver bisection series refines.
BISECTION_SERIES = {"syntax": "L_syntax", "plain": "N_visible"}


def length_verdict(outcome: dict, length: int) -> bool | None:
    """Whether a derived threshold, computed from one length's rows alone, passes at that length (None: undecided)."""
    if outcome.get("value") == length:
        return True
    if outcome.get("value") is None and ("upper_bound" in outcome or str(outcome.get("reason", "")).startswith("fails")):
        return False
    return None


def series_pass(name: str, rows: list[dict], length: int, budgets: dict, slack: int) -> bool | None:
    """Pass/fail for one bisection series at one length (None when not measured), by the report's rule."""
    subset = [r for r in rows if r.get("group") == "paragraph" and r.get("length") == length]
    return length_verdict(derive(subset, budgets, slack, True, {})[BISECTION_SERIES[name]], length)


def largest_passing(lengths: list[int], verdict: Callable[[int], bool | None]) -> dict[str, Any]:
    """The largest length whose verdict passes, provided every smaller measured length passes too."""
    decided: dict[int, bool | None] = {length: verdict(length) for length in sorted(lengths)}
    measured = [length for length, v in decided.items() if v is not None]
    if not measured:
        return {"value": None, "reason": "no usable cases"}
    passing = None
    for length in sorted(measured):
        if decided[length]:
            passing = length
        else:
            break
    if passing is None:
        return {"value": None, "reason": f"fails at the smallest measured length ({min(measured)})", "upper_bound": min(measured)}
    first_fail = next((length for length in sorted(measured) if length > passing and decided[length] is False), None)
    result: dict[str, Any] = {"value": passing, "first_failing": first_fail}
    if first_fail is None:
        result["note"] = "every measured length passed; the value is a lower bound"
    return result


def derive(rows: list[dict], budgets: dict, slack: int | None, rd_available: bool, axiom_obs: dict) -> dict[str, Any]:
    slack = slack or 0
    tick = max([row.get("tick_us", 2000) for row in rows if row.get("case") != "baseline/calibration"] + [2000])
    b_i, b_s = budgets["B_INTERACT_US"], budgets["B_SETTLE_US"]
    frame_limit = budgets["REFERENCE_FRAME_US"] * budgets["INPUT_TO_FRAME_FRAMES"]
    para_lengths = sorted({r["length"] for r in rows if r.get("group") == "paragraph" and r.get("length")})
    derived: dict[str, Any] = {}
    noisy_cases = sorted({r["case"] for r in rows if r.get("noisy")})

    def describe(name: str, rule: str, outcome: dict, cases: list[str], variants: str = "worst of plain, at, probe-im") -> None:
        reps = sorted({r.get("rep", 0) for r in rows if r.get("case") in cases})
        outcome.update({"name": name, "rule": rule, "cases": sorted(set(cases))[:40], "variants": variants, "repetitions": len(reps)})
        if outcome.get("value") is None and "reason" not in outcome:
            outcome["reason"] = "not derived"
        derived[name] = outcome

    def attributable(group_rows: Callable[[int, Any], tuple[list[dict], list[dict]]], phases_budgets) -> Callable[[int], bool | None]:
        def verdict(length: int) -> bool | None:
            with_rows, without_rows = group_rows(length, None)
            with_rows = [r for r in with_rows if usable(r)]
            without_rows = [r for r in without_rows if usable(r)]
            if not with_rows or not without_rows:
                return None
            results = []
            for phases, budget in phases_budgets:
                if hung(with_rows, phases):
                    results.append("over")
                    continue
                if hung(without_rows, phases):
                    # The baseline side's cost is only a lower bound, so the
                    # attributable difference cannot be bounded: no verdict.
                    continue
                a, b = phase_lateness(with_rows, phases), phase_lateness(without_rows, phases)
                if a is None or b is None:
                    continue
                results.append(classify(max(0, a - b), tick, slack, budget))
            return None if not results else all(v == "within" for v in results)

        return verdict

    # L_syntax: the html-minus-txt difference within one flavour (same
    # bytes), and the worst (smallest) flavour decides.
    per_flavor: dict[str, dict] = {}
    for flavor in ("css", "script"):

        def syntax_rows(length: int, _unused: Any, flavor: str = flavor) -> tuple[list[dict], list[dict]]:
            return (
                [r for w in (True, False) for r in rows_for(rows, "paragraph", length, ext="html", wrap=w, minimap=False, flavor=flavor)],
                [r for w in (True, False) for r in rows_for(rows, "paragraph", length, ext="txt", wrap=w, minimap=False, flavor=flavor)],
            )

        lengths = sorted({r["length"] for r in rows if r.get("group") == "paragraph" and r.get("attrs", {}).get("flavor") == flavor})
        per_flavor[flavor] = largest_passing(lengths, attributable(syntax_rows, ((("scroll",), b_i), (SETTLE_PHASES, b_s))))
    measured = {f: o for f, o in per_flavor.items() if o.get("value") is not None or "upper_bound" in o}
    if measured:
        deciding = min(measured, key=lambda f: measured[f].get("value") if measured[f].get("value") is not None else -1)
        outcome = dict(measured[deciding])
        outcome["deciding_flavor"] = deciding
    else:
        deciding = None
        outcome = {"value": None, "reason": "no usable cases"}
    outcome["per_flavor"] = per_flavor
    describe("L_syntax", ".html minus .txt stall at the same length and flavour: scroll <= B_interact, open and background <= B_settle; a capped .html phase fails; the worst flavour decides", outcome, [r["case"] for r in rows if r.get("group") == "paragraph" and not r.get("attrs", {}).get("minimap")])

    minimap_rows = lambda length, _: (
        [r for w in (True, False) for r in rows_for(rows, "paragraph", length, ext="txt", wrap=w, minimap=True, flavor="css")],
        [r for w in (True, False) for r in rows_for(rows, "paragraph", length, ext="txt", wrap=w, minimap=False, flavor="css")],
    )
    outcome = largest_passing(para_lengths, attributable(minimap_rows, ((("scroll",), b_i), (SETTLE_PHASES, b_s))))
    describe("L_minimap", "minimap-on minus minimap-off stall (txt, same wrap): scroll <= B_interact, open and background <= B_settle; a capped minimap-on phase fails", outcome, [r["case"] for r in rows if r.get("group") == "paragraph" and r.get("attrs", {}).get("ext") == "txt"])

    inv_lengths = sorted({r["length"] for r in rows if r.get("group") == "invisibles"})
    invisibles_rows = lambda length, _: (rows_for(rows, "invisibles", length, invisibles="all"), rows_for(rows, "invisibles", length, invisibles="off"))
    outcome = largest_passing(inv_lengths, attributable(invisibles_rows, ((("scroll",), b_i),)))
    describe("L_invisibles", "invisibles on minus off stall while scrolling <= B_interact", outcome, [r["case"] for r in rows if r.get("group") == "invisibles"])

    def txt_rows(length: int) -> list[dict]:
        """Usable plain-text paragraph rows at one length, wrap on and off, minimap off."""
        return [r for w in (True, False) for r in rows_for(rows, "paragraph", length, ext="txt", wrap=w, minimap=False) if usable(r)]

    def plain_verdict(phases: tuple[str, ...]) -> Callable[[int], bool | None]:
        def verdict(length: int) -> bool | None:
            selected = txt_rows(length)
            if selected and hung(selected, phases):
                return False
            value = phase_lateness(selected, phases)
            return None if value is None else classify(value, tick, slack, b_i) == "within"

        return verdict

    outcome = largest_passing(para_lengths, plain_verdict(("find",)))
    describe("L_search_highlight", "every stall during in-tab find and match navigation <= B_interact (txt, minimap off)", outcome, [r["case"] for r in rows if r.get("group") == "paragraph"])

    outcome = largest_passing(para_lengths, plain_verdict(("scroll",)))
    a23 = axiom_obs.get("A23")
    if a23:
        outcome["a23_check"] = {k: v for k, v in a23.get("measured", {}).items() if re.search(r"(_us|_ns|per_char)", k)}
    describe("N_visible", "plain (txt, minimap off) scroll stalls <= B_interact, wrap on and off; checked against A23", outcome, [r["case"] for r in rows if r.get("group") == "paragraph"])

    if not rd_available:
        derived["L_protect"] = {"name": "L_protect", "value": None, "reason": "remote-desktop input was unavailable: caret, type and click interactions were not injected"}
    else:

        # Informational: the stall half alone. The 2-frame input-to-frame
        # limit sits at the lane's double-buffered 120 Hz floor (an input
        # landing just after a frame start waits one frame for the next and
        # one to present), which the baseline exceeds too, so the literal
        # L_protect cannot discriminate by length.
        def protect_stall_verdict(length: int) -> bool | None:
            selected = txt_rows(length)
            if not selected:
                return None
            if hung(selected, CARET_PHASES):
                return False
            value = phase_lateness(selected, CARET_PHASES)
            return None if value is None else classify(value, tick, slack, b_i) == "within"

        def protect_verdict(length: int) -> bool | None:
            stall = protect_stall_verdict(length)
            if stall is not True:
                return stall
            selected = txt_rows(length)
            latencies = [
                entry["latency_us"]
                for row in selected
                for phase in row.get("phases", [])
                if phase.get("name") in CARET_PHASES
                for entry in phase.get("input_to_frame", [])
                if "latency_us" in entry
            ]
            return all(latency <= frame_limit for latency in latencies) if latencies else False

        outcome = largest_passing(para_lengths, protect_verdict)
        describe("L_protect", "caret, type and click stalls <= B_interact and input-to-frame <= 2 reference frames, worst variant", outcome, [r["case"] for r in rows if r.get("group") == "paragraph"])

        outcome = largest_passing(para_lengths, protect_stall_verdict)
        describe("L_protect_stall", "informational: caret, type and click stalls <= B_interact, worst variant (L_protect without its frame half)", outcome, [r["case"] for r in rows if r.get("group") == "paragraph"])

    # Validation batches (A31).
    a31 = axiom_obs.get("A31", {}).get("measured", {})
    rows_per_batch = to_int(a31.get("rows_per_batch_nowrap") or a31.get("rows_per_batch"))
    row_height = to_int(a31.get("line_height") or a31.get("row_height_px"))
    dense_lengths = sorted({r["length"] for r in rows if r.get("group") == "dense"})

    # The background phase also holds the post-open stall every document
    # shows (baseline/small); validation is its excess over that stall.
    post_open_us = phase_lateness([r for r in rows if r.get("case") == "baseline/small" and usable(r)], ("background",)) or 0

    def dense_verdict(length: int) -> bool | None:
        selected = [r for r in rows_for(rows, "dense", length) if usable(r)]
        if selected and hung(selected, ("scroll", "pagedown", "background")):
            return False
        interaction = phase_lateness(selected, ("scroll", "pagedown"))
        background = phase_lateness(selected, ("background",))
        values = [v for v in (interaction, None if background is None else max(0, background - post_open_us)) if v is not None]
        return None if not values else classify(max(values), tick, slack, b_i) == "within"

    dense_outcome = largest_passing(dense_lengths, dense_verdict)
    if rows_per_batch and dense_outcome.get("value"):
        dense_outcome["line_length"] = dense_outcome["value"]
        dense_outcome["value"] = dense_outcome["value"] * rows_per_batch
        dense_outcome["rows_per_batch_default_zoom"] = rows_per_batch
    else:
        dense_outcome = {"value": None, "reason": "needs the A31 rows-per-batch observation (pass --axiom-observations) and a passing dense case", "dense_line_length": dense_outcome.get("value"), "dense_reason": dense_outcome.get("reason")}
    describe("V_batch", "largest line length x A31 rows per 2000 px batch whose dense validation stalls <= B_interact (scroll and Page Down as measured; background as its excess over the baseline/small post-open stall)", dense_outcome, [r["case"] for r in rows if r.get("group") == "dense"])

    if rows_per_batch:
        # Minimum editor zoom is 50 % (ui/window/zoom.rs ZOOM_MIN), which halves
        # the row height and doubles the rows a 2000 px batch validates.
        r_batch = {"value": rows_per_batch * 2, "rows_per_batch_default_zoom": rows_per_batch, "row_height_px_default": row_height, "note": "A31 rows at the probe font scaled to the 50 % minimum zoom"}
    else:
        r_batch = {"value": None, "reason": "A31 observation unavailable"}
    describe("R_batch", "rows per 2000 px validation batch, wrap off, at the smallest supported line height", r_batch, [], variants="n/a (axiom probe)")
    if derived["V_batch"].get("value") and derived["R_batch"].get("value"):
        n_dense = {"value": derived["V_batch"]["value"] // derived["R_batch"]["value"]}
    else:
        n_dense = {"value": None, "reason": "V_batch or R_batch not derived"}
    describe("N_dense", "V_batch / R_batch", n_dense, [], variants="derived")

    visible = derived["N_visible"].get("value")
    protect = derived.get("L_protect", {}).get("value")
    if visible and protect:
        inspect = {"value": min(visible, protect)}
    else:
        inspect = {"value": None, "reason": "N_visible or L_protect not derived"}
    describe("N_inspect_bound", "min(N_visible, L_protect)", inspect, [], variants="derived")

    sizes = sorted({r["length"] for r in rows if r.get("group") == "total-size"})

    # Syntax-attributable, like L_syntax: the same short-line bytes as .html
    # and as .txt. Without the subtraction the post-open stall that a
    # 400-byte .txt shows too would fail every size.
    size_rows = lambda size, _: (rows_for(rows, "total-size", size, ext="html"), rows_for(rows, "total-size", size, ext="txt"))
    size_phases = ("background", "scroll", "pagedown") + CARET_PHASES
    outcome = largest_passing(sizes, attributable(size_rows, ((size_phases, b_i),)))
    describe("S_syntax", "largest short-line total size whose .html minus .txt stall (background highlighting, scroll, Page Down, caret and typing) keeps every dispatch <= B_interact", outcome, [r["case"] for r in rows if r.get("group") == "total-size"])

    # Informational: the interaction half alone. On the reference run the
    # .html background phase carries a size-independent stall (the same
    # ~42 ms at 16 kB and 62 kB: the HTML language and first highlight in a
    # fresh process), which no size cutoff can remove.
    outcome = largest_passing(sizes, attributable(size_rows, ((("scroll", "pagedown") + CARET_PHASES, b_i),)))
    describe("S_syntax_interact", "informational: S_syntax without the background phase (.html minus .txt scroll, Page Down, caret and typing stalls <= B_interact)", outcome, [r["case"] for r in rows if r.get("group") == "total-size"])

    derived["budget_verdict"] = budget_verdict(rows, budgets, tick, slack)
    derived["noisy_cases"] = noisy_cases
    return derived


def to_int(value: Any) -> int | None:
    try:
        return int(float(value)) if value is not None else None
    except (TypeError, ValueError):
        return None


def frame_verdict(latencies: list[int], frame_limit: int) -> str:
    if not latencies:
        return "not measured"
    return "within" if max(latencies) <= frame_limit else "over"


def budget_verdict(rows: list[dict], budgets: dict, tick: int, slack: int) -> dict[str, Any]:
    baseline = [r for r in rows if r.get("group") == "baseline" and r.get("case") != "baseline/calibration" and usable(r)]
    if not baseline:
        return {"verdict": "not derived", "reason": "no usable baseline cases"}
    frame_limit = budgets["REFERENCE_FRAME_US"] * budgets["INPUT_TO_FRAME_FRAMES"]
    worst = {"interaction": 0, "settle": 0}
    worst_where: dict[str, str] = {}
    latencies: list[int] = []
    for row in baseline:
        for phase in row.get("phases", []):
            if "stall" not in phase or phase["name"] in STALL_EXCLUDED_PHASES:
                continue
            kind = phase.get("kind", "interaction")
            if kind in worst and phase["stall"]["lateness_us"] > worst[kind]:
                worst[kind] = phase["stall"]["lateness_us"]
                worst_where[kind] = f"{row['case']}@{row['variant']}#{row.get('rep', 0)}:{phase['name']}"
            latencies.extend(e["latency_us"] for e in phase.get("input_to_frame", []) if "latency_us" in e)
    verdicts = {
        "B_interact": classify(worst["interaction"], tick, slack, budgets["B_INTERACT_US"]),
        "B_settle": classify(worst["settle"], tick, slack, budgets["B_SETTLE_US"]),
        "input_to_frame": frame_verdict(latencies, frame_limit),
    }
    result: dict[str, Any] = {
        "cases": sorted({r["case"] for r in baseline}),
        "excluded": "window construction (the open phase), owned by bound-window-construction-dispatch (C4)",
        "worst_interaction_stall_us": worst["interaction"],
        "worst_settle_stall_us": worst["settle"],
        "stall_bounds_us": {k: [max(0, v - slack), v + tick] for k, v in worst.items()},
        "worst_input_to_frame_us": max(latencies) if latencies else None,
        "worst_at": worst_where,
        "per_budget": verdicts,
        "tick_us": tick,
        "slack_us": slack,
    }
    # Informational: the scroll frame-gap limit is not a D7 verdict input.
    # Gaps are read over presented frames only (frame_gap_p95).
    scroll_gaps = [p["frames"]["p95_gap_us"] for row in baseline for p in row.get("phases", []) if p.get("name") == "scroll" and p.get("frames", {}).get("p95_gap_us") is not None]
    result["scroll_frame_gap_p95_us"] = max(scroll_gaps) if scroll_gaps else None
    result["scroll_frame_gap_limit_us"] = budgets["SCROLL_P95_FRAME_GAP_US"]
    if all(v == "within" for v in verdicts.values()):
        result["verdict"] = "confirmed"
    elif any(v == "over" for v in verdicts.values()):
        result["verdict"] = "revise"
        result["proposed"] = {
            "B_INTERACT_US": max(budgets["B_INTERACT_US"], math.ceil((worst["interaction"] + tick) / 1000) * 1000),
            "B_SETTLE_US": max(budgets["B_SETTLE_US"], math.ceil((worst["settle"] + tick) / 1000) * 1000),
        }
    else:
        result["verdict"] = "indeterminate"
        result["reason"] = "a worst stall lies between L - eps and L + T of a budget; re-run with a smaller tick"
    return result


def load_axiom_observations(path: Path | None) -> dict[str, dict]:
    observations: dict[str, dict] = {}
    if path is None or not path.exists():
        return observations
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        line = line.strip()
        if line.startswith('{"axiom"'):
            try:
                record = json.loads(line)
            except json.JSONDecodeError:
                continue
            observations[record["axiom"]] = record
    return observations


def classify_warning(line: str, case: str) -> tuple[str, dict] | None:
    """The fixed defect (`"regression"`) or known defect (`"finding"`) a
    warning line matches, or None when it is unclassified. Fixed defects are
    checked first, so a fixed blocker can never be reported as owned work."""
    for defect in FIXED_DEFECTS:
        if defect["pattern"].search(line):
            return "regression", defect
    for defect in KNOWN_DEFECTS:
        hint = defect.get("critical_in_case")
        if defect["pattern"].search(line) or (hint and "CRITICAL" in line and hint in case):
            return "finding", defect
    return None


def scan_warnings(artifact_dir: Path) -> dict[str, Any]:
    findings: dict[str, dict] = {}
    regressions: dict[str, dict] = {}
    unclassified: list[dict] = []
    # Merged sets link their case directories, so follow symlinks.
    for path in sorted(Path(root) / name for root, _dirs, files in os.walk(artifact_dir / "cases", followlinks=True) for name in files if name.startswith("app-") and name.endswith(".stderr")):
        case = str(path.parent.relative_to(artifact_dir / "cases"))
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            if not re.search(r"(WARNING|CRITICAL)", line) or is_gdk_broken_pipe_teardown(line):
                continue
            match = classify_warning(line, case)
            if match is None:
                if len(unclassified) < 200:
                    unclassified.append({"case": case, "line": line[:300]})
                continue
            kind, defect = match
            if kind == "regression":
                entry = regressions.setdefault(defect["id"], {"fixed_by": defect["fixed_by"], "axiom": defect["axiom"], "count": 0, "cases": [], "example": line[:300]})
            else:
                entry = findings.setdefault(defect["id"], {"owner": defect["owner"], "axiom": defect["axiom"], "count": 0, "cases": [], "example": line[:300]})
            entry["count"] += 1
            if case not in entry["cases"] and len(entry["cases"]) < 50:
                entry["cases"].append(case)
    return {"known_findings": findings, "regressions": regressions, "unclassified": unclassified}


def run_failed(missing: list[str], integrity: dict, run_info: dict, warnings: dict) -> bool:
    """The lane fails on missing artifacts, an integrity violation, a driver
    failure, or a warning of a pre-existing blocker the programme has fixed."""
    return bool(missing) or bool(integrity.get("violations")) or bool(run_info.get("driver_failures")) or bool(warnings.get("regressions"))


def run_storage(artifact_dir: Path, rows: list[dict]) -> dict[str, Any]:
    tool = shutil.which("btrfs-stall-report")
    run_info = json.loads((artifact_dir / "run.json").read_text(encoding="utf-8")) if (artifact_dir / "run.json").exists() else {}
    if tool is None:
        return {"status": "unavailable", "reason": "btrfs-stall-report is not installed (never installed by the lane)"}
    end = run_info.get("wall_end") or time.time()
    wait = end + 15 - time.time()
    if wait > 0:
        time.sleep(wait)
    storage_dir = artifact_dir / "storage"
    storage_dir.mkdir(exist_ok=True)

    def query(since: float, until: float, no_below: bool) -> dict | None:
        command = [tool, "--since", f"@{int(since)}", "--until", f"@{int(math.ceil(until))}", "--json"]
        if no_below:
            command.append("--no-below")
        result = subprocess.run(command, capture_output=True, text=True, timeout=300, check=False)
        if result.returncode != 0:
            return None
        try:
            return json.loads(result.stdout)
        except json.JSONDecodeError:
            return None

    whole = query((run_info.get("wall_start") or end) - 60, end, False)
    (storage_dir / "run.json").write_text(json.dumps(whole, indent=2) + "\n", encoding="utf-8")
    if whole is None:
        return {"status": "unavailable", "reason": "btrfs-stall-report failed or its log directory is missing"}
    per_case = {}
    for row in rows:
        if "wall_start" not in row:
            continue
        key = f"{row['case']}@{row['variant']}#{row['rep']}"
        report = query(row["wall_start"], row["wall_end"], True)
        status = storage_status(report)
        per_case[key] = {"status": status, "verdict": (report or {}).get("verdict")}
        row["storage"] = status
        row["storage_contaminated"] = status == "contaminated"
    (storage_dir / "cases.json").write_text(json.dumps(per_case, indent=2) + "\n", encoding="utf-8")
    return {"status": storage_status(whole), "verdict": whole.get("verdict"), "alerts": whole.get("alerts"), "contaminated_cases": sorted(k for k, v in per_case.items() if v["status"] == "contaminated")}


def mark_noisy(rows: list[dict]) -> None:
    by_key: dict[tuple, list[dict]] = {}
    for row in rows:
        if row.get("outcome") in ("ok", "exceeded_cap"):
            by_key.setdefault((row["case"], row["variant"]), []).append(row)
    for group in by_key.values():
        if is_noisy([worst_stall(r) for r in group]):
            for row in group:
                row["noisy"] = True
                row["outcome_before_noise"] = row["outcome"]


def summarise_cases(rows: list[dict]) -> list[dict]:
    table: dict[tuple, dict] = {}
    for row in rows:
        key = (row["case"], row["variant"])
        entry = table.setdefault(key, {"case": row["case"], "variant": row["variant"], "reps": 0, "outcomes": [], "phases": {}})
        entry["reps"] += 1
        entry["outcomes"].append(row["outcome"])
        entry["noisy"] = entry.get("noisy", False) or bool(row.get("noisy"))
        entry["storage"] = row.get("storage", "unchecked")
        for phase in row.get("phases", []):
            if "stall" not in phase:
                if phase.get("status") in ("unavailable",):
                    entry["phases"].setdefault(phase["name"], {"status": "unavailable"})
                continue
            p = entry["phases"].setdefault(phase["name"], {"stalls": [], "frame_p95": [], "itf_us": [], "itf_frames": [], "im_calls": 0, "im_max_bytes": None, "at_events": 0, "at_max_payload": None, "status": []})
            p["stalls"].append(phase["stall"]["lateness_us"])
            p["status"].append(phase.get("status"))
            if phase["frames"].get("p95_gap_us") is not None:
                p["frame_p95"].append(phase["frames"]["p95_gap_us"])
            for e in phase.get("input_to_frame", []):
                if "latency_us" in e:
                    p["itf_us"].append(e["latency_us"])
                    p["itf_frames"].append(e["frames"])
            p["im_calls"] += phase.get("im", {}).get("calls", 0)
            mb = phase.get("im", {}).get("max_surrounding_bytes")
            if mb is not None:
                p["im_max_bytes"] = max(mb, p["im_max_bytes"] or 0)
            if "at" in phase:
                p["at_events"] += phase["at"]["events"]
                ap = phase["at"].get("max_payload_len")
                if ap is not None:
                    p["at_max_payload"] = max(ap, p["at_max_payload"] or 0)
    out = []
    for entry in table.values():
        for phase in entry["phases"].values():
            if "stalls" in phase:
                stalls = phase.pop("stalls")
                phase["stall_median_us"] = int(statistics.median(stalls))
                phase["stall_max_us"] = max(stalls)
                fp = phase.pop("frame_p95")
                phase["frame_gap_p95_us"] = max(fp) if fp else None
                itf = phase.pop("itf_us")
                frames = phase.pop("itf_frames")
                phase["input_to_frame_max_us"] = max(itf) if itf else None
                phase["input_to_frame_max_frames"] = max(frames) if frames else None
        out.append(entry)
    return out


def write_report(artifact_dir: Path, axiom_path: Path | None) -> int:
    rows = []
    results = artifact_dir / "results.jsonl"
    if results.exists():
        rows = [json.loads(line) for line in results.read_text(encoding="utf-8").splitlines() if line.strip()]
    run_info = json.loads((artifact_dir / "run.json").read_text(encoding="utf-8")) if (artifact_dir / "run.json").exists() else {}
    run_info["renderer_actual"] = sorted({r["renderer_actual"] for r in rows if r.get("renderer_actual")})
    if run_info.get("merged_from"):
        # merge-runs already stamped every row with its own run's storage
        # verdict (each part's report queried btrfs-stall-report).
        storage = run_info.get("storage", {"status": "unavailable"})
    elif os.environ.get("RESPONSIVENESS_SKIP_STORAGE") != "1":
        storage = run_storage(artifact_dir, rows)
    else:
        storage = {"status": "skipped"}
    mark_noisy(rows)
    calibration = json.loads((artifact_dir / "calibration.json").read_text(encoding="utf-8")) if (artifact_dir / "calibration.json").exists() else {}
    budgets = read_budgets()
    derived = derive(rows, budgets, calibration.get("slack_us"), bool(run_info.get("remote_desktop", {}).get("available")), load_axiom_observations(axiom_path))
    warnings = scan_warnings(artifact_dir) if (artifact_dir / "cases").exists() else {"known_findings": {}, "regressions": {}, "unclassified": []}
    integrity = json.loads((artifact_dir / "integrity.json").read_text(encoding="utf-8")) if (artifact_dir / "integrity.json").exists() else {"violations": ["integrity.json missing"]}
    window = [
        {"case": r["case"], "variant": r["variant"], "rep": r["rep"], "window_construction_us": r.get("window_construction_us"), "open_stall_us": next((p["stall"]["lateness_us"] for p in r.get("phases", []) if p.get("name") == "open" and "stall" in p), None)}
        for r in rows
        if r.get("group") == "baseline" and r.get("case") != "baseline/calibration"
    ]
    install = [
        {"case": r["case"], "variant": r["variant"], "outcome": r["outcome"], "open_stall_us": next((p["stall"]["lateness_us"] for p in r.get("phases", []) if p.get("name") == "open" and "stall" in p), None)}
        for r in rows
        if r.get("group") == "install"
    ]
    c3 = {"measured": install}
    if install:
        stalls = [i["open_stall_us"] for i in install if i["open_stall_us"] is not None]
        c3["verdict"] = "needed" if any(s > budgets["B_SETTLE_US"] for s in stalls) else "not needed"
    else:
        c3["verdict"] = "not measured (install group runs in the full tier only)"
    missing = [name for name in ("run.json", "results.jsonl", "calibration.json", "fixtures.json", "integrity.json") if not (artifact_dir / name).exists()]
    report = {
        "run": run_info,
        "budgets": budgets,
        "calibration": calibration,
        "storage": storage,
        "integrity": integrity,
        "warnings": warnings,
        "derived": derived,
        "window_construction_c4": window,
        "install_c3": c3,
        "cases": summarise_cases(rows),
        "missing_artifacts": missing,
    }
    (artifact_dir / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    (artifact_dir / "report.md").write_text(render_markdown(report), encoding="utf-8")
    failed = run_failed(missing, integrity, run_info, warnings)
    summary = {
        "status": "failed" if failed else "passed",
        "run_id": run_info.get("run_id"),
        "tier": run_info.get("tier"),
        "missing_artifacts": missing,
        "integrity_violations": len(integrity.get("violations", [])),
        "driver_failures": run_info.get("driver_failures", []),
        "fixed_defect_regressions": sorted(warnings.get("regressions", {})),
        "budget_verdict": derived["budget_verdict"].get("verdict"),
    }
    (artifact_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))
    return 1 if failed else 0


def merged_storage_status(parts: list[dict]) -> str:
    """One storage status for a sweep joined from several parts."""
    if any(p.get("status") == "contaminated" for p in parts):
        return "contaminated"
    if parts and all(p.get("status") == "clean" for p in parts):
        return "clean"
    return "mixed"


def merge_runs(out: Path, sources: list[Path], quiet: bool = False) -> int:
    """Join the artifacts of several lane runs (one per case group) into one.

    The reference sweep runs in parts (groups, bisection rounds, bracketing
    repetitions). Rows of a case measured by several parts accumulate as
    further repetitions, renumbered per case and variant in part order; an
    aborted part is simply not passed in. Each row carries its own part's
    storage verdict, read from that part's storage/cases.json. Case
    directories are linked, not copied, under one directory per part.
    """
    out.mkdir(parents=True, exist_ok=True)
    for name in ("results.jsonl", "run.json", "calibration.json", "fixtures.json", "integrity.json"):
        (out / name).unlink(missing_ok=True)
    if (out / "cases").exists():
        shutil.rmtree(out / "cases")
    rows: list[dict] = []
    next_rep: dict[tuple, int] = {}
    runs: list[dict] = []
    fixtures: list = []
    checks = 0
    violations: list = []
    calibration = None
    storage_parts: list[dict] = []
    for source in sources:
        source = source.resolve()
        run = json.loads((source / "run.json").read_text(encoding="utf-8"))
        runs.append(run)
        stamps = {}
        cases_json = source / "storage/cases.json"
        if cases_json.exists():
            stamps = json.loads(cases_json.read_text(encoding="utf-8"))
        report = source / "report.json"
        if report.exists():
            storage_parts.append({"run_id": run.get("run_id"), **json.loads(report.read_text(encoding="utf-8")).get("storage", {})})
        part_cases = set()
        for line in (source / "results.jsonl").read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            row = json.loads(line)
            stamp = stamps.get(f"{row['case']}@{row['variant']}#{row['rep']}")
            if stamp:
                row["storage"] = stamp["status"]
                row["storage_contaminated"] = stamp["status"] == "contaminated"
            row["run_id"] = run.get("run_id")
            row["part_rep"] = row.get("rep", 0)
            part_cases.add(row["case"])
            # Repetitions accumulate across parts (a reduced sweep adds the
            # bracketing repetitions in a later part), renumbered per case
            # and variant in part order.
            row["rep"] = next_rep.get((row["case"], row["variant"]), 0)
            next_rep[(row["case"], row["variant"])] = row["rep"] + 1
            rows.append(row)
        if (source / "calibration.json").exists() and calibration is None:
            calibration = json.loads((source / "calibration.json").read_text(encoding="utf-8"))
        if (source / "fixtures.json").exists():
            fixtures.extend(json.loads((source / "fixtures.json").read_text(encoding="utf-8")))
        if (source / "integrity.json").exists():
            integrity = json.loads((source / "integrity.json").read_text(encoding="utf-8"))
            checks += int(integrity.get("checks", 0))
            violations.extend(integrity.get("violations", []))
        for rep_dir in sorted((source / "cases").rglob("rep*")):
            if rep_dir.is_dir():
                link = out / "cases" / f"part{len(runs) - 1:02d}" / rep_dir.relative_to(source / "cases")
                link.parent.mkdir(parents=True, exist_ok=True)
                if link.is_symlink() or link.exists():
                    link.unlink()
                link.symlink_to(rep_dir)
    merged = dict(runs[0])
    merged.update(
        {
            "run_id": "+".join(r.get("run_id", "?") for r in runs),
            "merged_from": [{"run_id": r.get("run_id"), "artifact_dir": str(src), "wall_start": r.get("wall_start"), "wall_end": r.get("wall_end"), "repetitions": r.get("repetitions"), "bisection": r.get("bisection")} for r, src in zip(runs, sources)],
            "wall_start": min(r.get("wall_start") or math.inf for r in runs),
            "wall_end": max(r.get("wall_end") or 0 for r in runs),
            "remote_desktop": {"available": all(r.get("remote_desktop", {}).get("available") for r in runs), "error": next((r["remote_desktop"].get("error") for r in runs if r.get("remote_desktop", {}).get("error")), None)},
            "driver_failures": [f for r in runs for f in r.get("driver_failures", [])],
            "storage": {
                "status": merged_storage_status(storage_parts),
                "parts": storage_parts,
                "contaminated_cases": sorted(c for p in storage_parts for c in p.get("contaminated_cases", [])),
            },
        }
    )
    (out / "run.json").write_text(json.dumps(merged, indent=2) + "\n", encoding="utf-8")
    (out / "results.jsonl").write_text("".join(json.dumps(r) + "\n" for r in rows), encoding="utf-8")
    if calibration is not None:
        (out / "calibration.json").write_text(json.dumps(calibration, indent=2) + "\n", encoding="utf-8")
    (out / "fixtures.json").write_text(json.dumps(fixtures, indent=2) + "\n", encoding="utf-8")
    (out / "integrity.json").write_text(json.dumps({"checks": checks, "violations": violations}, indent=2) + "\n", encoding="utf-8")
    if not quiet:
        print(f"merged {len(rows)} row(s) from {len(sources)} run(s) into {out}")
    return 0


def render_markdown(report: dict) -> str:
    run = report["run"]
    lines = [
        f"# Responsiveness lane report — {run.get('run_id', '?')}",
        "",
        f"- Tier: `{run.get('tier')}`; renderer requested `{run.get('renderer')}`, got {run.get('renderer_actual')}; monitor: `{run.get('monitor')}`",
        f"- Toolkit: {run.get('toolkit')}",
        f"- Kernel: `{run.get('kernel')}`; CPU: `{run.get('cpu_model')}`; affinity: `{run.get('affinity')}`",
        f"- RemoteDesktop input: {run.get('remote_desktop')}",
        f"- Budgets ({report['budgets'].get('source')}): B_interact {report['budgets']['B_INTERACT_US']} us, B_settle {report['budgets']['B_SETTLE_US']} us, reference frame {report['budgets']['REFERENCE_FRAME_US']} us, input-to-frame {report['budgets']['INPUT_TO_FRAME_FRAMES']} frames, scroll p95 gap {report['budgets']['SCROLL_P95_FRAME_GAP_US']} us",
        f"- Watchdog slack eps: {report['calibration'].get('slack_us')} us; event clock monotonic: {report['calibration'].get('event_clock_monotonic')}",
        f"- Storage: {report['storage'].get('status')} — {report['storage'].get('verdict')}",
        f"- Integrity: {len(report['integrity'].get('violations', []))} violation(s) over {report['integrity'].get('checks')} checks",
        "",
        "Stall bounds read as [L - eps, L + T]: L is the worst watchdog lateness in the phase, T the tick.",
        "",
        "## Derived values",
        "",
        "| Name | Value | Rule | Note |",
        "| --- | --- | --- | --- |",
    ]
    for name in ("L_syntax", "L_minimap", "L_invisibles", "L_search_highlight", "N_visible", "L_protect", "V_batch", "R_batch", "N_dense", "N_inspect_bound", "S_syntax"):
        entry = report["derived"].get(name, {})
        note = entry.get("reason") or entry.get("note") or (f"first failing: {entry.get('first_failing')}" if entry.get("first_failing") else "")
        if entry.get("deciding_flavor"):
            note = f"{note}; deciding flavour: {entry['deciding_flavor']}".lstrip("; ")
        lines.append(f"| `{name}` | {entry.get('value')} | {entry.get('rule', '')} | {note} |")
    verdict = report["derived"]["budget_verdict"]
    lines += ["", "## Budget verdict", "", f"**{verdict.get('verdict')}** — {json.dumps({k: v for k, v in verdict.items() if k != 'cases'})}", ""]
    lines += ["## Findings (reproduced pre-existing defects)", ""]
    for key, finding in report["warnings"].get("known_findings", {}).items():
        lines.append(f"- `{key}` ({finding['axiom'] or 'no axiom'}), owner {finding['owner']}: {finding['count']} line(s) in {len(finding['cases'])} case(s). Example: `{finding['example']}`")
    for key, regression in report["warnings"].get("regressions", {}).items():
        lines.append(f"- **REGRESSION** `{key}` ({regression['axiom']}), fixed by {regression['fixed_by']}: {regression['count']} line(s) in {len(regression['cases'])} case(s); the lane fails. Example: `{regression['example']}`")
    if report["warnings"].get("unclassified"):
        lines.append(f"- {len(report['warnings']['unclassified'])} unclassified warning line(s); see report.json.")
    lines += ["", "## C3 install gate", "", f"{report['install_c3'].get('verdict')}", "", "## Window construction (C4)", ""]
    for entry in report["window_construction_c4"]:
        lines.append(f"- {entry['case']}@{entry['variant']}#{entry['rep']}: first frame {entry['window_construction_us']} us after spawn, open-phase worst stall {entry['open_stall_us']} us")
    lines += ["", "## Cases", "", "| Case | Variant | Outcomes | Phase | Stall median / max (us) | Frame p95 gap | Input-to-frame max (us / frames) | IM calls / max bytes | AT events / max payload |", "| --- | --- | --- | --- | --- | --- | --- | --- | --- |"]
    for case in report["cases"]:
        flags = (" noisy" if case.get("noisy") else "") + (f" storage:{case.get('storage')}" if case.get("storage") not in (None, "unchecked", "clean") else "")
        for name, phase in case["phases"].items():
            if "stall_max_us" not in phase:
                lines.append(f"| {case['case']} | {case['variant']} | {','.join(case['outcomes'])}{flags} | {name} | {phase.get('status')} | | | | |")
                continue
            lines.append(
                f"| {case['case']} | {case['variant']} | {','.join(case['outcomes'])}{flags} | {name} | {phase['stall_median_us']} / {phase['stall_max_us']} | {phase['frame_gap_p95_us']} | {phase['input_to_frame_max_us']} / {phase['input_to_frame_max_frames']} | {phase['im_calls']} / {phase['im_max_bytes']} | {phase['at_events']} / {phase['at_max_payload']} |"
            )
    return "\n".join(lines) + "\n"


# --------------------------------------------------------------------------
# Session plumbing
# --------------------------------------------------------------------------


def refuse_live_session() -> None:
    runtime = os.environ.get("XDG_RUNTIME_DIR", "")
    if not runtime or runtime == f"/run/user/{os.getuid()}":
        raise SystemExit("refusing: XDG_RUNTIME_DIR is the live session's runtime directory; run through scripts/run-responsiveness-smoke.sh")
    display = os.environ.get("WAYLAND_DISPLAY", "")
    if display and os.path.isabs(display) and not display.startswith(runtime):
        raise SystemExit(f"refusing: WAYLAND_DISPLAY={display} is outside the private runtime directory")
    if os.environ.get("DISPLAY"):
        raise SystemExit("refusing: DISPLAY is set; the lane never talks to an X server")


def session_main(args: argparse.Namespace) -> int:
    refuse_live_session()
    registry = None
    if "at" in selected_variants():
        if ATSPI_REGISTRYD.exists():
            registry = start_atspi(args.artifact_dir)
        else:
            os.environ["RESPONSIVENESS_AT_UNAVAILABLE"] = "at-spi2-registryd is not installed"
    ibus = None
    if shutil.which("ibus-daemon"):
        ibus = subprocess.Popen(["ibus-daemon", "--panel=disable", "--replace"], stdout=subprocess.DEVNULL, stderr=(args.artifact_dir / "logs/ibus.log").open("wb"))
    try:
        command = [
            "mutter", "--headless", "--wayland", "--no-x11", "--virtual-monitor", MONITOR, "--",
            str(SYSTEM_PYTHON), str(SCRIPT_PATH), "run", "--artifact-dir", str(args.artifact_dir), "--binary", str(args.binary), "--tier", args.tier,
        ]
        for pattern in args.case or []:
            command += ["--case", pattern]
        env = os.environ.copy()
        env["GDK_BACKEND"] = "wayland"
        with (args.artifact_dir / "logs/mutter.log").open("w", encoding="utf-8") as log:
            result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, check=False)
        print((args.artifact_dir / "logs/mutter.log").read_text(encoding="utf-8", errors="replace")[-4000:])
        return result.returncode
    finally:
        for process in (ibus, registry):
            if process is not None and process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()


def start_atspi(artifact_dir: Path) -> subprocess.Popen | None:
    try:
        address = subprocess.run(
            ["gdbus", "call", "--session", "--dest", "org.a11y.Bus", "--object-path", "/org/a11y/bus", "--method", "org.a11y.Bus.GetAddress"],
            text=True, capture_output=True, check=True, timeout=10,
        ).stdout
        match = re.search(r"'([^']+)'", address)
        if not match:
            raise RuntimeError(address)
        os.environ["AT_SPI_BUS_ADDRESS"] = match.group(1)
        subprocess.run(
            ["gdbus", "call", "--session", "--dest", "org.a11y.Bus", "--object-path", "/org/a11y/bus", "--method",
             "org.freedesktop.DBus.Properties.Set", "org.a11y.Status", "IsEnabled", "<true>"],
            capture_output=True, check=False, timeout=10,
        )
        registry = subprocess.Popen([str(ATSPI_REGISTRYD), "--dbus-name", "org.a11y.atspi.Registry"], stdout=subprocess.DEVNULL, stderr=(artifact_dir / "logs/atspi-registry.log").open("wb"))
        time.sleep(1.0)
        return registry
    except Exception as exc:  # noqa: BLE001
        os.environ["RESPONSIVENESS_AT_UNAVAILABLE"] = f"{type(exc).__name__}: {exc}"
        return None


def environment_info(tier: str, renderer: str) -> dict[str, Any]:
    def pkg(name: str) -> str | None:
        try:
            return subprocess.run(["pkg-config", "--modversion", name], capture_output=True, text=True, check=True).stdout.strip()
        except Exception:  # noqa: BLE001
            return None

    cpu_model = None
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                cpu_model = line.split(":", 1)[1].strip()
                break
    except OSError:
        pass
    mutter = None
    try:
        mutter = subprocess.run(["mutter", "--version"], capture_output=True, text=True, check=False).stdout.splitlines()[0]
    except Exception:  # noqa: BLE001
        pass
    affinity = sorted(os.sched_getaffinity(0))
    return {
        "run_id": f"rr-{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}-{socket.gethostname().split('.')[0]}-{os.getpid()}",
        "tier": tier,
        "renderer": renderer,
        "monitor": MONITOR,
        "gtk_cursor_blink": False,
        "toolkit": {"gtk4": pkg("gtk4"), "libadwaita": pkg("libadwaita-1"), "pango": pkg("pango"), "gtksourceview": pkg("gtksourceview-5"), "glib": pkg("glib-2.0"), "mutter": mutter},
        "kernel": " ".join(platform.uname()[i] for i in (0, 2, 4)),
        "cpu_model": cpu_model,
        "affinity": compress_cpus(affinity),
        "cpu_count": len(affinity),
        "python": platform.python_version(),
        "host": socket.gethostname(),
        "ci": bool(os.environ.get("CI")),
        "at_unavailable": os.environ.get("RESPONSIVENESS_AT_UNAVAILABLE"),
    }


def compress_cpus(cpus: list[int]) -> str:
    ranges = []
    for cpu in cpus:
        if ranges and cpu == ranges[-1][1] + 1:
            ranges[-1][1] = cpu
        else:
            ranges.append([cpu, cpu])
    return ",".join(f"{a}-{b}" if a != b else str(a) for a, b in ranges)


def preflight(runner: Runner) -> dict[str, Any]:
    """Launch the measurement binary once and require watchdog, frame and input records."""
    rep_dir = runner.artifact_dir / "preflight"
    rep_dir.mkdir(parents=True, exist_ok=True)
    state_env = runner.fresh_state(rep_dir, {"word-wrap": True})
    log_path = rep_dir / "probe.jsonl"
    env = runner.app_env("probe-im", state_env, log_path)
    # A small document gives the probe IM a focused text view to serve.
    files = runner.make_fixture(FixtureSpec("paragraph", 400, seed_for("preflight")), "preflight")
    launch = runner.launch(rep_dir, env, files, "preflight")
    result: dict[str, Any] = {}
    try:
        deadline = time.monotonic() + runner.cap_s
        result["bus"] = runner.wait_bus(launch, deadline) and runner.wait_ready("file-open-complete", deadline)[0]
        injections: list[dict] = []
        if runner.rd.available:
            # A freshly mapped window may not take focus from the first
            # click; retry until the probe IM has served a request.
            time.sleep(0.5)
            for attempt in range(4):
                runner.rd.click(SCREEN_W // 3, SCREEN_H // 3, injections)
                time.sleep(0.3)
                runner.rd.tap(ord("a"), injections)
                time.sleep(1.0)
                text = log_path.read_text(encoding="utf-8", errors="replace") if log_path.exists() else ""
                if '"kind":"im"' in text:
                    break
            result["focus_attempts"] = attempt + 1
            runner.rd.move_to(SCREEN_W // 2, SCREEN_H // 2)
            runner.rd.wheel(1, injections)
        time.sleep(1.5)
        runner.wait_ready("app-startup", time.monotonic() + runner.cap_s)
        time.sleep(0.5)
    finally:
        runner.kill(launch)
        shutil.rmtree(state_env["STATE_ROOT"], ignore_errors=True)
        if not runner.ledger.verify(files, "preflight"):
            runner.failures.append("preflight fixture digest changed")
        runner.drop_fixtures(files)
    parsed = parse_probe_log(log_path.read_text(encoding="utf-8", errors="replace").splitlines()) if log_path.exists() else None
    if parsed is None:
        result.update({"ok": False, "reason": "no probe log: is the binary built with --features responsiveness-probes (make responsiveness-build)?"})
        return result
    counts = {kind: len(parsed["records"][kind]) for kind in KNOWN_KINDS}
    result["counts"] = counts
    needs = ["start", "second", "frame"] + (["input", "im"] if runner.rd.available else [])
    result["ok"] = all(counts[k] > 0 for k in needs) and bool(result["bus"])
    result["probe_im_active"] = counts["im"] > 0
    if not result["ok"]:
        result["reason"] = f"missing records: {[k for k in needs if counts[k] == 0]}"
    return result


def run_main(args: argparse.Namespace) -> int:
    refuse_live_session()
    # A SIGTERM (CI cancellation, a manual stop) unwinds through every
    # `finally`: apps are killed, isolated state and fixtures are removed.
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))
    args.artifact_dir.mkdir(parents=True, exist_ok=True)
    renderer = requested_renderer()
    info = environment_info(args.tier, renderer)
    (args.artifact_dir / "run.json").write_text(json.dumps(info, indent=2) + "\n", encoding="utf-8")
    (args.artifact_dir / "results.jsonl").write_text("", encoding="utf-8")
    runner = Runner(args)
    if os.environ.get("RESPONSIVENESS_AT_UNAVAILABLE") and "at" in runner.variants:
        runner.variants.remove("at")
    check = preflight(runner)
    (args.artifact_dir / "preflight.json").write_text(json.dumps(check, indent=2) + "\n", encoding="utf-8")
    if not check.get("ok"):
        print(f"FAIL: preflight: {check.get('reason')}", flush=True)
        runner.rd.close()
        shutil.rmtree(runner.fixture_root, ignore_errors=True)
        return 1
    all_cases = build_cases(args.tier)
    cases = select_cases(all_cases, args.case or [])
    calibration = [c for c in all_cases if c.flow == "calibration"]
    if not any(c.flow == "calibration" for c in cases):
        cases = calibration + cases
    print(f"Running {len(cases)} case(s) x variants {runner.variants} x {runner.repetitions} rep(s)", flush=True)
    return runner.run(cases)


# --------------------------------------------------------------------------
# Self-test
# --------------------------------------------------------------------------


def synthetic_rows() -> list[dict]:
    """Rows shaped like the driver's own output for a quick sweep."""

    def row(case: str, group: str, length: int, attrs: dict, stalls: dict[str, int], variant: str = "plain", rep: int = 0, itf: int = 10_000) -> dict:
        phases = []
        for name, value in stalls.items():
            phases.append(
                {
                    "name": name,
                    "kind": PHASE_KINDS.get(name, "interaction"),
                    "status": "ok",
                    "stall": {"lateness_us": value},
                    "frames": {"p95_gap_us": 8_400},
                    "input_to_frame": [{"latency_us": itf, "frames": math.ceil(itf / 8333)}] if name in CARET_PHASES else [],
                    "im": {"calls": 3, "max_surrounding_bytes": length},
                }
            )
        return {"case": case, "group": group, "length": length, "attrs": attrs, "variant": variant, "rep": rep, "outcome": "ok", "probe_started": True, "tick_us": 2000, "phases": phases}

    rows = []
    base = {"background": 1_000, "scroll": 1_500, "click": 1_000, "type-start": 1_200, "find": 1_000}
    rows.extend([row("baseline/empty", "baseline", 0, {}, dict(base, open=300_000)), row("baseline/small", "baseline", 400, {}, dict(base, open=250_000))])
    for length, txt_scroll, html_extra, caret in ((10_000, 2_000, 1_000, 2_000), (50_000, 4_000, 2_000, 5_000), (250_000, 20_000, 40_000, 60_000)):
        for wrap in (True, False):
            txt = {"open": 10_000 + length // 100, "scroll": txt_scroll, "click": caret, "type-start": caret, "find": txt_scroll}
            html = dict(txt, open=txt["open"] + html_extra * 3, scroll=txt_scroll + html_extra)
            script_extra = 1_000 if length < 50_000 else 30_000
            rows.extend(
                [
                    row(f"paragraph/{length}/txt", "paragraph", length, {"ext": "txt", "wrap": wrap, "minimap": False, "flavor": "css"}, txt, itf=10_000 if length < 50_000 else 20_000),
                    row(f"paragraph/{length}/html", "paragraph", length, {"ext": "html", "wrap": wrap, "minimap": False, "flavor": "css"}, html),
                    row(f"paragraph/{length}/txt/mm", "paragraph", length, {"ext": "txt", "wrap": wrap, "minimap": True, "flavor": "css"}, dict(txt, scroll=txt_scroll + (500 if length < 250_000 else 30_000))),
                    row(f"paragraph/{length}/script/txt", "paragraph", length, {"ext": "txt", "wrap": wrap, "minimap": False, "flavor": "script"}, txt, itf=10_000 if length < 50_000 else 20_000),
                    row(f"paragraph/{length}/script/html", "paragraph", length, {"ext": "html", "wrap": wrap, "minimap": False, "flavor": "script"}, dict(txt, scroll=txt_scroll + script_extra)),
                ]
            )
    for size, stall in ((1_000_000, 3_000), (5_000_000, 12_000)):
        rows.extend(
            [
                row(f"total-size/{size}", "total-size", size, {"ext": "html"}, {"background": stall, "scroll": 1_000}),
                row(f"total-size/{size}/txt", "total-size", size, {"ext": "txt"}, {"background": 1_000, "scroll": 1_000}),
            ]
        )
    for line, stall in ((500, 2_000), (2_000, 5_000), (10_000, 30_000)):
        rows.append(row(f"dense/{line}", "dense", line, {"wrap": False}, {"scroll": stall, "background": 1_000}))
    return rows


def self_test() -> int:
    failures: list[str] = []

    def expect(condition: bool, label: str) -> None:
        if not condition:
            failures.append(label)

    # Classification edges (L + T == B is within; L - eps == B is not over).
    expect(classify(6_000, 2_000, 1_000, 8_000) == "within", "L + T == B is within")
    expect(classify(6_001, 2_000, 1_000, 8_000) == "indeterminate", "L + T == B + 1 is indeterminate")
    expect(classify(9_000, 2_000, 1_000, 8_000) == "indeterminate", "L - eps == B is indeterminate")
    expect(classify(9_001, 2_000, 1_000, 8_000) == "over", "L - eps == B + 1 is over")
    expect(nearest_rank([5, 1, 9, 3], 95) == 9 and nearest_rank([5, 1, 9, 3], 50) == 3, "nearest-rank percentile")

    # Probe log parsing and phase attribution (a stall overlapping the phase start counts).
    log = [
        json.dumps({"kind": "start", "t_us": 0, "version": 1, "tick_us": 2000, "late_threshold_us": 1000}),
        json.dumps({"kind": "late", "t_us": 1_050_000, "ready_us": 950_000, "lateness_us": 100_000}),
        json.dumps({"kind": "late", "t_us": 3_000_000, "ready_us": 2_999_000, "lateness_us": 1_000}),
        json.dumps({"kind": "frame", "t_us": 1, "window": 1, "frame_counter": 1, "frame_time_us": 1_060_000, "presentation_us": 1_062_000, "refresh_interval_us": 8333, "source": "presentation"}),
        json.dumps({"kind": "frame", "t_us": 1, "window": 1, "frame_counter": 2, "frame_time_us": 1_070_000, "presentation_us": 1_070_400, "refresh_interval_us": 8333, "source": "presentation"}),
        json.dumps({"kind": "input", "t_us": 1_051_000, "window": 1, "event": "key-press", "event_time_ms": 1_000}),
        json.dumps({"kind": "im", "t_us": 1_052_000, "trigger": "filter-keypress", "surrounding_bytes": 250_000, "cursor_index": 0, "handled": True}),
        json.dumps({"kind": "dropped", "t_us": 1_053_000, "count": 4}),
        '{"kind": "late", "t_us": 4',  # truncated by SIGKILL
    ]
    parsed = parse_probe_log(log)
    expect(parsed["malformed"] == 1 and parsed["dropped"] == 4, "truncated line tolerated and drops counted")
    stall = phase_stall(parsed, 1_000_000, 1_100_000)
    expect(stall["lateness_us"] == 100_000, "a stall that began before the phase is attributed to it")
    quiet = phase_stall(parsed, 5_000_000, 6_000_000)
    expect(quiet["below_threshold"] and quiet["lateness_us"] == 1000, "a quiet phase reports the late threshold as its bound")
    itf = input_to_frame(parsed, [{"event": "key-press", "inject_us": 1_000_000}], 8_333)
    expect(itf[0]["latency_us"] == 62_000 and itf[0]["frames"] == 8, "input-to-frame counts from the injection, not the handler")
    expect(im_summary(parsed, 0, 2_000_000)["max_surrounding_bytes"] == 250_000, "IM surrounding bytes recorded")
    late_frame = input_to_frame(parsed, [{"event": "key-press", "inject_us": 1_000_000}], 8_333, horizon_us=1_055_000)
    expect(late_frame[0]["status"] == "no-frame", "a frame after the phase is not the input's frame")

    # Noise rule.
    expect(is_noisy([3_000, 3_100, 9_000]), "max > 2 x median is noisy")
    expect(not is_noisy([3_000, 3_100, 5_000]), "a modest spread is not noisy")
    rows_noise = [
        {"case": "c", "variant": "plain", "outcome": "ok", "phases": [{"name": "scroll", "stall": {"lateness_us": v}}]} for v in (3_000, 3_100, 9_000)
    ]
    mark_noisy(rows_noise)
    expect(all(r.get("noisy") for r in rows_noise), "noisy rows are flagged")

    # The GtkGizmo warning was a pre-existing blocker; it is fixed (A37), so it
    # is a regression that fails the lane, never an owned finding.
    gizmo = "(lushtext:1): Gtk-WARNING **: 20:00:13.364: Trying to snapshot GtkGizmo 0x55596f2a1e80 without a current allocation"
    expect(not any(d["pattern"].search(gizmo) for d in KNOWN_DEFECTS), "the GtkGizmo snapshot warning is no longer an owned finding")
    gizmo_match = classify_warning(gizmo, "paragraph/500/css/txt/wrap-off/minimap-off/plain/rep0")
    expect(gizmo_match is not None and gizmo_match[0] == "regression" and gizmo_match[1]["id"] == "gizmo-snapshot-without-allocation", "the GtkGizmo snapshot warning is a fixed-defect regression")
    minimap = "(lushtext:1): Gtk-CRITICAL **: gtk_scrollable_get_vadjustment: assertion 'GTK_IS_SCROLLABLE (scrollable)' failed"
    minimap_match = classify_warning(minimap, "paragraph/50000/css/txt/wrap-off/minimap-on/plain/rep0")
    expect(minimap_match is not None and minimap_match[0] == "finding", "an owned defect is still a finding")
    expect(classify_warning("(lushtext:1): Gtk-WARNING **: something new", "c") is None, "an unknown warning stays unclassified")
    clean_run: dict[str, Any] = {"driver_failures": []}
    expect(not run_failed([], {"violations": []}, clean_run, {"regressions": {}, "known_findings": {"x": {}}}), "an owned finding does not fail the lane")
    expect(run_failed([], {"violations": []}, clean_run, {"regressions": {"gizmo-snapshot-without-allocation": {}}}), "a fixed-defect regression fails the lane")

    # Storage contamination.
    expect(storage_status({"verdict": ["Storage was NOT a bottleneck: ..."]}) == "clean", "clean verdict")
    expect(storage_status({"verdict": ["Writeback stall: ..."]}) == "contaminated", "a stall verdict contaminates")
    expect(storage_status({"verdict": ["INCOMPLETE DATA ..."]}) == "incomplete", "incomplete data is not a verdict")
    expect(storage_status(None) == "unavailable", "missing tool is unavailable")

    # Derivation on synthetic rows.
    budgets = dict(DEFAULT_BUDGETS)
    rows = synthetic_rows()
    obs = {"A31": {"measured": {"rows_per_batch_nowrap": "105", "line_height": "19"}}}
    derived = derive(rows, budgets, 1_000, True, obs)
    expect(derived["L_syntax"]["value"] == 10_000 and derived["L_syntax"]["deciding_flavor"] == "script", f"L_syntax: the worst flavour decides ({derived['L_syntax']})")
    expect(derived["L_syntax"]["per_flavor"]["css"]["value"] == 50_000, "L_syntax per flavour keeps the css value")
    expect(derived["N_visible"]["value"] == 50_000, f"N_visible ({derived['N_visible']})")
    expect(derived["L_minimap"]["value"] == 50_000, f"L_minimap ({derived['L_minimap']})")
    expect(derived["L_protect"]["value"] == 10_000, f"L_protect needs caret stalls and 2 frames ({derived['L_protect']})")
    expect(derived["L_protect_stall"]["value"] == 50_000, f"L_protect_stall reads the stall half only ({derived['L_protect_stall']})")
    expect(derived["S_syntax"]["value"] == 1_000_000, f"S_syntax ({derived['S_syntax']})")
    expect(derived["V_batch"]["value"] == 2_000 * 105, f"V_batch ({derived['V_batch']})")
    expect(derived["R_batch"]["value"] == 210 and derived["N_dense"]["value"] == 2_000 * 105 // 210, "R_batch and N_dense")
    expect(derived["N_inspect_bound"]["value"] == 10_000, "N_inspect bound is min(N_visible, L_protect)")
    expect(derived["budget_verdict"]["verdict"] == "confirmed", f"baseline fits the budgets ({derived['budget_verdict']})")
    contaminated = [dict(r, storage_contaminated=True) if r["group"] == "paragraph" and r["length"] == 10_000 else r for r in rows]
    derived_c = derive(contaminated, budgets, 1_000, True, obs)
    expect(derived_c["N_visible"]["value"] == 50_000 and derived_c["L_syntax"]["per_flavor"]["css"]["value"] == 50_000, "contaminated cases are excluded")
    no_rd = derive(rows, budgets, 1_000, False, obs)
    expect(no_rd["L_protect"]["value"] is None and "remote-desktop" in no_rd["L_protect"]["reason"], "L_protect needs input injection")
    slow = [dict(r) for r in rows]
    for row in slow:
        if row["group"] == "baseline":
            row["phases"] = [dict(p, stall={"lateness_us": 20_000}) if p["name"] == "scroll" else p for p in row["phases"]]
    revised = derive(slow, budgets, 1_000, True, obs)["budget_verdict"]
    expect(revised["verdict"] == "revise" and revised["proposed"]["B_INTERACT_US"] == 22_000, f"an over-budget baseline proposes a revision ({revised})")

    # merge-runs: a sweep in parts joins into one artifact set; repetitions
    # accumulate, and each row carries its own part's storage verdict.
    with tempfile.TemporaryDirectory(prefix="lt-resp-merge-") as raw:
        root = Path(raw)
        for part, (cases_rows, contaminated) in enumerate(((["a", "b"], "b@plain#0"), (["b"], None))):
            src = root / f"part{part}"
            (src / "storage").mkdir(parents=True)
            (src / "cases" / "g" / f"p{part}" / "plain" / "rep0").mkdir(parents=True)
            (src / "run.json").write_text(json.dumps({"run_id": f"rr-{part}", "wall_start": 10 + part, "wall_end": 20 + part, "remote_desktop": {"available": True}}), encoding="utf-8")
            (src / "results.jsonl").write_text("".join(json.dumps({"case": c, "variant": "plain", "rep": 0, "part": part}) + "\n" for c in cases_rows), encoding="utf-8")
            (src / "storage" / "cases.json").write_text(json.dumps({contaminated: {"status": "contaminated"}} if contaminated else {}), encoding="utf-8")
            (src / "integrity.json").write_text(json.dumps({"checks": 3, "violations": []}), encoding="utf-8")
            (src / "fixtures.json").write_text("[]", encoding="utf-8")
        merge_runs(root / "merged", [root / "part0", root / "part1"], quiet=True)
        merged_rows = [json.loads(line) for line in (root / "merged/results.jsonl").read_text(encoding="utf-8").splitlines()]
        b_rows = sorted((r for r in merged_rows if r["case"] == "b"), key=lambda r: r["rep"])
        expect(len(merged_rows) == 3 and [r["rep"] for r in b_rows] == [0, 1] and b_rows[0]["storage_contaminated"] and not b_rows[1].get("storage_contaminated"), f"repetitions accumulate across parts with their own storage verdicts ({merged_rows})")
        merged_run = json.loads((root / "merged/run.json").read_text(encoding="utf-8"))
        expect(merged_run["run_id"] == "rr-0+rr-1" and merged_run["wall_start"] == 10 and merged_run["wall_end"] == 21, "merged run id and window")
        expect((root / "merged/cases/part01/g/p1/plain/rep0").is_symlink(), "case directories are linked into the merged set, one directory per part")

    # Key guard: no Control+letter chord can be injected.
    class FakeRd(RemoteDesktop):
        def __init__(self) -> None:  # noqa: D401 - no bus
            self.session = "fake"
            self.error = None
            self.sent: list[int] = []

        def _call(self, method: str, params) -> None:
            # No bus in the self-test: the guards under test run before any call.
            pass

        def _notify_key(self, keysym: int, pressed: bool) -> None:
            # Accept the key without a bus; the allow-list check already ran.
            pass

    # A capped phase on the "with" side is a failure, never a ~0 difference
    # "within" (quick run #4 read L_syntax = N_visible = 250000 from pairs
    # whose html and txt sides both hit the cap).
    def capped(row: dict, names: tuple[str, ...]) -> dict:
        row = dict(row, outcome="exceeded_cap")
        row["phases"] = [dict(p, status="exceeded_cap", stall={"lateness_us": 20_000_000, "lower_bound_only": True}) if p["name"] in names else p for p in row["phases"]]
        return row

    capped_rows = [capped(r, ("scroll",)) if r["group"] == "paragraph" and r["length"] == 50_000 and r["attrs"].get("flavor") == "css" else r for r in rows]
    derived_cap = derive(capped_rows, budgets, 1_000, True, obs)
    expect(derived_cap["L_syntax"]["per_flavor"]["css"]["value"] == 10_000, f"a capped html phase fails L_syntax even when txt is capped too ({derived_cap['L_syntax']['per_flavor']['css']})")
    expect(derived_cap["L_minimap"]["value"] == 10_000, f"a capped minimap-on phase fails L_minimap ({derived_cap['L_minimap']})")
    expect(derived_cap["N_visible"]["value"] == 10_000, f"a capped plain scroll fails N_visible ({derived_cap['N_visible']})")

    # A capped phase on the baseline side only (txt, minimap off) leaves the
    # difference unbounded: that phase gives no verdict either way.
    base_capped = [capped(r, ("scroll", "open")) if r["group"] == "paragraph" and r["length"] == 50_000 and r["attrs"].get("ext") == "txt" and not r["attrs"].get("minimap") and r["attrs"].get("flavor") == "css" else r for r in rows]
    derived_base = derive(base_capped, budgets, 1_000, True, obs)
    expect(derived_base["L_minimap"]["value"] == 10_000 and derived_base["L_minimap"]["first_failing"] == 250_000, f"a length whose minimap-off side hung is unmeasured, not passing ({derived_base['L_minimap']})")

    # S_syntax is syntax-attributable (.html minus the same bytes as .txt):
    # the post-open stall a 400-byte .txt shows too (12-20 ms on the
    # reference run) is not background highlighting.
    generic = []
    for r in rows:
        r = dict(r)
        if r["group"] == "total-size":
            r["phases"] = [dict(p, stall={"lateness_us": p["stall"]["lateness_us"] + 15_000}) if p["name"] == "background" else p for p in r["phases"]]
        generic.append(r)
    expect(derive(generic, budgets, 1_000, True, obs)["S_syntax"]["value"] == 1_000_000, "a stall the .txt counterpart shows too does not fail S_syntax")
    fixed = []
    for r in rows:
        r = dict(r)
        if r["group"] == "total-size" and r["attrs"].get("ext") == "html":
            r["phases"] = [dict(p, stall={"lateness_us": p["stall"]["lateness_us"] + 40_000}) if p["name"] == "background" else p for p in r["phases"]]
        fixed.append(r)
    fixed_derived = derive(fixed, budgets, 1_000, True, obs)
    expect(fixed_derived["S_syntax"]["value"] is None and fixed_derived["S_syntax_interact"]["value"] == 5_000_000, f"a size-independent background stall fails S_syntax; S_syntax_interact reads the interactions only ({fixed_derived['S_syntax_interact']})")

    # V_batch reads validation, not the post-open stall every document shows
    # (baseline/small 12-20 ms on the reference run): the dense background
    # phase is read as its excess over the baseline/small background stall.
    post_open = []
    for r in rows:
        r = dict(r)
        if r["group"] in ("dense", "baseline"):
            r["phases"] = [dict(p, stall={"lateness_us": p["stall"]["lateness_us"] + 15_000}) if p["name"] == "background" else p for p in r["phases"]]
        post_open.append(r)
    expect(derive(post_open, budgets, 1_000, True, obs)["V_batch"]["value"] == 2_000 * 105, "the post-open stall the baseline shows too does not fail V_batch")

    # A phase a reduced run chose not to measure is not a hang.
    partial = dict(rows_for(rows, "paragraph", 10_000, ext="txt", wrap=True, minimap=False, flavor="css")[0], outcome="exceeded_cap", only_phases=["click", "type-start"])
    partial["phases"] = [p for p in partial["phases"] if p["name"] != "scroll"]
    expect(not hung([partial], ("scroll",)) and hung([dict(partial, only_phases=None)], ("scroll",)), "an unmeasured phase is not a hang; a phase lost to a hang is")

    # Reduced sweeps choose their own probe points.
    expect(sweep_lengths("full", "paragraph=3000,7000;invisibles=5000")["paragraph"] == (3_000, 7_000) and sweep_lengths("full", "")["dense"] == DENSE_LINE_LENGTHS["full"], "RESPONSIVENESS_LENGTHS replaces a group's grid")

    # The settle half of L_syntax and L_minimap reads open AND background:
    # the context engine's 2000 ms cut-off fires after load readiness.
    settle_rows = []
    for r in rows:
        r = dict(r)
        if r["group"] == "paragraph" and r["length"] == 50_000 and r["attrs"].get("ext") == "html" and r["attrs"].get("flavor") == "css":
            r["phases"] = r["phases"] + [{"name": "background", "kind": "settle", "status": "ok", "stall": {"lateness_us": 2_100_000}}]
        elif r["group"] == "paragraph" and r["length"] == 50_000 and r["attrs"].get("minimap"):
            r["phases"] = r["phases"] + [{"name": "background", "kind": "settle", "status": "ok", "stall": {"lateness_us": 300_000}}]
        elif r["group"] == "paragraph":
            r["phases"] = r["phases"] + [{"name": "background", "kind": "settle", "status": "ok", "stall": {"lateness_us": 3_000}}]
        settle_rows.append(r)
    derived_settle = derive(settle_rows, budgets, 1_000, True, obs)
    expect(derived_settle["L_syntax"]["per_flavor"]["css"]["value"] == 10_000, f"a background-phase syntax stall fails L_syntax ({derived_settle['L_syntax']['per_flavor']['css']})")
    expect(derived_settle["L_minimap"]["value"] == 10_000, f"a background-phase minimap stall fails L_minimap ({derived_settle['L_minimap']})")

    # Input-to-frame attribution (quick run #4: 102 ms / 13 frames on the
    # empty document's Ctrl+Home). The Control press is its own input
    # record and must not stand in for Home; and a frame that starts after
    # the NEXT input was handled, while the loop was free in between, is
    # not the earlier input's frame: that input changed nothing visible.
    fake_inj: list[dict] = []
    fake_rd = FakeRd()
    fake_rd.tap(KEY_HOME, fake_inj, control=True)
    expect([e.get("measured", True) for e in fake_inj] == [False, True], f"a modifier press is recorded as an unmeasured injection ({fake_inj})")
    home_inject = fake_inj[-1]["inject_us"]
    attribution_log = [
        json.dumps({"kind": "start", "t_us": 0, "version": 1, "tick_us": 2000, "late_threshold_us": 1000}),
        json.dumps({"kind": "input", "t_us": home_inject - 300, "window": 1, "event": "key-press", "event_time_ms": 0}),
        json.dumps({"kind": "input", "t_us": home_inject + 500, "window": 1, "event": "key-press", "event_time_ms": 0}),
        json.dumps({"kind": "input", "t_us": home_inject + 100_500, "window": 1, "event": "key-press", "event_time_ms": 0}),
        json.dumps({"kind": "frame", "t_us": 1, "window": 1, "frame_counter": 1, "frame_time_us": home_inject + 101_000, "presentation_us": home_inject + 102_000, "refresh_interval_us": 8333, "source": "presentation"}),
    ]
    right = {"event": "key-press", "inject_us": home_inject + 100_000}
    attributed = input_to_frame(parse_probe_log(attribution_log), fake_inj + [right], 8_333)
    expect(len(attributed) == 2, f"unmeasured injections produce no result ({attributed})")
    expect(attributed[0].get("status") == "no-visible-change", f"a frame after the next handled input is not this input's frame ({attributed[0]})")
    expect(attributed[1].get("latency_us") == 2_000, f"the next input keeps its own frame ({attributed[1]})")
    stalled_log = attribution_log + [json.dumps({"kind": "late", "t_us": home_inject + 100_400, "ready_us": home_inject + 2_000, "lateness_us": 98_400})]
    stalled = input_to_frame(parse_probe_log(stalled_log), fake_inj + [right], 8_333)
    expect(stalled[0].get("latency_us") == 102_000 - 0, f"while the loop was stalled the late frame still counts ({stalled[0]})")

    # Headless Mutter presents a lone frame after an idle period only when
    # the next commit arrives (verify-baseline: a frame started 1 ms after
    # its key press was "presented" 52 ms later, right after the next
    # frame started). A presentation more than two reference frames after
    # the frame started, while the main loop was free, is the compositor's
    # deferral: the frame counts as presented one reference frame after it
    # started. A long app paint is a main-loop stall, so it still counts.
    deferred_log = [
        json.dumps({"kind": "start", "t_us": 0, "version": 1, "tick_us": 2000, "late_threshold_us": 1000}),
        json.dumps({"kind": "input", "t_us": 10_000_600, "window": 1, "event": "key-press", "event_time_ms": 0}),
        json.dumps({"kind": "frame", "t_us": 1, "window": 1, "frame_counter": 1, "frame_time_us": 10_001_000, "presentation_us": 10_052_000, "refresh_interval_us": 8333, "source": "presentation"}),
    ]
    deferred = input_to_frame(parse_probe_log(deferred_log), [{"event": "key-press", "inject_us": 10_000_000}], 8_333)
    expect(deferred[0].get("latency_us") == 9_333 and deferred[0].get("source") == "deferred-presentation", f"a compositor-deferred presentation counts one reference frame after the frame start ({deferred[0]})")
    painted = input_to_frame(parse_probe_log(deferred_log + [json.dumps({"kind": "late", "t_us": 10_050_000, "ready_us": 10_002_000, "lateness_us": 48_000})]), [{"event": "key-press", "inject_us": 10_000_000}], 8_333)
    expect(painted[0].get("latency_us") == 52_000, f"a presentation delayed by a stall keeps its real time ({painted[0]})")

    # Frame gaps: only gaps between presented frames, and an idle gap (no
    # frame requested, the loop free) is not a dropped frame.
    gap_log = [json.dumps({"kind": "start", "t_us": 0, "version": 1, "tick_us": 2000, "late_threshold_us": 1000})]
    t = 1_000_000
    for i in range(40):
        gap_log.append(json.dumps({"kind": "frame", "t_us": 1, "window": 1, "frame_counter": i, "frame_time_us": t, "presentation_us": t + 8_333, "refresh_interval_us": 8333, "source": "presentation"}))
        t += 8_333
    for i in range(40, 44):
        t += 16_667 - 8_333
        gap_log.append(json.dumps({"kind": "frame", "t_us": 1, "window": 1, "frame_counter": i, "frame_time_us": t, "presentation_us": None, "refresh_interval_us": None, "source": "frame-time"}))
        t += 8_333
    t += 8_333
    gap_log.append(json.dumps({"kind": "frame", "t_us": 1, "window": 1, "frame_counter": 50, "frame_time_us": t, "presentation_us": t + 8_333, "refresh_interval_us": 8333, "source": "presentation"}))
    t += 200_000
    gap_log.append(json.dumps({"kind": "frame", "t_us": 1, "window": 1, "frame_counter": 51, "frame_time_us": t, "presentation_us": t + 8_333, "refresh_interval_us": 8333, "source": "presentation"}))
    gaps = frame_gap_p95(parse_probe_log(gap_log), 0, t + 100_000)
    expect(gaps["p95_gap_us"] == 8_333, f"unpresented and idle gaps are not frame gaps ({gaps})")
    expect(gaps.get("unpresented_frames") == 4 and gaps.get("idle_gaps") == 1, f"unpresented frames and idle gaps are counted ({gaps})")
    stall_gap = frame_gap_p95(parse_probe_log(gap_log + [json.dumps({"kind": "late", "t_us": t - 1_000, "ready_us": t - 150_000, "lateness_us": 149_000})]), 0, t + 100_000)
    expect(stall_gap["max_gap_us"] is not None and stall_gap["max_gap_us"] > 100_000, f"a gap with a stall inside it counts ({stall_gap})")

    # Data integrity, failing first: a driver-side append outside the reload
    # case must fail the check; a recorded rewrite must pass it.
    with tempfile.TemporaryDirectory(prefix="lt-resp-selftest-") as raw:
        fixture = Path(raw) / "fixture.txt"
        fixture.write_bytes(b"original\n")
        ledger = IntegrityLedger()
        ledger.record(fixture)
        expect(ledger.verify([fixture], "unchanged"), "an untouched fixture passes")
        with fixture.open("ab") as handle:
            handle.write(b"rogue append\n")
        expect(not ledger.verify([fixture], "rogue append") and ledger.violations, "an unrecorded append fails the integrity check")
        data = b"rewritten by the reload case\n"
        fixture.write_bytes(data)
        ledger.record(fixture, hashlib.sha256(data).hexdigest())
        before = len(ledger.violations)
        expect(ledger.verify([fixture], "reload") and len(ledger.violations) == before, "a rewrite the driver recorded passes")

    fake = FakeRd()
    try:
        fake.tap(ord("s"), [], control=True)
        failures.append("Control+s must be refused")
    except RuntimeError:
        pass
    try:
        fake.key(0xFFBF, True)  # F2: not on the allow-list
        failures.append("an unlisted keysym must be refused")
    except RuntimeError:
        pass

    # Case table and filters.
    quick = build_cases("quick")
    full = build_cases("full")
    expect(not any(c.group == "install" for c in quick) and any(c.group == "install" for c in full), "install runs in full only")
    expect(len(select_cases(full, ["paragraph"])) == 5 * (8 + 4), "group filter selects the paragraph sweep (css matrix + script syntax series)")
    expect({c.group for c in full} == set(GROUPS), "every group has cases")
    expect(read_budgets(Path("/nonexistent"))["B_INTERACT_US"] == 8_000, "budget defaults")
    expect(length_verdict({"value": 10_000}, 10_000) is True, "a derived value at the length passes it")
    expect(length_verdict({"value": None, "upper_bound": 10_000}, 10_000) is False, "an upper bound fails the length")
    expect(length_verdict({"value": None, "reason": "no usable cases"}, 10_000) is None, "no usable cases is undecided")
    expect(hasattr(load_generator(), "generate"), "the fixture generator loads as a module")

    if failures:
        print("responsiveness-smoke-driver self-test failed:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    print("responsiveness-smoke-driver self-test passed.")
    return 0


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("session", "run"):
        p = sub.add_parser(name)
        p.add_argument("--artifact-dir", type=Path, required=True)
        p.add_argument("--binary", type=Path, required=True)
        p.add_argument("--tier", choices=("quick", "full"), default="quick")
        p.add_argument("--case", action="append")
    merge = sub.add_parser("merge-runs")
    merge.add_argument("--artifact-dir", type=Path, required=True)
    merge.add_argument("--from", dest="sources", type=Path, action="append", required=True)
    report = sub.add_parser("report")
    report.add_argument("--artifact-dir", type=Path, required=True)
    report.add_argument("--axiom-observations", type=Path)
    listing = sub.add_parser("list-cases")
    listing.add_argument("--tier", choices=("quick", "full"), default="quick")
    listing.add_argument("--case", action="append")
    sub.add_parser("self-test")
    affinity = sub.add_parser("affinity-list", help="the first N CPUs of this process's affinity, as a taskset list")
    affinity.add_argument("max_cpus", type=int)
    listener = sub.add_parser("atspi-listener")
    listener.add_argument("--output", type=Path, required=True)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    if args.command == "self-test":
        return self_test()
    if args.command == "list-cases":
        for case in select_cases(build_cases(args.tier), args.case or []):
            print(case.id)
        return 0
    if args.command == "affinity-list":
        print(compress_cpus(sorted(os.sched_getaffinity(0))[: args.max_cpus]))
        return 0
    if args.command == "atspi-listener":
        return atspi_listener_main(args.output)
    if args.command == "merge-runs":
        return merge_runs(args.artifact_dir.resolve(), args.sources)
    if args.command == "report":
        return write_report(args.artifact_dir.resolve(), args.axiom_observations)
    args.artifact_dir = args.artifact_dir.resolve()
    args.binary = args.binary.resolve()
    (args.artifact_dir / "logs").mkdir(parents=True, exist_ok=True)
    if args.command == "session":
        return session_main(args)
    return run_main(args)


if __name__ == "__main__":
    raise SystemExit(main())
