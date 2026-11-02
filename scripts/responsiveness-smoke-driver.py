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
``merge-runs`` join several runs' artifacts for one report; ``--from
             LABEL=DIR`` labels a run's rows with a side (``baseline``,
             ``branch``) for the C6 interleaved verdict.
``affinity-list`` the first N CPUs of the inherited affinity, as a
             ``taskset -c`` list (the wrapper's CPU cap).
``self-test`` exercise the parsing, classification, noise, contamination,
             derivation and data-integrity logic on synthetic records, without
             GTK or a compositor.

Post-open cases (bound-post-open-settle, C6) wait in ``background`` until the
settle the driver computes from the probe's ``post_open_*`` spans,
``highlight_updated``/``adjustment_upper`` marks and ``settle_sentinel``
records (the clauses of the app's ``document-settled`` predicate, which the
baseline build lacks), then run ``type-during-settle`` in a launch of its own
and, on the dense cases, ``wrap-toggle``, ``zoom-step`` and ``font-change``.
Each repetition's row carries its post-open settle tables (``post_open``);
the report adds the table and the C6 verdict.

The probe JSONL contract is v1 (see ``KNOWN_KINDS``); every ``t_us`` is
``CLOCK_MONOTONIC`` in microseconds, the same clock as ``time.monotonic_ns``.
"""

from __future__ import annotations

import argparse
import bisect
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
KNOWN_KINDS = ("start", "late", "second", "frame", "input", "im", "dropped", "topk", "span", "mark")
# Where an input arrived (`investigate-first-keys-without-frames`): the probe
# stamps each key with the window's focus widget and each button with the
# widget under the pointer. A key typed into either editor target reached a
# document; only an editable one inserts.
EDITOR_TARGETS = ("editor", "read-only-editor")
# The probe's mark after each insertion an editor buffer makes inside a user
# action (typing), never a load or replacement install.
EDITOR_USER_INSERT_MARK = "editor_user_insert"
# How long a click waits for the probe's record of where it arrived.
CLICK_DELIVERY_TIMEOUT_S = 2.0
# Pointer moves an editor click may spend correcting its aim by where the
# probe saw the pointer arrive (a power of two; one correction sufficed).
POINTER_AIM_ATTEMPTS = 4
# How long an editor click waits for the snapshot to show a free point of the
# text view: right after a launch the populated profile's layout (sidebar and
# properties bottom sheet) may not have settled, and a blind fallback point
# lands in the sidebar.
EDITOR_POINT_WAIT_S = 3.0

# Keysyms. Only these may be injected; the guard refuses any Control+letter so
# no save chord can ever reach the app.
KEY_RIGHT = 0xFF53
KEY_PAGE_DOWN = 0xFF56
KEY_HOME = 0xFF50
KEY_END = 0xFF57
KEY_ESCAPE = 0xFF1B
KEY_CONTROL_L = 0xFFE3
KEY_SHIFT_L = 0xFFE1
BTN_LEFT = 0x110
ALLOWED_KEYSYMS = {KEY_RIGHT, KEY_PAGE_DOWN, KEY_HOME, KEY_END, KEY_ESCAPE, KEY_CONTROL_L, KEY_SHIFT_L} | set(
    range(0x61, 0x7B)
)
TYPE_BURST = "abcdefghij"

# Sweep definition (design D7).
PARAGRAPH_LENGTHS = {"quick": (10_000, 250_000), "full": (2_000, 10_000, 50_000, 250_000, 1_900_000)}
DENSE_LINE_LENGTHS = {"quick": (2_000,), "full": (120, 500, 2_000, 10_000, 50_000)}
DENSE_LINE_COUNT = 160
# The dense reference case: a full screen of short dense lines. V_batch reads
# the other dense lengths' scroll and Page Down as their excess over it, so the
# cost of painting a screen of text is not read as validation cost.
DENSE_REFERENCE_LENGTH = 120
TOTAL_SIZES = {"quick": (1_000_000, 5_000_000), "full": (1_000_000, 2_000_000, 5_000_000, 10_000_000)}
DELIMITER_LONGEST = {"quick": 250_000, "full": 1_894_586}
SESSION_LENGTH = {"quick": 250_000, "full": 1_900_000}
# The keystroke group (long-line-protected-view, C2; kept by
# explore-editing-long-paragraphs, C5): one editable paragraph of the swept
# length between short lines, TYPE_BURST typed at its start, middle and end,
# and in the short paragraph right after it (C5: typing there must be
# unaffected by anything the long paragraph turns off). It re-measures the
# toolkit-floor exception: keystrokes in editable paragraphs between
# L_EDIT_STALL (3,500) and L_PROTECT_EDIT (50,000) miss B_interact (ledger
# A23, A26), which C5 reduces. Quick brackets the exception; full adds the
# points around L_EDIT_STALL and inside the range, and C5's short lengths. The
# top length leaves room for the typed bursts: a paragraph the bursts push past
# L_PROTECT_EDIT enters Protected View mid-case and stops taking keys (the C2
# verdict's 49,990 case did), so it would no longer measure the editable
# exception; for the same reason C5's pre-C2 sweep points above 50,000 are not
# editable lengths any more.
L_PROTECT_EDIT = 50_000
KEYSTROKE_EDGE = 49_950
KEYSTROKE_LENGTHS = {"quick": (3_500, 10_000, KEYSTROKE_EDGE), "full": (500, 1_000, 2_000, 2_500, 3_500, 5_000, 10_000, 20_000, KEYSTROKE_EDGE)}
KEYSTROKE_SHAPES = (("css", "html"), ("prose", "txt"))
KEYSTROKE_POSITIONS = ("start", "middle", "end", "short")
KEYSTROKE_GAP_S = float(os.environ.get("RESPONSIVENESS_KEYSTROKE_GAP_S", "0.15"))
INVISIBLES_LENGTHS = {"quick": (250_000,), "full": (2_000, 10_000, 50_000, 250_000, 1_900_000)}
INSTALL_SIZES = (100_000_000, 250_000_000, 500_000_000)
EVICTION_FILE_BYTES = 8 * 1024 * 1024
REPETITIONS = {"quick": 1, "full": 3}
GROUPS = ("baseline", "paragraph", "dense", "total-size", "delimiters", "session", "eviction", "reload", "install", "invisibles", "markdown", "editorconfig", "recovery", "keystroke")
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
    # Protected View's reading surfaces (protected-view-inspector-and-surfaces):
    # paging the Line Inspector is an interaction; Copy Whole Line's chunked
    # clipboard writes, served while an external reader pastes, are settles.
    "inspect": "interaction",
    "copy-line": "settle",
    "reselect": "interaction",
    "idle": "calibration",
    # The post-open phases (bound-post-open-settle, C6) have their own kinds,
    # so C0's budget verdict, which reads `interaction` and `settle`, is
    # unchanged by them; the C6 verdict reads their post-open entries.
    "type-during-settle": "post-open-typing",
    "wrap-toggle": "post-open-restyle",
    "zoom-step": "post-open-restyle",
    "font-change": "post-open-restyle",
    "tab-settle": "post-open-tab",
    # The keystroke group: placing the caret (find the marker, close the
    # search bar, collapse the selection) and proving where the bursts
    # landed are navigation, not measured interactions; each burst is.
    "place-start": "navigation",
    "place-middle": "navigation",
    "place-end": "navigation",
    "key-start": "interaction",
    "key-middle": "interaction",
    "key-end": "interaction",
    "place-short": "navigation",
    "key-short": "interaction",
    "verify": "navigation",
}
# The C6 cases (design D8): a ~200 kB Markdown document with the side-by-side
# preview, an EditorConfig tab width other than the setting's on the dense 710
# document, three 16 kB documents restored from the session, and two 64 kB
# file-backed drafts recovered at startup.
MARKDOWN_PREVIEW_BYTES = 200_000
EDITORCONFIG_LINE_LENGTH = 710
EDITORCONFIG_TAB_WIDTH = 8
EDITORCONFIG_SETTING_TAB_WIDTH = 4
SESSION_SMALL_BYTES = 16_000
RECOVERY_DRAFT_BYTES = 64_000
RECOVERY_DRAFTS = 2
RECOVERY_TYPED = "draft"
CARET_PHASES = ("click", "type-start", "type-middle", "type-end")
# Window construction (open, restore) is owned by C4 and left out of stall verdicts.
STALL_EXCLUDED_PHASES = ("open", "restore")

# Window construction (bound-window-construction-dispatch, C4). The baseline
# small case runs on all four settings profiles: a fresh profile (nothing
# persisted, so the split-view migration runs) and a steady-state one (a
# previous launch persisted and quit), under the keyfile and the dconf
# GSettings backends. The probe's `span` records name these top-level spans;
# every other span name is a construction stage.
CONSTRUCTION_PROFILES = ("fresh", "steady")
CONSTRUCTION_BACKENDS = ("keyfile", "dconf")
# The populated profile (shorten-window-content-fill-in, design D4): a steady
# keyfile profile with a restored workspace folder of a few hundred synthetic
# entries and both side panes shown, so the reveal frame carries the sidebar
# and the properties panel. The other profiles show neither pane.
POPULATED_CASE_ID = "baseline/small/steady-populated"
POPULATED_DIRECTORIES = 20
POPULATED_FILES = 280
CONSTRUCTION_SPANS = ("startup_toolkit", "startup_lushtext", "window_construction", "first_frame")
# The LushText-owned spans the settle budget applies to; GTK's own startup
# (`startup_toolkit`) is reported, not budgeted, and the first-frame span is
# several dispatches, so only the largest dispatch before it is budgeted.
CONSTRUCTION_BUDGETED = ("startup_lushtext", "window_construction", "largest_dispatch_before_first_frame", "largest_composition_dispatch")
# Staged window composition (first-frame-window-shell, C4b): the window is
# built whole and revealed in bounded dispatches. `composition_complete` spans
# from the start of the window work until the last stage's frame painted; each
# `composition_<stage>` span is one stage's dispatch. The probe also records the
# layout and paint phases of each window's first frames and the window's first
# measures and allocations; none of these is a construction stage.
COMPOSITION_COMPLETE = "composition_complete"
# The first frame painted after the activation's file opens ran
# (shorten-window-content-fill-in, C4c): the opened document on screen.
COMPOSITION_ACTIVATION_SHOWN = "composition_activation_shown"
# The present stage's span. The first `frame_paint` that starts after it ended
# is the first frame painted on the mapped window in every build, including
# one that ran an unseen frame on the unmapped window first (ledger A51), so
# main and a branch compare alike.
COMPOSITION_PRESENT = "composition_present"
COMPOSITION_STAGE_PREFIX = "composition_"
FRAME_PHASE_SPANS = ("frame_layout", "frame_paint")
LAYOUT_PROBE_SPANS = ("window_measure_h", "window_measure_v", "window_size_allocate")
# How many of the first frames the report tabulates (the probe records four).
FRAME_PHASE_FRAMES = 4
# Dispatches at least this long are listed in time order in the report.
LISTED_DISPATCH_US = 4_000
# The content fill-in the maintainer accepted after the first frame
# (first-frame-window-shell D7, shorten-window-content-fill-in): 40 ms, within
# five 120 Hz reference frames. Every run is judged against it, and a run over
# it fails the lane only under RESPONSIVENESS_CONSTRUCTION_ENFORCE=1. Not a
# budget constant.
ACCEPTED_FILL_IN_US = 40_000
# The fill-in median target, in reference frames (REFERENCE_FRAME_US).
FILL_IN_MEDIAN_TARGET_FRAMES = 3
DCONF_LOAD = Path("/usr/bin/dconf")


def construction_case_id(profile: str, backend: str) -> str:
    """C0's `baseline/small` is the fresh keyfile profile; the other three are suffixed."""
    if (profile, backend) == ("fresh", "keyfile"):
        return "baseline/small"
    return f"baseline/small/{profile}-{backend}"

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


def keystroke_stalls(parsed: dict, start_us: int, end_us: int, b_interact_us: int) -> dict[str, Any]:
    """Per-keystroke stalls of one `key-*` phase (the keystroke group).

    Each key press opens a window that ends at the next press (the last at
    the phase end). Per window: the worst stall, and the summed stall time of
    every late dispatch in it. The sum matters because a keystroke's work can
    land in two dispatches that are each under B_interact (a second text view
    re-laying the paragraph out from an idle) and the worst-stall figure alone
    hides the second one.
    """
    presses = sorted(int(r["t_us"]) for r in parsed["records"]["input"] if r.get("event") == "key-press" and start_us <= int(r["t_us"]) <= end_us)
    worst: list[int] = []
    summed: list[int] = []
    for index, press in enumerate(presses):
        window_end = presses[index + 1] if index + 1 < len(presses) else end_us
        stalls = [int(r["lateness_us"]) for r in overlapping_lates(parsed, press, window_end - 1) if int(r["t_us"]) >= press]
        worst.append(max(stalls, default=0))
        summed.append(sum(stalls))
    return {
        "keys": len(presses),
        "worst_us": worst,
        "summed_us": summed,
        "worst_median_us": nearest_rank(worst, 50),
        "worst_p90_us": nearest_rank(worst, 90),
        "summed_median_us": nearest_rank(summed, 50),
        "over_budget_keys": sum(1 for value in worst if value > b_interact_us),
        "summed_over_budget_keys": sum(1 for value in summed if value > b_interact_us),
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

    A text key that reached an editable editor (`inserts`, target `editor`)
    always changes what is drawn, so that rule never applies to it: its
    latency runs to the eventual frame and the row is flagged
    `withheld_frame`, which fails the lane (`withheld_frame_cases`). So is
    such a key whose first later frame starts only after the phase ended; a
    key after which the log holds no frame at all (the session's last frame,
    which the probe cannot write before its timings are final) stays
    `no-frame` unflagged. A key into a protected, read-only
    editor is flagged `protected`; its latency is reported apart from
    editable typing (`editable_latency`).
    """
    inputs = sorted(parsed["records"]["input"], key=lambda r: r["t_us"])
    frames = sorted(parsed["records"]["frame"], key=lambda r: int(r["frame_time_us"]))
    matched: list[tuple[dict, int | None]] = []
    targets: list[str | None] = []
    used: set[int] = set()
    for injection in injections:
        origin = injection["inject_us"]
        wanted = injection["event"]
        handler = None
        target = None
        for index, record in enumerate(inputs):
            if index in used or record["t_us"] < origin - 2_000:
                continue
            if record.get("event") == wanted:
                handler = int(record["t_us"])
                target = record.get("target")
                used.add(index)
                break
        matched.append((injection, handler))
        targets.append(target)
    results = []
    for position, (injection, handler) in enumerate(matched):
        if not injection.get("measured", True):
            continue
        origin = injection["inject_us"]
        wanted = injection["event"]
        if handler is None:
            # Still a text key: `input_delivery` counts it as undelivered.
            results.append({"event": wanted, "inject_us": origin, "status": "no-input-record", **({"inserts": True} if injection.get("inserts") else {})})
            continue
        # Where the input arrived and whether it was a text key
        # (`input_delivery`); absent from an older probe's records.
        delivery = {k: v for k, v in (("target", targets[position]), ("inserts", injection.get("inserts"))) if v is not None}
        if wanted == "key-press" and targets[position] == "read-only-editor":
            delivery["protected"] = True
        typed_into_editor = bool(injection.get("inserts")) and targets[position] == "editor"
        frame = next((f for f in frames if int(f["frame_time_us"]) >= handler), None)
        # A frame recorded at all after the handler: the probe writes a frame
        # only once it or a later one has its final timings, and headless
        # Mutter completes a lone last frame only at the next commit, so the
        # session's last frame can be missing from the log without having
        # been withheld.
        frame_recorded_later = frame is not None
        if frame is not None and horizon_us is not None and int(frame["frame_time_us"]) > horizon_us:
            # The input changed nothing visible (for example Page Down at the
            # end of a short document); a later unrelated frame is not its frame.
            frame = None
        if frame is None:
            withheld = {"withheld_frame": True} if typed_into_editor and frame_recorded_later else {}
            results.append({"event": wanted, "inject_us": origin, "handler_us": handler, "status": "no-frame", **delivery, **withheld})
            continue
        next_handler = next((h for _inj, h in matched[position + 1 :] if h is not None and h > handler), None)
        if next_handler is not None and int(frame["frame_time_us"]) > next_handler and not loop_stalled(parsed, handler, next_handler, reference_frame_us):
            if typed_into_editor:
                # The insertion is drawn only in this later frame: hidden
                # latency, measured to that frame and flagged.
                delivery["withheld_frame"] = True
            else:
                results.append({"event": wanted, "inject_us": origin, "handler_us": handler, "status": "no-visible-change", **delivery})
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
                **delivery,
            }
        )
    return results


def input_delivery(parsed: dict, input_to_frame_rows: list[dict], start_us: int, end_us: int) -> dict | None:
    """Whether a phase's text keys reached a document (`investigate-first-keys-without-frames`).

    `typed` counts the phase's measured text keys (printable, no Control),
    `undelivered` those with no input record at all (they never reached the
    application), `to_editor` those the probe saw arrive while an editable
    editor had keyboard focus, `elsewhere` those that arrived anywhere else,
    and `inserted` the editor insertions typing made in the phase. A probe
    whose input records carry no targets (an older build) leaves `verified`
    false. `None` when the phase typed no text.
    """
    typed = [row for row in input_to_frame_rows if row.get("inserts")]
    if not typed:
        return None
    delivered = [row for row in typed if row.get("status") != "no-input-record"]
    targets = [row.get("target") for row in delivered]
    verified = all(target is not None for target in targets)
    inserted = sum(1 for r in parsed["records"]["mark"] if r.get("name") == EDITOR_USER_INSERT_MARK and start_us <= int(r["t_us"]) <= end_us)
    return {
        "typed": len(typed),
        "undelivered": len(typed) - len(delivered),
        "to_editor": sum(1 for target in targets if target == "editor"),
        "to_read_only_editor": sum(1 for target in targets if target == "read-only-editor"),
        "elsewhere": sum(1 for target in targets if target is not None and target not in EDITOR_TARGETS),
        "inserted": inserted,
        "verified": verified,
    }


# Surfaces that can lie over the editor's text view (an overlaid workspace
# sidebar in a narrow layout, the document-properties bottom sheet).
EDITOR_COVERS = ("workspace-sidebar", "document-properties")


def editor_point_in(surfaces: list, x_fraction: float = 1 / 3, y_fraction: float = 1 / 2) -> tuple[int, int] | None:
    """`Runner.editor_point` on a snapshot's surfaces (see there)."""
    rects = {s.get("name"): s.get("rect") for s in surfaces if isinstance(s, dict)}
    view = rects.get("source-view")
    if not view or int(view.get("width", 0)) <= 0 or int(view.get("height", 0)) <= 0:
        return None
    covers = [rects[name] for name in EDITOR_COVERS if rects.get(name)]

    def covered(x: int, y: int) -> bool:
        return any(c["x"] <= x < c["x"] + c["width"] and c["y"] <= y < c["y"] + c["height"] for c in covers)

    for fx in (x_fraction, 1 / 2, 2 / 3, 5 / 6):
        for fy in (y_fraction, 1 / 3, 1 / 6):
            x, y = int(view["x"] + view["width"] * fx), int(view["y"] + view["height"] * fy)
            if not covered(x, y):
                return x, y
    return None


def misdirected_phase(delivery: dict | None) -> bool:
    """A phase whose text keys never reached the application (whatever the
    probe's vintage), or a verified phase whose text keys went elsewhere or
    whose keys into an editable editor inserted less than one character each."""
    if not delivery:
        return False
    if delivery.get("undelivered", 0) > 0:
        return True
    if not delivery.get("verified"):
        return False
    return delivery["elsewhere"] > 0 or delivery["inserted"] < delivery["to_editor"]


def editable_latency(entry: dict) -> bool:
    """Whether an input-to-frame row has a latency that counts as editable
    interaction: every measured row except a key into a protected (read-only)
    editor, which is reported apart (`protected`)."""
    return "latency_us" in entry and not entry.get("protected")


def protected_itf_note(phase: dict) -> str:
    """The report-table suffix for keys typed into a protected document,
    which are kept out of the editable input-to-frame figures."""
    protected = phase.get("protected_input_to_frame")
    if not protected:
        return ""
    return f" (protected: {protected['keys']} keys, max {protected['max_us']} us)"


def withheld_frame_cases(rows: list[dict]) -> list[str]:
    """`case@variant#rep:phase` of every phase where a text key typed into an
    editable editor was drawn only in a frame that started after the next
    key was handled, while the main loop was free, or never: the frame was
    withheld, and the delay is latency the user sees."""
    return [
        f"{row['case']}@{row['variant']}#{row['rep']}:{phase.get('name')}"
        for row in rows
        for phase in row.get("phases", [])
        if any(entry.get("withheld_frame") for entry in phase.get("input_to_frame", []))
    ]


def misdirected_cases(rows: list[dict]) -> list[str]:
    """`case@variant#rep:phase` of every phase whose typing reached no document."""
    return [
        f"{row['case']}@{row['variant']}#{row['rep']}:{phase.get('name')}"
        for row in rows
        for phase in row.get("phases", [])
        if misdirected_phase(phase.get("delivery"))
    ]


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
# Post-open settle (bound-post-open-settle, C6; design D1, D8)
# --------------------------------------------------------------------------
#
# The probe's post-open records (crates/lushtext-core/src/services/
# responsiveness_log.rs): one `span` per post-open stage, a `mark` on every
# GtkSourceBuffer `highlight-updated` and every change of the editor's
# vertical adjustment `upper`, and a `settle_sentinel` mark each time the
# G_PRIORITY_LOW sentinel idle dispatches. The driver computes "settled" from
# these with the clauses of the app's `document-settled` predicate
# (ui/editor_page/load/policy.rs `settle_stage`), so the baseline build, which
# has the records but not the predicate, is measured identically (review R24).

SETTLE_POLICY_SOURCE = REPO_ROOT / "crates/lushtext-core/src/ui/editor_page/load/policy.rs"
# `SETTLE_QUIET_US` when the load policy cannot be read.
DEFAULT_SETTLE_QUIET_US = 100_000
POST_OPEN_SPAN_PREFIX = "post_open_"
POST_OPEN_ACTIVATION = "post_open_activation"
HIGHLIGHT_UPDATED_MARK = "highlight_updated"
ADJUSTMENT_UPPER_MARK = "adjustment_upper"
POST_OPEN_LANGUAGE_ATTACH = "post_open_language_attach"
# The dispatches that lay a document out: GtkTextView's validation (the
# vertical upper changing) and the install slices that feed it.
LAYOUT_ATTRIBUTIONS = ("validation", "post_open_install_slice")
# `ATTACH_DEADLINE_US` in crates/lushtext-core/src/ui/editor_page/load/policy.rs:
# the longest a deferred language attach waits for input quiet. The
# no-regression rule exempts that wait, and only it, from
# activation-to-laid-out (`--self-test` checks the two agree).
ATTACH_DEADLINE_US = 2_048_000
SETTLE_SENTINEL_MARK = "settle_sentinel"
# An unspanned dispatch that ends at one of these marks is attributed to it.
MARK_ATTRIBUTION = {HIGHLIGHT_UPDATED_MARK: "highlight", ADJUSTMENT_UPPER_MARK: "validation"}
# How long a post-open phase may wait for the driver-computed settle; a cap
# hit is a failed phase. The 5 MB HTML background analysis ran to 4.3 s on
# main (design, measured breakdown). RESPONSIVENESS_SETTLE_CAP_S overrides it.
SETTLE_CAP_S = 10.0
SETTLE_POLL_S = 0.05
# Settle is only concluded this long before "now", so a record the writer
# thread has not appended yet cannot be missed.
SETTLE_LOG_MARGIN_US = 50_000
# Dispatches at least this long are listed in the post-open settle table.
POST_OPEN_DISPATCH_FLOOR_US = 1_000
# Frames with phase spans in a post-open case (LUSHTEXT_RESPONSIVENESS_FRAME_PHASES):
# ten seconds of 120 Hz frames, the settle cap.
POST_OPEN_FRAME_PHASES = 1_200
# type-during-settle (design D8, review R26): one key every 50 ms from load
# readiness, then stop and wait for settle. 60 keys are three seconds, past
# the 5 MB case's sliced install and language attach (about one second on
# main) while its background highlighting still runs.
SETTLE_TYPING_KEYS = 60
SETTLE_TYPING_INTERVAL_S = 0.05
SETTLE_TYPING_LETTERS = "etaoinshrdlucmfwypvbgkjqxz"
# A GSettings write reaches the app through the backend's file monitor
# (keyfile) or the dconf service, not synchronously: settle is not concluded
# before this long after the write returned.
SETTING_PROPAGATION_US = 1_000_000
# The wrap-toggle, zoom-step and font-change phases (design D9) and the font
# the font change selects.
DENSE_RESTYLE_PHASES = ("wrap-toggle", "zoom-step", "font-change")
FONT_CHANGE_VALUES = {"use-system-font": False, "custom-font": "Monospace 14"}
# Input-to-frame is reported, never gated (coordinator decision 2026-10-30):
# headless Mutter with the cairo renderer double-buffers at 120 Hz, so an
# input waits one frame for the next frame and one more to be presented, plus
# a full-screen paint.
INPUT_TO_FRAME_FLOOR_US = (16_700, 19_300)
FULL_SCREEN_PAINT_US = (5_000, 7_000)
INPUT_TO_FRAME_FLOOR_TEXT = "double-buffered 120 Hz headless presentation 16.7-19.3 ms plus a full-screen paint 5-7 ms (20-25 ms on any document)"
# Toolkit-floor exceptions (design D8, review R7): `attribution=AXIOM[:report]`
# entries separated by `;`, for example
# `post_open_language_attach=A67:.claude/handoffs/c6-upstream-gtksourceview-language-parse.md`.
# None is accepted by default: the maintainer records each one.
TOOLKIT_FLOORS_ENV = "RESPONSIVENESS_TOOLKIT_FLOORS"
DEFAULT_TOOLKIT_FLOORS = ""
# The C6 grid (design D8): total-size 16 kB, 1 MB and 5 MB, dense 120, 500
# and 710. Not in the tiers' grids, whose smallest total size and dense
# lengths C1's derivations read; run it with RESPONSIVENESS_LENGTHS.
C6_LENGTHS = "total-size=16000,1000000,5000000;dense=120,500,710"
# The GIO client `Runner.write_settings` runs: argv is the schema id and a
# JSON object of keys; each value is converted to the key's own type.
SETTINGS_WRITER = """
import json, sys
from gi.repository import Gio, GLib
settings = Gio.Settings.new(sys.argv[1])
settings.delay()
for key, value in json.loads(sys.argv[2]).items():
    settings.set_value(key, GLib.Variant(settings.get_value(key).get_type_string(), value))
settings.apply()
Gio.Settings.sync()
"""
C6_BASELINE_SIDE = "baseline"
C6_BRANCH_SIDE = "branch"
# The post-open entry kinds the B_settle condition and the regression rule read.
POST_OPEN_SETTLE_KINDS = ("open", "tab")


def read_settle_quiet(source: Path = SETTLE_POLICY_SOURCE) -> int:
    """`SETTLE_QUIET_US` from the load policy, the single source of truth."""
    try:
        text = source.read_text(encoding="utf-8")
    except OSError:
        return DEFAULT_SETTLE_QUIET_US
    match = re.search(r"const\s+SETTLE_QUIET_US\s*:\s*\w+\s*=\s*([0-9_]+)\s*;", text)
    return int(match.group(1).replace("_", "")) if match else DEFAULT_SETTLE_QUIET_US


def _post_open_spans(parsed: dict, origin_us: int) -> list[dict]:
    return [r for r in parsed["records"]["span"] if str(r.get("name", "")).startswith(POST_OPEN_SPAN_PREFIX) and int(r["end_us"]) >= origin_us]


def _marks(parsed: dict, origin_us: int, names: Iterable[str]) -> list[int]:
    wanted = set(names)
    return sorted(int(r["t_us"]) for r in parsed["records"]["mark"] if r.get("name") in wanted and int(r["t_us"]) >= origin_us)


def compute_settle(parsed: dict, origin_us: int, floor_us: int, quiet_us: int, until_us: int | None = None, required: Iterable[str] = (), require_activity: bool = False, idle_ok: bool = False) -> dict[str, Any]:
    """The first time T >= floor at which the opened document counts as settled.

    The clauses of the app's `document-settled` predicate, read from the
    records at or before T (records before `origin_us`, another launch's,
    are ignored):
    (a) no `highlight_updated` or `adjustment_upper` mark in (T - quiet, T];
    (b) a `settle_sentinel` at or after the last such mark and after the end
        of the last post-open span (each of them arms the sentinel);
    (c) no post-open span running at T.
    `required` names stages that must have run (the predicate's "language
    attach stage has run" when a case knows it attaches one), and
    `require_activity` waits for the first post-open span (a document open).
    `idle_ok` settles a launch with no document, where nothing ever arms the
    sentinel, once it has been quiet (`baseline/empty`).
    The open completing is the floor: the caller passes the open phase's
    readiness. `until_us` bounds the evaluation (the live poll's "now").
    """
    marks = _marks(parsed, origin_us, MARK_ATTRIBUTION)
    sentinels = _marks(parsed, origin_us, (SETTLE_SENTINEL_MARK,))
    spans = _post_open_spans(parsed, origin_us)
    result: dict[str, Any] = {
        "settled_us": None,
        "floor_us": floor_us,
        "marks": len(marks),
        "sentinels": len(sentinels),
        "last_mark_us": marks[-1] if marks else None,
        "last_sentinel_us": sentinels[-1] if sentinels else None,
    }
    floor = floor_us
    for name in required:
        ends = [int(s["end_us"]) for s in spans if s.get("name") == name]
        if not ends:
            result["waiting_for"] = name
            return result
        floor = max(floor, min(ends))
    if require_activity:
        if not spans:
            result["waiting_for"] = "post-open activity"
            return result
        floor = max(floor, min(int(s["end_us"]) for s in spans))
    # Settle can first hold at the floor, at a sentinel, or when a mark's
    # quiet period ends. Sorted lists and bisection keep a live poll cheap
    # while the app is being measured.
    candidates = {floor}
    candidates.update(s for s in sentinels if s >= floor)
    candidates.update(m + quiet_us for m in marks if m + quiet_us >= floor)
    span_ends = sorted(int(s["end_us"]) for s in spans)
    intervals = [(int(s["start_us"]), int(s["end_us"])) for s in spans]
    for t in sorted(candidates):
        if until_us is not None and t > until_us:
            break
        index = bisect.bisect_right(marks, t)
        last_mark = marks[index - 1] if index else None
        if last_mark is not None and last_mark > t - quiet_us:
            continue
        if any(start <= t < end for start, end in intervals):
            continue
        index = bisect.bisect_right(span_ends, t)
        last_end = span_ends[index - 1] if index else None
        activity = max((v for v in (last_mark, last_end) if v is not None), default=None)
        if activity is None:
            ok = idle_ok or bool(sentinels and sentinels[0] <= t)
        else:
            index = bisect.bisect_left(sentinels, activity)
            ok = index < len(sentinels) and sentinels[index] <= t
        if ok:
            result["settled_us"] = t
            return result
    return result


def post_open_dispatches(parsed: dict, start_us: int, end_us: int, tick_us: int, floor_us: int = POST_OPEN_DISPATCH_FLOOR_US) -> list[dict]:
    """Every main-loop dispatch of at least `floor_us` that overlaps [start, end].

    A late record at t with lateness L is a dispatch that ended at about t
    and lasted L to L + T (the watchdog tick): it started in
    [t - L - T, t - L]. The window carries the earlier bound.
    """
    out = []
    for record in sorted(parsed["records"]["late"], key=lambda r: int(r["t_us"])):
        t = int(record["t_us"])
        lateness = int(record["lateness_us"])
        if lateness < floor_us or t < start_us or t - lateness > end_us:
            continue
        out.append({"start_us": t - lateness - tick_us, "end_us": t, "duration_us": lateness})
    return out


def attribute_dispatch(dispatch: dict, spans: list[dict], marks: list[dict], tick_us: int) -> tuple[str, str]:
    """The post-open stage, mark, or `unattributed` a dispatch belongs to.

    Spans first: the innermost span covering at least half of the dispatch,
    else the span covering most of it (an activation turn whose short
    editor-construction span sits inside it is the activation). A dispatch no
    span covers is highlighting or validation when a `highlight_updated` or
    `adjustment_upper` mark was recorded inside it (the mark closest to its
    end decides), else unattributed.
    """
    start, end, duration = dispatch["start_us"], dispatch["end_us"], max(1, dispatch["duration_us"])
    covering = []
    for s in spans:
        overlap = min(int(s["end_us"]), end) - max(int(s["start_us"]), start)
        if overlap > 0:
            covering.append((overlap, int(s["end_us"]) - int(s["start_us"]), s["name"]))
    if covering:
        majority = [c for c in covering if 2 * c[0] >= duration]
        if majority:
            return min(majority, key=lambda c: (c[1], -c[0]))[2], "span"
        return max(covering, key=lambda c: (c[0], -c[1]))[2], "span"
    inside = [m for m in marks if start <= int(m["t_us"]) <= end + tick_us // 4]
    if inside:
        closest = min(inside, key=lambda m: abs(end - int(m["t_us"])))
        return MARK_ATTRIBUTION[closest["name"]], "mark"
    return "unattributed", "none"


def key_dispatch_latencies(parsed: dict, injections: list[dict]) -> list[dict]:
    """Each injected key's dispatch latency: compositor event time to its handler.

    Injections are matched to `input` records in order, as input-to-frame
    matches them (an unmeasured modifier still consumes its record). The
    event time is the compositor's millisecond stamp on the same monotonic
    clock, so the latency is exact to a millisecond.
    """
    inputs = sorted(parsed["records"]["input"], key=lambda r: r["t_us"])
    used: set[int] = set()
    out = []
    for injection in injections:
        record = None
        for index, candidate in enumerate(inputs):
            if index in used or candidate["t_us"] < injection["inject_us"] - 2_000:
                continue
            if candidate.get("event") == injection["event"]:
                record = candidate
                used.add(index)
                break
        if not injection.get("measured", True):
            continue
        if record is None:
            out.append({"inject_us": injection["inject_us"], "status": "no-input-record"})
            continue
        handler = int(record["t_us"])
        event_us = int(record.get("event_time_ms", 0)) * 1000
        out.append({"inject_us": injection["inject_us"], "event_us": event_us, "handler_us": handler, "latency_us": max(0, handler - event_us)})
    return out


def post_open_entry(parsed: dict, origin_us: int, floor_us: int, quiet_us: int, phase: str, kind: str, until_us: int | None = None, required: Iterable[str] = (), require_activity: bool = False, trigger_us: int | None = None, injections: list[dict] | None = None, idle_ok: bool = False) -> dict[str, Any]:
    """One post-open settle table: settle, then every dispatch until it, attributed.

    `kind` is `open` (a launch's opened document; the table starts at its
    first activation span, not at the open phase boundary, because
    `file-open-complete` can hold before the activation's opens run), `tab`
    (a restored tab selected; starts at the selection), `typing`
    (type-during-settle; adds each key's dispatch latency) or `interaction`
    (wrap/zoom/font; adds the interaction dispatch and the later ones).
    """
    settle = compute_settle(parsed, origin_us, floor_us, quiet_us, until_us, required, require_activity, idle_ok)
    tick = int(parsed.get("tick_us", 2000))
    spans = _post_open_spans(parsed, origin_us)
    start = trigger_us if trigger_us is not None else origin_us
    if kind in ("open", "typing"):
        activations = [int(s["start_us"]) for s in spans if s.get("name") == POST_OPEN_ACTIVATION and int(s["start_us"]) >= origin_us]
        if activations:
            start = min(activations)
    settled = settle["settled_us"]
    if settled is not None:
        end = settled
    elif until_us is not None:
        end = until_us
    else:
        end = max([int(r["t_us"]) for r in parsed["records"]["late"]] + [start])
    marks = [r for r in parsed["records"]["mark"] if r.get("name") in MARK_ATTRIBUTION and int(r["t_us"]) >= origin_us]
    dispatches = []
    worst: dict[str, int] = {}
    for dispatch in post_open_dispatches(parsed, start, end, tick):
        attribution, source = attribute_dispatch(dispatch, spans, marks, tick)
        dispatch.update({"at_us": dispatch["end_us"] - start, "attribution": attribution, "source": source})
        dispatches.append(dispatch)
        worst[attribution] = max(worst.get(attribution, 0), dispatch["duration_us"])
    layout_ends = [int(r["t_us"]) for r in parsed["records"]["mark"] if r.get("name") == ADJUSTMENT_UPPER_MARK and start <= int(r["t_us"]) <= end]
    layout_ends += [d["end_us"] for d in dispatches if d["attribution"] in LAYOUT_ATTRIBUTIONS]
    attaches = sorted(int(s["start_us"]) for s in spans if s.get("name") == POST_OPEN_LANGUAGE_ATTACH and start <= int(s["start_us"]) <= end)
    entry: dict[str, Any] = {
        "phase": phase,
        "kind": kind,
        "status": "settled" if settled is not None else "pending",
        "origin_us": origin_us,
        "activation_us": start,
        "settled_us": settled,
        "settle_us": settled - start if settled is not None else None,
        "laid_out_us": activation_to_laid_out(start, layout_ends, attaches[0] if attaches else None) if settled is not None else None,
        "settle": settle,
        "dispatches": dispatches,
        "stage_worst_us": worst,
    }
    if injections is not None:
        entry["keys"] = key_dispatch_latencies(parsed, injections)
    if kind == "interaction":
        entry["interaction"] = dispatches[0] if dispatches else None
        entry["later"] = dispatches[1:]
    return entry


def activation_to_laid_out(start_us: int, layout_ends: list[int], attach_us: int | None) -> int | None:
    """Activation to fully laid out: the end of the last layout dispatch or
    vertical-upper change, less the deferred language attach's wait.

    The wait (from the last layout before the attach to the attach) is the
    only exemption, bounded by ATTACH_DEADLINE_US: the attach waits for input
    quiet by design, and the layout its highlighting causes counts.
    """
    if not layout_ends:
        return None
    last = max(layout_ends)
    wait = 0
    if attach_us is not None and last > attach_us:
        before = max((t for t in layout_ends if t <= attach_us), default=start_us)
        wait = min(max(attach_us - before, 0), ATTACH_DEADLINE_US)
    return last - start_us - wait


def post_open_entries(parsed: dict, phases: list[dict], quiet_us: int) -> list[dict]:
    """The post-open settle table of every post-open phase of one repetition.

    Recomputed from the merged probe records with the inputs the live phase
    recorded (origin, floor, kind), bounded by the phase's end, so the
    settle found here is the one the phase waited for, and a cap hit is an
    entry that never settled.
    """
    entries = []
    for phase in phases:
        kind = phase.get("post_open")
        if not kind or "start_us" not in phase:
            continue
        entries.append(
            post_open_entry(
                parsed,
                int(phase["origin_us"]),
                int(phase["floor_us"]),
                quiet_us,
                phase.get("label", phase["name"]),
                kind,
                until_us=int(phase["end_us"]),
                require_activity=bool(phase.get("require_activity")),
                idle_ok=bool(phase.get("idle_ok")),
                trigger_us=phase.get("trigger_us"),
                injections=phase.get("injections", []) if kind == "typing" else None,
            )
        )
    return entries


class ProbeTail:
    """Incrementally read probe logs while a phase waits (complete lines only)."""

    def __init__(self, paths: list[Path]) -> None:
        self.paths = paths
        self.offsets: dict[Path, int] = {}
        self.parsed = parse_probe_log([])

    def poll(self) -> dict[str, Any]:
        for path in self.paths:
            try:
                with path.open("rb") as handle:
                    handle.seek(self.offsets.get(path, 0))
                    data = handle.read()
            except OSError:
                continue
            complete = data.rfind(b"\n") + 1
            if not complete:
                continue
            self.offsets[path] = self.offsets.get(path, 0) + complete
            fresh = parse_probe_log(data[:complete].decode("utf-8", errors="replace").splitlines())
            for kind, records in fresh["records"].items():
                self.parsed["records"][kind].extend(records)
            if fresh["started"] and not self.parsed["started"]:
                self.parsed.update({k: fresh[k] for k in ("tick_us", "late_threshold_us", "started")})
        return self.parsed


def wait_for_settle(poll: Callable[[], dict], origin_us: int, floor_us: int, quiet_us: int, cap_s: float, clock: Callable[[], float] = time.monotonic, sleep: Callable[[float], None] = time.sleep, now: Callable[[], int] | None = None, required: Iterable[str] = (), require_activity: bool = False, idle_ok: bool = False) -> dict[str, Any]:
    """Poll the probe log until the driver-computed settle, or the cap."""
    now = now or now_us
    deadline = clock() + cap_s
    while True:
        result: dict[str, Any] = compute_settle(poll(), origin_us, floor_us, quiet_us, now() - SETTLE_LOG_MARGIN_US, required, require_activity, idle_ok)
        if result["settled_us"] is not None:
            result["status"] = "settled"
            return result
        if clock() >= deadline:
            result["status"] = "exceeded_cap"
            return result
        sleep(SETTLE_POLL_S)


def parse_toolkit_floors(spec: str | None) -> dict[str, dict]:
    """`attribution=AXIOM[:report];...` into {attribution: {axiom, report}}."""
    floors: dict[str, dict] = {}
    for item in filter(None, (part.strip() for part in (spec or "").split(";"))):
        attribution, _, rest = item.partition("=")
        axiom, _, report = rest.partition(":")
        if not attribution.strip() or not axiom.strip():
            raise SystemExit(f"{TOOLKIT_FLOORS_ENV}: expected attribution=AXIOM[:report], got {item!r}")
        floors[attribution.strip()] = {"axiom": axiom.strip(), "report": report.strip() or None}
    return floors


def _median(values: list[int]) -> int:
    return int(statistics.median(values)) if values else 0


def c6_verdict(rows: list[dict], budgets: dict, floors: dict[str, dict], baseline_side: str = C6_BASELINE_SIDE, branch_side: str = C6_BRANCH_SIDE) -> dict[str, Any]:
    """The C6 verdict (design D8, reviews R7 and R27) over side-labelled rows.

    Met when, on the branch, in every case (profile) and repetition:
    - every open and restored-tab post-open dispatch until settled is within
      B_SETTLE_US, and the document settled before the cap;
    - every type-during-settle key is dispatched within B_INTERACT_US;
    - every wrap/zoom/font interaction dispatch is within B_INTERACT_US and
      every later dispatch until settled within B_SETTLE_US;
    - no stage regresses: per case, the median over the repetitions of each
      attributed stage's worst dispatch, and the median activation-to-laid-out
      time (the deferred language attach's wait exempt, at most
      ATTACH_DEADLINE_US), are each at most the baseline's plus one
      REFERENCE_FRAME_US. Activation-to-settled is reported, not gated.
      A stage only the branch has is listed as new, not compared (it is
      still under the absolute bounds).
    A dispatch attributed to a listed toolkit-floor exception is exempt from
    the dispatch bound that applies to it and reported on its own line; the
    keys whose dispatch (event to handler) overlaps it are exempt from
    B_INTERACT_US and listed there with their latencies. Nothing else is exempt.
    Rows without a side are the branch (a single-build run has no regression check).
    """
    b_settle, b_interact, frame = budgets["B_SETTLE_US"], budgets["B_INTERACT_US"], budgets["REFERENCE_FRAME_US"]
    branch = [r for r in rows if r.get("post_open") and r.get("side", branch_side) == branch_side]
    baseline = [r for r in rows if r.get("post_open") and r.get("side") == baseline_side]
    failures: list[str] = []
    exceptions: list[dict] = []
    baseline_over: list[dict] = []
    if not branch:
        return {"verdict": "not measured", "reason": "no post-open rows on the branch side", "failures": [], "regressions": [], "exceptions": [], "new_stages": [], "baseline_over_budget": [], "input_to_frame": []}

    def exempt(attribution: str) -> bool:
        return attribution in floors

    for row in branch:
        label = f"{row['case']}@{row['variant']}#{row['rep']}"
        if row.get("outcome") not in (None, "ok"):
            failures.append(f"{label}: outcome {row.get('outcome')}")
        exception_dispatches = []
        for entry in row["post_open"]:
            where = f"{label} {entry['phase']}"
            if entry.get("status") != "settled":
                failures.append(f"{where}: not settled before the cap")
            for dispatch in entry.get("dispatches", []):
                if exempt(dispatch["attribution"]):
                    exception_dispatches.append((entry, dispatch))
            if entry["kind"] in POST_OPEN_SETTLE_KINDS:
                for dispatch in entry.get("dispatches", []):
                    if dispatch["duration_us"] > b_settle and not exempt(dispatch["attribution"]):
                        failures.append(f"{where}: {dispatch['attribution']} dispatch {dispatch['duration_us']} us > B_SETTLE_US {b_settle}")
            elif entry["kind"] == "interaction":
                first = entry.get("interaction")
                if first and first["duration_us"] > b_interact and not exempt(first["attribution"]):
                    failures.append(f"{where}: interaction dispatch {first['duration_us']} us > B_INTERACT_US {b_interact}")
                for dispatch in entry.get("later", []):
                    if dispatch["duration_us"] > b_settle and not exempt(dispatch["attribution"]):
                        failures.append(f"{where}: later {dispatch['attribution']} dispatch {dispatch['duration_us']} us > B_SETTLE_US {b_settle}")
        keyed = [(entry, key) for entry in row["post_open"] for key in entry.get("keys", [])]
        overlapping: dict[int, list[int]] = {}
        for entry, key in keyed:
            where = f"{label} {entry['phase']}"
            if "latency_us" not in key:
                failures.append(f"{where}: a typed key has no input record")
                continue
            hits = [index for index, (_e, d) in enumerate(exception_dispatches) if key["event_us"] <= d["end_us"] and key["handler_us"] >= d["start_us"]]
            for index in hits:
                overlapping.setdefault(index, []).append(key["latency_us"])
            if key["latency_us"] > b_interact and not hits:
                failures.append(f"{where}: key {key['latency_us']} us > B_INTERACT_US {b_interact}")
        for index, (entry, dispatch) in enumerate(exception_dispatches):
            floor = floors[dispatch["attribution"]]
            exceptions.append(
                {
                    "case": row["case"],
                    "variant": row["variant"],
                    "rep": row["rep"],
                    "phase": entry["phase"],
                    "attribution": dispatch["attribution"],
                    "duration_us": dispatch["duration_us"],
                    "axiom": floor["axiom"],
                    "report": floor["report"],
                    "overlapping_keys_us": overlapping.get(index, []),
                }
            )
    for row in baseline:
        for entry in row["post_open"]:
            for dispatch in entry.get("dispatches", []):
                if dispatch["duration_us"] > b_settle:
                    baseline_over.append({"case": row["case"], "variant": row["variant"], "rep": row["rep"], "phase": entry["phase"], "attribution": dispatch["attribution"], "duration_us": dispatch["duration_us"]})

    def stage_series(side_rows: list[dict]) -> dict[tuple, dict[str, list[int]]]:
        """Per (case, variant, phase): each stage's worst dispatch per repetition, and settle times."""
        series: dict[tuple, dict[str, list[int]]] = {}
        reps: dict[tuple, list[dict]] = {}
        for row in side_rows:
            for entry in row["post_open"]:
                if entry["kind"] in POST_OPEN_SETTLE_KINDS:
                    reps.setdefault((row["case"], row["variant"], entry["phase"]), []).append(entry)
        for key, entries in reps.items():
            stages = sorted({s for e in entries for s in e.get("stage_worst_us", {})})
            out = {stage: [e.get("stage_worst_us", {}).get(stage, 0) for e in entries] for stage in stages}
            laid_out = [e["laid_out_us"] for e in entries if e.get("laid_out_us") is not None]
            if laid_out:
                out["activation-to-laid-out"] = laid_out
            series[key] = out
        return series

    regressions: list[dict] = []
    new_stages: list[dict] = []
    if baseline:
        base_series = stage_series(baseline)
        for key, stages in stage_series(branch).items():
            base = base_series.get(key)
            if base is None:
                new_stages.append({"case": key[0], "variant": key[1], "phase": key[2], "stage": "*", "reason": "no baseline rows"})
                continue
            for stage, values in stages.items():
                if stage not in base:
                    new_stages.append({"case": key[0], "variant": key[1], "phase": key[2], "stage": stage, "branch_median_us": _median(values)})
                    continue
                if exempt(stage):
                    continue
                branch_median, base_median = _median(values), _median(base[stage])
                if branch_median > base_median + frame:
                    regressions.append({"case": key[0], "variant": key[1], "phase": key[2], "stage": stage, "branch_median_us": branch_median, "baseline_median_us": base_median, "limit_us": base_median + frame})
    for regression in regressions:
        failures.append(f"{regression['case']}@{regression['variant']} {regression['phase']}: {regression['stage']} median {regression['branch_median_us']} us > baseline {regression['baseline_median_us']} + one frame")
    itf = []
    for side, side_rows in ((branch_side, branch), (baseline_side, baseline)):
        cases: dict[str, list[int]] = {}
        for row in side_rows:
            for phase in row.get("phases", []):
                if phase.get("name") in ("scroll", "type-during-settle"):
                    cases.setdefault(f"{row['case']}@{row['variant']} {phase['name']}", []).extend(e["latency_us"] for e in phase.get("input_to_frame", []) if editable_latency(e))
        for label, values in sorted(cases.items()):
            if values:
                itf.append({"side": side, "case": label, "median_us": _median(values), "max_us": max(values), "events": len(values)})
    return {
        "verdict": "not met" if failures else "met",
        "failures": failures,
        "regressions": regressions,
        "new_stages": new_stages,
        "exceptions": exceptions,
        "baseline_over_budget": baseline_over,
        "toolkit_floors": floors,
        "branch_rows": len(branch),
        "baseline_rows": len(baseline),
        "sides": {"baseline": baseline_side, "branch": branch_side},
        "input_to_frame": itf,
        "input_to_frame_floor": INPUT_TO_FRAME_FLOOR_TEXT,
    }


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
    # Written beside the fixture as `.editorconfig` (the `editorconfig/` case).
    editorconfig: dict[str, Any] | None = None


@dataclass
class Case:
    id: str
    group: str
    fixture: FixtureSpec | None
    settings: dict[str, Any] = field(default_factory=dict)
    flow: str = "standard"  # standard | session | eviction | reload | install | calibration | keystroke
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


def keystroke_case(length: int, flavor: str, ext: str, wrap: bool) -> Case:
    return Case(
        f"keystroke/{length}/{flavor}/{ext}/wrap-{'on' if wrap else 'off'}",
        "keystroke",
        FixtureSpec("keystroke", length, seed_for("keystroke", length, flavor), ext=ext, flavor=flavor),
        settings={"word-wrap": wrap, "show-minimap": False},
        flow="keystroke",
        length=length,
        attrs={"ext": ext, "wrap": wrap, "flavor": flavor},
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
        "keystroke": KEYSTROKE_LENGTHS[tier],
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
        Case("baseline/empty", "baseline", None, settings={"word-wrap": True}, attrs={"profile": "fresh", "backend": "keyfile", "post_open": True}),
    ]
    for profile in CONSTRUCTION_PROFILES:
        for backend in CONSTRUCTION_BACKENDS:
            cases.append(
                Case(
                    construction_case_id(profile, backend),
                    "baseline",
                    FixtureSpec("paragraph", 400, seed_for("small")),
                    settings={"word-wrap": True},
                    length=400,
                    attrs={"profile": profile, "backend": backend, "post_open": True, "typing": True},
                )
            )
    cases.append(
        Case(
            POPULATED_CASE_ID,
            "baseline",
            FixtureSpec("paragraph", 400, seed_for("small")),
            settings={"word-wrap": True, "workspace-sidebar-visible": True, "properties-sidebar-visible": True},
            length=400,
            attrs={"profile": "steady", "backend": "keyfile", "populated": True, "post_open": True, "typing": True},
        )
    )
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
                    attrs={"wrap": wrap, "lines": DENSE_LINE_COUNT, "post_open": True, "typing": True, "restyle": True},
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
                    attrs={"ext": ext, "post_open": True, "typing": True},
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
    cases.extend(post_open_cases())
    for length in grid["keystroke"]:
        for flavor, ext in KEYSTROKE_SHAPES:
            for wrap in (True, False):
                cases.append(keystroke_case(length, flavor, ext, wrap))
    return cases


def post_open_cases() -> list[Case]:
    """The cases C6 added (design D8); both tiers run them."""
    return [
        Case(
            "markdown/preview",
            "markdown",
            FixtureSpec("markdown", MARKDOWN_PREVIEW_BYTES, seed_for("markdown"), ext="md"),
            settings={"word-wrap": True, "show-minimap": False},
            flow="preview",
            length=MARKDOWN_PREVIEW_BYTES,
            attrs={"ext": "md", "post_open": True},
        ),
        Case(
            "editorconfig/tab-width",
            "editorconfig",
            FixtureSpec(
                "dense",
                EDITORCONFIG_LINE_LENGTH,
                seed_for("editorconfig", EDITORCONFIG_LINE_LENGTH),
                count=DENSE_LINE_COUNT,
                editorconfig={"indent_style": "space", "indent_size": EDITORCONFIG_TAB_WIDTH, "tab_width": EDITORCONFIG_TAB_WIDTH},
            ),
            settings={"word-wrap": False, "show-minimap": False, "tab-width": EDITORCONFIG_SETTING_TAB_WIDTH, "use-editorconfig": True},
            length=EDITORCONFIG_LINE_LENGTH,
            attrs={"wrap": False, "tab_width": EDITORCONFIG_TAB_WIDTH, "post_open": True, "typing": True},
        ),
        Case(
            "session/restore-3-small",
            "session",
            FixtureSpec("eviction", SESSION_SMALL_BYTES, seed_for("session-small"), count=3),
            settings={"word-wrap": True, "show-minimap": False},
            flow="session",
            length=SESSION_SMALL_BYTES,
            attrs={"post_open": True, "tab_settle": True},
        ),
        Case(
            "recovery/small-drafts",
            "recovery",
            FixtureSpec("eviction", RECOVERY_DRAFT_BYTES, seed_for("recovery"), count=RECOVERY_DRAFTS),
            settings={"word-wrap": True, "show-minimap": False},
            flow="recovery",
            length=RECOVERY_DRAFT_BYTES,
            attrs={"post_open": True, "tab_settle": True},
        ),
    ]


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
        # A printable key without Control inserts text where it lands: the
        # lane checks it reached an editor and inserted (`input_delivery`).
        inserts = not control and 0x20 <= keysym <= 0x7E
        injections.append({"event": "key-press", "inject_us": self.key(keysym, True), **({"inserts": True} if inserts else {})})
        self.key(keysym, False)
        if control:
            self.key(KEY_CONTROL_L, False)

    def wheel(self, steps: int, injections: list[dict]) -> None:
        from gi.repository import GLib

        stamp = now_us()
        self._call("NotifyPointerAxisDiscrete", GLib.Variant("(ui)", (0, steps)))
        injections.append({"event": "scroll", "inject_us": stamp})

    def click(self, x: int, y: int, injections: list[dict], measured: bool = True) -> int:
        """Move to screen `(x, y)` and click; returns the press's stamp.

        An unmeasured click (focus) is still recorded, so its button-press
        record cannot stand in for a later measured one.
        """
        from gi.repository import GLib

        self.move_to(x, y)
        time.sleep(0.05)
        stamp = now_us()
        self._call("NotifyPointerButton", GLib.Variant("(ib)", (BTN_LEFT, True)))
        self._call("NotifyPointerButton", GLib.Variant("(ib)", (BTN_LEFT, False)))
        injections.append({"event": "button-press", "inject_us": stamp, **({} if measured else {"measured": False})})
        return stamp

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
        self.disk_scope = self.artifact_dir / "state-on-disk"
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
        # Post-open settle (bound-post-open-settle, C6): the quiet period is
        # the app's own constant; RESPONSIVENESS_POST_OPEN=0 restores the
        # fixed two-second background phase for every case.
        self.settle_quiet_us = read_settle_quiet()
        self.settle_cap_s = float(os.environ.get("RESPONSIVENESS_SETTLE_CAP_S", str(SETTLE_CAP_S)))
        self.post_open_enabled = os.environ.get("RESPONSIVENESS_POST_OPEN", "1") != "0"

    # -- fixtures -----------------------------------------------------------

    def make_fixture(self, spec: FixtureSpec, label: str) -> list[Path]:
        out_dir = self.fixture_root / re.sub(r"[^A-Za-z0-9_.-]", "_", label)
        manifest = self.generator.generate(
            spec.kind, spec.size, spec.seed, self.scope, out_dir, ext=spec.ext, delimiter=spec.delimiter, charset=spec.charset, count=spec.count, flavor=spec.flavor, editorconfig=spec.editorconfig
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

    def fresh_state(self, case_dir: Path, settings: dict[str, Any], backend: str = "keyfile", on_disk: bool = False, populated: bool = False, legacy_layout: bool = False) -> dict[str, str]:
        # The construction cases keep their XDG state on the artifact
        # filesystem, which is the home filesystem on the reference machine:
        # a GSettings keyfile write fsyncs, and the temp scope (tmpfs) would
        # hide what that costs a real profile.
        parent = self.disk_scope if on_disk else self.scope
        parent.mkdir(parents=True, exist_ok=True)
        state = Path(tempfile.mkdtemp(prefix="state-", dir=parent))
        for name in ("cache", "config", "data"):
            (state / name).mkdir()
        keyfile = state / "config/glib-2.0/settings/keyfile"
        keyfile.parent.mkdir(parents=True)
        # The layout keys. Unmigrated, the app runs the one-shot migration
        # from the legacy `sidebar-visible` key (default true), which showed
        # the workspace sidebar the case asks to hide
        # (`investigate-first-keys-without-frames`). A fresh profile
        # (`legacy_layout`) seeds that legacy key as hidden, so its launch
        # still runs the migration (and writes its result once, after the
        # first frame) and renders the same layout; every other profile is
        # seeded migrated.
        layout: dict[str, Any] = (
            {"sidebar-visible": False}
            if legacy_layout
            else {"split-view-layout-migrated": True, "workspace-sidebar-visible": False, "properties-sidebar-visible": False}
        )
        values = {
            "window-maximized": True,
            **layout,
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
        if backend == "dconf":
            self.seed_dconf(state / "config", lines[1:])
        else:
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
        if populated:
            seed_populated_workspace(state / "data/lushtext", state / "workspace")
        return {
            "XDG_CACHE_HOME": str(state / "cache"),
            "XDG_CONFIG_HOME": str(state / "config"),
            "XDG_DATA_HOME": str(state / "data"),
            "LUSHTEXT_DATA_DIR": str(state / "data/lushtext"),
            "GSETTINGS_BACKEND": backend,
            "STATE_ROOT": str(state),
        }

    def seed_dconf(self, config_dir: Path, lines: list[str]) -> None:
        """Point the private bus's dconf service at `config_dir` and seed it.

        The dconf client reads its database from its own
        `$XDG_CONFIG_HOME/dconf/user` and the service writes the one under the
        service's, so both must name the same directory. The service is started
        by the private session bus with the bus's activation environment: this
        sets it, stops a service started for an earlier state, and loads the
        values through `dconf load`, which waits for the service's reply. It
        never reaches the user's real dconf database (`session_main` isolates
        the activation environment before anything can start the service).
        """
        isolate_dconf_activation(self.bus, config_dir)
        stop_dconf_service(self.bus)
        if not DCONF_LOAD.exists():
            raise RuntimeError("dconf is not installed; the dconf backend cases cannot run")
        env = os.environ.copy()
        env["XDG_CONFIG_HOME"] = str(config_dir)
        subprocess.run(
            [str(DCONF_LOAD), "load", f"/{SCHEMA_GROUP}/"],
            input="[/]\n" + "\n".join(lines) + "\n",
            text=True,
            env=env,
            check=True,
            timeout=30,
        )

    def app_env(self, variant: str, state_env: dict[str, str], log_path: Path, tick_us: int | None = None, frame_phases: int | None = None) -> dict[str, str]:
        env = os.environ.copy()
        for key in ("NO_AT_BRIDGE", "GTK_A11Y", "LUSHTEXT_PROBE_IM", "GTK_IM_MODULE", "LUSHTEXT_RESPONSIVENESS_TICK_US", "LUSHTEXT_RESPONSIVENESS_FRAME_PHASES"):
            env.pop(key, None)
        env.update({k: v for k, v in state_env.items() if k != "STATE_ROOT"})
        env.update(
            {
                "GDK_BACKEND": "wayland",
                "GSETTINGS_BACKEND": state_env.get("GSETTINGS_BACKEND", "keyfile"),
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
        if frame_phases:
            # Only the post-open cases ask for a longer frame-phase window (design D1).
            env["LUSHTEXT_RESPONSIVENESS_FRAME_PHASES"] = str(frame_phases)
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
        # The probe log of the launch the next editor click is checked against.
        self.current_log = Path(env["LUSHTEXT_RESPONSIVENESS_LOG"]) if env.get("LUSHTEXT_RESPONSIVENESS_LOG") else None
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

    def editor_point(self, x_fraction: float = 1 / 3, y_fraction: float = 1 / 2) -> tuple[int, int] | None:
        """A screen point inside the selected editor's text view, from the
        snapshot's `source-view` surface (window coordinates; the lane's
        window is maximized at the monitor's origin), outside every surface
        that can cover it (an overlaid workspace sidebar, the document-
        properties bottom sheet). The requested fractions come first, then a
        few others. `None` without a free point."""
        try:
            # `window.surfaces` is the shell's visibility summary; the
            # rectangles are under `visual_geometry`.
            geometry = find_key(self.snapshot(), "visual_geometry") or {}
        except Exception:  # noqa: BLE001 - no snapshot, no geometry
            return None
        return editor_point_in(geometry.get("surfaces") or [], x_fraction, y_fraction)

    def editor_geometry_note(self) -> str:
        """The snapshot's text-view and cover rectangles, for a click that
        found no free point."""
        try:
            geometry = find_key(self.snapshot(), "visual_geometry") or {}
        except Exception:  # noqa: BLE001 - no snapshot
            return "no snapshot"
        rects = {s.get("name"): s.get("rect") for s in geometry.get("surfaces") or [] if isinstance(s, dict)}
        return ", ".join(f"{name} {rects.get(name)}" for name in ("source-view", *EDITOR_COVERS))

    def pointer_arrival(self, event: str, since_us: int) -> dict | None:
        """The probe's last `event` input record at or after `since_us`, once
        the pointer stopped producing them; `None` when none arrived in
        `CLICK_DELIVERY_TIMEOUT_S` (or the launch has no probe log)."""
        log = getattr(self, "current_log", None)
        if log is None:
            return None
        tail = ProbeTail([log])
        deadline = time.monotonic() + CLICK_DELIVERY_TIMEOUT_S
        while time.monotonic() < deadline:
            found = [r for r in tail.poll()["records"]["input"] if r.get("event") == event and int(r["t_us"]) >= since_us - 2_000]
            if found:
                time.sleep(0.1)
                found = [r for r in tail.poll()["records"]["input"] if r.get("event") == event and int(r["t_us"]) >= since_us - 2_000]
                return found[-1]
            time.sleep(0.05)
        return None

    def editor_click(self, injections: list[dict], x_fraction: float = 1 / 3, y_fraction: float = 1 / 2, measured: bool = True) -> tuple[bool, str]:
        """Click inside the selected editor's text and prove it arrived there.

        `investigate-first-keys-without-frames`: a click aimed at a fixed
        screen point landed in the workspace sidebar, the keys after it
        reached no document, and the lane measured a window that drew
        nothing. The point now comes from the editor's own geometry; the
        pointer is moved there first and corrected by where the probe saw
        it arrive (a requested screen point arrived up to 130 px left of it
        in window coordinates, so screen and window do not coincide), and the
        probe's record of the press must name an editor. A probe that records
        no motion or target (an older build) cannot prove it, and is trusted.
        """
        point = self.editor_point(x_fraction, y_fraction)
        wait_until = time.monotonic() + EDITOR_POINT_WAIT_S
        while point is None and time.monotonic() < wait_until:
            time.sleep(0.1)
            point = self.editor_point(x_fraction, y_fraction)
        if point is None:
            if getattr(self, "current_log", None) is not None:
                # A probe build can verify the click, so a blind point would
                # only fail later; say why instead.
                return False, f"no-editor-point ({self.editor_geometry_note()})"
            point = (SCREEN_W // 3, SCREEN_H // 2)
        screen = point
        for _ in range(POINTER_AIM_ATTEMPTS):
            stamp = now_us()
            self.rd.move_to(*screen)
            motion = self.pointer_arrival("motion", stamp)
            if motion is None or motion.get("x") is None:
                break
            dx, dy = point[0] - int(motion["x"]), point[1] - int(motion["y"])
            if abs(dx) <= 1 and abs(dy) <= 1:
                break
            screen = (min(max(screen[0] + dx, 0), SCREEN_W - 1), min(max(screen[1] + dy, 0), SCREEN_H - 1))
        stamp = self.rd.click(screen[0], screen[1], injections, measured)
        press = self.pointer_arrival("button-press", stamp)
        if press is None:
            return (True, "ok") if getattr(self, "current_log", None) is None else (False, "click-not-delivered")
        target = press.get("target")
        if target is None or target in EDITOR_TARGETS:
            return True, "ok"
        return False, f"click-missed-editor ({target} at window {press.get('x')},{press.get('y')}; aimed at window {point[0]},{point[1]} via screen {screen[0]},{screen[1]})"

    # -- phases -------------------------------------------------------------

    def run_phase(self, phases: list[dict], name: str, cap_s: float, body: Callable[[list[dict], float], tuple[bool, str]], extra: dict | None = None) -> bool:
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
                # What the body recorded (a post-open phase's settle inputs).
                **(extra or {}),
            }
        )
        return ok

    def step(self, phases: list[dict], name: str, cap: float, body, recover: Callable[[], bool] | None, extra: dict | None = None) -> bool:
        """Run one phase; after a cap hit, relaunch the app and carry on.

        A hang is a result, not the end of the case: the remaining
        interactions still run against a fresh launch, so a stall in one
        phase does not hide the cost of the phases after it.
        """
        if self.run_phase(phases, name, cap, body, extra):
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

    def settle_until(self, ctx: dict, kind: str, extra: dict, floor_us: int | None = None, propagation_us: int = 0, then: Callable[[float], tuple[bool, str]] | None = None) -> Callable[[Any, float], tuple[bool, str]]:
        """A post-open phase body: wait for the driver-computed settle (design D1, review R24).

        Settle is computed from the launch's probe log with the clauses of
        the `document-settled` predicate, so a build without the predicate is
        measured the same way. The phase's cap is the settle cap: a cap hit
        is a failed phase. `then` runs after settle (the preview's own
        readiness); the phase records its origin, floor and kind for the
        post-open settle table.
        """

        def body(_inj, deadline):
            floor = (floor_us if floor_us is not None else now_us()) + propagation_us
            extra.update({"post_open": kind, "origin_us": ctx["origin"], "floor_us": floor, "require_activity": ctx["documents"], "idle_ok": not ctx["documents"]})
            tail = ProbeTail([ctx["log"]])
            result = wait_for_settle(
                # The settle cap, from when the settle wait starts (a phase
                # may type or trigger first); never past the phase's deadline.
                tail.poll, ctx["origin"], floor, self.settle_quiet_us, min(self.settle_cap_s, max(0.0, deadline - time.monotonic())),
                require_activity=ctx["documents"], idle_ok=not ctx["documents"],
            )
            extra["settled_us"] = result.get("settled_us")
            if result["status"] != "settled":
                return False, "settle-cap"
            if then is not None:
                return then(deadline)
            return True, "ok"

        return body

    def write_settings(self, state_env: dict[str, str], values: dict[str, Any]) -> None:
        """Change GSettings keys of the running app's profile, as one apply.

        A separate GIO client with the app's backend and XDG directories
        writes the keys; the app sees the change through its backend (the
        keyfile's file monitor, or the private bus's dconf service), as it
        sees one from another process. No setting action exists for word
        wrap or the font.
        """
        env = os.environ.copy()
        env.update({k: v for k, v in state_env.items() if k != "STATE_ROOT"})
        env["GSETTINGS_SCHEMA_DIR"] = str(REPO_ROOT / "data")
        subprocess.run(
            [str(SYSTEM_PYTHON), "-c", SETTINGS_WRITER, APP_ID, json.dumps(values)],
            env=env,
            check=True,
            timeout=30,
            stdout=subprocess.DEVNULL,
        )

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

    def standard_interactions(self, phases: list[dict], tabs: int = 1, recover: Callable[[], bool] | None = None, ctx: dict | None = None, settle_then: Callable[[float], tuple[bool, str]] | None = None) -> bool:
        """Background, then the standard interactions.

        With a post-open context (`ctx`, bound-post-open-settle) the
        background phase lasts until the driver-computed settle, capped by
        the settle cap, instead of a fixed two seconds; a restored session
        then selects each tab and waits for it to settle, and a dense case
        ends with the wrap, zoom and font restyles.
        """
        cap = self.cap_s

        if ctx is None:
            if not self.step(phases, "background", cap, self.settle_after(2.0), recover):
                return False
        else:
            extra: dict = {}
            if not self.step(phases, "background", self.settle_cap_s, self.settle_until(ctx, "open", extra, then=settle_then), recover, extra):
                return False
            if ctx.get("tab_settle") and not self.tab_settles(phases, tabs, ctx, recover):
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
                # Inside the text, away from the header bar: a header click
                # starts a compositor grab and the window never gets keyboard
                # focus (observed headless).
                return self.editor_click(inj)

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
                        ok, status = self.editor_click(inj, x_fraction=1 / 2)
                        if not ok:
                            return ok, status
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
        if self.protected_tab() and not self.protected_reading(phases, cap, recover):
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
        if ctx is not None and ctx.get("restyle"):
            return self.restyles(phases, ctx, recover)
        return True

    def protected_tab(self) -> bool:
        """Whether the active tab is in Protected View (the reading phases
        exist only there)."""
        try:
            window = self.snapshot().get("window") or {}
            tabs = window.get("tabs") or []
            active = window.get("active_tab_index")
            tab = tabs[active] if isinstance(active, int) and 0 <= active < len(tabs) else (tabs[0] if tabs else {})
            return bool(tab.get("protected_view"))
        except Exception:  # noqa: BLE001 - a snapshot failure skips the phases
            return False

    def window_action_exists(self, name: str) -> bool:
        """Whether the window exports `name` (a build before the reading
        surfaces has neither; its phases are then recorded unavailable)."""
        try:
            reply = self.bus.call(APP_ID, WINDOW_OBJECT_PATH, "org.gtk.Actions", "List", None, None, 5_000)
            return name in reply.unpack()[0]
        except Exception:  # noqa: BLE001
            return False

    def protected_reading(self, phases: list[dict], cap: float, recover: Callable[[], bool] | None) -> bool:
        """The Line Inspector (open, then Page Down through ten windows) and
        Copy Whole Line read by an external Wayland client."""
        if not self.window_action_exists("inspect-line"):
            for name in ("inspect", "copy-line"):
                if self.wanted(name):
                    phases.append({"name": name, "kind": PHASE_KINDS[name], "status": "unavailable", "reason": "the build exports no win.inspect-line"})
            return True

        def inspect(inj, deadline):
            if not self.action("win", "inspect-line", None, deadline):
                return False, "action-failed"
            ok, status = self.wait_ready("idle", deadline)
            if not ok:
                return ok, status
            # Let the dialog's open transition (about 370 ms) finish first, so
            # each Page Down's dispatch is measured on its own.
            time.sleep(0.5)
            if self.rd.available:
                for _ in range(10):
                    self.rd.tap(KEY_PAGE_DOWN, inj)
                    time.sleep(0.15)
                self.rd.tap(KEY_ESCAPE, inj)
            return self.wait_ready("idle", deadline)

        def copy_line(_inj, deadline):
            if not self.action("win", "copy-whole-line", None, deadline):
                return False, "action-failed"
            ok, status = self.wait_ready("idle", deadline)
            if not ok:
                return ok, status
            try:
                pasted = subprocess.run(
                    ["wl-paste", "--no-newline", "--type", "text/plain;charset=utf-8"],
                    capture_output=True,
                    timeout=max(1.0, deadline - time.monotonic()),
                    check=False,
                )
            except (OSError, subprocess.TimeoutExpired) as error:
                return False, f"wl-paste: {error}"
            if pasted.returncode != 0:
                return False, f"wl-paste exit {pasted.returncode}"
            if not pasted.stdout:
                return False, "wl-paste read nothing"
            return True, f"ok {len(pasted.stdout)} bytes"

        for name, body in (("inspect", inspect), ("copy-line", copy_line)):
            if self.wanted(name) and not self.step(phases, name, cap, body, recover):
                return False
        return True

    def tab_settles(self, phases: list[dict], tabs: int, ctx: dict, recover: Callable[[], bool] | None) -> bool:
        """Select each restored tab in turn and wait for it to settle (review R8)."""
        for index in range(tabs):
            extra: dict = {"label": f"tab-settle-{index}", "tab_index": index}

            def body(inj, deadline, index=index, extra=extra):
                extra["trigger_us"] = now_us()
                self.action("win", "select-tab", [self._uint32(index)], deadline)
                ok, status = self.wait_ready("file-open-complete", deadline)
                if not ok:
                    return ok, status
                return self.settle_until(ctx, "tab", extra)(inj, deadline)

            if not self.step(phases, "tab-settle", self.settle_cap_s + 5, body, recover, extra):
                return False
        return True

    def restyles(self, phases: list[dict], ctx: dict, recover: Callable[[], bool] | None) -> bool:
        """The dense restyles (design D9): word wrap, one zoom step, a font change.

        Each runs until settled and records its interaction dispatch and every
        later dispatch. They run last, so no earlier measurement of the case
        sees the changed wrap, zoom or font.
        """

        # In-process triggers (review S4): the measurement build's
        # app.probe-set-* actions write through the app's settings writer, so
        # the restyle runs in the action's dispatch and is measured against
        # B_INTERACT_US. RESPONSIVENESS_RESTYLE_TRIGGER=keyfile keeps the
        # external keyfile write, whose GIO file-monitor dispatch comes first.
        keyfile = os.environ.get("RESPONSIVENESS_RESTYLE_TRIGGER") == "keyfile"

        def wrap(deadline: float) -> int:
            ctx["wrap"] = not ctx["wrap"]
            if keyfile:
                self.write_settings(ctx["state_env"], {"word-wrap": ctx["wrap"]})
                return SETTING_PROPAGATION_US
            from gi.repository import GLib

            if not self.action("app", "probe-set-word-wrap", [GLib.Variant("b", ctx["wrap"])], deadline):
                raise RuntimeError("app.probe-set-word-wrap failed")
            return 0

        def zoom(deadline: float) -> int:
            if not self.action("win", "zoom-in", None, deadline):
                raise RuntimeError("win.zoom-in failed")
            return 0

        def font(deadline: float) -> int:
            if keyfile:
                self.write_settings(ctx["state_env"], FONT_CHANGE_VALUES)
                return SETTING_PROPAGATION_US
            from gi.repository import GLib

            if not self.action("app", "probe-set-font", [GLib.Variant("s", FONT_CHANGE_VALUES["custom-font"])], deadline):
                raise RuntimeError("app.probe-set-font failed")
            return 0

        for name, trigger in (("wrap-toggle", wrap), ("zoom-step", zoom), ("font-change", font)):
            if not self.wanted(name):
                continue
            extra: dict = {}

            def body(inj, deadline, trigger=trigger, extra=extra):
                extra["trigger_us"] = now_us()
                propagation = trigger(deadline)
                return self.settle_until(ctx, "interaction", extra, propagation_us=propagation)(inj, deadline)

            if not self.step(phases, name, self.settle_cap_s + 5, body, recover, extra):
                return False
        return True

    def type_during_settle(self, rep_dir: Path, files: list[Path], phases: list[dict], launches: list[Launch], new_state: Callable[[str], tuple[dict, dict]]) -> bool:
        """`type-during-settle` (design D8, review R26), in a launch of its own.

        From load readiness one key every 50 ms, SETTLE_TYPING_KEYS keys in
        all, then stop typing and wait for settle. Each key's dispatch
        latency (compositor event time to handler) is read from the probe's
        input records. The earlier launch is gone first: a second instance
        would only forward its files to it.
        """
        name = "type-during-settle"
        if not self.wanted(name):
            return True
        if not self.rd.available:
            phases.append({"name": name, "kind": PHASE_KINDS[name], "status": "unavailable", "reason": self.rd.error or "RemoteDesktop unavailable"})
            return True
        for live in launches:
            if live.process.poll() is None:
                self.kill(live)
        state_env, env = new_state("probe-typing.jsonl")
        launch = self.launch(rep_dir, env, files, "typing")
        launches.append(launch)
        scratch: list[dict] = []
        if not self.open_phase(scratch, launch, self.cap_s):
            phases.append({"name": name, "kind": PHASE_KINDS[name], "status": f"open-{scratch[-1]['status'] if scratch else 'failed'}"})
            return False
        ctx = {"log": Path(env["LUSHTEXT_RESPONSIVENESS_LOG"]), "origin": launch.spawn_us, "documents": bool(files), "state_env": state_env}
        extra: dict = {"open_ready_us": scratch[-1]["end_us"]}

        def body(inj, deadline):
            # Keyboard focus in the text, unmeasured (a click is a button
            # press, never matched to a key), proven to have arrived there.
            ok, status = self.editor_click(inj, measured=False)
            if not ok:
                return ok, status
            for index in range(SETTLE_TYPING_KEYS):
                self.rd.tap(ord(SETTLE_TYPING_LETTERS[index % len(SETTLE_TYPING_LETTERS)]), inj)
                time.sleep(SETTLE_TYPING_INTERVAL_S)
            return self.settle_until(ctx, "typing", extra)(inj, deadline)

        return self.run_phase(phases, name, self.settle_cap_s + SETTLE_TYPING_KEYS * SETTLE_TYPING_INTERVAL_S + 5, body, extra)
    def snapshot(self) -> dict:
        (raw,) = self.bus.call(APP_ID, AUTOMATION_OBJECT_PATH, AUTOMATION_INTERFACE, "GetSnapshot", None, "(s)").unpack()
        return json.loads(raw)

    def match_count(self, query: str, deadline: float) -> Any:
        """The selected editor's occurrence count of `query` (the editor search bar)."""
        self.action("win", "set-search-query", [self._string(query)], deadline)
        # GtkSearchEntry delays `search-changed` (150 ms): wait past it before
        # asking whether the search is complete.
        time.sleep(0.6)
        self.wait_ready("search-complete", deadline)
        time.sleep(0.3)
        return find_key(self.snapshot(), "editor_match_count")

    def keystroke_interactions(self, phases: list[dict]) -> bool:
        """The keystroke group: TYPE_BURST typed at three places in one long paragraph and in the short one after it.

        Each position is placed in a `place-*` navigation phase (find the
        position's marker, close the search bar, collapse the selection to its
        end, so the caret sits just after the marker), then `key-*` types
        TYPE_BURST one key per KEYSTROKE_GAP_S. `verify` counts marker + burst
        for each position and the burst overall (and, as diagnostics, the marker
        alone, the burst inside it and before it); a burst that did not land at
        its marker fails the case.
        """
        cap = self.cap_s
        if not self.run_phase(phases, "background", cap, self.settle_after(2.0)):
            return False
        if not self.rd.available:
            phases.append({"name": "key-start", "kind": "interaction", "status": "unavailable", "reason": self.rd.error or "RemoteDesktop unavailable"})
            return False
        markers = self.generator.KEYSTROKE_MARKERS
        positions = [where for where in KEYSTROKE_POSITIONS if self.wanted(f"key-{where}")]
        # Unmeasured focus click inside the text: keys are not delivered before.
        ok, status = self.editor_click([], measured=False)
        if not ok:
            phases.append({"name": "key-start", "kind": "interaction", "status": status})
            return False
        time.sleep(0.3)
        self.wait_ready("app-startup", time.monotonic() + cap)

        placed: dict[str, Any] = {}

        def place(where: str):
            def body(inj, deadline):
                count = self.match_count(markers[where], deadline)
                surface_before = find_key(self.snapshot(), "active_transient_surface")
                self.action("win", "next-match", None, deadline)
                time.sleep(0.3)
                placed[where] = {"matches": count, "surface_before": surface_before}
                self.rd.tap(KEY_ESCAPE, inj)
                time.sleep(0.3)
                self.rd.tap(KEY_RIGHT, inj)
                time.sleep(0.5)
                snap = self.snapshot()
                placed[where]["surface_after"] = find_key(snap, "active_transient_surface")
                placed[where]["search_visible_after"] = find_key(snap, "editor_search_visible")
                return (True, "ok") if count == 1 else (False, f"marker-matches-{count}")

            return body

        def keys(inj, _deadline):
            for char in TYPE_BURST:
                self.rd.tap(ord(char), inj)
                time.sleep(KEYSTROKE_GAP_S)
            return True, "ok"

        for where in positions:
            if not self.run_phase(phases, f"place-{where}", cap, place(where)):
                phases[-1]["placed"] = placed
                return False
            if not self.run_phase(phases, f"key-{where}", cap, keys):
                return False
        verified: dict[str, Any] = {}

        def verify(inj, deadline):
            for where in positions:
                verified[where] = self.match_count(markers[where] + TYPE_BURST, deadline)
            # Diagnostics only (not part of the verdict): where a misplaced
            # burst went relative to its marker.
            for where in positions:
                m = markers[where]
                for label, q in (("marker", m), ("inside1", m[:1] + TYPE_BURST + m[1:4]), ("before", TYPE_BURST + m)):
                    verified[f"{where}-{label}"] = self.match_count(q, deadline)
            verified["burst_anywhere"] = self.match_count(TYPE_BURST, deadline)
            verified["modified"] = find_key(self.snapshot(), "modified")
            self.rd.tap(KEY_ESCAPE, inj)
            misses = keystroke_misses(verified, positions)
            return (True, "ok") if not misses else (False, "burst-misplaced:" + ",".join(misses))

        ok = self.run_phase(phases, "verify", cap, verify)
        phases[-1]["verified"] = verified
        phases[-1]["placed"] = placed
        return ok

    @staticmethod
    def _string(value: str):
        from gi.repository import GLib

        return GLib.Variant("s", value)

    @staticmethod
    def _uint32(value: int):
        from gi.repository import GLib

        return GLib.Variant("u", value)

    def open_phase(self, phases: list[dict], launch: Launch, cap_s: float, predicate: str = "file-open-complete", after_bus: Callable[[float], Any] | None = None) -> bool:
        start = launch.spawn_us
        wall = time.time()
        deadline = time.monotonic() + cap_s
        ok = self.wait_bus(launch, deadline)
        if ok and after_bus is not None:
            after_bus(deadline)
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
            {"name": "restore" if predicate in ("session-restore-complete", "recovery-restore-complete") else "open", "kind": "settle", "start_us": start, "end_us": now_us(), "wall_start": wall, "wall_end": time.time(), "status": "ok" if ok else status, "injections": []}
        )
        return ok

    def warm_up_profile(self, rep_dir: Path, env: dict[str, str], cap: float) -> bool:
        """Run the unmeasured launch that turns a fresh profile into a steady one."""
        warm_env = dict(env)
        # Not `probe*.jsonl`: the analysis reads only the measured launch.
        warm_env["LUSHTEXT_RESPONSIVENESS_LOG"] = str(rep_dir / "warmup-probe.jsonl")
        warm = self.launch(rep_dir, warm_env, [], "warmup")
        scratch: list[dict] = []
        ok = self.open_phase(scratch, warm, cap, predicate="app-startup")
        if ok:
            time.sleep(1.5)
            self.action("app", "quit", None, time.monotonic() + cap)
            try:
                warm.process.wait(timeout=cap)
            except subprocess.TimeoutExpired:
                ok = False
        if warm.process.poll() is None:
            self.kill(warm)
        return ok and warm.process.returncode == 0

    # -- one repetition -----------------------------------------------------

    def run_rep(self, case: Case, variant: str, rep: int, files: list[Path]) -> dict[str, Any]:
        rep_dir = self.artifact_dir / "cases" / case.id / variant / f"rep{rep}"
        rep_dir.mkdir(parents=True, exist_ok=True)
        backend = case.attrs.get("backend", "keyfile")
        on_disk = "profile" in case.attrs
        populated = bool(case.attrs.get("populated"))
        legacy_layout = fresh_profile_runs_migration(case)
        state_env = self.fresh_state(rep_dir, case.settings, backend, on_disk, populated, legacy_layout)
        log_path = rep_dir / "probe.jsonl"
        post_open = bool(case.attrs.get("post_open")) and self.post_open_enabled
        frame_phases = POST_OPEN_FRAME_PHASES if post_open else None
        env = self.app_env(variant, state_env, log_path, frame_phases=frame_phases)
        phases: list[dict] = []
        launches: list[Launch] = []
        listener = AtspiListener(rep_dir / "atspi.jsonl") if variant == "at" else None
        outcome = "ok"
        crashed = False
        wall_start = time.time()
        extra_states: list[str] = []
        recoveries = [0]
        # The post-open context (bound-post-open-settle): the launch whose
        # probe log the settle is computed from, its spawn time (records
        # before it are another launch's), and the live profile the restyle
        # phases write to.
        ctx: dict | None = None
        if post_open:
            ctx = {
                "log": log_path,
                "origin": 0,
                "documents": bool(files) or case.flow in ("session", "recovery"),
                "state_env": state_env,
                "wrap": bool(case.settings.get("word-wrap", True)),
                "tab_settle": bool(case.attrs.get("tab_settle")),
                "restyle": bool(case.attrs.get("restyle")),
            }

        def attach(launch: Launch, log: Path, live_state: dict[str, str]) -> None:
            if ctx is not None:
                ctx.update({"log": log, "origin": launch.spawn_us, "state_env": live_state, "wrap": bool(case.settings.get("word-wrap", True))})

        def new_state(log_name: str) -> tuple[dict[str, str], dict[str, str]]:
            """A fresh profile for another measured launch (steady ones warmed up first)."""
            fresh = self.fresh_state(rep_dir, case.settings, backend, on_disk, populated, legacy_layout)
            extra_states.append(fresh["STATE_ROOT"])
            fresh_env = self.app_env(variant, fresh, rep_dir / log_name, frame_phases=frame_phases)
            if case.attrs.get("profile") == "steady" and not self.warm_up_profile(rep_dir, fresh_env, self.cap_s):
                self.failures.append(f"steady-state warm-up failed for {case.id}@{variant}#{rep} ({log_name})")
            return fresh, fresh_env

        def recover() -> bool:
            for live in launches:
                if live.process.poll() is None:
                    self.kill(live)
            recoveries[0] += 1
            if recoveries[0] > 6:
                return False
            fresh = self.fresh_state(rep_dir, case.settings, backend, on_disk, populated, legacy_layout)
            extra_states.append(fresh["STATE_ROOT"])
            rlog = rep_dir / f"probe-r{recoveries[0]}.jsonl"
            renv = self.app_env(variant, fresh, rlog, frame_phases=frame_phases)
            relaunched = self.launch(rep_dir, renv, files, f"r{recoveries[0]}")
            launches.append(relaunched)
            attach(relaunched, rlog, fresh)
            scratch: list[dict] = []
            ok = self.open_phase(scratch, relaunched, cap)
            phases.append({"name": "recovery", "kind": "recovery", "status": scratch[-1]["status"] if scratch else "failed", "after": phases[-1]["name"] if phases else None})
            if ok:
                time.sleep(1.0)
                if self.rd.available:
                    # Give the fresh window keyboard focus (unmeasured),
                    # proven by the probe; the remaining paging and typing
                    # phases must not run on a window whose keys go
                    # elsewhere, so a miss ends the case here.
                    focused, focus_status = False, "not attempted"
                    for _attempt in range(3):
                        focused, focus_status = self.editor_click([], measured=False)
                        if focused:
                            break
                        time.sleep(0.5)
                    if not focused:
                        phases[-1]["status"] = f"focus-{focus_status}"
                        self.failures.append(f"{case.id}@{variant}#{rep}: no editor focus after recovery ({focus_status})")
                        return False
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
                    attach(relaunch, rep_dir / "probe-restore.jsonl", state_env)
                    ok = self.open_phase(phases, relaunch, cap, predicate="session-restore-complete") and self.standard_interactions(phases, tabs=len(files), recover=recover, ctx=ctx)
            elif case.flow == "recovery":
                ok = self.recovery_flow(rep_dir, env, files, phases, launches, cap, attach, state_env, ctx, recover)
            elif case.flow == "calibration":
                ok = self.run_calibration(rep_dir, state_env, phases, launches)
            else:
                if case.attrs.get("profile") == "steady":
                    # A steady-state profile is one a previous launch wrote:
                    # launch once with no document (so no session tab is
                    # restored into the measured launch), let it settle past
                    # the settings writer's persistence window, and quit
                    # gracefully so everything it holds is persisted.
                    if not self.warm_up_profile(rep_dir, env, cap):
                        self.failures.append(f"steady-state warm-up failed for {case.id}@{variant}#{rep}")
                launch = self.launch(rep_dir, env, files, "main")
                launches.append(launch)
                attach(launch, log_path, state_env)
                preview = None
                if case.flow == "preview":
                    # The side-by-side preview as early as the window can take
                    # it, and again once the document is open (the action
                    # needs an active tab; startup resets the pane hidden).
                    def preview(deadline: float) -> bool:
                        return self.action("win", "set-preview-pane-visible", [self._bool(True)], deadline)

                ok = self.open_phase(phases, launch, cap, after_bus=preview)
                if ok and preview is not None:
                    ok = preview(time.monotonic() + cap)
                    if not ok:
                        phases.append({"name": "preview", "kind": "settle", "status": "action-failed"})
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
                elif ok and case.flow == "keystroke":
                    ok = self.keystroke_interactions(phases)
                elif ok and case.flow == "reload":
                    ok = self.run_phase(phases, "background", cap, self.settle_after(1.0))

                    def reload(_inj, deadline):
                        self.rewrite_fixture(case, files[0])
                        time.sleep(1.5)
                        return self.wait_ready("file-open-complete", deadline)

                    ok = ok and self.run_phase(phases, "reload", cap, reload)
                    ok = ok and self.standard_interactions(phases, recover=recover)
                elif ok and case.flow == "preview":
                    ok = self.standard_interactions(phases, recover=recover, ctx=ctx, settle_then=lambda deadline: self.wait_ready("visual-geometry-settled", deadline))
                elif ok:
                    ok = self.standard_interactions(phases, tabs=max(1, len(files)), recover=recover, ctx=ctx)
                if ok and ctx is not None and case.attrs.get("typing"):
                    ok = self.type_during_settle(rep_dir, files, phases, launches, new_state)
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
        if frameless_row(row):
            self.failures.append(f"no frame records in {case.id}@{variant}#{rep}: the probe's frame stream is broken")
        row["wall_start"] = wall_start
        row["wall_end"] = time.time()
        (rep_dir / "phases.json").write_text(json.dumps(row, indent=2) + "\n", encoding="utf-8")
        self.append_result(row)
        return row

    def recovery_flow(self, rep_dir: Path, env: dict[str, str], files: list[Path], phases: list[dict], launches: list[Launch], cap: float, attach: Callable, state_env: dict[str, str], ctx: dict | None, recover: Callable[[], bool]) -> bool:
        """`recovery/small-drafts`: file-backed drafts recovered at startup.

        An unmeasured seeding launch opens the documents, types a few letters
        into each, waits for the draft autosave and the session save, and is
        killed (SIGKILL: no close flow runs, as in a crash). The measured
        launch then starts with no file arguments and restores the session
        with both drafts. Typed edits are never saved: the documents' bytes
        are unchanged, which the integrity ledger checks.
        """
        if not self.rd.available:
            phases.append({"name": "restore", "kind": "settle", "status": "unavailable", "reason": self.rd.error or "RemoteDesktop unavailable (the drafts are typed)"})
            return False
        seed_env = dict(env)
        # Not `probe*.jsonl`: the analysis reads only the measured launch.
        seed_env["LUSHTEXT_RESPONSIVENESS_LOG"] = str(rep_dir / "seed-probe.jsonl")
        seed = self.launch(rep_dir, seed_env, files, "seed")
        launches.append(seed)
        scratch: list[dict] = []
        if not self.open_phase(scratch, seed, cap):
            phases.append({"name": "restore", "kind": "settle", "status": f"seed-{scratch[-1]['status'] if scratch else 'failed'}"})
            return False
        deadline = time.monotonic() + cap
        for index in range(len(files)):
            self.action("win", "select-tab", [self._uint32(index)], deadline)
            self.wait_ready("file-open-complete", deadline)
            ok, status = self.editor_click([], measured=False)
            if not ok:
                phases.append({"name": "restore", "kind": "settle", "status": f"seed-{status}"})
                return False
            time.sleep(0.1)
            for char in RECOVERY_TYPED:
                self.rd.tap(ord(char), [])
                time.sleep(0.05)
        # The first dirty autosave is debounced (750 ms), the session save 500 ms.
        time.sleep(1.5)
        saved, status = self.wait_ready("save-complete", deadline)
        time.sleep(0.6)
        self.wait_ready("app-startup", deadline)
        self.kill(seed)
        if not saved:
            phases.append({"name": "restore", "kind": "settle", "status": f"seed-drafts-{status}"})
            return False
        restore = self.launch(rep_dir, env, [], "restore")
        launches.append(restore)
        attach(restore, Path(env["LUSHTEXT_RESPONSIVENESS_LOG"]), state_env)
        return self.open_phase(phases, restore, cap, predicate="recovery-restore-complete") and self.standard_interactions(phases, tabs=len(files), recover=recover, ctx=ctx)

    @staticmethod
    def _bool(value: bool):
        from gi.repository import GLib

        return GLib.Variant("b", value)

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
                delivery = input_delivery(parsed, row["input_to_frame"], start, end)
                if delivery is not None:
                    row["delivery"] = delivery
                row["im"] = im_summary(parsed, start, end)
                if phase["name"].startswith("key-"):
                    row["keystrokes"] = keystroke_stalls(parsed, start, end, budgets["B_INTERACT_US"])
                if variant == "at":
                    row["at"] = at_summary(at_events, start, end)
            phase_rows.append(row)
        post_open = post_open_entries(parsed, phases, read_settle_quiet())
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
            "frame_records": len(merged["frame"]),
            "window_construction_us": window_us,
            "construction": construction_summary(merged, case.attrs, self.binary),
            "only_phases": sorted(self.only_phases) if self.only_phases else None,
            "exit_codes": [launch.process.returncode for launch in launches],
            "renderer": self.renderer,
            "renderer_actual": actual_renderer(launches),
            "phases": phase_rows,
            "post_open": post_open,
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


def find_key(value: Any, key: str) -> Any:
    """The first value under `key` anywhere in a JSON tree, depth first."""
    if isinstance(value, dict):
        if key in value:
            return value[key]
        children = list(value.values())
    elif isinstance(value, list):
        children = value
    else:
        return None
    for child in children:
        found = find_key(child, key)
        if found is not None:
            return found
    return None


def keystroke_misses(verified: dict[str, Any], positions: Iterable[str]) -> list[str]:
    """The keystroke positions whose burst did not land right after its marker.

    `verified[where]` is the count of marker + TYPE_BURST, which must be 1;
    `verified["burst_anywhere"]`, the count of TYPE_BURST, must equal the
    number of positions typed, so no burst also landed somewhere else.
    """
    positions = list(positions)
    misses = [where for where in positions if verified.get(where) != 1]
    if verified.get("burst_anywhere") != len(positions):
        misses.append("burst-count")
    return misses


def construction_summary(records: dict[str, list[dict]], attrs: dict[str, Any], binary: Path) -> dict[str, Any] | None:
    """The startup and window-construction spans of the measured launch.

    `startup_*`, `window_construction` and `first_frame` come from the
    probe's `span` records; every other span is a construction stage. The
    largest dispatch before the first frame is the largest watchdog lateness
    whose dispatch happened before the first frame's paint (it bounds the
    longest main-loop stall, design D4 of probe-long-line-toolkit-costs).
    Only the first window's spans count: a later window is not first launch.
    """
    spans = records.get("span", [])
    if not spans:
        return None
    first: dict[str, dict] = {}
    stages: dict[str, int] = {}
    phase_spans: list[dict] = []
    shown_spans: list[dict] = []
    layout: list[list[Any]] = []
    composition: dict[str, int] = {}
    for record in spans:
        name = str(record.get("name"))
        duration = int(record["end_us"]) - int(record["start_us"])
        if name in CONSTRUCTION_SPANS or name == COMPOSITION_COMPLETE:
            first.setdefault(name, record)
        elif name == COMPOSITION_ACTIVATION_SHOWN:
            # Read below by its origin; it is a span to a frame, not a stage.
            shown_spans.append(record)
        elif name in FRAME_PHASE_SPANS:
            phase_spans.append(record)
        elif name in LAYOUT_PROBE_SPANS:
            layout.append([name, duration])
        elif name.startswith(COMPOSITION_STAGE_PREFIX):
            composition.setdefault(name[len(COMPOSITION_STAGE_PREFIX):], duration)
            if name == COMPOSITION_PRESENT:
                first.setdefault(name, record)
        elif "first_frame" not in first:
            stages[name] = stages.get(name, 0) + duration
    result: dict[str, Any] = {
        "profile": attrs.get("profile", "fresh"),
        "backend": attrs.get("backend", "keyfile"),
        "populated": bool(attrs.get("populated")),
        "build_profile": "release" if "/release/" in str(binary) else "debug",
    }
    for name in CONSTRUCTION_SPANS:
        record = first.get(name)
        result[f"{name}_us"] = int(record["end_us"]) - int(record["start_us"]) if record else None
    painted = first.get("first_frame")
    largest = None
    if painted is not None:
        befores = [int(r["lateness_us"]) for r in records.get("late", []) if int(r["t_us"]) <= int(painted["end_us"]) + 1]
        largest = max(befores) if befores else 0
    result["largest_dispatch_before_first_frame_us"] = largest
    result["stages_us"] = dict(sorted(stages.items(), key=lambda item: -item[1]))
    # Composition: every dispatch from the start until the later of the last
    # stage's frame and the activation's shown frame (or, before staged
    # composition, until the first frame) is budgeted.
    complete = first.get(COMPOSITION_COMPLETE)
    origin = int(complete["start_us"]) if complete else None

    def since_origin(at_us: int | None) -> int | None:
        return at_us - origin if at_us is not None and origin is not None else None

    # The first window's own activation-shown span: it shares that window's
    # origin (another window's span would stretch the fill-in).
    shown = next((r for r in shown_spans if int(r["start_us"]) == origin), None)
    result["composition_complete_us"] = since_origin(int(complete["end_us"])) if complete else None
    result["activation_shown_us"] = since_origin(int(shown["end_us"])) if shown else None
    # The first frame painted on the mapped window, the same way in every
    # build: the end of the first frame paint after the present stage ended.
    # (Derived here rather than read from the app's own first-frame span so a
    # build that reported an unseen frame, ledger A51, compares honestly; once
    # every compared build reports the mapped frame, `first_frame` suffices.)
    present = first.get(COMPOSITION_PRESENT)
    present_end = int(present["end_us"]) if present is not None else None
    mapped_paint_end = None
    if present_end is not None:
        mapped_paint_end = min((int(r["end_us"]) for r in phase_spans if r.get("name") == "frame_paint" and int(r["start_us"]) >= present_end), default=None)
    if mapped_paint_end is None and painted is not None:
        mapped_paint_end = int(painted["end_us"])
    result["first_painted_frame_us"] = since_origin(mapped_paint_end)
    # The presentation of that first mapped frame: the first frame record that
    # started after the present stage ended. `None` when the compositor gave
    # that frame no feedback; a later frame's presentation is not it.
    first_mapped = None
    if present_end is not None:
        first_mapped = min((r for r in records.get("frame", []) if int(r["frame_time_us"]) >= present_end), key=lambda r: int(r["frame_time_us"]), default=None)
    presented = first_mapped.get("presentation_us") if first_mapped else None
    result["first_presentation_us"] = since_origin(int(presented)) if presented else None
    filled_end = max((int(r["end_us"]) for r in (complete, shown) if r is not None), default=None)
    result["fill_in_us"] = filled_end - mapped_paint_end if complete and mapped_paint_end is not None else None
    window_end_us = filled_end if filled_end is not None else (int(painted["end_us"]) if painted is not None else None)
    lates = [r for r in records.get("late", []) if window_end_us is not None and int(r["t_us"]) <= window_end_us + 1]
    result["largest_composition_dispatch_us"] = max((int(r["lateness_us"]) for r in lates), default=0) if window_end_us is not None else None
    result["dispatches_over_4ms_us"] = [int(r["lateness_us"]) for r in sorted(lates, key=lambda r: int(r["t_us"])) if int(r["lateness_us"]) >= LISTED_DISPATCH_US]
    result["composition_stages_us"] = composition
    result["frame_phases_us"] = frame_phases(phase_spans)[:FRAME_PHASE_FRAMES]
    result["layout_probe_us"] = layout
    return result


def frame_phases(spans: list[dict]) -> list[dict[str, int | None]]:
    """One `{layout, paint}` per frame, in time order.

    GDK runs the layout phase again within one frame when a layout handler
    queues another (`gdkframeclockidle.c`, up to four times), and the probe
    stamps each run, so a frame may carry several `frame_layout` spans before
    its one `frame_paint`. Every layout span up to a paint belongs to that
    paint's frame; pairing the i-th layout with the i-th paint instead
    attributed a frame's second, near-empty layout run to the next frame.
    """
    frames: list[dict[str, int | None]] = []
    layout: int | None = None
    for record in sorted(spans, key=lambda r: (int(r["start_us"]), r.get("name") != "frame_layout")):
        duration = int(record["end_us"]) - int(record["start_us"])
        if record.get("name") == "frame_layout":
            layout = (layout or 0) + duration
        else:
            frames.append({"layout": layout, "paint": duration})
            layout = None
    return frames


def construction_verdicts(rows: list[dict], budget_us: int) -> list[dict]:
    """One row per measured launch: every budgeted construction span against B_settle."""
    out = []
    for row in rows:
        construction = row.get("construction")
        if not construction:
            continue
        over = [name for name in CONSTRUCTION_BUDGETED if (construction.get(f"{name}_us") or 0) > budget_us]
        fill_in = construction.get("fill_in_us")
        out.append(
            {
                "case": row["case"],
                "variant": row["variant"],
                "rep": row["rep"],
                **{k: v for k, v in construction.items() if k != "stages_us"},
                "largest_stages_us": dict(list(construction.get("stages_us", {}).items())[:6]),
                "fill_in_within_accepted": None if fill_in is None else fill_in <= ACCEPTED_FILL_IN_US,
                "fill_in_over": fill_in is not None and fill_in > ACCEPTED_FILL_IN_US,
                "over_budget": over,
                "verdict": "over" if over else "within",
            }
        )
    return out


def fill_in_median(construction: list[dict], reference_frame_us: int) -> dict[str, Any]:
    """The content fill-in's median over the measured runs, against its target
    of FILL_IN_MEDIAN_TARGET_FRAMES reference frames (reported, never enforced)."""
    values = [int(c["fill_in_us"]) for c in construction if c.get("fill_in_us") is not None]
    target = FILL_IN_MEDIAN_TARGET_FRAMES * reference_frame_us
    if not values:
        return {"median_us": None, "target_us": target, "within_target": None, "runs": 0}
    median = int(statistics.median(values))
    return {"median_us": median, "target_us": target, "within_target": median <= target, "runs": len(values)}


def seed_populated_workspace(data_dir: Path, folder: Path) -> int:
    """Write one workspace folder of synthetic entries and the `workspaces.json`
    that restores it; return the number of top-level entries. Names only, with
    one line of synthetic text per file: never a user file."""
    folder.mkdir(parents=True, exist_ok=True)
    for index in range(POPULATED_DIRECTORIES):
        directory = folder / f"dir-{index:03}"
        directory.mkdir(exist_ok=True)
        (directory / "nested.txt").write_text("nested\n", encoding="utf-8")
    for index in range(POPULATED_FILES):
        (folder / f"file-{index:03}.txt").write_text(f"synthetic {index}\n", encoding="utf-8")
    data_dir.mkdir(parents=True, exist_ok=True)
    workspaces = {
        "kind": "dev.cominotti.lushtext.workspace-state",
        "version": 1,
        "data": {
            "current_scope": {"kind": "all"},
            "workspaces": [
                {
                    "id": "responsiveness-populated",
                    "name": "Populated",
                    "folders": [{"id": "responsiveness-populated-folder", "path": str(folder)}],
                }
            ],
        },
    }
    (data_dir / "workspaces.json").write_text(json.dumps(workspaces, indent=2) + "\n", encoding="utf-8")
    return POPULATED_DIRECTORIES + POPULATED_FILES


def isolate_dconf_activation(bus: Bus, config_dir: Path) -> None:
    """Make the private bus start dconf-service with `config_dir` as its config home."""
    from gi.repository import GLib

    bus.call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "UpdateActivationEnvironment",
        GLib.Variant("(a{ss})", ({"XDG_CONFIG_HOME": str(config_dir)},)),
    )


def stop_dconf_service(bus: Bus) -> None:
    """Stop a dconf-service the private bus started, so the next write restarts it."""
    try:
        (pid,) = bus.call(
            "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "GetConnectionUnixProcessID", dbus_string_arg("ca.desrt.dconf"), "(u)"
        ).unpack()
    except Exception:  # noqa: BLE001 - not running
        return
    try:
        os.kill(int(pid), signal.SIGTERM)
    except ProcessLookupError:
        return
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        try:
            (owned,) = bus.call(
                "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner", dbus_string_arg("ca.desrt.dconf"), "(b)"
            ).unpack()
        except Exception:  # noqa: BLE001
            return
        if not owned:
            return
        time.sleep(0.05)
    raise RuntimeError("dconf-service did not stop")


def dbus_string_arg(value: str):
    from gi.repository import GLib

    return GLib.Variant("(s)", (value,))


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

    def attributable(group_rows: Callable[[int, Any], tuple[list[dict], list[dict]]], phases_budgets, allowance: dict[str, int] | None = None) -> Callable[[int], bool | None]:
        """Per phase, the measured side's stall minus its counterpart's (less
        any per-phase allowance for a cost that is not the threshold's) within
        the phase group's budget."""
        allowance = allowance or {}

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
                # Phase by phase: pooled maxima let a counterpart outlier in
                # one phase hide the measured side's cost in another.
                for phase in phases:
                    a, b = phase_lateness(with_rows, (phase,)), phase_lateness(without_rows, (phase,))
                    if a is None or b is None:
                        continue
                    results.append(classify(max(0, a - b - allowance.get(phase, 0)), tick, slack, budget))
            return None if not results else all(v == "within" for v in results)

        return verdict

    # The size-independent first highlight of an HTML document in a fresh
    # process (~42 ms on the reference run, C6's), taken as the html-minus-txt
    # background stall of the smallest short-line total-size document. It is
    # not a length cost, so L_syntax's background half reads the difference in
    # excess of it; without that document nothing is discounted.
    size_list = sorted({r["length"] for r in rows if r.get("group") == "total-size" and usable(r)})
    first_highlight_us = 0
    if size_list:
        smallest = [r for r in rows if r.get("group") == "total-size" and r.get("length") == size_list[0] and usable(r)]
        html_bg = phase_lateness([r for r in smallest if r.get("attrs", {}).get("ext") == "html"], ("background",))
        txt_bg = phase_lateness([r for r in smallest if r.get("attrs", {}).get("ext") == "txt"], ("background",))
        if html_bg is not None and txt_bg is not None:
            first_highlight_us = max(0, html_bg - txt_bg)
    derived["first_highlight_us"] = first_highlight_us

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
        per_flavor[flavor] = largest_passing(lengths, attributable(syntax_rows, ((("scroll",), b_i), (SETTLE_PHASES, b_s)), {"background": first_highlight_us}))
    measured = {f: o for f, o in per_flavor.items() if o.get("value") is not None or "upper_bound" in o}
    if measured:
        deciding = min(measured, key=lambda f: measured[f].get("value") if measured[f].get("value") is not None else -1)
        outcome = dict(measured[deciding])
        outcome["deciding_flavor"] = deciding
    else:
        deciding = None
        outcome = {"value": None, "reason": "no usable cases"}
    outcome["per_flavor"] = per_flavor
    describe("L_syntax", ".html minus .txt stall at the same length and flavour, phase by phase: scroll <= B_interact, open and background <= B_settle (background in excess of the size-independent first highlight, the smallest total-size document's .html minus .txt background); a capped .html phase fails; the worst flavour decides", outcome, [r["case"] for r in rows if r.get("group") == "paragraph" and not r.get("attrs", {}).get("minimap")])

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

    # The dense reference case (a full screen of short dense lines): the cost
    # of painting and scrolling a screen of text, which every document that
    # fills the screen pays whatever its line lengths (5-7 ms per scroll
    # dispatch on the idle reference machine, 2-3 ms under load). V_batch
    # bounds validation beyond that paint, so it reads scroll and Page Down as
    # their excess over the reference; without it, as measured. N_visible
    # deliberately does not: it bounds what is painted, paint included.
    reference_rows = [r for r in rows_for(rows, "dense", DENSE_REFERENCE_LENGTH) if usable(r)]
    dense_floor = {phase: phase_lateness(reference_rows, (phase,)) or 0 for phase in ("scroll", "pagedown")}
    derived["dense_floor_us"] = dense_floor if reference_rows else None

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
                if editable_latency(entry)
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
    dense_lengths = sorted({r["length"] for r in rows if r.get("group") == "dense" and not (reference_rows and r.get("length") == DENSE_REFERENCE_LENGTH)})

    # The background phase also holds the post-open stall every document
    # shows (baseline/small); validation is its excess over that stall.
    post_open_us = phase_lateness([r for r in rows if r.get("case") == "baseline/small" and usable(r)], ("background",)) or 0

    def dense_verdict(length: int) -> bool | None:
        selected = [r for r in rows_for(rows, "dense", length) if usable(r)]
        if selected and hung(selected, ("scroll", "pagedown", "background")):
            return False
        interactions = [None if (v := phase_lateness(selected, (phase,))) is None else max(0, v - dense_floor[phase]) for phase in ("scroll", "pagedown")]
        background = phase_lateness(selected, ("background",))
        values = [v for v in (*interactions, None if background is None else max(0, background - post_open_us)) if v is not None]
        return None if not values else classify(max(values), tick, slack, b_i) == "within"

    dense_outcome = largest_passing(dense_lengths, dense_verdict)
    if rows_per_batch and dense_outcome.get("value"):
        dense_outcome["line_length"] = dense_outcome["value"]
        dense_outcome["value"] = dense_outcome["value"] * rows_per_batch
        dense_outcome["rows_per_batch_default_zoom"] = rows_per_batch
    else:
        dense_outcome = {"value": None, "reason": "needs the A31 rows-per-batch observation (pass --axiom-observations) and a passing dense case", "dense_line_length": dense_outcome.get("value"), "dense_reason": dense_outcome.get("reason")}
    describe("V_batch", "largest line length x A31 rows per 2000 px batch whose dense validation stalls <= B_interact (scroll and Page Down as their excess over the dense reference case, a full screen of 120-character lines, or as measured without it; background as its excess over the baseline/small post-open stall)", dense_outcome, [r["case"] for r in rows if r.get("group") == "dense"])

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
            latencies.extend(e["latency_us"] for e in phase.get("input_to_frame", []) if editable_latency(e))
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


def frameless_row(row: dict) -> bool:
    """Whether a measured case ran with the probe but wrote no `frame` record.

    Every launch paints, so such a row means the probe's frame stream is
    broken, not that the case was fast: the first-frame time, the frame-gap
    p95, and the input-to-frame verdicts would all read None or empty. After
    `first-frame-window-shell` the probe held every frame behind one that never
    received presentation feedback, and the lane reported None silently. The
    calibration case and rows that did not run (skipped, crashed, failed) are
    not judged; a row from an older run without `frame_records` is unknown."""
    if row.get("case") == "baseline/calibration" or row.get("outcome") not in ("ok", "exceeded_cap"):
        return False
    if not row.get("probe_started", True) or "frame_records" not in row:
        return False
    return int(row["frame_records"]) == 0


def count_frame_records(artifact_dir: Path, row: dict) -> int | None:
    """The `frame` records a row's case directory holds, for rows written
    before the driver recorded `frame_records`; None when it has no log."""
    rep_dir = artifact_dir / "cases" / str(row.get("case")) / str(row.get("variant")) / f"rep{row.get('rep')}"
    logs = sorted(rep_dir.glob("probe*.jsonl")) if rep_dir.is_dir() else []
    if not logs:
        return None
    return sum(len(parse_probe_log(log.read_text(encoding="utf-8", errors="replace").splitlines())["records"]["frame"]) for log in logs)


def fresh_profile_runs_migration(case: "Case") -> bool:
    """Whether a case's profile is seeded with the legacy layout key, so its
    launch runs the one-shot split-view migration: the fresh construction
    profiles that show no pane (a populated profile names its panes)."""
    return case.attrs.get("profile") == "fresh" and not case.attrs.get("populated") and not any(key in case.settings for key in ("workspace-sidebar-visible", "properties-sidebar-visible"))


def frameless_cases(artifact_dir: Path, rows: list[dict]) -> list[str]:
    """`case@variant#rep` of every frameless row (`frameless_row`)."""
    out = []
    for row in rows:
        if "frame_records" not in row:
            counted = count_frame_records(artifact_dir, row)
            if counted is not None:
                row = dict(row, frame_records=counted)
        if frameless_row(row):
            out.append(f"{row['case']}@{row['variant']}#{row['rep']}")
    return out


def run_failed(
    missing: list[str],
    integrity: dict,
    run_info: dict,
    warnings: dict,
    frameless: list[str] | None = None,
    misdirected: list[str] | None = None,
    withheld: list[str] | None = None,
) -> bool:
    """The lane fails on missing artifacts, an integrity violation, a driver
    failure, a warning of a pre-existing blocker the programme has fixed, a
    measured case that wrote no `frame` record (`frameless_row`), a phase
    whose typing reached no document (`misdirected_phase`), or a phase whose
    typed text was drawn only in a withheld frame (`withheld_frame_cases`)."""
    return bool(missing) or bool(integrity.get("violations")) or bool(run_info.get("driver_failures")) or bool(warnings.get("regressions")) or bool(frameless) or bool(misdirected) or bool(withheld)


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
            if phase.get("name") == "verify" and isinstance(phase.get("verified"), dict):
                # A burst counts only when the search found it exactly once next
                # to its position's marker.
                unverified = entry.setdefault("unverified_positions", set())
                for position in KEYSTROKE_POSITIONS:
                    count = phase["verified"].get(position)
                    if count is not None and count != 1:
                        unverified.add(position)
            if "stall" not in phase:
                if phase.get("status") in ("unavailable",):
                    entry["phases"].setdefault(phase["name"], {"status": "unavailable"})
                continue
            p = entry["phases"].setdefault(phase["name"], {"stalls": [], "frame_p95": [], "itf_us": [], "itf_frames": [], "im_calls": 0, "im_max_bytes": None, "at_events": 0, "at_max_payload": None, "status": []})
            p["stalls"].append(phase["stall"]["lateness_us"])
            p["status"].append(phase.get("status"))
            if "keystrokes" in phase:
                keys = p.setdefault("keys", {"worst": [], "summed": []})
                keys["worst"].extend(phase["keystrokes"]["worst_us"])
                keys["summed"].extend(phase["keystrokes"]["summed_us"])
            if phase["frames"].get("p95_gap_us") is not None:
                p["frame_p95"].append(phase["frames"]["p95_gap_us"])
            for e in phase.get("input_to_frame", []):
                if editable_latency(e):
                    p["itf_us"].append(e["latency_us"])
                    p["itf_frames"].append(e["frames"])
                elif "latency_us" in e:
                    # Keys into a protected document, apart from editable typing.
                    p.setdefault("itf_protected_us", []).append(e["latency_us"])
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
            if "itf_protected_us" in phase:
                protected = phase.pop("itf_protected_us")
                phase["protected_input_to_frame"] = {"keys": len(protected), "max_us": max(protected), "median_us": nearest_rank(protected, 50)}
            if "keys" in phase:
                keys = phase.pop("keys")
                phase["keystroke"] = {
                    "keys": len(keys["worst"]),
                    "worst_median_us": nearest_rank(keys["worst"], 50),
                    "worst_p90_us": nearest_rank(keys["worst"], 90),
                    "summed_median_us": nearest_rank(keys["summed"], 50),
                }
        if "unverified_positions" in entry:
            entry["unverified_positions"] = sorted(entry["unverified_positions"])
        out.append(entry)
    return out


def write_report(artifact_dir: Path, axiom_path: Path | None, toolkit_floors: list[str] | None = None) -> int:
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
    construction = construction_verdicts([r for r in rows if r.get("group") == "baseline"], budgets["B_SETTLE_US"])
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
    frameless = frameless_cases(artifact_dir, rows)
    misdirected = misdirected_cases(rows)
    withheld = withheld_frame_cases(rows)
    # The C6 verdict (bound-post-open-settle D8) over the post-open rows; with
    # a side-labelled merge (merge-runs --from baseline=DIR --from branch=DIR)
    # it also applies the no-regression rule.
    floor_spec = ";".join([os.environ.get(TOOLKIT_FLOORS_ENV, DEFAULT_TOOLKIT_FLOORS)] + list(toolkit_floors or []))
    c6 = None
    if any(r.get("post_open") for r in rows):
        c6 = c6_verdict(
            rows,
            budgets,
            parse_toolkit_floors(floor_spec),
            os.environ.get("RESPONSIVENESS_C6_BASELINE_SIDE", C6_BASELINE_SIDE),
            os.environ.get("RESPONSIVENESS_C6_BRANCH_SIDE", C6_BRANCH_SIDE),
        )
    report = {
        "run": run_info,
        "budgets": budgets,
        "calibration": calibration,
        "storage": storage,
        "integrity": integrity,
        "warnings": warnings,
        "derived": derived,
        "window_construction_c4": window,
        "construction_c4": construction,
        "install_c3": c3,
        "cases": summarise_cases(rows),
        "missing_artifacts": missing,
        "frameless_cases": frameless,
        "misdirected_typing": misdirected,
        "withheld_typing_frames": withheld,
        "post_open": post_open_report(rows),
        "c6_verdict": c6,
    }
    (artifact_dir / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    (artifact_dir / "report.md").write_text(render_markdown(report), encoding="utf-8")
    failed = run_failed(missing, integrity, run_info, warnings, frameless, misdirected, withheld)
    # Window construction is the one budget this lane can enforce
    # (bound-window-construction-dispatch): with
    # RESPONSIVENESS_CONSTRUCTION_ENFORCE=1, a LushText-owned span or a
    # dispatch before the first frame over B_settle on the release build fails
    # the run (never in CI, which is trend evidence only). It is opt-in until
    # the residual it measures has an owner that can meet it: after the
    # settings writer removed every construction-time backend write, the
    # remaining 29-42 ms are template instantiation and GTK's first present and
    # paint of the whole window, which no staging of secondary surfaces brings
    # under B_settle (docs/next/long-line-documents.md, C4).
    # The content fill-in (shorten-window-content-fill-in) is enforced the same
    # way: a release run over the accepted 40 ms fails only under the switch.
    construction_over = [c for c in construction if c["verdict"] == "over" and c.get("build_profile") == "release"]
    fill_in_over = [c for c in construction if c.get("fill_in_over") and c.get("build_profile") == "release"]
    construction_enforced = not os.environ.get("CI") and os.environ.get("RESPONSIVENESS_CONSTRUCTION_ENFORCE") == "1"
    if (construction_over or fill_in_over) and construction_enforced:
        failed = True
    summary = {
        "status": "failed" if failed else "passed",
        "run_id": run_info.get("run_id"),
        "tier": run_info.get("tier"),
        "missing_artifacts": missing,
        "integrity_violations": len(integrity.get("violations", [])),
        "driver_failures": run_info.get("driver_failures", []),
        "fixed_defect_regressions": sorted(warnings.get("regressions", {})),
        "frameless_cases": frameless,
        "misdirected_typing": misdirected,
        "withheld_typing_frames": withheld,
        "budget_verdict": derived["budget_verdict"].get("verdict"),
        "construction_over_budget": [f"{c['case']}@{c['variant']}#{c['rep']}: {','.join(c['over_budget'])}" for c in construction_over],
        "fill_in_over_accepted": [f"{c['case']}@{c['variant']}#{c['rep']}: fill-in {c['fill_in_us']} us" for c in fill_in_over],
        "fill_in_median": fill_in_median(construction, budgets["REFERENCE_FRAME_US"]),
        "construction_enforced": construction_enforced,
        "c6_verdict": c6.get("verdict") if c6 else None,
    }
    # The C6 verdict is a record for docs/next/long-line-documents.md, not a
    # gate, unless RESPONSIVENESS_C6_ENFORCE=1 asks for one.
    if c6 and c6.get("verdict") == "not met" and os.environ.get("RESPONSIVENESS_C6_ENFORCE") == "1":
        failed = True
        summary["status"] = "failed"
    (artifact_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))
    return 1 if failed else 0


def post_open_report(rows: list[dict]) -> list[dict]:
    """Every post-open settle table, one entry per case, repetition and phase."""
    out = []
    for row in rows:
        for entry in row.get("post_open") or []:
            keys = [k["latency_us"] for k in entry.get("keys", []) if "latency_us" in k]
            key_summary = None
            if "keys" in entry:
                key_summary = {
                    "count": len(entry["keys"]),
                    "matched": len(keys),
                    "median_us": _median(keys) if keys else None,
                    "max_us": max(keys, default=None),
                }
            out.append(
                {
                    "case": row["case"],
                    "variant": row["variant"],
                    "rep": row.get("rep", 0),
                    "side": row.get("side"),
                    "phase": entry["phase"],
                    "kind": entry["kind"],
                    "status": entry["status"],
                    "settle_us": entry.get("settle_us"),
                    "laid_out_us": entry.get("laid_out_us"),
                    "worst_us": max((d["duration_us"] for d in entry.get("dispatches", [])), default=0),
                    "stage_worst_us": entry.get("stage_worst_us", {}),
                    "dispatches": [{k: d[k] for k in ("at_us", "duration_us", "attribution", "source")} for d in entry.get("dispatches", [])],
                    "keys": key_summary,
                    "interaction_us": (entry.get("interaction") or {}).get("duration_us") if entry["kind"] == "interaction" else None,
                }
            )
    return out


def render_post_open(report: dict) -> list[str]:
    """The post-open settle table and the C6 verdict (bound-post-open-settle)."""
    entries = report.get("post_open") or []
    if not entries:
        return []
    budgets = report["budgets"]
    lines = [
        "## Post-open settle (C6)",
        "",
        f"Every dispatch of at least {POST_OPEN_DISPATCH_FLOOR_US} us from the activation turn (a restored tab's selection, a restyle's trigger) until the driver-computed settle "
        f"(no highlight or upper mark for SETTLE_QUIET_US, a G_PRIORITY_LOW sentinel after the last mark and post-open span), attributed by the post-open span that covers it, "
        "else `highlight` or `validation` when it ends at that mark, else `unattributed`. Read by span, not by phase. Times in ms after the activation; durations in us.",
        "",
        "| Case | Side | Phase | Status | Activation to settled (ms) | Worst (us) | Keys (median / max us) | Dispatches (at ms: us stage) |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for e in entries:
        settle = f"{e['settle_us'] / 1000:.1f}" if e.get("settle_us") is not None else "cap"
        keys = f"{e['keys']['median_us']} / {e['keys']['max_us']} ({e['keys']['matched']}/{e['keys']['count']})" if e.get("keys") else ""
        if e.get("interaction_us") is not None:
            keys = f"interaction {e['interaction_us']}"
        dispatches = ", ".join(f"{d['at_us'] / 1000:.1f}: {d['duration_us']} {d['attribution']}" for d in e["dispatches"])
        lines.append(f"| {e['case']}@{e['variant']}#{e['rep']} | {e.get('side') or ''} | {e['phase']} | {e['status']} | {settle} | {e['worst_us']} | {keys} | {dispatches} |")
    c6 = report.get("c6_verdict")
    if not c6:
        return lines + [""]
    lines += [
        "",
        "## C6 verdict",
        "",
        f"**{c6['verdict']}** over {c6.get('branch_rows', 0)} branch row(s) (side `{c6.get('sides', {}).get('branch')}`) and {c6.get('baseline_rows', 0)} baseline row(s) (side `{c6.get('sides', {}).get('baseline')}`). "
        f"Rules: every open and restored-tab post-open dispatch until settled <= B_SETTLE_US {budgets['B_SETTLE_US']} us; every type-during-settle key <= B_INTERACT_US {budgets['B_INTERACT_US']} us; "
        f"every restyle interaction dispatch <= B_INTERACT_US and each later dispatch <= B_SETTLE_US; per case, the median of each stage's worst dispatch and of activation-to-laid-out (the language attach's wait exempt, at most ATTACH_DEADLINE_US) <= the baseline's + REFERENCE_FRAME_US {budgets['REFERENCE_FRAME_US']} us.",
        "",
    ]
    for failure in c6.get("failures", [])[:60]:
        lines.append(f"- {failure}")
    if len(c6.get("failures", [])) > 60:
        lines.append(f"- ... {len(c6['failures']) - 60} more in report.json")
    if c6.get("regressions"):
        lines += ["", "| Case | Phase | Stage | Branch median (us) | Baseline median (us) | Limit (us) |", "|---|---|---|---|---|---|"]
        for r in c6["regressions"]:
            lines.append(f"| {r['case']}@{r['variant']} | {r['phase']} | {r['stage']} | {r['branch_median_us']} | {r['baseline_median_us']} | {r['limit_us']} |")
    if c6.get("new_stages"):
        lines += ["", "New on the branch (no baseline median; under the absolute bounds only): " + ", ".join(f"{n['case']}@{n['variant']} {n['phase']} {n['stage']}" for n in c6["new_stages"])]
    lines += ["", "### Toolkit-floor exceptions", ""]
    if not c6.get("exceptions"):
        lines.append(f"None listed ({TOOLKIT_FLOORS_ENV} / report --toolkit-floor).")
    for e in c6.get("exceptions", []):
        lines.append(f"- {e['case']}@{e['variant']}#{e['rep']} {e['phase']}: `{e['attribution']}` {e['duration_us']} us, axiom {e['axiom']}, upstream report {e['report'] or 'none recorded'}; keys that waited behind it: {e['overlapping_keys_us'] or 'none'} us")
    if c6.get("baseline_over_budget"):
        lines += ["", "### Baseline dispatches over B_SETTLE_US", ""]
        for d in c6["baseline_over_budget"]:
            lines.append(f"- {d['case']}@{d['variant']}#{d['rep']} {d['phase']}: `{d['attribution']}` {d['duration_us']} us")
    lines += ["", f"### Input-to-frame (reported, not gated; lane floor: {c6.get('input_to_frame_floor')})", "", "| Side | Case and phase | Median (us) | Max (us) | Events |", "|---|---|---|---|---|"]
    for i in c6.get("input_to_frame", []):
        lines.append(f"| {i['side']} | {i['case']} | {i['median_us']} | {i['max_us']} | {i['events']} |")
    return lines + [""]


def merged_storage_status(parts: list[dict]) -> str:
    """One storage status for a sweep joined from several parts."""
    if any(p.get("status") == "contaminated" for p in parts):
        return "contaminated"
    if parts and all(p.get("status") == "clean" for p in parts):
        return "clean"
    return "mixed"


def split_source(value: str | Path) -> tuple[str | None, Path]:
    """A merge source, `DIR` or `LABEL=DIR` (the side of an interleaved verdict)."""
    text = str(value)
    match = re.match(r"^([A-Za-z][\w.-]*)=(.+)$", text)
    if match and not Path(text).exists():
        return match.group(1), Path(match.group(2))
    return None, Path(text)


def merge_runs(out: Path, sources: list, quiet: bool = False) -> int:
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
    labelled = [split_source(source) for source in sources]
    sources = [path for _label, path in labelled]
    sides = [label for label, _path in labelled]
    for side, source in zip(sides, sources):
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
            if side is not None:
                # An interleaved verdict's side (baseline, branch): the C6
                # verdict compares the sides.
                row["side"] = side
            part_cases.add(row["case"])
            # Repetitions accumulate across parts (a reduced sweep adds the
            # bracketing repetitions in a later part), renumbered per side,
            # case and variant in part order.
            rep_key = (side, row["case"], row["variant"])
            row["rep"] = next_rep.get(rep_key, 0)
            next_rep[rep_key] = row["rep"] + 1
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
            "merged_from": [{"run_id": r.get("run_id"), "side": side, "artifact_dir": str(src), "wall_start": r.get("wall_start"), "wall_end": r.get("wall_end"), "repetitions": r.get("repetitions"), "bisection": r.get("bisection")} for r, src, side in zip(runs, sources, sides)],
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
    if report.get("misdirected_typing"):
        lines.append(f"- **TYPING REACHED NO DOCUMENT** in {len(report['misdirected_typing'])} phase(s): their keys arrived outside an editor or inserted nothing, so their input-to-frame figures measure a window with nothing to draw; the lane fails: {', '.join(report['misdirected_typing'][:8])}")
    if report.get("withheld_typing_frames"):
        lines.append(f"- **TYPED TEXT DRAWN LATE** in {len(report['withheld_typing_frames'])} phase(s): a key typed into an editable editor was drawn only after the next key was handled while the main loop was free, or never; its latency runs to that frame and the lane fails: {', '.join(report['withheld_typing_frames'][:8])}")
    if report.get("frameless_cases"):
        lines.append(f"- **NO FRAME RECORDS** in {len(report['frameless_cases'])} measured case(s), so first-frame, frame-gap and input-to-frame figures are missing; the lane fails: {', '.join(report['frameless_cases'][:8])}")
    if report["warnings"].get("unclassified"):
        lines.append(f"- {len(report['warnings']['unclassified'])} unclassified warning line(s); see report.json.")
    lines += ["", "## C3 install gate", "", f"{report['install_c3'].get('verdict')}", "", "## Window construction (C4)", ""]
    for entry in report["window_construction_c4"]:
        lines.append(f"- {entry['case']}@{entry['variant']}#{entry['rep']}: first frame {entry['window_construction_us']} us after spawn, open-phase worst stall {entry['open_stall_us']} us")
    lines += ["", "### Construction spans against B_settle", ""]
    lines.append("| Case | Profile | Backend | Build | toolkit | lushtext | construction | first frame | largest dispatch | verdict | largest stages |")
    lines.append("|---|---|---|---|---|---|---|---|---|---|---|")
    for entry in report.get("construction_c4", []):
        stages = ", ".join(f"{k} {v}" for k, v in entry.get("largest_stages_us", {}).items())
        lines.append(
            f"| {entry['case']}@{entry['variant']}#{entry['rep']} | {entry['profile']} | {entry['backend']} | {entry['build_profile']} | "
            f"{entry.get('startup_toolkit_us')} | {entry.get('startup_lushtext_us')} | {entry.get('window_construction_us')} | "
            f"{entry.get('first_frame_us')} | {entry.get('largest_dispatch_before_first_frame_us')} | {entry['verdict']} {' '.join(entry['over_budget'])} | {stages} |"
        )
    lines += ["", "### Composition dispatches and first frames", ""]
    median = fill_in_median(report.get("construction_c4", []), report["budgets"]["REFERENCE_FRAME_US"])
    lines.append(
        f"Fill-in median {median['median_us']} us over {median['runs']} run(s), target ≤ {median['target_us']} us "
        f"({FILL_IN_MEDIAN_TARGET_FRAMES} reference frames): {'met' if median['within_target'] else 'not met' if median['within_target'] is False else 'n/a'}. "
        f"Every run is judged against the accepted {ACCEPTED_FILL_IN_US} us; the first painted frame is the first frame paint after the present stage, in every build."
    )
    lines.append("")
    lines.append(f"| Case | first painted frame | first presentation | composition complete | activation shown | fill-in (accepted ≤ {ACCEPTED_FILL_IN_US}) | largest composition dispatch | dispatches ≥ 4 ms, in order | stage dispatches | frame layout / paint |")
    lines.append("|---|---|---|---|---|---|---|---|---|---|")
    for entry in report.get("construction_c4", []):
        stages = ", ".join(f"{k} {v}" for k, v in entry.get("composition_stages_us", {}).items())
        frames = ", ".join(f"{f.get('layout')}/{f.get('paint')}" for f in entry.get("frame_phases_us", []))
        dispatches = ", ".join(str(d) for d in entry.get("dispatches_over_4ms_us", []))
        lines.append(
            f"| {entry['case']}@{entry['variant']}#{entry['rep']} | {entry.get('first_painted_frame_us')} | {entry.get('first_presentation_us')} | "
            f"{entry.get('composition_complete_us')} | {entry.get('activation_shown_us')} | {entry.get('fill_in_us')} | "
            f"{entry.get('largest_composition_dispatch_us')} | {dispatches} | {stages} | {frames} |"
        )
    lines += [""] + render_post_open(report)
    keystroke_rows = [
        (case, name, phase["keystroke"])
        for case in report["cases"]
        for name, phase in case["phases"].items()
        if phase.get("keystroke")
    ]
    if keystroke_rows:
        lines += [
            "",
            "## Keystrokes",
            "",
            "Per keystroke: the worst stall between this key and the next, and the summed stall time of every late dispatch in that window.",
            "",
            "| Case | Variant | Phase | Keys | Worst median / p90 (us) | Summed median (us) |",
            "| --- | --- | --- | --- | --- | --- |",
        ]
        for case, name, keys in keystroke_rows:
            position = name.removeprefix("key-")
            verified = "" if position not in case.get("unverified_positions", []) else " (burst not verified: excluded)"
            lines.append(f"| {case['case']} | {case['variant']} | {name}{verified} | {keys['keys']} | {keys['worst_median_us']} / {keys['worst_p90_us']} | {keys['summed_median_us']} |")
    lines += ["", "## Cases", "", "| Case | Variant | Outcomes | Phase | Stall median / max (us) | Frame p95 gap | Input-to-frame max (us / frames) | IM calls / max bytes | AT events / max payload |", "| --- | --- | --- | --- | --- | --- | --- | --- | --- |"]
    for case in report["cases"]:
        flags = (" noisy" if case.get("noisy") else "") + (f" storage:{case.get('storage')}" if case.get("storage") not in (None, "unchecked", "clean") else "")
        for name, phase in case["phases"].items():
            if "stall_max_us" not in phase:
                lines.append(f"| {case['case']} | {case['variant']} | {','.join(case['outcomes'])}{flags} | {name} | {phase.get('status')} | | | | |")
                continue
            lines.append(
                f"| {case['case']} | {case['variant']} | {','.join(case['outcomes'])}{flags} | {name} | {phase['stall_median_us']} / {phase['stall_max_us']} | {phase['frame_gap_p95_us']} | {phase['input_to_frame_max_us']} / {phase['input_to_frame_max_frames']}{protected_itf_note(phase)} | {phase['im_calls']} / {phase['im_max_bytes']} | {phase['at_events']} / {phase['at_max_payload']} |"
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
    # Before anything can start dconf-service on this private bus, point its
    # activation environment at a scratch config home inside the run's temp
    # scope, so the service can never write the user's real dconf database.
    dconf_home = Path(os.environ.get("TMPDIR", tempfile.gettempdir())) / "dconf-session-config"
    dconf_home.mkdir(parents=True, exist_ok=True)
    isolate_dconf_activation(Bus(), dconf_home)
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
        # The scratch config home the private bus's dconf-service was pointed
        # at belongs to this run's temp scope.
        shutil.rmtree(dconf_home, ignore_errors=True)


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
                runner.editor_click(injections, y_fraction=1 / 3)
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



def post_open_self_test(expect: Callable[[bool, str], None]) -> None:
    """Driver-computed settle, the post-open settle table, and the C6 verdict (tasks 9.1)."""

    def late(t: int, lateness: int) -> str:
        return json.dumps({"kind": "late", "t_us": t, "ready_us": t - lateness, "lateness_us": lateness})

    def span(name: str, start: int, end: int) -> str:
        return json.dumps({"kind": "span", "t_us": end, "name": name, "start_us": start, "end_us": end})

    def mark(name: str, t: int) -> str:
        return json.dumps({"kind": "mark", "t_us": t, "name": name})

    start = json.dumps({"kind": "start", "t_us": 0, "version": 1, "tick_us": 2000, "late_threshold_us": 1000})
    expect(read_settle_quiet() == 100_000, f"SETTLE_QUIET_US is read from the load policy ({read_settle_quiet()})")
    expect(read_settle_quiet(Path("/nonexistent/policy.rs")) == DEFAULT_SETTLE_QUIET_US, "a missing load policy falls back to the default quiet period")

    # Attribution by span: the innermost span covering at least half of the
    # dispatch, else the span covering most of it; then the marks.
    lines = [
        start,
        span("post_open_editor_construct", 1_000_500, 1_002_000),
        span("post_open_language_attach", 1_004_000, 1_028_000),
        span("post_open_activation", 1_000_000, 1_030_000),
        late(1_030_500, 29_000),
        span("post_open_editor_construct", 1_101_000, 1_103_000),
        span("post_open_activation", 1_100_000, 1_120_000),
        late(1_120_400, 19_000),
        mark("highlight_updated", 1_999_800),
        late(2_000_000, 40_000),
        mark("adjustment_upper", 2_099_900),
        late(2_100_000, 21_000),
        late(2_200_000, 17_000),
        late(2_300_000, 900),
        mark("settle_sentinel", 2_210_000),
    ]
    parsed = parse_probe_log(lines)
    entry = post_open_entry(parsed, 900_000, 1_010_000, 100_000, "background", "open")
    by_end = {d.get("end_us"): d for d in entry.get("dispatches", [])}
    expect(by_end.get(1_030_500, {}).get("attribution") == "post_open_language_attach", f"a dispatch mostly inside the attach span is the attach stage ({by_end.get(1_030_500)})")
    expect(by_end.get(1_120_400, {}).get("attribution") == "post_open_activation", f"a short inner span does not take a dispatch it covers less than half of ({by_end.get(1_120_400)})")
    expect(by_end.get(2_000_000, {}).get("attribution") == "highlight", f"an unspanned dispatch ending at highlight-updated is highlighting ({by_end.get(2_000_000)})")
    expect(by_end.get(2_100_000, {}).get("attribution") == "validation", f"an unspanned dispatch ending at an upper change is validation ({by_end.get(2_100_000)})")
    expect(by_end.get(2_200_000, {}).get("attribution") == "unattributed", f"an unspanned dispatch with no mark is unattributed ({by_end.get(2_200_000)})")
    expect(2_300_000 not in by_end, "a dispatch under 1 ms is not listed")
    # Read by span, not by phase: the activation's dispatch ends after the
    # open phase's readiness (1.01 s) and is still the activation stage, and
    # the table starts at the activation span, not the phase boundary.
    expect(entry.get("activation_us") == 1_000_000, f"the table starts at the activation span ({entry.get('activation_us')})")
    expect(entry.get("settled_us") == 2_210_000 and entry.get("settle_us") == 1_210_000, f"settled at the sentinel after the last mark plus quiet ({entry.get('settled_us')}, {entry.get('settle_us')})")
    expect(entry.get("stage_worst_us", {}).get("highlight") == 40_000 and entry.get("stage_worst_us", {}).get("post_open_language_attach") == 29_000, f"per-stage worst dispatch ({entry.get('stage_worst_us')})")

    # The driver-computed settle: a sentinel at or after the last mark, no
    # mark for the quiet period, no post-open span still running.
    settle_lines = [
        start,
        mark("highlight_updated", 1_200_000),
        mark("settle_sentinel", 1_250_000),
        mark("adjustment_upper", 1_300_000),
    ]
    pending = compute_settle(parse_probe_log(settle_lines), 1_000_000, 1_100_000, 100_000)
    expect(pending.get("settled_us") is None, f"a sentinel only before the last mark is not settled ({pending})")
    settled = compute_settle(parse_probe_log(settle_lines + [mark("settle_sentinel", 1_310_000)]), 1_000_000, 1_100_000, 100_000)
    expect(settled.get("settled_us") == 1_400_000, f"settled once the quiet period after the last mark ends ({settled})")
    running = compute_settle(parse_probe_log(settle_lines + [mark("settle_sentinel", 1_310_000), span("post_open_language_attach", 1_390_000, 1_450_000)]), 1_000_000, 1_100_000, 100_000)
    expect(running.get("settled_us") is None, f"a post-open span ending after the last sentinel holds settle ({running})")
    after = compute_settle(parse_probe_log(settle_lines + [mark("settle_sentinel", 1_310_000), span("post_open_language_attach", 1_390_000, 1_450_000), mark("settle_sentinel", 1_455_000)]), 1_000_000, 1_100_000, 100_000)
    expect(after.get("settled_us") == 1_455_000, f"the sentinel after the span settles it ({after})")
    early = compute_settle(parse_probe_log(settle_lines + [mark("settle_sentinel", 1_310_000)]), 1_000_000, 1_500_000, 100_000)
    expect(early.get("settled_us") == 1_500_000, f"settle is never before the floor ({early})")
    idle_open = compute_settle(parse_probe_log([start, mark("settle_sentinel", 1_050_000)]), 1_000_000, 1_100_000, 100_000, require_activity=True)
    expect(idle_open.get("settled_us") is None, f"a document open waits for its first post-open span ({idle_open})")
    required = compute_settle(parse_probe_log(settle_lines + [mark("settle_sentinel", 1_310_000)]), 1_000_000, 1_100_000, 100_000, required=("post_open_language_attach",))
    expect(required.get("settled_us") is None, f"a required stage that has not run holds settle ({required})")

    # Background runs until the driver-computed settle (records arrive over
    # time), and a cap hit fails the phase.
    clock = [0.0]
    stream = [start]
    timeline = [
        (0.20, mark("highlight_updated", 1_200_000)),
        (0.21, mark("settle_sentinel", 1_210_000)),
        (0.28, mark("highlight_updated", 1_280_000)),
        (0.29, mark("settle_sentinel", 1_290_000)),
    ]

    def poll() -> dict:
        while timeline and timeline[0][0] <= clock[0] + 1e-9:
            stream.append(timeline.pop(0)[1])
        return parse_probe_log(stream)

    def sleep(seconds: float) -> None:
        clock[0] += seconds

    waited = wait_for_settle(poll, 1_000_000, 1_100_000, 100_000, 5.0, clock=lambda: clock[0], sleep=sleep, now=lambda: 1_000_000 + int(clock[0] * 1_000_000))
    expect(waited.get("status") == "settled" and waited.get("settled_us") == 1_380_000 and clock[0] >= 0.43, f"background waits past the first quiet sentinel to the driver-computed settle ({waited}, clock {clock[0]:.2f})")
    clock[0] = 0.0
    capped_wait = wait_for_settle(lambda: parse_probe_log([start, mark("highlight_updated", 1_200_000)]), 1_000_000, 1_100_000, 100_000, 2.0, clock=lambda: clock[0], sleep=sleep, now=lambda: 9_000_000)
    expect(capped_wait.get("status") == "exceeded_cap" and capped_wait.get("settled_us") is None, f"a settle that never comes is a cap hit ({capped_wait})")
    expect(any(c.attrs.get("post_open") for c in build_cases("quick") if c.group == "dense"), "dense cases opt in to the until-settled background")
    with tempfile.TemporaryDirectory(prefix="lt-resp-tail-") as raw:
        log_file = Path(raw) / "probe.jsonl"
        whole = mark("highlight_updated", 1_200_000)
        log_file.write_text(start + "\n" + whole[:10], encoding="utf-8")
        tail = ProbeTail([log_file])
        first_poll = len(tail.poll()["records"]["mark"])
        with log_file.open("a", encoding="utf-8") as handle:
            handle.write(whole[10:] + "\n")
        second = tail.poll()
        expect(first_poll == 0 and len(second["records"]["mark"]) == 1 and second["started"] and not second["malformed"], "the live tail reads complete lines only, incrementally")

    # Toolkit-floor exceptions are configured, not hard-coded.
    floors = parse_toolkit_floors("post_open_language_attach=A67:.claude/handoffs/c6-upstream-gtksourceview-language-parse.md; highlight=S3")
    expect(floors.get("post_open_language_attach", {}).get("axiom") == "A67" and floors.get("highlight", {}).get("report") is None, f"toolkit-floor list parsing ({floors})")

    # The C6 verdict.
    budgets = dict(DEFAULT_BUDGETS)

    def open_entry(dispatches: list[tuple[str, int]], settle_us: int = 500_000, status: str = "settled", laid_out_us: int = 400_000) -> dict:
        rows_ = []
        at = 10_000
        for attribution, duration in dispatches:
            rows_.append({"at_us": at, "start_us": 1_000_000 + at - duration, "end_us": 1_000_000 + at, "duration_us": duration, "attribution": attribution, "source": "span"})
            at += 50_000
        worst: dict[str, int] = {}
        for d in rows_:
            worst[d["attribution"]] = max(worst.get(d["attribution"], 0), d["duration_us"])
        return {"phase": "background", "kind": "open", "status": status, "activation_us": 1_000_000, "settled_us": 1_000_000 + settle_us, "settle_us": settle_us, "laid_out_us": laid_out_us, "dispatches": rows_, "stage_worst_us": worst}

    def row(side: str, rep: int, entries: list[dict], case: str = "total-size/1000000") -> dict:
        return {"case": case, "variant": "plain", "rep": rep, "side": side, "outcome": "ok", "post_open": entries, "phases": []}

    baseline_rows = [row("baseline", r, [open_entry([("post_open_language_attach", 3_000), ("highlight", 9_000)])]) for r in range(3)]
    good_branch = [row("branch", r, [open_entry([("post_open_language_attach", 3_000), ("highlight", 9_000)])]) for r in range(3)]
    met = c6_verdict(baseline_rows + good_branch, budgets, {})
    expect(met.get("verdict") == "met", f"a branch within every bound and no slower than the baseline meets the verdict ({met.get('verdict')}, {met.get('failures')})")
    over = c6_verdict(baseline_rows + good_branch[:2] + [row("branch", 2, [open_entry([("unattributed", 20_000)])])], budgets, {})
    expect(over.get("verdict") == "not met" and any("B_SETTLE" in f for f in over.get("failures", [])), f"one post-open dispatch over B_settle fails ({over.get('failures')})")
    capped_row = c6_verdict(baseline_rows + good_branch[:2] + [row("branch", 2, [open_entry([], status="pending")])], budgets, {})
    expect(capped_row.get("verdict") == "not met", f"a background that never settled fails ({capped_row.get('failures')})")
    # The median, not the maximum, plus one reference frame.
    regress = [row("branch", r, [open_entry([("post_open_language_attach", d)])]) for r, d in enumerate((12_000, 12_000, 2_000))]
    regressed = c6_verdict(baseline_rows + regress, budgets, {})
    expect(regressed.get("verdict") == "not met" and any(r.get("stage") == "post_open_language_attach" for r in regressed.get("regressions", [])), f"a stage whose median worst exceeds the baseline's plus one frame regresses ({regressed.get('regressions')})")
    outlier = [row("branch", r, [open_entry([("post_open_language_attach", d)])]) for r, d in enumerate((12_000, 2_000, 2_000))]
    tolerated = c6_verdict(baseline_rows + outlier, budgets, {})
    expect(not tolerated.get("regressions"), f"one slow repetition is not a regression of the median ({tolerated.get('regressions')})")
    slow_layout = [row("branch", r, [open_entry([("post_open_language_attach", 3_000)], laid_out_us=420_000)]) for r in range(3)]
    slowed = c6_verdict(baseline_rows + slow_layout, budgets, {})
    expect(any(r.get("stage") == "activation-to-laid-out" for r in slowed.get("regressions", [])), f"activation-to-laid-out over the baseline's plus one frame regresses ({slowed.get('regressions')})")
    late_attach = [row("branch", r, [open_entry([("post_open_language_attach", 3_000)], settle_us=900_000)]) for r in range(3)]
    attach_only = c6_verdict(baseline_rows + late_attach, budgets, {})
    expect(not attach_only.get("regressions"), f"a later settle with the same layout time is not a regression ({attach_only.get('regressions')})")
    expect(activation_to_laid_out(0, [10_000, 30_000, 5_000_000], 4_000_000) == 5_000_000 - ATTACH_DEADLINE_US, "the attach wait exempted is at most ATTACH_DEADLINE_US")
    expect(activation_to_laid_out(0, [10_000, 30_000, 400_000], 300_000) == 400_000 - 270_000, "only the wait from the last layout to the attach is exempt")
    expect(activation_to_laid_out(0, [10_000, 30_000], 300_000) == 30_000, "layout that ends before the attach is not shortened")
    policy = (REPO_ROOT / "crates/lushtext-core/src/ui/editor_page/load/policy.rs").read_text()
    expect(f"pub const ATTACH_DEADLINE_US: i64 = {ATTACH_DEADLINE_US:_};" in policy, "ATTACH_DEADLINE_US agrees with the load policy")
    # A toolkit-floor exception: exempt from B_settle, and the keys that
    # overlap it are exempt from B_interact; every other key is still gated.
    attach = {"at_us": 100_000, "start_us": 1_060_000, "end_us": 1_100_000, "duration_us": 40_000, "attribution": "post_open_language_attach", "source": "span"}
    typing = {"phase": "type-during-settle", "kind": "typing", "status": "settled", "activation_us": 1_000_000, "settled_us": 2_000_000, "settle_us": 1_000_000, "dispatches": [attach], "stage_worst_us": {"post_open_language_attach": 40_000},
              "keys": [{"event_us": 1_070_000, "handler_us": 1_101_000, "latency_us": 31_000}, {"event_us": 1_500_000, "handler_us": 1_503_000, "latency_us": 3_000}]}
    slow_key = dict(typing, keys=typing["keys"] + [{"event_us": 1_600_000, "handler_us": 1_609_000, "latency_us": 9_000}])
    floor_list = {"post_open_language_attach": {"axiom": "A67", "report": "handoff.md"}}
    exempt_rows = [row("branch", r, [open_entry([("post_open_language_attach", 40_000)]), typing]) for r in range(3)]
    exempt = c6_verdict(exempt_rows, budgets, floor_list)
    expect(exempt.get("verdict") == "met", f"an exception dispatch and the keys that waited behind it are exempt ({exempt.get('failures')})")
    lines_ = exempt.get("exceptions", [])
    expect(any(e.get("axiom") == "A67" and e.get("overlapping_keys_us") == [31_000] for e in lines_), f"the exception line lists its axiom and the overlapping keys' latencies ({lines_})")
    not_exempt = c6_verdict([row("branch", 0, [open_entry([]), slow_key])], budgets, floor_list)
    expect(not_exempt.get("verdict") == "not met" and any("B_INTERACT" in f for f in not_exempt.get("failures", [])), f"a key that overlaps no exception is gated ({not_exempt.get('failures')})")
    unlisted = c6_verdict([row("branch", 0, [open_entry([("post_open_language_attach", 40_000)]), typing])], budgets, {})
    expect(unlisted.get("verdict") == "not met", "without a listed exception the same dispatch and key fail")
    # Wrap/zoom/font: the interaction dispatch within B_interact, every later one within B_settle.
    interaction = {"phase": "zoom-step", "kind": "interaction", "status": "settled", "activation_us": 1_000_000, "settled_us": 1_300_000, "settle_us": 300_000, "dispatches": [], "stage_worst_us": {},
                   "interaction": {"duration_us": 9_000, "attribution": "unattributed"}, "later": [{"duration_us": 15_000, "attribution": "validation"}]}
    zoom = c6_verdict([row("branch", 0, [open_entry([]), interaction])], budgets, {})
    expect(zoom.get("verdict") == "not met" and any("zoom-step" in f for f in zoom.get("failures", [])), f"an interaction dispatch over B_interact fails ({zoom.get('failures')})")
    zoom_ok = c6_verdict([row("branch", 0, [open_entry([]), dict(interaction, interaction={"duration_us": 7_000, "attribution": "unattributed"})])], budgets, {})
    expect(zoom_ok.get("verdict") == "met", f"an interaction within its bounds passes ({zoom_ok.get('failures')})")

    # The live phases record the settle inputs; the analysis recomputes the
    # same tables from them, typing keys included.
    typing_lines = lines + [
        json.dumps({"kind": "input", "t_us": 2_400_000, "window": 1, "event": "button-press", "event_time_ms": 2_399}),
        json.dumps({"kind": "input", "t_us": 2_450_000, "window": 1, "event": "key-press", "event_time_ms": 2_444}),
        json.dumps({"kind": "input", "t_us": 2_500_500, "window": 1, "event": "key-press", "event_time_ms": 2_500}),
    ]
    phase_records = [
        {"name": "background", "start_us": 1_010_000, "end_us": 2_400_000, "post_open": "open", "origin_us": 900_000, "floor_us": 1_010_000, "require_activity": True, "injections": []},
        {"name": "type-during-settle", "start_us": 2_420_000, "end_us": 2_700_000, "post_open": "typing", "origin_us": 900_000, "floor_us": 2_500_500, "injections": [{"event": "key-press", "inject_us": 2_443_000}, {"event": "key-press", "inject_us": 2_499_000}]},
        {"name": "scroll", "start_us": 2_700_000, "end_us": 2_800_000, "injections": []},
    ]
    analysed = post_open_entries(parse_probe_log(typing_lines + [mark("settle_sentinel", 2_501_000)]), phase_records, 100_000)
    expect([e["phase"] for e in analysed] == ["background", "type-during-settle"] and analysed[0]["settled_us"] == 2_210_000, f"each post-open phase gets its table ({[(e['phase'], e['settled_us']) for e in analysed]})")
    keys = analysed[1].get("keys", []) if len(analysed) > 1 else []
    expect([k.get("latency_us") for k in keys] == [6_000, 500], f"each key's dispatch latency is its handler minus its compositor time ({keys})")
    expect(split_source("baseline=/x/run1") == ("baseline", Path("/x/run1")) and split_source("/x/run1") == (None, Path("/x/run1")), "merge sources take an optional side label")

    # An interleaved, side-labelled merge reports the C6 verdict.
    with tempfile.TemporaryDirectory(prefix="lt-resp-c6-") as raw:
        root = Path(raw)
        sides = (("baseline", 3_000), ("branch", 3_000), ("baseline", 3_000), ("branch", 3_000))
        for index, (side, attach_us) in enumerate(sides):
            src = root / f"{side}{index}"
            src.mkdir()
            (src / "run.json").write_text(json.dumps({"run_id": f"rr-{side}-{index}", "remote_desktop": {"available": True}}), encoding="utf-8")
            (src / "results.jsonl").write_text(json.dumps(row(side, 0, [open_entry([("post_open_language_attach", attach_us)])])) + "\n", encoding="utf-8")
            for name in ("integrity.json",):
                (src / name).write_text(json.dumps({"checks": 1, "violations": []}), encoding="utf-8")
            (src / "fixtures.json").write_text("[]", encoding="utf-8")
            (src / "calibration.json").write_text("{}", encoding="utf-8")
        merge_runs(root / "merged", [f"{side}={root / f'{side}{index}'}" for index, (side, _us) in enumerate(sides)], quiet=True)
        merged = [json.loads(line) for line in (root / "merged/results.jsonl").read_text(encoding="utf-8").splitlines()]
        expect(sorted((r["side"], r["rep"]) for r in merged) == [("baseline", 0), ("baseline", 1), ("branch", 0), ("branch", 1)], f"repetitions are numbered per side ({[(r.get('side'), r['rep']) for r in merged]})")
        saved = {k: os.environ.pop(k, None) for k in ("RESPONSIVENESS_SKIP_STORAGE", TOOLKIT_FLOORS_ENV)}
        os.environ["RESPONSIVENESS_SKIP_STORAGE"] = "1"
        try:
            import contextlib
            import io

            with contextlib.redirect_stdout(io.StringIO()):
                write_report(root / "merged", None, ["post_open_language_attach=A67:handoff.md"])
        finally:
            for k, v in saved.items():
                os.environ.pop(k, None)
                if v is not None:
                    os.environ[k] = v
        written = json.loads((root / "merged/report.json").read_text(encoding="utf-8"))
        markdown = (root / "merged/report.md").read_text(encoding="utf-8")
        expect(written.get("c6_verdict", {}).get("verdict") == "met" and written["c6_verdict"]["baseline_rows"] == 2, f"the merged report carries the C6 verdict ({written.get('c6_verdict', {}).get('failures')})")
        expect("## Post-open settle (C6)" in markdown and "## C6 verdict" in markdown and "axiom A67" in markdown, "report.md renders the post-open table, the verdict and the exception line")


def self_test() -> int:
    failures: list[str] = []

    def expect(condition: bool, label: str) -> None:
        if not condition:
            failures.append(label)

    # Every keystroke length stays editable after the case types its bursts.
    for tier, lengths in KEYSTROKE_LENGTHS.items():
        expect(
            max(lengths) + len(KEYSTROKE_POSITIONS) * len(TYPE_BURST) <= L_PROTECT_EDIT,
            f"{tier} keystroke lengths stay at or below L_PROTECT_EDIT after the bursts",
        )

    # Classification edges (L + T == B is within; L - eps == B is not over).
    expect(classify(6_000, 2_000, 1_000, 8_000) == "within", "L + T == B is within")
    expect(classify(6_001, 2_000, 1_000, 8_000) == "indeterminate", "L + T == B + 1 is indeterminate")
    expect(classify(9_000, 2_000, 1_000, 8_000) == "indeterminate", "L - eps == B is indeterminate")
    expect(classify(9_001, 2_000, 1_000, 8_000) == "over", "L - eps == B + 1 is over")
    expect(nearest_rank([5, 1, 9, 3], 95) == 9 and nearest_rank([5, 1, 9, 3], 50) == 3, "nearest-rank percentile")

    # Keystroke windows: two sub-budget dispatches after one key sum above
    # B_interact, which the worst stall alone does not show.
    keys = {
        "records": {
            "input": [{"t_us": 0, "event": "key-press"}, {"t_us": 150_000, "event": "key-press"}, {"t_us": 150_500, "event": "key-release"}],
            "late": [
                {"t_us": 6_000, "ready_us": 0, "lateness_us": 6_000},
                {"t_us": 40_000, "ready_us": 35_000, "lateness_us": 5_000},
                {"t_us": 152_000, "ready_us": 150_000, "lateness_us": 2_000},
            ],
        }
    }
    per_key = keystroke_stalls(keys, 0, 300_000, 8_000)
    expect(per_key["keys"] == 2 and per_key["worst_us"] == [6_000, 2_000], "keystroke windows split at the next press")
    expect(per_key["summed_us"] == [11_000, 2_000] and per_key["over_budget_keys"] == 0 and per_key["summed_over_budget_keys"] == 1, "a second dispatch shows in the summed stall")

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

    # A measured case without frame records fails the lane (the probe's frame
    # stream broke after first-frame-window-shell and the report said None).
    framed = {"case": "baseline/small", "variant": "plain", "rep": 0, "outcome": "ok", "probe_started": True, "frame_records": 25}
    frameless = dict(framed, frame_records=0)
    expect(frameless_row(frameless), "a measured case with zero frame records is frameless")
    expect(not frameless_row(framed), "a case with frame records is not frameless")
    expect(not frameless_row(dict(frameless, case="baseline/calibration")), "calibration is not judged")
    expect(not frameless_row(dict(frameless, outcome="skipped")), "a skipped case is not judged")
    expect(not frameless_row(dict(frameless, probe_started=False)), "a case without the probe is not judged")
    expect(not frameless_row({k: v for k, v in frameless.items() if k != "frame_records"}), "a row without a count is unknown")
    expect(run_failed([], {"violations": []}, clean_run, {"regressions": {}}, ["baseline/small@plain#0"]), "a frameless case fails the lane")
    with tempfile.TemporaryDirectory() as frame_tmp:
        frame_root = Path(frame_tmp)
        frame_rep = frame_root / "cases" / "baseline" / "small" / "plain" / "rep0"
        frame_rep.mkdir(parents=True)
        (frame_rep / "probe.jsonl").write_text(json.dumps({"kind": "start", "t_us": 1, "version": 1, "pid": 1, "tick_us": 2000, "late_threshold_us": 1000, "probe_im": False}) + "\n", encoding="utf-8")
        legacy = {k: v for k, v in frameless.items() if k != "frame_records"}
        expect(frameless_cases(frame_root, [legacy]) == ["baseline/small@plain#0"], "an older row is judged from its probe log")
        expect(frameless_cases(frame_root, [framed]) == [], "a recorded count is used as is")

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

    # The .html-minus-.txt difference is taken phase by phase. Pooling the
    # phases (max over every .html phase minus max over every .txt phase)
    # let one .txt outlier in another phase hide the .html cost: on the idle
    # re-confirmation a 9.8 ms .txt click masked 13.8 ms .html typing
    # against 2.9 ms .txt typing at 44 kB.
    masked = []
    for r in rows:
        r = dict(r)
        if r["group"] == "total-size" and r["length"] == 5_000_000:
            stalls = {"background": 1_000, "scroll": 1_000, "click": 1_000, "type-start": 14_000} if r["attrs"].get("ext") == "html" else {"background": 1_000, "scroll": 1_000, "click": 10_000, "type-start": 2_000}
            r["phases"] = [{"name": n, "kind": PHASE_KINDS.get(n, "interaction"), "status": "ok", "stall": {"lateness_us": v}} for n, v in stalls.items()]
        masked.append(r)
    masked_derived = derive(masked, budgets, 1_000, True, obs)
    expect(masked_derived["S_syntax_interact"]["value"] == 1_000_000, f"a .txt outlier in one phase does not hide the .html cost in another ({masked_derived['S_syntax_interact']})")

    # Read phase by phase, L_syntax's background half sees the size-independent
    # first highlight of an HTML document in a fresh process (~42 ms, the
    # stall that leaves S_syntax underivable). That stall is C6's, not a
    # length cost: L_syntax reads the background difference in excess of it,
    # measured on the smallest short-line total-size document.
    first_highlight = []
    for r in rows:
        r = dict(r)
        if r["group"] == "paragraph":
            extra = 40_000 if r["attrs"].get("ext") == "html" else 0
            r["phases"] = [*r["phases"], {"name": "background", "kind": "settle", "status": "ok", "stall": {"lateness_us": 1_000 + extra}}]
        elif r["group"] == "total-size" and r["length"] == 1_000_000 and r["attrs"].get("ext") == "html":
            r["phases"] = [dict(p, stall={"lateness_us": 41_000}) if p["name"] == "background" else p for p in r["phases"]]
        first_highlight.append(r)
    first_derived = derive(first_highlight, budgets, 1_000, True, obs)
    expect(first_derived["L_syntax"]["value"] == 10_000, f"the size-independent first highlight does not fail L_syntax ({first_derived['L_syntax']})")
    no_reference = [r for r in first_highlight if r["group"] != "total-size"]
    expect(derive(no_reference, budgets, 1_000, True, obs)["L_syntax"]["value"] is None, "without a total-size reference the first highlight is not discounted")

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

    # V_batch reads validation, not the cost of painting a full screen of
    # text: on an idle reference machine every dense length from 120 to 500
    # scrolled with the same 5-7 ms stall (2-3 ms under load), so Page Down
    # and scroll are read as their excess over the dense reference case.
    floor = []
    for r in rows:
        r = dict(r)
        if r["group"] == "dense":
            r["phases"] = [dict(p, stall={"lateness_us": p["stall"]["lateness_us"] + 6_000}) if p["name"] == "scroll" else p for p in r["phases"]]
        floor.append(r)
    floor_reference = dict(next(r for r in floor if r["group"] == "dense"), case=f"dense/{DENSE_REFERENCE_LENGTH}", length=DENSE_REFERENCE_LENGTH)
    floor_reference["phases"] = [{"name": "scroll", "kind": "interaction", "status": "ok", "stall": {"lateness_us": 6_500}}, {"name": "background", "kind": "settle", "status": "ok", "stall": {"lateness_us": 1_000}}]
    floor_derived = derive([*floor, floor_reference], budgets, 1_000, True, obs)
    expect(floor_derived["V_batch"]["value"] == 2_000 * 105, f"a full-screen scroll floor the reference shows too does not fail V_batch ({floor_derived['V_batch']})")
    expect(derive(floor, budgets, 1_000, True, obs)["V_batch"]["value"] is None, "without the dense reference the scroll is read as measured")
    # N_visible bounds what is painted, paint included, so the reference
    # does not discount its scroll.
    visible_floor = []
    for r in rows:
        r = dict(r)
        if r["group"] == "paragraph" and r["attrs"].get("ext") == "txt" and not r["attrs"].get("minimap"):
            r["phases"] = [dict(p, stall={"lateness_us": p["stall"]["lateness_us"] + 6_000}) if p["name"] == "scroll" else p for p in r["phases"]]
        visible_floor.append(r)
    expect(derive([*visible_floor, floor_reference], budgets, 1_000, True, obs)["N_visible"]["value"] != 50_000, "the dense reference does not discount N_visible's scroll")

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

    # Typing that reached no document (investigate-first-keys-without-frames):
    # a focus click that landed in the workspace sidebar sent every typed key
    # to the status bar's toggle, and the lane measured a window with nothing
    # to draw. Text keys are flagged, the probe's targets ride along, and a
    # verified phase whose keys went elsewhere or inserted nothing fails.
    typed_inj: list[dict] = []
    fake_rd.tap(ord("e"), typed_inj)
    fake_rd.tap(KEY_RIGHT, typed_inj)
    fake_rd.tap(ord("t"), typed_inj)
    expect([e.get("inserts", False) for e in typed_inj] == [True, False, True], f"printable keys insert, navigation keys do not ({typed_inj})")
    base = typed_inj[0]["inject_us"]

    def delivery_log(target: str | None, inserted: int) -> dict:
        lines = [json.dumps({"kind": "start", "t_us": 0, "version": 1, "tick_us": 2000, "late_threshold_us": 1000})]
        for offset, inj in enumerate(typed_inj):
            record = {"kind": "input", "t_us": inj["inject_us"] + 300, "window": 1, "event": "key-press", "event_time_ms": offset}
            if target is not None:
                record["target"] = target
            lines.append(json.dumps(record))
        for index in range(inserted):
            lines.append(json.dumps({"kind": "mark", "t_us": base + 400 + index, "name": EDITOR_USER_INSERT_MARK}))
        return parse_probe_log(lines)

    def delivery_of(target: str | None, inserted: int) -> dict | None:
        parsed_delivery = delivery_log(target, inserted)
        rows_itf = input_to_frame(parsed_delivery, typed_inj, 8_333)
        return input_delivery(parsed_delivery, rows_itf, base - 1_000, base + 10_000_000)

    echoed = delivery_of("editor", 2)
    expect(echoed == {"typed": 2, "undelivered": 0, "to_editor": 2, "to_read_only_editor": 0, "elsewhere": 0, "inserted": 2, "verified": True}, f"two keys into the editor, two insertions ({echoed})")
    expect(not misdirected_phase(echoed), "keys that reached the editor and inserted pass")
    sidebar = delivery_of("other", 0)
    expect(misdirected_phase(sidebar), f"keys that reached the status bar's toggle fail ({sidebar})")
    swallowed = delivery_of("editor", 1)
    expect(misdirected_phase(swallowed), f"a key into an editable editor that inserted nothing fails ({swallowed})")
    protected = delivery_of("read-only-editor", 0)
    expect(not misdirected_phase(protected), f"keys into a read-only editor (Protected View) insert nothing and pass ({protected})")
    older = delivery_of(None, 0)
    expect(older is not None and not older["verified"] and not misdirected_phase(older), f"an older probe without targets is unverified, never failed ({older})")
    expect(input_delivery(delivery_log("editor", 0), input_to_frame(delivery_log("editor", 0), [typed_inj[1]], 8_333), 0, base + 10_000_000) is None, "a phase that typed no text has no delivery")
    # Keys that never reached the application leave no input record: the
    # gate must not pass vacuously because there is no target to judge.
    silent = parse_probe_log([json.dumps({"kind": "start", "t_us": 0, "version": 1, "tick_us": 2000, "late_threshold_us": 1000})])
    silent_rows = input_to_frame(silent, typed_inj, 8_333)
    expect([r.get("inserts", False) for r in silent_rows] == [True, False, True], f"a text key with no input record stays a text key ({silent_rows})")
    unheard = input_delivery(silent, silent_rows, base - 1_000, base + 10_000_000)
    expect(unheard is not None and unheard["undelivered"] == 2 and misdirected_phase(unheard), f"text keys with no input record at all fail ({unheard})")
    half = parse_probe_log(
        [json.dumps({"kind": "start", "t_us": 0, "version": 1, "tick_us": 2000, "late_threshold_us": 1000})]
        + [json.dumps({"kind": "input", "t_us": typed_inj[0]["inject_us"] + 300, "window": 1, "event": "key-press", "event_time_ms": 0, "target": "editor"})]
        + [json.dumps({"kind": "mark", "t_us": base + 400, "name": EDITOR_USER_INSERT_MARK})]
    )
    lost = input_delivery(half, input_to_frame(half, typed_inj, 8_333), base - 1_000, base + 10_000_000)
    expect(lost is not None and lost["undelivered"] >= 1 and misdirected_phase(lost), f"a verified phase that lost a text key fails ({lost})")

    # A text key into an editable editor always changes what is drawn: a
    # frame that starts only after the next key was handled, while the loop
    # was free, is that key's withheld frame, measured and flagged.
    # Typed 50 ms apart, as the settle-typing phase types.
    spaced_inj = [dict(inj, inject_us=50_000_000 + index * 50_000) for index, inj in enumerate(typed_inj)]
    late_log_lines = (
        [json.dumps({"kind": "start", "t_us": 0, "version": 1, "tick_us": 2000, "late_threshold_us": 1000})]
        + [json.dumps({"kind": "input", "t_us": inj["inject_us"] + 300, "window": 1, "event": "key-press", "event_time_ms": i, "target": "editor"}) for i, inj in enumerate(spaced_inj)]
        + [json.dumps({"kind": "frame", "t_us": 1, "window": 1, "frame_counter": 1, "frame_time_us": spaced_inj[-1]["inject_us"] + 1_000, "presentation_us": spaced_inj[-1]["inject_us"] + 2_000, "refresh_interval_us": 8333, "source": "presentation"})]
    )
    late_log = parse_probe_log(late_log_lines)
    late_rows = input_to_frame(late_log, spaced_inj, 8_333)
    expect(late_rows[0].get("withheld_frame") and "latency_us" in late_rows[0], f"an inserted key drawn only after the next key is measured and flagged ({late_rows[0]})")
    expect(late_rows[0]["latency_us"] == spaced_inj[-1]["inject_us"] + 2_000 - spaced_inj[0]["inject_us"], f"its latency runs to the eventual frame ({late_rows[0]})")
    expect(late_rows[1].get("status") == "no-visible-change", f"a navigation key that changed nothing keeps the old rule ({late_rows[1]})")
    beyond = input_to_frame(late_log, spaced_inj, 8_333, horizon_us=spaced_inj[-1]["inject_us"] - 1_000)
    expect(beyond[0].get("status") == "no-frame" and beyond[0].get("withheld_frame"), f"an inserted key whose first frame starts after the phase is withheld ({beyond[0]})")
    unrecorded = input_to_frame(parse_probe_log(late_log_lines[:-1]), spaced_inj, 8_333)
    expect(unrecorded[-1].get("status") == "no-frame" and not unrecorded[-1].get("withheld_frame"), f"a key after which the log holds no frame is not judged ({unrecorded[-1]})")
    late_row = {"case": "dense/500/wrap-off", "variant": "plain", "rep": 0, "phases": [{"name": "type-during-settle", "input_to_frame": late_rows}]}
    expect(withheld_frame_cases([late_row]) == ["dense/500/wrap-off@plain#0:type-during-settle"], "the phase with a withheld frame is named")
    expect(run_failed([], {"violations": []}, {"driver_failures": []}, {"regressions": {}}, [], [], ["x"]), "a withheld typing frame fails the lane")
    on_time_log = parse_probe_log(
        [json.dumps({"kind": "start", "t_us": 0, "version": 1, "tick_us": 2000, "late_threshold_us": 1000})]
        + [json.dumps({"kind": "input", "t_us": inj["inject_us"] + 300, "window": 1, "event": "key-press", "event_time_ms": i, "target": "editor"}) for i, inj in enumerate(spaced_inj)]
        + [json.dumps({"kind": "frame", "t_us": 1, "window": 1, "frame_counter": i, "frame_time_us": inj["inject_us"] + 1_000, "presentation_us": inj["inject_us"] + 2_000, "refresh_interval_us": 8333, "source": "presentation"}) for i, inj in enumerate(spaced_inj)]
    )
    on_time = withheld_frame_cases([{"case": "c", "variant": "v", "rep": 0, "phases": [{"name": "p", "input_to_frame": input_to_frame(on_time_log, spaced_inj, 8_333)}]}])
    expect(on_time == [], f"keys drawn in their own frames are not withheld ({on_time})")

    # A key into a protected (read-only) editor is reported apart.
    protected_log = parse_probe_log(
        [json.dumps({"kind": "start", "t_us": 0, "version": 1, "tick_us": 2000, "late_threshold_us": 1000})]
        + [json.dumps({"kind": "input", "t_us": typed_inj[0]["inject_us"] + 300, "window": 1, "event": "key-press", "event_time_ms": 0, "target": "read-only-editor"})]
        + [json.dumps({"kind": "frame", "t_us": 1, "window": 1, "frame_counter": 1, "frame_time_us": typed_inj[0]["inject_us"] + 1_000, "presentation_us": typed_inj[0]["inject_us"] + 2_000, "refresh_interval_us": 8333, "source": "presentation"})]
    )
    protected_rows = input_to_frame(protected_log, typed_inj[:1], 8_333)
    expect(protected_rows[0].get("protected") and not editable_latency(protected_rows[0]), f"a key into a protected document is kept out of editable latency ({protected_rows[0]})")

    misdirected_row = {"case": "dense/500/wrap-off", "variant": "plain", "rep": 1, "phases": [{"name": "type-during-settle", "delivery": sidebar}, {"name": "click"}]}
    expect(misdirected_cases([misdirected_row]) == ["dense/500/wrap-off@plain#1:type-during-settle"], "the misdirected phase is named")
    expect(run_failed([], {"violations": []}, {"driver_failures": []}, {"regressions": {}}, [], ["dense/500/wrap-off@plain#1:type-during-settle"]), "typing that reached no document fails the lane")

    class GeometryOnly:
        def snapshot(self) -> dict:
            return {"window": {"surfaces": {"workspace_sidebar_visible": True}, "visual_geometry": {"surfaces": [{"name": "workspace-sidebar", "rect": {"x": 0, "y": 94, "width": 360, "height": 673}}, {"name": "source-view", "rect": {"x": 360, "y": 94, "width": 920, "height": 673}}]}}}

    expect(Runner.editor_point(GeometryOnly()) == (666, 430), f"the focus click aims inside the editor's text, past the sidebar ({Runner.editor_point(GeometryOnly())})")

    class NoGeometry:
        def snapshot(self) -> dict:
            return {"window": {"surfaces": {"workspace_sidebar_visible": False}, "visual_geometry": {"surfaces": [{"name": "source-view", "rect": None, "absence_reason": "no-active-editor"}]}}}

    expect(Runner.editor_point(NoGeometry()) is None, "no editor geometry, no point")
    sheet = [{"name": "source-view", "rect": {"x": 0, "y": 94, "width": 1280, "height": 673}}, {"name": "document-properties", "rect": {"x": 0, "y": 347, "width": 1280, "height": 420}}]
    expect(editor_point_in(sheet) == (426, 318), f"an open properties bottom sheet is clicked around ({editor_point_in(sheet)})")
    overlay = [{"name": "source-view", "rect": {"x": 0, "y": 94, "width": 1280, "height": 673}}, {"name": "workspace-sidebar", "rect": {"x": 0, "y": 94, "width": 640, "height": 673}}]
    expect(editor_point_in(overlay) == (640, 430), f"an overlaid sidebar is clicked around ({editor_point_in(overlay)})")
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

    # The keystroke group (C2's toolkit-floor exception, reduced by C5): its case grid per
    # tier, the override, the phases' kinds, and the landing verification.
    for tier, lengths in (("quick", (3_500, 10_000, 49_950)), ("full", (500, 1_000, 2_000, 2_500, 3_500, 5_000, 10_000, 20_000, 49_950))):
        grid = [c for c in build_cases(tier) if c.group == "keystroke"]
        ids = [c.id for c in grid]
        expected_ids = [f"keystroke/{n}/{flavor}/{ext}/wrap-{wrap}" for n in lengths for flavor, ext in (("css", "html"), ("prose", "txt")) for wrap in ("on", "off")]
        expect(ids == expected_ids, f"keystroke {tier}: case ids {ids}")
        expect(len(grid) == 4 * len(lengths), f"keystroke {tier}: four cases per length")
        expect(all(c.flow == "keystroke" and c.fixture is not None and c.fixture.kind == "keystroke" and c.fixture.size == c.length for c in grid), f"keystroke {tier}: keystroke fixtures of the swept length")
        expect(all(c.settings.get("word-wrap") is (c.id.endswith("wrap-on")) and c.settings.get("show-minimap") is False for c in grid), f"keystroke {tier}: word wrap on and off, minimap off")
        expect(all(c.fixture.ext == c.attrs["ext"] and c.fixture.flavor == c.attrs["flavor"] for c in grid), f"keystroke {tier}: shape attrs match the fixture")
        expect(len({c.fixture.seed for c in grid if c.attrs["wrap"]}) == 2 * len(lengths), f"keystroke {tier}: one seed per length and flavour")
    expect(sweep_lengths("quick", "keystroke=4000,6000")["keystroke"] == (4_000, 6_000), "RESPONSIVENESS_LENGTHS replaces the keystroke grid")
    expect(all(PHASE_KINDS[f"key-{w}"] == "interaction" and PHASE_KINDS[f"place-{w}"] == "navigation" for w in KEYSTROKE_POSITIONS) and PHASE_KINDS["verify"] == "navigation", "keystroke phases: bursts are interactions, placement and verification navigation")
    expect(KEY_ESCAPE in ALLOWED_KEYSYMS, "Escape closes the search bar")
    expect(set(load_generator().KEYSTROKE_MARKERS) == set(KEYSTROKE_POSITIONS), "a marker per keystroke position")
    landed = {"start": 1, "middle": 1, "end": 1, "short": 1, "burst_anywhere": 4}
    expect(keystroke_misses(landed, KEYSTROKE_POSITIONS) == [], "bursts at their markers pass")
    expect(keystroke_misses({**landed, "middle": 0, "burst_anywhere": 4}, KEYSTROKE_POSITIONS) == ["middle"], "a burst away from its marker fails the case")
    expect(keystroke_misses({**landed, "burst_anywhere": 5}, KEYSTROKE_POSITIONS) == ["burst-count"], "an extra burst fails the case")
    expect(keystroke_misses({"end": 1, "burst_anywhere": 1}, ["end"]) == [], "only the typed positions are checked")
    expect(keystroke_misses({"start": None, "burst_anywhere": None}, ["start"]) == ["start", "burst-count"], "a missing count fails the case")
    expect(find_key({"a": [{"b": None}, {"search": {"editor_match_count": 2}}]}, "editor_match_count") == 2 and find_key({"a": 1}, "x") is None, "find_key searches the snapshot tree")

    # Window construction (C4): spans by name, stages summed, the largest
    # lateness before the first frame, and the verdict against B_settle.
    span_records = {
        "span": [
            {"kind": "span", "name": "startup_toolkit", "start_us": 0, "end_us": 40_000},
            {"kind": "span", "name": "startup_lushtext", "start_us": 40_000, "end_us": 45_000},
            {"kind": "span", "name": "migrate_split_view", "start_us": 50_000, "end_us": 90_000},
            {"kind": "span", "name": "template", "start_us": 45_000, "end_us": 50_000},
            {"kind": "span", "name": "window_construction", "start_us": 45_000, "end_us": 120_000},
            {"kind": "span", "name": "first_frame", "start_us": 45_000, "end_us": 200_000},
            {"kind": "span", "name": "late_stage", "start_us": 300_000, "end_us": 301_000},
        ],
        "late": [{"t_us": 121_000, "lateness_us": 80_000}, {"t_us": 250_000, "lateness_us": 99_000}],
    }
    summary_c4 = construction_summary(span_records, {"profile": "steady", "backend": "dconf"}, Path("target/responsiveness/release/lushtext"))
    expect(summary_c4 is not None and summary_c4["window_construction_us"] == 75_000 and summary_c4["first_frame_us"] == 155_000, f"construction spans by name ({summary_c4})")
    expect(summary_c4 is not None and summary_c4["largest_dispatch_before_first_frame_us"] == 80_000, "only latenesses before the first frame count")
    expect(summary_c4 is not None and list(summary_c4["stages_us"]) == ["migrate_split_view", "template"], "stages after the first frame are not construction stages")
    expect(summary_c4 is not None and summary_c4["build_profile"] == "release" and summary_c4["backend"] == "dconf", "profile, backend and build are recorded")
    verdicts_c4 = construction_verdicts([{"case": "baseline/small", "variant": "plain", "rep": 0, "construction": summary_c4}], 16_000)
    expect(verdicts_c4[0]["verdict"] == "over" and verdicts_c4[0]["over_budget"] == ["window_construction", "largest_dispatch_before_first_frame", "largest_composition_dispatch"], f"construction verdict ({verdicts_c4})")
    expect(construction_case_id("fresh", "keyfile") == "baseline/small" and construction_case_id("steady", "dconf") == "baseline/small/steady-dconf", "construction case ids")
    populated_case = next((c for c in full if c.id == POPULATED_CASE_ID), None)
    expect(populated_case is not None and {k: populated_case.attrs.get(k) for k in ("profile", "backend", "populated")} == {"profile": "steady", "backend": "keyfile", "populated": True} and populated_case.settings.get("workspace-sidebar-visible") is True and populated_case.settings.get("properties-sidebar-visible") is True, "the populated profile shows both panes over a steady keyfile profile")
    by_id = {c.id: c for c in full}
    expect(
        fresh_profile_runs_migration(by_id["baseline/small"])
        and fresh_profile_runs_migration(by_id[construction_case_id("fresh", "dconf")])
        and not fresh_profile_runs_migration(by_id[construction_case_id("steady", "keyfile")])
        and not fresh_profile_runs_migration(populated_case),
        "the fresh profiles that show no pane run the legacy split-view migration; steady and populated ones are seeded migrated",
    )
    with tempfile.TemporaryDirectory() as scratch:
        root = Path(scratch)
        entries = seed_populated_workspace(root / "data", root / "workspace")
        state = json.loads((root / "data/workspaces.json").read_text(encoding="utf-8"))
        expect(entries == len(list((root / "workspace").iterdir())) == 300 and state["kind"] == "dev.cominotti.lushtext.workspace-state" and state["data"]["workspaces"][0]["folders"][0]["path"] == str(root / "workspace"), "the populated workspace seeds a few hundred entries and restores them")

    # Staged composition (C4b): stage, frame-phase and layout spans are not
    # construction stages; every dispatch until composition completes counts.
    composed_records = {
        "span": [
            {"kind": "span", "name": "startup_lushtext", "start_us": 0, "end_us": 1_000},
            {"kind": "span", "name": "composition_warm", "start_us": 2_000, "end_us": 9_000},
            {"kind": "span", "name": "template", "start_us": 10_000, "end_us": 19_000},
            {"kind": "span", "name": "window_construction", "start_us": 10_000, "end_us": 22_000},
            {"kind": "span", "name": "composition_construct", "start_us": 10_000, "end_us": 22_000},
            {"kind": "span", "name": "window_measure_h", "start_us": 40_000, "end_us": 43_000},
            {"kind": "span", "name": "frame_layout", "start_us": 40_000, "end_us": 45_000},
            {"kind": "span", "name": "frame_paint", "start_us": 45_000, "end_us": 48_000},
            {"kind": "span", "name": "first_frame", "start_us": 10_000, "end_us": 48_000},
            {"kind": "span", "name": "frame_layout", "start_us": 50_000, "end_us": 52_000},
            {"kind": "span", "name": "frame_paint", "start_us": 52_000, "end_us": 53_000},
            {"kind": "span", "name": "composition_content", "start_us": 49_000, "end_us": 50_000},
            {"kind": "span", "name": "composition_complete", "start_us": 2_000, "end_us": 70_000},
        ],
        "late": [{"t_us": 9_100, "lateness_us": 7_000}, {"t_us": 22_100, "lateness_us": 12_000}, {"t_us": 60_000, "lateness_us": 18_000}, {"t_us": 90_000, "lateness_us": 50_000}],
    }
    composed = construction_summary(composed_records, {}, Path("target/responsiveness/release/lushtext"))
    expect(composed is not None and list(composed["stages_us"]) == ["template"], f"composition, frame and layout spans are not construction stages ({composed and composed['stages_us']})")
    expect(composed is not None and composed["composition_stages_us"] == {"warm": 7_000, "construct": 12_000, "content": 1_000}, "composition stage dispatches by stage")
    expect(composed is not None and composed["frame_phases_us"] == [{"layout": 5_000, "paint": 3_000}, {"layout": 2_000, "paint": 1_000}], "frame phases in frame order")
    rerun = frame_phases([
        {"name": "frame_layout", "start_us": 0, "end_us": 7_000},
        {"name": "frame_layout", "start_us": 7_000, "end_us": 7_020},
        {"name": "frame_paint", "start_us": 7_020, "end_us": 9_000},
        {"name": "frame_layout", "start_us": 20_000, "end_us": 20_300},
        {"name": "frame_paint", "start_us": 20_300, "end_us": 21_000},
    ])
    expect(rerun == [{"layout": 7_020, "paint": 1_980}, {"layout": 300, "paint": 700}], f"a frame's repeated layout runs belong to that frame ({rerun})")
    expect(composed is not None and composed["layout_probe_us"] == [["window_measure_h", 3_000]], "the window's first measures are listed")
    expect(composed is not None and composed["composition_complete_us"] == 68_000 and composed["fill_in_us"] == 22_000, "composition complete and the content fill-in")
    expect(composed is not None and composed["largest_composition_dispatch_us"] == 18_000 and composed["dispatches_over_4ms_us"] == [7_000, 12_000, 18_000], "every dispatch until composition completes counts, none after")
    composed_verdict = construction_verdicts([{"case": "baseline/small", "variant": "plain", "rep": 0, "construction": composed}], 16_000)
    expect(composed_verdict[0]["over_budget"] == ["largest_composition_dispatch"] and composed_verdict[0]["fill_in_within_accepted"] is True, f"composition verdict ({composed_verdict})")

    # The first frame where it is seen (shorten-window-content-fill-in, C4c).
    # A build that ran an unseen frame on the unmapped window (ledger A51)
    # reports `first_frame` on it; the lane takes the first frame paint after
    # the present stage instead, in every build, and the first presentation
    # from the frame records.
    unseen_records = {
        "span": [
            {"kind": "span", "name": "frame_layout", "start_us": 18_000, "end_us": 20_000},
            {"kind": "span", "name": "frame_paint", "start_us": 20_000, "end_us": 21_000},
            {"kind": "span", "name": "first_frame", "start_us": 10_000, "end_us": 21_000},
            {"kind": "span", "name": "composition_present", "start_us": 29_000, "end_us": 30_000},
            {"kind": "span", "name": "frame_layout", "start_us": 43_000, "end_us": 46_000},
            {"kind": "span", "name": "frame_paint", "start_us": 46_000, "end_us": 48_000},
            {"kind": "span", "name": "composition_complete", "start_us": 2_000, "end_us": 70_000},
        ],
        "frame": [
            {"kind": "frame", "frame_counter": 0, "frame_time_us": 18_000, "presentation_us": None},
            {"kind": "frame", "frame_counter": 1, "frame_time_us": 43_000, "presentation_us": 51_000},
        ],
        "late": [],
    }
    unseen = construction_summary(unseen_records, {}, Path("target/responsiveness/release/lushtext"))
    expect(unseen is not None and unseen["first_painted_frame_us"] == 46_000 and unseen["first_presentation_us"] == 49_000, f"the first painted frame is the first paint after present, not the unseen frame ({unseen})")
    expect(unseen is not None and unseen["fill_in_us"] == 22_000, f"the fill-in starts at the first mapped frame ({unseen and unseen['fill_in_us']})")
    unpresented = construction_summary({**unseen_records, "frame": [dict(unseen_records["frame"][1], presentation_us=None), {"kind": "frame", "frame_counter": 2, "frame_time_us": 60_000, "presentation_us": 62_000}]}, {}, Path("target/responsiveness/release/lushtext"))
    expect(unpresented is not None and unpresented["first_presentation_us"] is None, f"a later frame's presentation is not the first mapped frame's ({unpresented and unpresented['first_presentation_us']})")

    # The opened document shown after composition completed: the fill-in and
    # the budgeted dispatches reach the activation's shown frame.
    shown_records = {
        "span": unseen_records["span"] + [
            # Another window's span (another origin), logged first, is not
            # this window's.
            {"kind": "span", "name": "composition_activation_shown", "start_us": 1_000, "end_us": 190_000},
            {"kind": "span", "name": "composition_activation_shown", "start_us": 2_000, "end_us": 80_000},
        ],
        "frame": unseen_records["frame"],
        "late": [{"t_us": 75_000, "lateness_us": 17_000}, {"t_us": 95_000, "lateness_us": 30_000}],
    }
    shown = construction_summary(shown_records, {}, Path("target/responsiveness/release/lushtext"))
    expect(shown is not None and shown["activation_shown_us"] == 78_000 and shown["fill_in_us"] == 32_000, f"the fill-in reaches the activation's shown frame ({shown})")
    expect(shown is not None and shown["largest_composition_dispatch_us"] == 17_000, "a dispatch between complete and activation shown is budgeted, none after")
    expect(shown is not None and "activation_shown" not in shown["composition_stages_us"], f"activation shown is not a stage dispatch ({shown and shown['composition_stages_us']})")
    shown_verdict = construction_verdicts([{"case": "baseline/small", "variant": "plain", "rep": 0, "construction": shown}], 16_000)
    expect(shown_verdict[0]["over_budget"] == ["largest_composition_dispatch"] and shown_verdict[0]["fill_in_over"] is False, f"activation-shown verdict ({shown_verdict})")

    # A fill-in over the accepted 40 ms is flagged (and fails the run only
    # under RESPONSIVENESS_CONSTRUCTION_ENFORCE=1); the median is reported
    # against three reference frames.
    slow_records = {**shown_records, "span": unseen_records["span"][:-1] + [{"kind": "span", "name": "composition_complete", "start_us": 2_000, "end_us": 90_000}], "late": []}
    slow = construction_summary(slow_records, {}, Path("target/responsiveness/release/lushtext"))
    slow_verdict = construction_verdicts([{"case": "baseline/small", "variant": "plain", "rep": 0, "construction": slow}], 16_000)
    expect(slow is not None and slow["fill_in_us"] == 42_000 and slow_verdict[0]["fill_in_over"] is True and slow_verdict[0]["fill_in_within_accepted"] is False, f"a fill-in over 40 ms is flagged ({slow_verdict})")
    median = fill_in_median(shown_verdict + slow_verdict + composed_verdict, 8_333)
    expect(median == {"median_us": 32_000, "target_us": 24_999, "within_target": False, "runs": 3}, f"the fill-in median against three reference frames ({median})")

    # Post-open settle (bound-post-open-settle, C6). An exception here is a
    # failure, not an abort, so the other checks still report.
    try:
        post_open_self_test(expect)
    except Exception as exc:  # noqa: BLE001 - reported as a failure
        failures.append(f"post-open self-test raised {type(exc).__name__}: {exc}")

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
    merge.add_argument("--from", dest="sources", action="append", required=True, help="a run's artifact dir, or LABEL=DIR to label its rows with a side (baseline, branch)")
    report = sub.add_parser("report")
    report.add_argument("--artifact-dir", type=Path, required=True)
    report.add_argument("--axiom-observations", type=Path)
    report.add_argument("--toolkit-floor", action="append", default=[], help=f"a toolkit-floor exception ATTRIBUTION=AXIOM[:REPORT] (also {TOOLKIT_FLOORS_ENV})")
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
        return write_report(args.artifact_dir.resolve(), args.axiom_observations, args.toolkit_floor)
    args.artifact_dir = args.artifact_dir.resolve()
    args.binary = args.binary.resolve()
    (args.artifact_dir / "logs").mkdir(parents=True, exist_ok=True)
    if args.command == "session":
        return session_main(args)
    return run_main(args)


if __name__ == "__main__":
    raise SystemExit(main())
