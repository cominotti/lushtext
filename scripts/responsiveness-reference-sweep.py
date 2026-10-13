#!/usr/bin/python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Reduced reference sweep for the responsiveness lane (a documented deviation
from the `full` tier's grid; docs/next/long-line-documents.md).

The `full` grid (95 cases x 3 variants x 3 repetitions) takes 8-10 hours on the
reference machine. Every threshold C1 and C2 read is the boundary of a
monotone series over paragraph length (or line length, or total size), so this
sweep finds each boundary instead of measuring the whole grid:

1. Anchors: one plain repetition at a few lengths per family.
2. Bisection: per threshold, one plain repetition at the geometric midpoint of
   its bracket [largest passing, smallest failing], at most four steps, until
   the bracket ratio is at most 1.5. A threshold that fails at its smallest
   length steps down by 4x; one that passes at its largest steps up by 4x.
3. Bracketing: the plain, AT and probe-IM variants, three repetitions each, at
   the two points that bracket each boundary. A failing point whose case hit
   the interaction cap is already decided (a hang only confirms a fail) and
   is not repeated.

Each run measures only the interactions its thresholds read
(RESPONSIVENESS_PHASES; open and background always run), with a 5 s
interaction cap (10 s above 500k characters, so the open completes): any
phase that long is over every budget, so the shorter cap changes no verdict.
The baseline (budget verdict and calibration) and dense (V_batch) groups are
taken from their own runs, passed with --reuse.

Every part is a normal lane run into OUT/parts/<name>; a part whose
summary.json passed is reused, so the sweep resumes after an interruption.
At the end the parts are joined with `merge-runs` into OUT/merged and
reported with `report`.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import math
import os
import subprocess
import sys
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
WRAPPER = REPO_ROOT / "scripts/run-responsiveness-smoke.sh"
DRIVER_PATH = REPO_ROOT / "scripts/responsiveness-smoke-driver.py"

LAYOUT = "scroll"
FIND = "find"
CARET = "click,type-start,type-middle,type-end"
ALL_BUT_FIND = "scroll,pagedown,click,type-start,type-middle,type-end"
DENSE = "scroll,pagedown"

PARAGRAPH_SPAN = ((2_000, 10_000, 50_000, 250_000), 500, 1_900_000)  # anchors, floor, ceiling

# threshold -> (group, phases, case patterns at length L, anchors, floor, ceiling)
THRESHOLDS = {
    "L_syntax": ("paragraph", LAYOUT, ["paragraph/{L}/*/*/*/minimap-off"], *PARAGRAPH_SPAN),
    "L_minimap": ("paragraph", LAYOUT, ["paragraph/{L}/css/txt/*/*"], *PARAGRAPH_SPAN),
    "N_visible": ("paragraph", LAYOUT, ["paragraph/{L}/css/txt/*/minimap-off"], *PARAGRAPH_SPAN),
    "L_search_highlight": ("paragraph", FIND, ["paragraph/{L}/css/txt/*/minimap-off"], *PARAGRAPH_SPAN),
    "L_protect": ("paragraph", CARET, ["paragraph/{L}/css/txt/*/minimap-off"], *PARAGRAPH_SPAN),
    "L_protect_stall": ("paragraph", CARET, ["paragraph/{L}/css/txt/*/minimap-off"], *PARAGRAPH_SPAN),
    "L_invisibles": ("invisibles", LAYOUT, ["invisibles/{L}/*"], (10_000, 250_000), 500, 1_900_000),
    "S_syntax_interact": ("total-size", ALL_BUT_FIND, ["total-size/{L}"], (250_000, 1_000_000), 8_000, 10_000_000),
    # 120 is the driver's DENSE_REFERENCE_LENGTH: the floor V_batch is read against, so never stepped below.
    "V_batch": ("dense", DENSE, ["dense/{L}/"], (120, 500, 2_000, 10_000, 50_000), 120, 50_000),
    "S_syntax": ("total-size", ALL_BUT_FIND, ["total-size/{L}"], (250_000, 1_000_000), 30_000, 10_000_000),
}
RATIO = 1.5
MAX_STEPS = 4


def load_driver():
    spec = importlib.util.spec_from_file_location("responsiveness_driver", DRIVER_PATH)
    module = importlib.util.module_from_spec(spec)
    sys.modules["responsiveness_driver"] = module
    spec.loader.exec_module(module)
    return module


def round2(value: float) -> int:
    """Two significant digits, so probe points stay readable."""
    if value <= 0:
        return 0
    digits = int(math.floor(math.log10(value))) - 1
    return int(round(value / 10**digits) * 10**digits)


class Sweep:
    def __init__(self, out: Path, reuse: list[Path], axiom_log: Path | None, prior: list[Path] | None = None, only: set[str] | None = None, anchors: dict[str, tuple[int, ...]] | None = None, floors: dict[str, int] | None = None) -> None:
        self.out = out
        # Parts of an earlier sweep whose rows count for the brackets (a
        # follow-up sweep refines some thresholds without re-measuring).
        self.prior_parts = [p for d in (prior or []) for p in sorted((d / "parts").iterdir()) if p.is_dir() and (p / "summary.json").exists() and json.loads((p / "summary.json").read_text(encoding="utf-8")).get("status") == "passed"]
        self.only = only
        self.anchor_override = anchors or {}
        self.floor_override = floors or {}
        self.parts_dir = out / "parts"
        self.parts_dir.mkdir(parents=True, exist_ok=True)
        self.reuse = reuse
        self.axiom_log = axiom_log
        self.driver = load_driver()
        self.parts: list[Path] = []
        self.steps = {name: 0 for name in THRESHOLDS}
        self.log = (out / "sweep.log").open("a", encoding="utf-8")
        calibration = next((p / "calibration.json" for p in reuse if (p / "calibration.json").exists()), None)
        self.slack = json.loads(calibration.read_text(encoding="utf-8"))["slack_us"] if calibration else 0
        self.axioms = self.driver.load_axiom_observations(axiom_log)

    def note(self, message: str) -> None:
        line = f"{time.strftime('%Y-%m-%dT%H:%M:%S')} {message}"
        print(line, flush=True)
        self.log.write(line + "\n")
        self.log.flush()

    def run_part(self, name: str, group_lengths: dict[str, list[int]], patterns: list[str], phases: str, variants: str, reps: int) -> None:
        part = self.parts_dir / name
        self.parts.append(part)
        summary = part / "summary.json"
        if summary.exists() and json.loads(summary.read_text(encoding="utf-8")).get("status") == "passed":
            self.note(f"reuse {name}")
            return
        big = max((max(v) for g, v in group_lengths.items() if v and (g == "paragraph" or g == "invisibles")), default=0)
        cap = "10" if big > 500_000 else "5"
        env = dict(os.environ)
        env.update(
            {
                "RESPONSIVENESS_LENGTHS": ";".join(f"{g}={','.join(str(x) for x in sorted(set(v)))}" for g, v in group_lengths.items()),
                "RESPONSIVENESS_PHASES": phases,
                "RESPONSIVENESS_VARIANTS": variants,
                "RESPONSIVENESS_REPETITIONS": str(reps),
                "RESPONSIVENESS_BISECT": "0",
                "RESPONSIVENESS_CASE_CAP_S": cap,
            }
        )
        command = [str(WRAPPER), "--tier", "full", "--artifact-dir", str(part)]
        for pattern in patterns:
            command += ["--case", pattern]
        load = Path("/proc/loadavg").read_text().split()[:3]
        self.note(f"heavy {name}: {self.heavy_processes()}")
        self.note(f"start {name} lengths={env['RESPONSIVENESS_LENGTHS']} phases={phases} variants={variants} reps={reps} cap={cap}s cases={len(patterns)} patterns load={' '.join(load)}")
        started = time.monotonic()
        with (self.parts_dir / f"{name}.log").open("w", encoding="utf-8") as handle:
            status = subprocess.run(command, env=env, stdout=handle, stderr=subprocess.STDOUT, check=False).returncode
        self.note(f"end {name} status={status} wall={time.monotonic() - started:.0f}s")
        if status != 0:
            raise SystemExit(f"part {name} failed (status {status}); see {self.parts_dir / (name + '.log')}")

    def spec(self, name: str) -> tuple:
        group, phases, patterns, anchors, floor, ceiling = THRESHOLDS[name]
        return group, phases, patterns, self.anchor_override.get(name, anchors), self.floor_override.get(name, floor), ceiling

    def names(self) -> list[str]:
        return [n for n in THRESHOLDS if self.only is None or n in self.only]

    def rows(self) -> list[dict]:
        rows: list[dict] = []
        for part in [*self.reuse, *self.prior_parts, *self.parts]:
            results = part / "results.jsonl"
            if results.exists():
                rows.extend(json.loads(line) for line in results.read_text(encoding="utf-8").splitlines() if line.strip())
        return rows

    def verdicts(self, name: str, rows: list[dict]) -> dict[int, bool]:
        """Per-length pass/fail of one threshold, each length judged on its own rows."""
        group = THRESHOLDS[name][0]
        budgets = self.driver.read_budgets()
        baseline = [r for r in rows if r.get("case") == "baseline/small"]
        if name == "V_batch":
            # V_batch reads scroll and Page Down against the dense reference case.
            reference = self.driver.DENSE_REFERENCE_LENGTH
            baseline += [r for r in rows if r.get("group") == "dense" and r.get("length") == reference]
        if name == "L_syntax":
            # L_syntax discounts the size-independent first highlight, read
            # from the smallest short-line total-size document.
            sizes = sorted({r["length"] for r in rows if r.get("group") == "total-size" and r.get("length")})
            baseline += [r for r in rows if r.get("group") == "total-size" and sizes and r.get("length") == sizes[0]]
        by_length: dict[int, list[dict]] = {}
        for r in rows:
            if r.get("group") == group and r.get("length"):
                by_length.setdefault(r["length"], []).append(r)
        out: dict[int, bool] = {}
        for length in sorted(by_length):
            derived = self.driver.derive(by_length[length] + baseline, budgets, self.slack, True, self.axioms)[name]
            if name == "V_batch":
                if derived.get("line_length") == length:
                    out[length] = True
                elif str(derived.get("dense_reason", "")).startswith("fails"):
                    out[length] = False
                continue
            verdict = self.driver.length_verdict(derived, length)
            if verdict is not None:
                out[length] = verdict
        return out

    def bracket(self, name: str, rows: list[dict]) -> tuple[int | None, int | None, dict[int, bool]]:
        decided = self.verdicts(name, rows)
        lo = hi = None
        for length in sorted(decided):
            if decided[length] and hi is None:
                lo = length
            elif not decided[length] and hi is None:
                hi = length
        return lo, hi, decided

    def bracket_record(self, name: str, rows: list[dict]) -> dict:
        lo, hi, decided = self.bracket(name, rows)
        return {"lo": lo, "hi": hi, "verdicts": {str(k): v for k, v in decided.items()}}

    def next_point(self, name: str, rows: list[dict]) -> int | None:
        _group, _phases, _patterns, _anchors, floor, ceiling = self.spec(name)
        lo, hi, decided = self.bracket(name, rows)
        if not decided:
            return None
        if self.steps[name] >= MAX_STEPS:
            return None
        if lo is None:
            point = round2(min(decided) / 4)
            return point if point >= floor and point not in decided else None
        if hi is None:
            point = min(ceiling, round2(max(decided) * 4)) if max(decided) < ceiling else None
            return point if point and point not in decided else None
        if hi / lo <= RATIO:
            return None
        point = round2(math.sqrt(lo * hi))
        return point if lo < point < hi and point not in decided else None

    def measure(self, label: str, requests: dict[str, list[int]], variants: str, reps: int) -> None:
        """One lane run per phase set, covering every requested (threshold, length)."""
        by_phases: dict[str, tuple[dict[str, list[int]], list[str]]] = {}
        for name, lengths in requests.items():
            group, phases, patterns, *_ = THRESHOLDS[name]
            bucket = by_phases.setdefault(phases, ({}, []))
            for length in lengths:
                bucket[0].setdefault(group, []).append(length)
                for pattern in patterns:
                    case = pattern.format(L=length)
                    if case not in bucket[1]:
                        bucket[1].append(case)
        for index, (phases, (group_lengths, patterns)) in enumerate(sorted(by_phases.items())):
            kind = {LAYOUT: "layout", FIND: "find", CARET: "caret", ALL_BUT_FIND: "total", DENSE: "dense"}[phases]
            # Split by cap class so a 1.9M open never runs under the 5 s cap
            # and small lengths never wait under the 10 s one.
            small = {g: [x for x in v if x <= 500_000 or g == "total-size"] for g, v in group_lengths.items()}
            large = {g: [x for x in v if x > 500_000 and g != "total-size"] for g, v in group_lengths.items()}
            for suffix, lengths in (("", small), ("-large", large)):
                lengths = {g: v for g, v in lengths.items() if v}
                if not lengths:
                    continue
                wanted = [p for p in patterns if int(p.split("/")[1]) in {x for v in lengths.values() for x in v}]
                self.run_part(f"{label}-{kind}{suffix}", lengths, wanted, phases, variants, reps)

    def heavy_processes(self) -> str:
        """Co-running processes above one CPU, recorded with every part."""
        out = subprocess.run(["ps", "-eo", "pcpu=,comm="], capture_output=True, text=True, check=False).stdout.splitlines()
        heavy = []
        for line in out:
            parts = line.split(None, 1)
            if len(parts) == 2 and float(parts[0]) >= 100.0:
                heavy.append(f"{parts[1].strip()}:{parts[0]}%")
        return ",".join(heavy) or "none"

    def finish(self, sources: list[Path]) -> int:
        merged = self.out / "merged"
        args = ["merge-runs", "--artifact-dir", str(merged)]
        for source in sources:
            args += ["--from", str(source)]
        self.driver.main(args)
        report_args = ["report", "--artifact-dir", str(merged)]
        if self.axiom_log:
            report_args += ["--axiom-observations", str(self.axiom_log)]
        status = self.driver.main(report_args)
        rows = [json.loads(line) for line in (merged / "results.jsonl").read_text(encoding="utf-8").splitlines() if line.strip()]
        final = {name: self.bracket_record(name, rows) for name in self.names()}
        (self.out / "brackets-final.json").write_text(json.dumps(final, indent=2) + "\n", encoding="utf-8")
        self.note(f"sweep done report={merged / 'report.md'} status={status}")
        return status

    def reconfirm(self, prior_out: Path) -> int:
        """Re-measure the baseline and every bracketing point on an idle machine.

        Plain variant, three repetitions per point (the AT and probe-IM
        bracketing stays with the original sweep); the brackets come from the
        original sweep's brackets-final.json. Resumable like the sweep.
        """
        source = prior_out if prior_out.is_file() else prior_out / "brackets-final.json"
        brackets = json.loads(source.read_text(encoding="utf-8"))
        self.note(f"reconfirm start prior={prior_out} heavy={self.heavy_processes()}")
        self.run_part("idle-baseline", {}, ["baseline"], "", "plain,at,probe-im", 3)
        calibration = self.parts_dir / "idle-baseline" / "calibration.json"
        if calibration.exists():
            self.slack = json.loads(calibration.read_text(encoding="utf-8"))["slack_us"]
        points = {name: [p for p in (b["lo"], b["hi"]) if p is not None] for name, b in brackets.items() if name in THRESHOLDS}
        if "V_batch" in points:
            # The floor V_batch is read against is re-measured with it.
            points["V_batch"] = sorted({self.driver.DENSE_REFERENCE_LENGTH, *points["V_batch"]})
        self.measure("idle-bracket", {k: v for k, v in points.items() if v}, "plain", 3)
        status = self.finish(list(self.parts))
        rows = self.rows()
        comparison = {}
        for name, bracket in brackets.items():
            idle = self.verdicts(name, rows)
            comparison[name] = {"loaded": {str(p): bracket["verdicts"].get(str(p)) for p in points.get(name, [])}, "idle": {str(p): idle.get(p) for p in points.get(name, [])}}
        (self.out / "reconfirm.json").write_text(json.dumps(comparison, indent=2) + "\n", encoding="utf-8")
        self.note(f"reconfirm comparison {comparison}")
        return status

    def run(self) -> int:
        self.note(f"sweep start out={self.out} reuse={[str(p) for p in self.reuse]} slack={self.slack}")
        rows = self.rows()
        anchors = {}
        for name in self.names():
            group = THRESHOLDS[name][0]
            known = {r["length"] for r in rows if r.get("group") == group}
            missing = [a for a in self.spec(name)[3] if a not in known or name in self.anchor_override]
            if missing:
                anchors[name] = missing
        self.measure("r0-anchors", anchors, "plain", 1)
        for round_index in range(1, MAX_STEPS + 3):
            rows = self.rows()
            requests: dict[str, list[int]] = {}
            for name in self.names():
                point = self.next_point(name, rows)
                if point is not None:
                    requests[name] = [point]
                    self.steps[name] += 1
            if not requests:
                break
            self.note(f"round {round_index} requests {requests}")
            self.measure(f"r{round_index}-bisect", requests, "plain", 1)
        rows = self.rows()
        confirm: dict[str, list[int]] = {}
        brackets = {}
        for name in self.names():
            brackets[name] = self.bracket_record(name, rows)
            lo, hi = brackets[name]["lo"], brackets[name]["hi"]
            points = []
            if lo is not None:
                points.append(lo)
            if hi is not None:
                group = THRESHOLDS[name][0]
                at_hi = [r for r in rows if r.get("group") == group and r.get("length") == hi]
                phases = set(THRESHOLDS[name][1].split(",")) | {"open", "background"}
                if not self.driver.hung(at_hi, phases):
                    points.append(hi)
            if points:
                confirm[name] = points
        (self.out / "brackets-before-confirmation.json").write_text(json.dumps(brackets, indent=2) + "\n", encoding="utf-8")
        self.note(f"bracketing points {confirm}")
        self.measure("rc-bracket", confirm, "plain,at,probe-im", 3)
        return self.finish([*self.reuse, *self.prior_parts, *self.parts])


def self_test() -> int:
    """Probe-point arithmetic: rounding, bracketing and the step rules."""
    failures = []

    def expect(condition: bool, label: str) -> None:
        if not condition:
            failures.append(label)

    expect(round2(4472.1) == 4500 and round2(70_710) == 71_000 and round2(15_500) == 16_000, "two significant digits")
    sweep = Sweep.__new__(Sweep)
    sweep.anchor_override, sweep.floor_override, sweep.only = {}, {}, None
    sweep.steps = {name: 0 for name in THRESHOLDS}
    cases = {
        "between": ({2_000: True, 10_000: False}, 4_500),
        "below the smallest": ({2_000: False, 10_000: False}, 500),
        "above the largest": ({250_000: True}, 1_000_000),
        "narrow bracket": ({2_000: True, 3_000: False}, None),
        "a failure below a pass ends the passing run": ({2_000: True, 3_000: False, 10_000: True}, None),
    }
    for label, (verdicts, expected) in cases.items():
        sweep.verdicts = lambda _name, _rows, v=verdicts: dict(v)
        point = sweep.next_point("N_visible", [])
        expect(point == expected, f"{label}: expected {expected}, got {point}")
    sweep.verdicts = lambda _name, _rows: {500: False}
    expect(sweep.next_point("L_protect", []) is None, "never below the floor")
    sweep.steps["N_visible"] = MAX_STEPS
    sweep.verdicts = lambda _name, _rows: {2_000: True, 10_000: False}
    expect(sweep.next_point("N_visible", []) is None, "at most four steps")
    if failures:
        print("responsiveness-reference-sweep self-test failed:", *failures, sep="\n  - ", file=sys.stderr)
        return 1
    print("responsiveness-reference-sweep self-test passed.")
    return 0


def main() -> int:
    if sys.argv[1:] == ["--self-test"]:
        return self_test()
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--reuse", type=Path, action="append", default=[], help="a finished lane run to include (baseline, dense)")
    parser.add_argument("--axiom-observations", type=Path)
    parser.add_argument("--prior", type=Path, action="append", default=[], help="an earlier sweep's --out whose parts count for the brackets")
    parser.add_argument("--only", help="comma-separated thresholds to refine")
    parser.add_argument("--anchors", action="append", default=[], help="NAME=L1,L2 anchor override")
    parser.add_argument("--floor", action="append", default=[], help="NAME=L lowest length to step down to")
    parser.add_argument("--reconfirm", type=Path, help="re-measure the baseline and the bracketing points of an earlier sweep (its --out, or its brackets-final.json) on an idle machine")
    parser.add_argument("--finalize", action="store_true", help="measure nothing: merge --reuse and every --prior part, report, and write brackets-final.json")
    parser.add_argument("--plan", action="store_true", help="print the anchor plan and exit")
    args = parser.parse_args()
    if args.plan:
        for name, spec in THRESHOLDS.items():
            print(f"{name}: group {spec[0]}, phases {spec[1]}, anchors {spec[3]}, floor {spec[4]}, ceiling {spec[5]}")
        return 0
    anchors = {k: tuple(int(x) for x in v.split(",")) for k, _, v in (a.partition("=") for a in args.anchors)}
    floors = {k: int(v) for k, _, v in (f.partition("=") for f in args.floor)}
    only = set(args.only.split(",")) if args.only else None
    sweep = Sweep(args.out.resolve(), [p.resolve() for p in args.reuse], args.axiom_observations, [p.resolve() for p in args.prior], only, anchors, floors)
    if args.reconfirm:
        return sweep.reconfirm(args.reconfirm.resolve())
    if args.finalize:
        return sweep.finish([*sweep.reuse, *sweep.prior_parts])
    return sweep.run()


if __name__ == "__main__":
    raise SystemExit(main())
