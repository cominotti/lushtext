#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Same-session visual geometry invariant smoke runner for LushText."""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from visual_geometry_png import Rect, clamp_rect, compare_crops, read_png


REPO_ROOT = Path(__file__).resolve().parents[1]
CAPTURE_HELPER = (
    REPO_ROOT / ".agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py"
)
VISUAL_PROOF_POLICY_HELPER = REPO_ROOT / "scripts/check-visual-proof-policy.py"
SYSTEM_PYTHON = Path("/usr/bin/python3")
APP_ID = "dev.cominotti.lushtext"
WINDOW_OBJECT_PATH = "/dev/cominotti/lushtext/window/1"
ARTIFACT_TEXT_LIMIT = 1200


def load_capture_helper():
    spec = importlib.util.spec_from_file_location("lushtext_mutter_capture", CAPTURE_HELPER)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load capture helper: {CAPTURE_HELPER}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def load_visual_proof_policy_helper():
    spec = importlib.util.spec_from_file_location(
        "lushtext_visual_proof_policy", VISUAL_PROOF_POLICY_HELPER
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load visual proof policy helper: {VISUAL_PROOF_POLICY_HELPER}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


mutter = load_capture_helper()
visual_proof_policy = load_visual_proof_policy_helper()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact-dir", type=Path, default=Path("build/smoke/visual-geometry"))
    parser.add_argument("--binary", type=Path, default=REPO_ROOT / "target/debug/lushtext")
    parser.add_argument(
        "--scenario-dir",
        type=Path,
        default=REPO_ROOT / "scripts/visual-geometry-scenarios",
    )
    parser.add_argument("--case-filter", help="Run only cases whose id contains this text.")
    parser.add_argument("--internal-run", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--mutter-child", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--case-json", type=Path, help=argparse.SUPPRESS)
    return parser.parse_args()


def bounded(text: object) -> str:
    value = str(text)
    if len(value) <= ARTIFACT_TEXT_LIMIT:
        return value
    return value[:ARTIFACT_TEXT_LIMIT] + " [truncated]"


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def require_tooling(binary: Path) -> str | None:
    for command in (
        "dbus-run-session",
        "gdbus",
        "gsettings",
        "gst-launch-1.0",
        "mutter",
        "pipewire",
        "pw-dump",
        "wireplumber",
    ):
        if shutil.which(command) is None:
            return f"missing required command: {command}"
    if not SYSTEM_PYTHON.is_file():
        return "missing /usr/bin/python3"
    if not binary.is_file() or not os.access(binary, os.X_OK):
        return f"LushText debug binary is missing or not executable: {binary}"
    return None


def load_manifests(scenario_dir: Path) -> list[dict[str, Any]]:
    manifests = []
    for path in sorted(scenario_dir.glob("*.json")):
        payload = json.loads(path.read_text(encoding="utf-8"))
        payload["_manifest_path"] = str(path)
        validate_manifest(payload, path)
        manifests.append(payload)
    if not manifests:
        raise RuntimeError(f"no visual geometry scenario manifests found in {scenario_dir}")
    return manifests


def validate_manifest(manifest: dict[str, Any], path: Path) -> None:
    for field in ("schema_version", "scenario_id", "scenario_type", "matrix", "protected_regions"):
        if field not in manifest:
            raise RuntimeError(f"{path} is missing {field}")
    if int(manifest["schema_version"]) != 1:
        raise RuntimeError(f"{path} has unsupported schema_version")
    if not isinstance(manifest["protected_regions"], list) or not manifest["protected_regions"]:
        raise RuntimeError(f"{path} must declare protected regions")


def expand_cases(manifest: dict[str, Any]) -> list[dict[str, Any]]:
    matrix = manifest["matrix"]
    cases: list[dict[str, Any]] = []
    if manifest["scenario_type"] == "minimap-sidebar":
        for size in matrix["sizes"]:
            for color_scheme in matrix["color_schemes"]:
                for word_wrap in matrix["word_wrap"]:
                    for direction in matrix["directions"]:
                        case_id = (
                            f"{manifest['scenario_id']}--{size['id']}--{color_scheme}"
                            f"--wrap-{str(word_wrap).lower()}--{direction}"
                        )
                        cases.append(
                            {
                                "case_id": case_id,
                                "manifest": manifest,
                                "size": size,
                                "color_scheme": color_scheme,
                                "word_wrap": bool(word_wrap),
                                "direction": direction,
                            }
                        )
    elif manifest["scenario_type"] == "command-palette-overlay":
        for size in matrix["sizes"]:
            for color_scheme in matrix["color_schemes"]:
                case_id = f"{manifest['scenario_id']}--{size['id']}--{color_scheme}"
                cases.append(
                    {
                        "case_id": case_id,
                        "manifest": manifest,
                        "size": size,
                        "color_scheme": color_scheme,
                        "word_wrap": False,
                        "direction": "open",
                    }
                )
    else:
        raise RuntimeError(f"unsupported scenario_type: {manifest['scenario_type']}")
    return cases


def write_case_json(case: dict[str, Any], path: Path, binary: Path, artifact_dir: Path) -> None:
    payload = {**case, "binary": str(binary.resolve()), "artifact_dir": str(artifact_dir.resolve())}
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def create_fixture(case: dict[str, Any], artifact_dir: Path) -> Path:
    fixture_dir = artifact_dir / "fixtures"
    fixture_dir.mkdir(parents=True, exist_ok=True)
    path = fixture_dir / f"{case['case_id']}.txt"
    if case["manifest"]["scenario_type"] == "minimap-sidebar":
        long_tail = "x" * 150
        lines = []
        for index in range(280):
            if case["word_wrap"]:
                lines.append(f"line {index:04d} {long_tail}")
            else:
                lines.append(f"line {index:04d}")
        path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    else:
        path.write_text("Command palette visual geometry fixture\n", encoding="utf-8")
    return path


def outer_run(args: argparse.Namespace) -> int:
    args.artifact_dir.mkdir(parents=True, exist_ok=True)
    skip_reason = require_tooling(args.binary.resolve())
    if skip_reason:
        write_skip_summary(args.artifact_dir, skip_reason)
        print(f"SKIP: {skip_reason}")
        return 0

    manifests = load_manifests(args.scenario_dir)
    cases = [case for manifest in manifests for case in expand_cases(manifest)]
    if args.case_filter:
        cases = [case for case in cases if args.case_filter in case["case_id"]]
    if not cases:
        raise RuntimeError("case filter matched no visual geometry scenarios")

    results = []
    for case in cases:
        case_dir = args.artifact_dir / case["case_id"]
        case_dir.mkdir(parents=True, exist_ok=True)
        case_json = case_dir / "case.json"
        write_case_json(case, case_json, args.binary, case_dir)
        result = run_case_outer(case_json, case_dir, case)
        results.append(result)

    summary = {
        "schema_version": 1,
        "status": "failed" if any(item["status"] == "failed" for item in results) else "passed",
        "case_filter": args.case_filter,
        "case_count": len(results),
        "passed": sum(1 for item in results if item["status"] == "passed"),
        "failed": sum(1 for item in results if item["status"] == "failed"),
        "skipped": sum(1 for item in results if item["status"] == "skipped"),
        "visual_proof_policy": visual_proof_policy.visual_proof_policy_metadata(),
        "cases": results,
    }
    (args.artifact_dir / "summary.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return 1 if summary["status"] == "failed" else 0


def run_case_outer(case_json: Path, case_dir: Path, case: dict[str, Any]) -> dict[str, Any]:
    runtime_root = Path(tempfile.mkdtemp(prefix="lt-vg-"))
    runtime_dir = runtime_root / "runtime"
    runtime_dir.mkdir()
    os.chmod(runtime_dir, 0o700)
    for name in ("data", "config", "cache"):
        (case_dir / name).mkdir(exist_ok=True)
    (case_dir / "runtime-dir.txt").write_text(str(runtime_dir) + "\n", encoding="utf-8")

    env = os.environ.copy()
    env.update(
        {
            "GSETTINGS_BACKEND": "keyfile",
            "GSETTINGS_SCHEMA_DIR": str(REPO_ROOT / "data"),
            "LUSHTEXT_MUTTER_ARTIFACT_DIR": str(case_dir),
            "LUSHTEXT_DATA_DIR": str(case_dir / "data"),
            "XDG_CACHE_HOME": str(case_dir / "cache"),
            "XDG_CONFIG_HOME": str(case_dir / "config"),
            "XDG_DATA_HOME": str(case_dir / "data"),
            "XDG_RUNTIME_DIR": str(runtime_dir),
        }
    )
    log_path = case_dir / "session.log"
    command = [
        "dbus-run-session",
        "--",
        str(SYSTEM_PYTHON),
        str(Path(__file__).resolve()),
        "--internal-run",
        "--case-json",
        str(case_json),
    ]
    with log_path.open("w", encoding="utf-8") as log:
        completed = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)

    manifest_path = case_dir / "scenario-manifest.json"
    if completed.returncode != 0 and not manifest_path.exists():
        write_case_manifest(
            case_dir,
            case,
            "failed",
            "workflow-failure",
            f"visual geometry runner exited {completed.returncode}",
        )
    cleanup = mutter.cleanup_runtime_root(runtime_root, case_dir)
    (case_dir / "runtime-dir-status.txt").write_text(
        f"path={runtime_dir}\ncleanup={cleanup}\nreturncode={completed.returncode}\n",
        encoding="utf-8",
    )
    status = "failed"
    failure_status = "workflow-failure"
    if manifest_path.exists():
        payload = json.loads(manifest_path.read_text(encoding="utf-8"))
        status = payload.get("status", status)
        failure_status = payload.get("failure_status") or failure_status
    return {
        "case_id": case["case_id"],
        "status": status,
        "failure_status": failure_status if status == "failed" else None,
        "artifact_dir": case_dir.name,
        "manifest": "scenario-manifest.json",
    }


def internal_run(args: argparse.Namespace) -> int:
    case = json.loads(args.case_json.read_text(encoding="utf-8"))
    case_dir = Path(case["artifact_dir"])
    processes: list[subprocess.Popen | None] = []
    try:
        pipewire = mutter.start_logged(["pipewire"], case_dir / "pipewire.log")
        processes.append(pipewire)
        mutter.wait_for_pipewire(Path(os.environ["XDG_RUNTIME_DIR"]))
        wireplumber = mutter.start_logged(["wireplumber"], case_dir / "wireplumber.log")
        processes.append(wireplumber)
        apply_gsettings(case)

        env = os.environ.copy()
        env["NO_AT_BRIDGE"] = "1"
        env.pop("AT_SPI_BUS_ADDRESS", None)
        command = [
            "mutter",
            "--headless",
            "--wayland",
            "--no-x11",
            "--virtual-monitor",
            f"{case['size']['width']}x{case['size']['height']}",
            "--",
            str(SYSTEM_PYTHON),
            str(Path(__file__).resolve()),
            "--mutter-child",
            "--case-json",
            str(args.case_json),
        ]
        with (case_dir / "mutter-child.log").open("w", encoding="utf-8") as log:
            completed = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
        print((case_dir / "mutter-child.log").read_text(encoding="utf-8", errors="replace"))
        return completed.returncode
    finally:
        for process in reversed(processes):
            mutter.terminate_process(process)


def gsettings_set(key: str, value: str) -> None:
    subprocess.run(["gsettings", "set", APP_ID, key, value], check=True)


def apply_gsettings(case: dict[str, Any]) -> None:
    initial_sidebar = "true" if case.get("direction") == "hide" else "false"
    gsettings_set("show-minimap", "true")
    gsettings_set("word-wrap", "true" if case.get("word_wrap") else "false")
    gsettings_set("split-view-layout-migrated", "true")
    gsettings_set("workspace-sidebar-visible", initial_sidebar)
    gsettings_set("workspace-sidebar-width-fraction", "0.3")
    gsettings_set("properties-sidebar-visible", "false")
    gsettings_set("window-width", str(int(case["size"]["width"])))
    gsettings_set("window-height", str(int(case["size"]["height"])))
    if case.get("color_scheme") != "default":
        gsettings_set("color-scheme", str(case["color_scheme"]))


def mutter_child(args: argparse.Namespace) -> int:
    import gi

    gi.require_version("Gio", "2.0")
    from gi.repository import Gio

    case = json.loads(args.case_json.read_text(encoding="utf-8"))
    case_dir = Path(case["artifact_dir"])
    fixture = create_fixture(case, case_dir)
    app_env = os.environ.copy()
    app_env.update(
        {
            "GDK_BACKEND": "wayland",
            "GSETTINGS_BACKEND": "keyfile",
            "GSETTINGS_SCHEMA_DIR": str(REPO_ROOT / "data"),
            "GSK_RENDERER": app_env.get("GSK_RENDERER", "cairo"),
            "GTK_USE_PORTAL": "0",
            "NO_AT_BRIDGE": "1",
        }
    )
    app = subprocess.Popen(
        [case["binary"], str(fixture)],
        stdout=(case_dir / "lushtext.stdout").open("wb"),
        stderr=(case_dir / "lushtext.stderr").open("wb"),
        env=app_env,
    )
    (case_dir / "app.pid").write_text(str(app.pid), encoding="utf-8")
    try:
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        mutter.wait_for_window_actions(bus)
        mutter.wait_for_automation_object(bus)
        mutter.wait_for_ready(bus, case_dir, "file-open-complete", 5000)
        mutter.wait_for_ready(bus, case_dir, "visual-geometry-settled", 5000)
        before = capture_step(bus, case_dir, "before")
        run_case_action(bus, case)
        mutter.wait_for_ready(bus, case_dir, "visual-geometry-settled", 5000)
        after = capture_step(bus, case_dir, "after")
        compare_case(case_dir, case, before, after)
        warning_status = scan_warnings(case_dir)
        if warning_status["status"] == "failed":
            write_case_manifest(
                case_dir,
                case,
                "failed",
                "warning-scan-failed",
                "unexpected GTK/Adwaita/GDK warning output",
                warning_status=warning_status,
            )
            return 1
        write_case_manifest(case_dir, case, "passed", None, None, warning_status=warning_status)
        return 0
    except Exception as exc:
        failure_status = classify_failure(exc)
        write_case_manifest(case_dir, case, "failed", failure_status, bounded(exc))
        return 1
    finally:
        mutter.terminate_process(app)


def capture_step(bus, case_dir: Path, step: str) -> dict[str, Any]:
    snapshot_path = case_dir / f"{step}-geometry-snapshot.json"
    screenshot_path = case_dir / f"{step}.png"
    warmup_path = case_dir / f"{step}-warmup.png"
    time.sleep(0.15)
    mutter.capture_monitor(bus, warmup_path)
    time.sleep(0.05)
    snapshot = mutter.snapshot_json(bus)
    snapshot_path.write_text(json.dumps(snapshot, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    mutter.capture_monitor(bus, screenshot_path)
    warmup_path.unlink(missing_ok=True)
    return {"step": step, "snapshot": snapshot, "snapshot_path": snapshot_path, "screenshot": screenshot_path}


def run_case_action(bus, case: dict[str, Any]) -> None:
    scenario_type = case["manifest"]["scenario_type"]
    if scenario_type == "minimap-sidebar":
        mutter.activate_window_action(bus, "toggle-sidebar")
    elif scenario_type == "command-palette-overlay":
        mutter.activate_window_action(bus, "toggle-command-palette")
    else:
        raise RuntimeError(f"unsupported scenario type: {scenario_type}")


def visual_geometry(snapshot: dict[str, Any]) -> dict[str, Any]:
    window = snapshot.get("window")
    if not window:
        raise RuntimeError("snapshot has no active window")
    geometry = window.get("visual_geometry")
    if not geometry:
        raise RuntimeError("snapshot has no visual_geometry")
    return geometry


def surface(snapshot: dict[str, Any], name: str) -> dict[str, Any]:
    for row in visual_geometry(snapshot).get("surfaces", []):
        if row.get("name") == name:
            return row
    raise RuntimeError(f"visual surface not found: {name}")


def scroll_anchor(snapshot: dict[str, Any], name: str) -> dict[str, Any]:
    for row in visual_geometry(snapshot).get("scroll_anchors", []):
        if row.get("name") == name:
            return row
    raise RuntimeError(f"visual scroll anchor not found: {name}")


def rect_for(snapshot: dict[str, Any], name: str) -> Rect:
    row = surface(snapshot, name)
    if not row.get("visible") or not row.get("rect"):
        raise RuntimeError(f"surface {name} is not visible: {row}")
    return Rect.from_mapping(row["rect"])


def compare_case(case_dir: Path, case: dict[str, Any], before: dict[str, Any], after: dict[str, Any]) -> None:
    comparison_dir = case_dir / "comparisons"
    comparison_dir.mkdir(exist_ok=True)
    reports = []
    for region in case["manifest"]["protected_regions"]:
        before_rect = rect_for(before["snapshot"], region["surface"])
        after_rect = rect_for(after["snapshot"], region["surface"])
        if region.get("require_same_rect", True) and before_rect != after_rect:
            raise RuntimeError(
                f"protected region {region['name']} moved: before={before_rect} after={after_rect}"
            )
        before_rect = clamp_rect(read_png(before["screenshot"]), before_rect)
        after_rect = clamp_rect(read_png(after["screenshot"]), after_rect)
        masks = [Rect.from_mapping(mask) for mask in region.get("mask_rects", [])]
        report = compare_crops(
            before["screenshot"],
            after["screenshot"],
            before_rect,
            after_rect,
            masks,
            comparison_dir / region["name"],
        )
        report["name"] = region["name"]
        report["surface"] = region["surface"]
        reports.append(report)
        if report["status"] != "passed":
            (comparison_dir / "comparison-report.json").write_text(
                json.dumps({"status": "failed", "regions": reports}, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            raise RuntimeError(f"visual comparison failed for protected region {region['name']}")

    assert_allowed_region_relationships(case, before["snapshot"], after["snapshot"])
    (comparison_dir / "comparison-report.json").write_text(
        json.dumps({"status": "passed", "regions": reports}, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def assert_allowed_region_relationships(case: dict[str, Any], before: dict[str, Any], after: dict[str, Any]) -> None:
    scenario_type = case["manifest"]["scenario_type"]
    if scenario_type == "minimap-sidebar":
        before_editor = rect_for(before, "editor-viewport")
        after_editor = rect_for(after, "editor-viewport")
        rect_for(after, "minimap-shell")
        rect_for(after, "minimap-source-map")
        rect_for(after, "minimap-marker-strip")
        if case["direction"] == "hide" and after_editor.width <= before_editor.width:
            raise RuntimeError(f"sidebar hide should widen editor: before={before_editor} after={after_editor}")
        if case["direction"] == "show" and after_editor.width >= before_editor.width:
            raise RuntimeError(f"sidebar show should narrow editor: before={before_editor} after={after_editor}")
        anchor = scroll_anchor(after, "source-view")
        if anchor.get("at_top") is not True or anchor.get("at_left") is not True:
            raise RuntimeError(f"source-view should remain top-left anchored: {anchor}")
    elif scenario_type == "command-palette-overlay":
        active = surface(after, "active-transient")
        if active.get("visible") is not True:
            raise RuntimeError(f"command palette should expose active transient geometry: {active}")


WARNING_RE = re.compile(
    r"(Gtk|Gdk|GSK|Adwaita|Libadwaita|AT-SPI|accessibility).*(warning|critical|error)"
    r"|GLib-GObject-CRITICAL|gtk_[a-z0-9_]+.*assertion|gdk_[a-z0-9_]+.*assertion",
    re.IGNORECASE,
)
KNOWN_HEADLESS_WARNING_RE = re.compile(
    r"AT-SPI: Could not obtain desktop path or name"
    r"|atk-bridge: GetRegisteredEvents returned message with unknown signature"
    r"|atk-bridge: get_device_events_reply: unknown signature"
    r"|Unable to register the application: .*org\.a11y\.atspi\.Registry",
    re.IGNORECASE,
)


def scan_warnings(case_dir: Path) -> dict[str, Any]:
    logs = [
        case_dir / "session.log",
        case_dir / "mutter-child.log",
        case_dir / "lushtext.stdout",
        case_dir / "lushtext.stderr",
    ]
    matches = []
    for path in logs:
        if not path.is_file():
            continue
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            if "Error reading events from display: Broken pipe" in line:
                continue
            if KNOWN_HEADLESS_WARNING_RE.search(line):
                continue
            if WARNING_RE.search(line):
                matches.append({"artifact": path.name, "line": bounded(line)})
    report = {"status": "failed" if matches else "passed", "matches": matches}
    (case_dir / "warning-scan.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return report


def classify_failure(exc: Exception) -> str:
    text = str(exc)
    if "WaitForReady" in text or "Timed out" in text:
        return "predicate-timeout"
    if "visual comparison failed" in text:
        return "visual-comparison-failed"
    if "surface" in text or "anchor" in text or "sidebar" in text:
        return "state-mismatch"
    return "workflow-failure"


def write_case_manifest(
    case_dir: Path,
    case: dict[str, Any],
    status: str,
    failure_status: str | None,
    failure_reason: str | None,
    *,
    warning_status: dict[str, Any] | None = None,
) -> None:
    comparison_report = case_dir / "comparisons" / "comparison-report.json"
    manifest = {
        "schema_version": 1,
        "scenario_id": case["case_id"],
        "scenario_type": case["manifest"]["scenario_type"],
        "status": status,
        "failure_status": failure_status,
        "failure_reason": failure_reason,
        "skip_reason": None,
        "started_at": now_iso(),
        "finished_at": now_iso(),
        "source_manifest": case["manifest"].get("_manifest_path"),
        "same_session": True,
        "case": {
            "size": case["size"],
            "color_scheme": case.get("color_scheme"),
            "word_wrap": case.get("word_wrap"),
            "direction": case.get("direction"),
        },
        "screenshots": [
            {"name": "before", "artifact": "before.png"},
            {"name": "after", "artifact": "after.png"},
        ],
        "geometry_snapshots": [
            {"name": "before", "artifact": "before-geometry-snapshot.json"},
            {"name": "after", "artifact": "after-geometry-snapshot.json"},
        ],
        "protected_regions": case["manifest"]["protected_regions"],
        "allowed_changing_regions": case["manifest"].get("allowed_changing_regions", []),
        "comparison_report": "comparisons/comparison-report.json" if comparison_report.exists() else None,
        "warnings": warning_status or {"status": "not-run"},
    }
    (case_dir / "scenario-manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    summary = {
        "status": status,
        "failure_status": failure_status,
        "scenario_id": case["case_id"],
        "comparison_report": manifest["comparison_report"],
    }
    (case_dir / "summary.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def write_skip_summary(artifact_dir: Path, reason: str) -> None:
    payload = {
        "schema_version": 1,
        "status": "skipped",
        "skip_reason": reason,
        "case_count": 0,
        "cases": [],
    }
    (artifact_dir / "summary.json").write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def main() -> int:
    args = parse_args()
    if args.internal_run:
        return internal_run(args)
    if args.mutter_child:
        return mutter_child(args)
    return outer_run(args)


if __name__ == "__main__":
    raise SystemExit(main())
