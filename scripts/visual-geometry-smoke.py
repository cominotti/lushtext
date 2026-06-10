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

from visual_geometry_png import (
    Rect,
    clamp_rect,
    compare_crops,
    crop_rows,
    detect_pixel_anchor,
    read_png,
    write_png,
)


REPO_ROOT = Path(__file__).resolve().parents[1]
CAPTURE_HELPER = (
    REPO_ROOT / ".agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py"
)
VISUAL_PROOF_POLICY_HELPER = REPO_ROOT / "scripts/check-visual-proof-policy.py"
SYSTEM_PYTHON = Path("/usr/bin/python3")
APP_ID = "dev.cominotti.lushtext"
WINDOW_OBJECT_PATH = "/dev/cominotti/lushtext/window/1"
ARTIFACT_TEXT_LIMIT = 1200
FINAL_GEOMETRY_SAMPLE_COUNT = 3
FINAL_GEOMETRY_SAMPLE_INTERVAL_SECONDS = 0.05
FINAL_GEOMETRY_TIMEOUT_MS = 5000
FINAL_GEOMETRY_SURFACES = (
    "workspace-sidebar",
    "workspace-sidebar-transition",
    "editor-viewport",
    "source-view",
    "minimap-shell",
    "minimap-source-map",
    "minimap-native-viewport",
    "minimap-marker-strip",
)
APP_PIXEL_ANCHOR_ALIASES = {
    "minimap-native-viewport-top-edge": "minimap-viewport-top-edge",
}
DEFAULT_ANIMATION_SAMPLE_COUNT = 8
DEFAULT_ANIMATION_SAMPLE_INTERVAL_SECONDS = 0.016
DEFAULT_ANIMATION_STREAM_FRAME_COUNT = 48
DEFAULT_ANIMATION_MAX_SAMPLE_SKEW_MS = 80
DEFAULT_ANIMATION_INVARIANT_ID = "native-minimap-animation-highlight-anchors"


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


def reset_artifact_dir(artifact_dir: Path) -> None:
    resolved = artifact_dir.resolve()
    forbidden = {Path("/"), Path.home().resolve(), REPO_ROOT.resolve(), REPO_ROOT.parent.resolve()}
    if resolved in forbidden:
        raise RuntimeError(f"refusing to reset unsafe visual geometry artifact dir: {resolved}")
    if artifact_dir.exists():
        shutil.rmtree(artifact_dir)
    artifact_dir.mkdir(parents=True, exist_ok=True)


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
    if manifest.get("pixel_anchors") and not manifest.get("invariant_id"):
        raise RuntimeError(f"{path} declares pixel_anchors without invariant_id")
    animation = manifest.get("animation_sampling")
    if animation is not None:
        if not isinstance(animation, dict):
            raise RuntimeError(f"{path} animation_sampling must be an object")
        if animation.get("enabled", True):
            if not manifest.get("pixel_anchors"):
                raise RuntimeError(f"{path} animation_sampling requires pixel_anchors")
            if not animation.get("invariant_id"):
                raise RuntimeError(f"{path} animation_sampling requires invariant_id")
            sample_count = int(animation.get("sample_count", DEFAULT_ANIMATION_SAMPLE_COUNT))
            if sample_count <= 0:
                raise RuntimeError(f"{path} animation_sampling sample_count must be positive")


def expand_cases(manifest: dict[str, Any]) -> list[dict[str, Any]]:
    matrix = manifest["matrix"]
    cases: list[dict[str, Any]] = []
    if manifest["scenario_type"] == "minimap-sidebar":
        viewport_positions = matrix.get("viewport_positions", ["top"])
        fixture_kinds = matrix.get("fixture_kinds", ["plain-lines"])
        for size in matrix["sizes"]:
            for color_scheme in matrix["color_schemes"]:
                for word_wrap in matrix["word_wrap"]:
                    for direction in matrix["directions"]:
                        for viewport_position in viewport_positions:
                            for fixture_kind in fixture_kinds:
                                suffix = ""
                                if viewport_position != "top":
                                    suffix += f"--{viewport_position}"
                                if fixture_kind != "plain-lines":
                                    suffix += f"--{fixture_kind}"
                                case_id = (
                                    f"{manifest['scenario_id']}--{size['id']}--{color_scheme}"
                                    f"--wrap-{str(word_wrap).lower()}--{direction}{suffix}"
                                )
                                if matrix_case_excluded(
                                    manifest,
                                    size,
                                    color_scheme,
                                    bool(word_wrap),
                                    direction,
                                    viewport_position,
                                    fixture_kind,
                                ):
                                    continue
                                cases.append(
                                    {
                                        "case_id": case_id,
                                        "manifest": manifest,
                                        "size": size,
                                        "color_scheme": color_scheme,
                                        "word_wrap": bool(word_wrap),
                                        "direction": direction,
                                        "viewport_position": viewport_position,
                                        "fixture_kind": fixture_kind,
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


def matrix_case_excluded(
    manifest: dict[str, Any],
    size: dict[str, Any],
    color_scheme: str,
    word_wrap: bool,
    direction: str,
    viewport_position: str,
    fixture_kind: str,
) -> bool:
    """Return whether a generated visual-geometry matrix case is intentionally skipped."""

    for rule in manifest.get("matrix", {}).get("exclude", []):
        if "size" in rule and rule["size"] != size.get("id"):
            continue
        if "color_scheme" in rule and rule["color_scheme"] != color_scheme:
            continue
        if "word_wrap" in rule and bool(rule["word_wrap"]) != word_wrap:
            continue
        if "direction" in rule and rule["direction"] != direction:
            continue
        if "viewport_position" in rule and rule["viewport_position"] != viewport_position:
            continue
        if "fixture_kind" in rule and rule["fixture_kind"] != fixture_kind:
            continue
        return True
    return False


def write_case_json(case: dict[str, Any], path: Path, binary: Path, artifact_dir: Path) -> None:
    payload = {**case, "binary": str(binary.resolve()), "artifact_dir": str(artifact_dir.resolve())}
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def create_fixture(case: dict[str, Any], artifact_dir: Path) -> Path:
    fixture_dir = artifact_dir / "fixtures"
    fixture_dir.mkdir(parents=True, exist_ok=True)
    fixture_kind = case.get("fixture_kind", "plain-lines")
    suffix = ".md" if fixture_kind == "markdown-dense" else ".txt"
    path = fixture_dir / f"{case['case_id']}{suffix}"
    if case["manifest"]["scenario_type"] == "minimap-sidebar":
        path.write_text(minimap_fixture_text(case), encoding="utf-8")
    else:
        path.write_text("Command palette visual geometry fixture\n", encoding="utf-8")
    return path


def minimap_fixture_text(case: dict[str, Any]) -> str:
    if case.get("fixture_kind") == "markdown-dense":
        return dense_markdown_minimap_fixture()

    long_tail = "x" * 150
    lines = []
    for index in range(280):
        if case["word_wrap"]:
            lines.append(f"line {index:04d} {long_tail}")
        else:
            lines.append(f"line {index:04d}")
    return "\n".join(lines) + "\n"


def dense_markdown_minimap_fixture() -> str:
    lines = [
        "# Volume3 Synology Residual Defrag Evidence",
        "",
        "Date: 2026-06-08 23:59:31 -0300",
        "",
        "Scope: targeted cleanup of the 11 residual files in `/volume3/_pandora` that were intentionally left for Synology/RD inspection. No broad volume3 defrag was run.",
        "",
        "## Result",
        "",
        "- Before 256K scan: `BEFORE threshold=256K mapped_candidates=11 logical_GiB=1.630783 slack_sum_GiB=0.003716 slack_sum_MiB=3.805`",
        "- After 256K scan: `AFTER threshold=256K mapped_candidates=0 logical_GiB=0.000000 slack_sum_GiB=0.000000 slack_sum_MiB=0.000`",
        "- Defrag summary: `2026-06-08T23:55:25-0300 DONE action=defrag-fix run=20260608-235510 rewritten=11 cleaned_backups=0 failures=0 log=/volume1/_hermes/issue-logs/20260604-172942--03/volume3-synology-residual-20260608-235359/10-defrag-fix/rewrite-20260608-235510.log`",
        "",
    ]
    for index in range(60):
        lines.append(
            f"- Detail {index:02d}: `/volume3/_pandora/path-{index:02d}` "
            f"`logical={index + 1:04d}` `slack={index % 7}` "
            "abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ"
        )
    return "\n".join(lines) + "\n"


def outer_run(args: argparse.Namespace) -> int:
    reset_artifact_dir(args.artifact_dir)
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
        "verified_invariant_ids": sorted(
            {
                item["invariant_id"]
                for item in results
                if item["status"] == "passed" and item.get("invariant_id")
            }
        ),
        "pixel_verified_invariant_ids": sorted(
            {
                invariant_id
                for item in results
                if item["status"] == "passed"
                for invariant_id in item.get("pixel_verified_invariant_ids", [])
            }
        ),
        "animation_verified_invariant_ids": sorted(
            {
                invariant_id
                for item in results
                if item["status"] == "passed"
                for invariant_id in item.get("animation_verified_invariant_ids", [])
            }
        ),
        "pixel_anchor_assertion_count": sum(
            int(item.get("pixel_anchor_assertion_count") or 0) for item in results
        ),
        "animation_frame_sample_count": sum(
            int(item.get("animation_frame_sample_count") or 0) for item in results
        ),
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
        invariant_id = payload.get("invariant_id")
        pixel_anchor_assertion_count = payload.get("pixel_anchor_assertion_count", 0)
        pixel_verified_invariant_ids = payload.get("pixel_verified_invariant_ids", [])
        final_geometry = payload.get("final_geometry")
        pixel_anchor_evidence = payload.get("pixel_anchor_evidence", [])
        app_vs_rendered_disagreements = payload.get("app_vs_rendered_disagreements", [])
        rendered_anchor_stability = payload.get("rendered_anchor_stability", [])
        animation_verified_invariant_ids = payload.get("animation_verified_invariant_ids", [])
        animation_frame_evidence = payload.get("animation_frame_evidence")
        animation_frame_sample_count = int(payload.get("animation_frame_sample_count") or 0)
    else:
        invariant_id = case["manifest"].get("invariant_id")
        pixel_anchor_assertion_count = 0
        pixel_verified_invariant_ids = []
        final_geometry = None
        pixel_anchor_evidence = []
        app_vs_rendered_disagreements = []
        rendered_anchor_stability = []
        animation_verified_invariant_ids = []
        animation_frame_evidence = None
        animation_frame_sample_count = 0
    return {
        "case_id": case["case_id"],
        "status": status,
        "failure_status": failure_status if status == "failed" else None,
        "invariant_id": invariant_id,
        "pixel_anchor_assertion_count": pixel_anchor_assertion_count,
        "pixel_verified_invariant_ids": pixel_verified_invariant_ids,
        "final_geometry": final_geometry,
        "pixel_anchor_evidence": pixel_anchor_evidence,
        "app_vs_rendered_disagreements": app_vs_rendered_disagreements,
        "rendered_anchor_stability": rendered_anchor_stability,
        "animation_verified_invariant_ids": animation_verified_invariant_ids,
        "animation_frame_evidence": animation_frame_evidence,
        "animation_frame_sample_count": animation_frame_sample_count,
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
        prepare_case_state(bus, case, case_dir)
        wait_for_case_final_geometry(bus, case_dir, case, "before")
        before = capture_step(bus, case_dir, case, "before")
        assert_capture_final_geometry(case, before["snapshot"], "before")
        animation_report = run_case_action_with_optional_animation_sampling(
            bus,
            case_dir,
            case,
            before,
        )
        mutter.wait_for_ready(bus, case_dir, "visual-geometry-settled", 5000)
        wait_for_case_final_geometry(bus, case_dir, case, "after")
        after = capture_step(bus, case_dir, case, "after")
        assert_capture_final_geometry(case, after["snapshot"], "after")
        compare_case(case_dir, case, before, after)
        if animation_report is not None and animation_report["status"] != "passed":
            raise RuntimeError(
                "animation-frame pixel anchor assertion failed: "
                f"{animation_report.get('failure_reason') or animation_report}"
            )
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


def capture_step(bus, case_dir: Path, case: dict[str, Any], step: str) -> dict[str, Any]:
    snapshot_path = case_dir / f"{step}-geometry-snapshot.json"
    screenshot_path = case_dir / f"{step}.png"
    warmup_path = case_dir / f"{step}-warmup.png"
    time.sleep(0.15)
    mutter.capture_monitor(bus, warmup_path)
    time.sleep(0.05)
    snapshot = mutter.snapshot_json(bus)
    snapshot_path.write_text(json.dumps(snapshot, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    mutter.capture_monitor(bus, screenshot_path)
    stability = evaluate_final_frame_pixel_stability(case, snapshot, warmup_path, screenshot_path)
    (case_dir / f"{step}-rendered-anchor-stability.json").write_text(
        json.dumps(stability, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    if stability["status"] != "passed":
        raise RuntimeError(f"final rendered pixel anchors did not stabilize for {step}: {stability}")
    if not case["manifest"].get("pixel_anchors"):
        warmup_path.unlink(missing_ok=True)
    return {
        "step": step,
        "snapshot": snapshot,
        "snapshot_path": snapshot_path,
        "screenshot": screenshot_path,
        "warmup_screenshot": warmup_path if warmup_path.is_file() else None,
        "rendered_anchor_stability": stability,
    }


def evaluate_final_frame_pixel_stability(
    case: dict[str, Any],
    snapshot: dict[str, Any],
    warmup_path: Path,
    screenshot_path: Path,
) -> dict[str, Any]:
    """Assert declared pixel anchors are stable across the final render frames."""

    anchor_specs = case["manifest"].get("pixel_anchors", [])
    if not anchor_specs:
        return {"status": "passed", "anchors": []}

    warmup_image = read_png(warmup_path)
    final_image = read_png(screenshot_path)
    status = "passed"
    reports = []
    for spec in anchor_specs:
        name = spec["name"]
        detector = spec["detector"]
        min_pixels = int(spec.get("min_pixels", 1))
        rect = clamp_rect(final_image, rect_for_anchor_search(snapshot, spec))
        warmup_detection = detect_pixel_anchor(warmup_image, name, rect, detector, min_pixels)
        final_detection = detect_pixel_anchor(final_image, name, rect, detector, min_pixels)
        row_delta = (
            abs(final_detection.row_y - warmup_detection.row_y)
            if final_detection.row_y is not None and warmup_detection.row_y is not None
            else None
        )
        max_delta = int(spec.get("max_final_frame_y_delta", 0))
        report = {
            "name": name,
            "detector": detector,
            "rect": rect.to_dict(),
            "warmup": warmup_detection.to_dict(),
            "final": final_detection.to_dict(),
            "row_delta": row_delta,
            "max_row_delta": max_delta,
            "status": "passed",
        }
        if (
            warmup_detection.status != "passed"
            or final_detection.status != "passed"
            or row_delta is None
            or row_delta > max_delta
        ):
            report["status"] = "failed"
            status = "failed"
        reports.append(report)

    return {"status": status, "anchors": reports}


def run_case_action(bus, case: dict[str, Any]) -> None:
    scenario_type = case["manifest"]["scenario_type"]
    if scenario_type == "minimap-sidebar":
        mutter.activate_window_action(bus, "toggle-sidebar")
    elif scenario_type == "command-palette-overlay":
        mutter.activate_window_action(bus, "toggle-command-palette")
    else:
        raise RuntimeError(f"unsupported scenario type: {scenario_type}")


def run_case_action_with_optional_animation_sampling(
    bus,
    case_dir: Path,
    case: dict[str, Any],
    before: dict[str, Any],
) -> dict[str, Any] | None:
    animation = case["manifest"].get("animation_sampling")
    if not isinstance(animation, dict) or not animation.get("enabled", True):
        run_case_action(bus, case)
        return None
    return capture_animation_frames(bus, case_dir, case, before, animation)


def capture_animation_frames(
    bus,
    case_dir: Path,
    case: dict[str, Any],
    before: dict[str, Any],
    config: dict[str, Any],
) -> dict[str, Any]:
    if str(config.get("capture_mode", "stream")) == "stream":
        return capture_animation_stream_frames(bus, case_dir, case, before, config)
    return capture_animation_screenshot_frames(bus, case_dir, case, before, config)


def capture_animation_stream_frames(
    bus,
    case_dir: Path,
    case: dict[str, Any],
    before: dict[str, Any],
    config: dict[str, Any],
) -> dict[str, Any]:
    animation_dir = case_dir / "animation"
    frames_dir = animation_dir / "frames"
    crops_dir = animation_dir / "crops"
    frames_dir.mkdir(parents=True, exist_ok=True)
    crops_dir.mkdir(parents=True, exist_ok=True)

    anchor_specs = animation_anchor_specs(case, config)
    baseline = detect_animation_baseline(case, before, anchor_specs, crops_dir)
    frame_count = int(config.get("stream_frame_count", DEFAULT_ANIMATION_STREAM_FRAME_COUNT))
    snapshot_interval_seconds = float(
        config.get("sample_interval_ms", DEFAULT_ANIMATION_SAMPLE_INTERVAL_SECONDS * 1000)
    ) / 1000.0
    stream_timeout_seconds = float(config.get("stream_timeout_ms", 1400)) / 1000.0
    max_sample_skew_ms = int(config.get("max_sample_skew_ms", DEFAULT_ANIMATION_MAX_SAMPLE_SKEW_MS))
    max_screen_y_delta = int(config.get("max_screen_y_delta", 0))
    invariant_id = str(config.get("invariant_id") or DEFAULT_ANIMATION_INVARIANT_ID)

    wall_started = time.time()
    recording = start_monitor_frame_recording(bus, animation_dir, frame_count)
    samples: list[dict[str, Any]] = []
    frames: list[dict[str, Any]] = []
    failures: list[dict[str, Any]] = []
    max_row_drift = 0
    max_sample_skew_observed_ms: int | None = None
    status = "passed"
    started = time.monotonic()
    try:
        # Let the PipeWire client attach before the action so the burst includes
        # the first animation frames instead of only the settled endpoint.
        time.sleep(0.03)
        action_started_ms = int(round((time.monotonic() - started) * 1000))
        run_case_action(bus, case)
        deadline = started + stream_timeout_seconds
        while recording["process"].poll() is None and time.monotonic() < deadline:
            elapsed_ms = int(round((time.monotonic() - started) * 1000))
            snapshot = mutter.snapshot_json(bus)
            samples.append(animation_geometry_sample(snapshot, elapsed_ms))
            time.sleep(snapshot_interval_seconds)
        if recording["process"].poll() is None:
            mutter.terminate_process(recording["process"])
        else:
            recording["process"].wait(timeout=2)
    finally:
        stop_monitor_frame_recording(bus, recording)
    action_started_ms = locals().get("action_started_ms", 0)

    frame_paths = sorted(frames_dir.glob("stream-frame-*.png"))
    if not frame_paths:
        status = "failed"
        failure_reason = "stream animation capture produced no PNG frames"
    else:
        failure_reason = None
        for frame_index, frame_path in enumerate(frame_paths):
            frame_elapsed_ms = animation_frame_elapsed_ms(
                frame_path,
                wall_started,
                frame_index,
                snapshot_interval_seconds,
            )
            sample, sample_skew_ms = animation_sample_for_frame(
                samples,
                frame_elapsed_ms,
                max_sample_skew_ms,
            )
            if sample_skew_ms is not None:
                max_sample_skew_observed_ms = (
                    sample_skew_ms
                    if max_sample_skew_observed_ms is None
                    else max(max_sample_skew_observed_ms, sample_skew_ms)
                )
            snapshot = sample["snapshot"] if sample else before["snapshot"]
            snapshot_path = frames_dir / f"frame-{frame_index:03d}-geometry-snapshot.json"
            snapshot_path.write_text(
                json.dumps(snapshot, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            if sample is None:
                frame_report = stale_animation_frame_report(
                    frame_index,
                    frame_elapsed_ms,
                    frame_path,
                    snapshot_path,
                    max_sample_skew_ms,
                    sample_skew_ms,
                )
            else:
                frame_report = evaluate_animation_frame(
                    case,
                    frame_index,
                    frame_elapsed_ms,
                    snapshot,
                    read_png(frame_path),
                    frame_path,
                    snapshot_path,
                    anchor_specs,
                    baseline,
                    crops_dir,
                    max_screen_y_delta,
                    mapped_sample_elapsed_ms=int(sample.get("elapsed_ms", 0)),
                    sample_skew_ms=sample_skew_ms,
                    sidebar_phase=str(sample.get("sidebar_phase", "unknown")),
                )
            frames.append(frame_report)
            max_row_drift = max(max_row_drift, int(frame_report.get("max_row_drift") or 0))
            if frame_report["status"] != "passed":
                status = "failed"
                failures.append(frame_report)

    intermediate_count = sum(1 for sample in samples if sample.get("sidebar_phase") == "intermediate")
    mapped_intermediate_frame_count = sum(
        1 for frame in frames if frame.get("sidebar_phase") == "intermediate"
    )
    if config.get("require_intermediate_geometry", True) and intermediate_count <= 0:
        status = "failed"
        failure_reason = "animation sampling did not observe intermediate sidebar geometry"
    elif config.get("require_intermediate_geometry", True) and mapped_intermediate_frame_count <= 0:
        status = "failed"
        failure_reason = "animation stream did not capture a PNG frame mapped to intermediate sidebar geometry"

    report = {
        "schema_version": 1,
        "status": status,
        "capture_mode": "stream",
        "invariant_id": invariant_id,
        "stream_frame_count": frame_count,
        "stream_timeout_ms": int(round(stream_timeout_seconds * 1000)),
        "sample_interval_ms": int(round(snapshot_interval_seconds * 1000)),
        "max_sample_skew_ms": max_sample_skew_ms,
        "max_sample_skew_observed_ms": max_sample_skew_observed_ms,
        "action_started_ms": action_started_ms,
        "sampled_frame_count": len(frames),
        "geometry_sample_count": len(samples),
        "intermediate_geometry_sample_count": intermediate_count,
        "mapped_intermediate_frame_count": mapped_intermediate_frame_count,
        "phase_sequence": animation_phase_sequence(samples),
        "max_screen_y_delta": max_screen_y_delta,
        "max_row_drift": max_row_drift,
        "baseline": baseline_report_rows(baseline),
        "geometry_samples": [bounded_animation_geometry_sample(sample) for sample in samples],
        "frames": frames,
        "failures": summarize_animation_failures(failures),
        "failure_reason": failure_reason or animation_failure_reason(failures),
    }
    (animation_dir / "animation-report.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return report


def capture_animation_screenshot_frames(
    bus,
    case_dir: Path,
    case: dict[str, Any],
    before: dict[str, Any],
    config: dict[str, Any],
) -> dict[str, Any]:
    animation_dir = case_dir / "animation"
    frames_dir = animation_dir / "frames"
    crops_dir = animation_dir / "crops"
    frames_dir.mkdir(parents=True, exist_ok=True)
    crops_dir.mkdir(parents=True, exist_ok=True)

    anchor_specs = animation_anchor_specs(case, config)
    baseline = detect_animation_baseline(case, before, anchor_specs, crops_dir)
    sample_count = int(config.get("sample_count", DEFAULT_ANIMATION_SAMPLE_COUNT))
    interval_seconds = float(
        config.get("sample_interval_ms", DEFAULT_ANIMATION_SAMPLE_INTERVAL_SECONDS * 1000)
    ) / 1000.0
    max_screen_y_delta = int(config.get("max_screen_y_delta", 0))
    invariant_id = str(config.get("invariant_id") or DEFAULT_ANIMATION_INVARIANT_ID)

    frames: list[dict[str, Any]] = []
    failures: list[dict[str, Any]] = []
    max_row_drift = 0
    status = "passed"

    run_case_action(bus, case)
    started = time.monotonic()
    for frame_index in range(sample_count):
        if frame_index > 0 and interval_seconds > 0:
            time.sleep(interval_seconds)
        elapsed_ms = int(round((time.monotonic() - started) * 1000))
        snapshot = mutter.snapshot_json(bus)
        snapshot_path = frames_dir / f"frame-{frame_index:03d}-geometry-snapshot.json"
        snapshot_path.write_text(
            json.dumps(snapshot, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        screenshot_path = frames_dir / f"frame-{frame_index:03d}.png"
        mutter.capture_monitor(bus, screenshot_path)
        image = read_png(screenshot_path)
        frame_report = evaluate_animation_frame(
            case,
            frame_index,
            elapsed_ms,
            snapshot,
            image,
            screenshot_path,
            snapshot_path,
            anchor_specs,
            baseline,
            crops_dir,
            max_screen_y_delta,
        )
        frames.append(frame_report)
        max_row_drift = max(max_row_drift, int(frame_report.get("max_row_drift") or 0))
        if frame_report["status"] != "passed":
            status = "failed"
            failures.append(frame_report)

    report = {
        "schema_version": 1,
        "status": status,
        "invariant_id": invariant_id,
        "sample_count": sample_count,
        "sample_interval_ms": int(round(interval_seconds * 1000)),
        "sampled_frame_count": len(frames),
        "max_screen_y_delta": max_screen_y_delta,
        "max_row_drift": max_row_drift,
        "baseline": baseline_report_rows(baseline),
        "frames": frames,
        "failures": summarize_animation_failures(failures),
        "failure_reason": animation_failure_reason(failures),
    }
    (animation_dir / "animation-report.json").write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return report


def start_monitor_frame_recording(
    bus,
    animation_dir: Path,
    frame_count: int,
) -> dict[str, Any]:
    from gi.repository import Gio, GLib

    session_path = mutter.bus_call(
        bus,
        "org.gnome.Mutter.ScreenCast",
        "/org/gnome/Mutter/ScreenCast",
        "org.gnome.Mutter.ScreenCast",
        "CreateSession",
        GLib.Variant("(a{sv})", ({},)),
        "(o)",
    ).unpack()[0]
    stream_path = mutter.bus_call(
        bus,
        "org.gnome.Mutter.ScreenCast",
        session_path,
        "org.gnome.Mutter.ScreenCast.Session",
        "RecordMonitor",
        GLib.Variant(
            "(sa{sv})",
            ("Meta-0", {"is-recording": GLib.Variant("b", True)}),
        ),
        "(o)",
    ).unpack()[0]

    node_id: dict[str, int | None] = {"value": None}
    loop = GLib.MainLoop()

    def on_signal(_conn, _sender, _path, _iface, _signal, params):
        node_id["value"] = params.unpack()[0]
        loop.quit()

    subscription = bus.signal_subscribe(
        "org.gnome.Mutter.ScreenCast",
        "org.gnome.Mutter.ScreenCast.Stream",
        "PipeWireStreamAdded",
        stream_path,
        None,
        Gio.DBusSignalFlags.NONE,
        on_signal,
    )
    GLib.timeout_add_seconds(5, lambda: (loop.quit(), False)[1])
    mutter.bus_call(
        bus,
        "org.gnome.Mutter.ScreenCast",
        session_path,
        "org.gnome.Mutter.ScreenCast.Session",
        "Start",
    )
    loop.run()
    bus.signal_unsubscribe(subscription)
    if node_id["value"] is None:
        raise RuntimeError("Mutter did not emit PipeWireStreamAdded for animation capture.")

    frames_dir = animation_dir / "frames"
    frames_dir.mkdir(parents=True, exist_ok=True)
    pattern = frames_dir / "stream-frame-%03d.png"
    process = subprocess.Popen(
        [
            "gst-launch-1.0",
            "-q",
            "pipewiresrc",
            f"path={node_id['value']}",
            f"num-buffers={frame_count}",
            "!",
            "videoconvert",
            "!",
            "pngenc",
            "!",
            "multifilesink",
            f"location={pattern}",
        ],
        stdout=(animation_dir / "stream-gst.log").open("wb"),
        stderr=subprocess.STDOUT,
    )
    return {
        "session_path": session_path,
        "stream_path": stream_path,
        "node_id": node_id["value"],
        "process": process,
        "frame_pattern": str(pattern),
    }


def stop_monitor_frame_recording(bus, recording: dict[str, Any]) -> None:
    process = recording.get("process")
    if isinstance(process, subprocess.Popen) and process.poll() is None:
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            mutter.terminate_process(process)
    session_path = recording.get("session_path")
    if session_path:
        try:
            mutter.bus_call(
                bus,
                "org.gnome.Mutter.ScreenCast",
                session_path,
                "org.gnome.Mutter.ScreenCast.Session",
                "Stop",
            )
        except Exception:
            pass


def animation_geometry_sample(snapshot: dict[str, Any], elapsed_ms: int) -> dict[str, Any]:
    sidebar_phase = "unknown"
    try:
        try:
            sidebar = rect_for(snapshot, "workspace-sidebar-transition")
        except RuntimeError:
            sidebar = rect_for(snapshot, "workspace-sidebar")
        if sidebar.x == 0:
            sidebar_phase = "shown"
        elif sidebar.x == -sidebar.width:
            sidebar_phase = "hidden"
        elif -sidebar.width < sidebar.x < 0:
            sidebar_phase = "intermediate"
    except RuntimeError:
        sidebar_phase = "unavailable"
    return {
        "elapsed_ms": elapsed_ms,
        "sidebar_phase": sidebar_phase,
        "snapshot": snapshot,
    }


def animation_frame_elapsed_ms(
    frame_path: Path,
    wall_started: float,
    frame_index: int,
    fallback_interval_seconds: float,
) -> int:
    try:
        elapsed = frame_path.stat().st_mtime - wall_started
    except OSError:
        elapsed = frame_index * fallback_interval_seconds
    if elapsed < 0:
        elapsed = frame_index * fallback_interval_seconds
    return int(round(elapsed * 1000))


def animation_sample_for_frame(
    samples: list[dict[str, Any]],
    frame_elapsed_ms: int,
    max_sample_skew_ms: int,
) -> tuple[dict[str, Any] | None, int | None]:
    if not samples:
        return None, None
    nearest = min(
        samples,
        key=lambda sample: abs(int(sample.get("elapsed_ms", 0)) - frame_elapsed_ms),
    )
    skew = abs(int(nearest.get("elapsed_ms", 0)) - frame_elapsed_ms)
    if skew > max_sample_skew_ms:
        return None, skew
    return nearest, skew


def animation_phase_sequence(samples: list[dict[str, Any]]) -> list[str]:
    phases: list[str] = []
    for sample in samples:
        phase = str(sample.get("sidebar_phase", "unknown"))
        if not phases or phases[-1] != phase:
            phases.append(phase)
    return phases


def bounded_animation_geometry_sample(sample: dict[str, Any]) -> dict[str, Any]:
    snapshot = sample["snapshot"]
    return {
        "elapsed_ms": sample.get("elapsed_ms"),
        "sidebar_phase": sample.get("sidebar_phase"),
        "surfaces": selected_surface_rows(snapshot, FINAL_GEOMETRY_SURFACES),
        "native_minimap": visual_geometry(snapshot).get("native_minimap"),
        "scroll_anchors": visual_geometry(snapshot).get("scroll_anchors", []),
    }


def animation_anchor_specs(case: dict[str, Any], config: dict[str, Any]) -> list[dict[str, Any]]:
    requested = config.get("required_anchors")
    anchor_specs = case["manifest"].get("pixel_anchors", [])
    if not requested:
        return anchor_specs
    requested_names = {str(name) for name in requested}
    selected = [spec for spec in anchor_specs if str(spec.get("name")) in requested_names]
    missing = sorted(requested_names - {str(spec.get("name")) for spec in selected})
    if missing:
        raise RuntimeError(f"animation_sampling references unknown pixel anchors: {', '.join(missing)}")
    return selected


def detect_animation_baseline(
    case: dict[str, Any],
    before: dict[str, Any],
    anchor_specs: list[dict[str, Any]],
    crops_dir: Path,
) -> dict[str, Any]:
    image = read_png(before["screenshot"])
    rows: dict[str, Any] = {}
    for spec in anchor_specs:
        name = str(spec["name"])
        detector = str(spec["detector"])
        min_pixels = int(spec.get("min_pixels", 1))
        rect = clamp_rect(image, rect_for_anchor_search(before["snapshot"], spec))
        detection = detect_pixel_anchor(image, name, rect, detector, min_pixels)
        crop = write_anchor_crop(crops_dir, name, "baseline", image, rect)
        rows[name] = {
            "spec": spec,
            "detection": detection,
            "crop": crop.relative_to(crops_dir.parent).as_posix(),
            "status": detection.status,
            "row_y": detection.row_y,
        }
    return {
        "screenshot": Path(before["screenshot"]).name,
        "snapshot": Path(before["snapshot_path"]).name,
        "_snapshot_path": Path(before["snapshot_path"]),
        "_snapshot": before["snapshot"],
        "anchors": rows,
        "relationships": baseline_relationship_rows(case, rows),
    }


def evaluate_animation_frame(
    case: dict[str, Any],
    frame_index: int,
    elapsed_ms: int,
    snapshot: dict[str, Any],
    image,
    screenshot_path: Path,
    snapshot_path: Path,
    anchor_specs: list[dict[str, Any]],
    baseline: dict[str, Any],
    crops_dir: Path,
    max_screen_y_delta: int,
    mapped_sample_elapsed_ms: int | None = None,
    sample_skew_ms: int | None = None,
    sidebar_phase: str | None = None,
) -> dict[str, Any]:
    status = "passed"
    anchors = []
    app_vs_rendered_disagreements = []
    max_row_drift = 0
    for spec in anchor_specs:
        name = str(spec["name"])
        detector = str(spec["detector"])
        min_pixels = int(spec.get("min_pixels", 1))
        rect = clamp_rect(image, rect_for_anchor_search(snapshot, spec))
        detection = detect_pixel_anchor(image, name, rect, detector, min_pixels)
        crop = write_anchor_crop(crops_dir, name, f"frame-{frame_index:03d}", image, rect)
        baseline_row = baseline["anchors"].get(name, {}).get("row_y")
        row_delta = (
            abs(int(detection.row_y) - int(baseline_row))
            if detection.row_y is not None and baseline_row is not None
            else None
        )
        if row_delta is not None:
            max_row_drift = max(max_row_drift, row_delta)
        app_geometry = app_pixel_anchor_geometry(before_snapshot_from_baseline(baseline), snapshot, name)
        row = {
            "name": name,
            "detector": detector,
            "status": "passed",
            "baseline_row_y": baseline_row,
            "frame_row_y": detection.row_y,
            "row_delta_from_baseline": row_delta,
            "max_screen_y_delta": max_screen_y_delta,
            "detection": detection.to_dict(),
            "crop": crop.relative_to(crops_dir.parent).as_posix(),
            "app_geometry": app_geometry,
        }
        if detection.status != "passed" or row_delta is None or row_delta > max_screen_y_delta:
            row["status"] = "failed"
            status = "failed"
            if app_geometry and app_geometry.get("screen_y_delta") is not None:
                app_delta = int(app_geometry["screen_y_delta"])
                if app_delta <= max_screen_y_delta and row_delta is not None:
                    diagnostic = {
                        "name": name,
                        "status": "animation-app-vs-rendered-anchor-disagreement",
                        "app_screen_y_delta": app_delta,
                        "rendered_screen_y_delta": row_delta,
                        "max_screen_y_delta": max_screen_y_delta,
                    }
                    row.setdefault("diagnostics", []).append(diagnostic)
                    app_vs_rendered_disagreements.append(diagnostic)
        anchors.append(row)

    relationships = evaluate_animation_relationships(case, baseline, anchors)
    if any(row["status"] != "passed" for row in relationships):
        status = "failed"

    return {
        "frame_index": frame_index,
        "elapsed_ms": elapsed_ms,
        "mapped_sample_elapsed_ms": mapped_sample_elapsed_ms,
        "sample_skew_ms": sample_skew_ms,
        "sidebar_phase": sidebar_phase,
        "status": status,
        "screenshot": screenshot_path.relative_to(screenshot_path.parents[1]).as_posix(),
        "snapshot": snapshot_path.relative_to(snapshot_path.parents[1]).as_posix(),
        "max_row_drift": max_row_drift,
        "anchors": anchors,
        "relationships": relationships,
        "app_vs_rendered_disagreements": app_vs_rendered_disagreements,
        "surfaces": selected_surface_rows(snapshot, FINAL_GEOMETRY_SURFACES),
        "native_minimap": visual_geometry(snapshot).get("native_minimap"),
        "scroll_anchors": visual_geometry(snapshot).get("scroll_anchors", []),
    }


def stale_animation_frame_report(
    frame_index: int,
    elapsed_ms: int,
    screenshot_path: Path,
    snapshot_path: Path,
    max_sample_skew_ms: int,
    sample_skew_ms: int | None,
) -> dict[str, Any]:
    return {
        "frame_index": frame_index,
        "elapsed_ms": elapsed_ms,
        "mapped_sample_elapsed_ms": None,
        "sample_skew_ms": sample_skew_ms,
        "sidebar_phase": "unmapped",
        "status": "failed",
        "failure_reason": "stale-frame-geometry-pairing",
        "screenshot": screenshot_path.relative_to(screenshot_path.parents[1]).as_posix(),
        "snapshot": snapshot_path.relative_to(snapshot_path.parents[1]).as_posix(),
        "max_sample_skew_ms": max_sample_skew_ms,
        "max_row_drift": 0,
        "anchors": [],
        "relationships": [],
        "app_vs_rendered_disagreements": [],
        "surfaces": [],
        "native_minimap": None,
        "scroll_anchors": [],
    }


def before_snapshot_from_baseline(baseline: dict[str, Any]) -> dict[str, Any]:
    path = baseline.get("_snapshot_path")
    if isinstance(path, Path) and path.is_file():
        return json.loads(path.read_text(encoding="utf-8"))
    # The baseline dictionary is also used by self-tests. They can provide an
    # inline snapshot without needing real artifact files.
    inline = baseline.get("_snapshot")
    if isinstance(inline, dict):
        return inline
    raise RuntimeError("animation baseline has no readable snapshot")


def baseline_report_rows(baseline: dict[str, Any]) -> dict[str, Any]:
    rows = {}
    for name, row in baseline.get("anchors", {}).items():
        detection = row.get("detection")
        rows[name] = {
            "status": row.get("status"),
            "row_y": row.get("row_y"),
            "crop": row.get("crop"),
            "detection": detection.to_dict() if hasattr(detection, "to_dict") else detection,
        }
    return {
        "screenshot": baseline.get("screenshot"),
        "snapshot": baseline.get("snapshot"),
        "anchors": rows,
        "relationships": baseline.get("relationships", []),
    }


def baseline_relationship_rows(case: dict[str, Any], baseline_rows: dict[str, Any]) -> list[dict[str, Any]]:
    reports = []
    for spec in case["manifest"].get("relative_pixel_anchors", []):
        first = str(spec["from"])
        second = str(spec["to"])
        first_row = baseline_rows.get(first, {}).get("row_y")
        second_row = baseline_rows.get(second, {}).get("row_y")
        report = {"from": first, "to": second, "status": "passed"}
        if first_row is None or second_row is None:
            report["status"] = "failed"
        else:
            report["delta"] = int(first_row) - int(second_row)
        reports.append(report)
    return reports


def evaluate_animation_relationships(
    case: dict[str, Any],
    baseline: dict[str, Any],
    anchors: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    by_name = {str(anchor.get("name")): anchor for anchor in anchors}
    baseline_by_name = baseline.get("anchors", {})
    reports = []
    for spec in case["manifest"].get("relative_pixel_anchors", []):
        first = str(spec["from"])
        second = str(spec["to"])
        frame_first = by_name.get(first, {}).get("frame_row_y")
        frame_second = by_name.get(second, {}).get("frame_row_y")
        base_first = baseline_by_name.get(first, {}).get("row_y")
        base_second = baseline_by_name.get(second, {}).get("row_y")
        report = {"from": first, "to": second, "status": "passed"}
        if None in (frame_first, frame_second, base_first, base_second):
            report["status"] = "failed"
        else:
            frame_delta = int(frame_first) - int(frame_second)
            baseline_delta = int(base_first) - int(base_second)
            report["baseline_delta"] = baseline_delta
            report["frame_delta"] = frame_delta
            report["delta_change"] = frame_delta - baseline_delta
            if "max_delta_change" in spec and abs(frame_delta - baseline_delta) > int(spec["max_delta_change"]):
                report["status"] = "failed"
            if "min_delta" in spec and frame_delta < int(spec["min_delta"]):
                report["status"] = "failed"
            if "max_delta" in spec and frame_delta > int(spec["max_delta"]):
                report["status"] = "failed"
        reports.append(report)
    return reports


def summarize_animation_failures(failures: list[dict[str, Any]]) -> list[dict[str, Any]]:
    rows = []
    for failure in failures:
        failed_anchors = [
            {
                "name": anchor.get("name"),
                "baseline_row_y": anchor.get("baseline_row_y"),
                "frame_row_y": anchor.get("frame_row_y"),
                "row_delta_from_baseline": anchor.get("row_delta_from_baseline"),
                "crop": anchor.get("crop"),
                "diagnostics": anchor.get("diagnostics", []),
            }
            for anchor in failure.get("anchors", [])
            if isinstance(anchor, dict) and anchor.get("status") != "passed"
        ]
        rows.append(
            {
                "frame_index": failure.get("frame_index"),
                "elapsed_ms": failure.get("elapsed_ms"),
                "mapped_sample_elapsed_ms": failure.get("mapped_sample_elapsed_ms"),
                "sample_skew_ms": failure.get("sample_skew_ms"),
                "sidebar_phase": failure.get("sidebar_phase"),
                "failure_reason": failure.get("failure_reason"),
                "screenshot": failure.get("screenshot"),
                "snapshot": failure.get("snapshot"),
                "max_row_drift": failure.get("max_row_drift"),
                "anchors": failed_anchors,
                "relationships": [
                    rel
                    for rel in failure.get("relationships", [])
                    if isinstance(rel, dict) and rel.get("status") != "passed"
                ],
            }
        )
    return rows


def animation_failure_reason(failures: list[dict[str, Any]]) -> str | None:
    if not failures:
        return None
    first = failures[0]
    return (
        f"frame {first.get('frame_index')} at {first.get('elapsed_ms')}ms drifted "
        f"{first.get('max_row_drift')}px"
    )


def wait_for_case_final_geometry(bus, case_dir: Path, case: dict[str, Any], step: str) -> None:
    if case["manifest"]["scenario_type"] != "minimap-sidebar":
        return

    target_visible = sidebar_target_visible(case, step)
    deadline = time.monotonic() + FINAL_GEOMETRY_TIMEOUT_MS / 1000
    samples: list[dict[str, Any]] = []
    stable_signatures: list[tuple[Any, ...]] = []
    last_detail = "no samples collected"

    while time.monotonic() < deadline:
        snapshot = mutter.snapshot_json(bus)
        matches, detail = sidebar_final_geometry_matches(snapshot, target_visible)
        sample = final_geometry_sample(snapshot, target_visible, matches, detail)
        samples.append(sample)
        last_detail = detail
        if len(samples) > 64:
            samples = samples[-64:]

        if matches:
            signature = final_geometry_signature(snapshot)
            if stable_signatures and stable_signatures[-1] != signature:
                stable_signatures = []
            stable_signatures.append(signature)
            if len(stable_signatures) >= FINAL_GEOMETRY_SAMPLE_COUNT:
                write_final_geometry_samples(case_dir, step, target_visible, samples, "passed")
                return
        else:
            stable_signatures = []
        time.sleep(FINAL_GEOMETRY_SAMPLE_INTERVAL_SECONDS)

    write_final_geometry_samples(case_dir, step, target_visible, samples, "failed")
    raise RuntimeError(f"sidebar final geometry did not settle for {step}: {last_detail}")


def sidebar_target_visible(case: dict[str, Any], step: str) -> bool:
    direction = case.get("direction")
    if direction not in {"hide", "show"}:
        raise RuntimeError(f"unsupported sidebar direction: {direction}")
    if step == "before":
        return direction == "hide"
    if step == "after":
        return direction == "show"
    raise RuntimeError(f"unsupported capture step: {step}")


def assert_capture_final_geometry(case: dict[str, Any], snapshot: dict[str, Any], step: str) -> None:
    if case["manifest"]["scenario_type"] != "minimap-sidebar":
        return
    target_visible = sidebar_target_visible(case, step)
    matches, detail = sidebar_final_geometry_matches(snapshot, target_visible)
    if not matches:
        raise RuntimeError(f"sidebar final geometry changed before {step} capture: {detail}")


def sidebar_final_geometry_matches(snapshot: dict[str, Any], target_visible: bool) -> tuple[bool, str]:
    try:
        sidebar = rect_for(snapshot, "workspace-sidebar")
        editor = rect_for(snapshot, "editor-viewport")
        rect_for(snapshot, "minimap-shell")
        rect_for(snapshot, "minimap-source-map")
        rect_for(snapshot, "minimap-marker-strip")
    except RuntimeError as exc:
        return False, str(exc)

    if target_visible:
        if sidebar.x != 0:
            return False, f"workspace-sidebar x={sidebar.x}, expected 0"
        expected_editor_x = sidebar.x + sidebar.width
        if editor.x != expected_editor_x:
            return False, f"editor-viewport x={editor.x}, expected {expected_editor_x}"
        return True, "workspace sidebar is fully visible"

    expected_sidebar_x = -sidebar.width
    if sidebar.x != expected_sidebar_x:
        return False, f"workspace-sidebar x={sidebar.x}, expected {expected_sidebar_x}"
    if editor.x != 0:
        return False, f"editor-viewport x={editor.x}, expected 0"
    return True, "workspace sidebar is fully hidden"


def final_geometry_signature(snapshot: dict[str, Any]) -> tuple[Any, ...]:
    rows = []
    for name in FINAL_GEOMETRY_SURFACES:
        row = optional_surface(snapshot, name)
        rect = row.get("rect") if row else None
        allocation = row.get("allocation") if row else None
        rows.append(
            (
                name,
                bool(row.get("visible")) if row else False,
                rect_tuple(rect),
                rect_tuple(allocation),
            )
        )
    native_minimap = visual_geometry(snapshot).get("native_minimap") or {}
    rows.append(
        (
            "native_minimap",
            bool(native_minimap.get("visible")),
            rect_tuple(native_minimap.get("source_map_visible_rect")),
            rect_tuple(native_minimap.get("native_slider_estimate")),
            rect_tuple(native_minimap.get("native_slider_visible_bounds")),
            rect_tuple(native_minimap.get("line_projection_rect")),
            rect_tuple(native_minimap.get("first_content_row_rect")),
            adjustment_tuple(native_minimap.get("source_map_vadjustment")),
        )
    )
    return tuple(rows)


def final_geometry_sample(
    snapshot: dict[str, Any],
    target_visible: bool,
    matches: bool,
    detail: str,
) -> dict[str, Any]:
    return {
        "elapsed_sample": True,
        "target": "visible" if target_visible else "hidden",
        "matches": matches,
        "detail": detail,
        "surfaces": selected_surface_rows(snapshot, FINAL_GEOMETRY_SURFACES),
        "native_minimap": visual_geometry(snapshot).get("native_minimap"),
    }


def write_final_geometry_samples(
    case_dir: Path,
    step: str,
    target_visible: bool,
    samples: list[dict[str, Any]],
    status: str,
) -> None:
    payload = {
        "status": status,
        "step": step,
        "target": "visible" if target_visible else "hidden",
        "required_stable_samples": FINAL_GEOMETRY_SAMPLE_COUNT,
        "sample_interval_seconds": FINAL_GEOMETRY_SAMPLE_INTERVAL_SECONDS,
        "samples": samples,
    }
    (case_dir / f"{step}-final-geometry-samples.json").write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def prepare_case_state(bus, case: dict[str, Any], case_dir: Path) -> None:
    if case["manifest"]["scenario_type"] != "minimap-sidebar":
        return
    if case.get("viewport_position", "top") != "mid":
        return

    query = "line 0180"
    # Drive scrolling through public search/navigation actions so the smoke case
    # exercises the same path agents can use instead of mutating GTK adjustments.
    mutter.activate_window_action(bus, "set-search-query", query)
    mutter.wait_for_ready(bus, case_dir, "search-complete", 5000)
    mutter.wait_for_snapshot_predicate(
        bus,
        f"editor search query {query!r} with one match",
        lambda snapshot: snapshot.get("window") is not None
        and snapshot["window"]["search"]["editor_search_visible"]
        and snapshot["window"]["search"]["editor_query"] == query
        and snapshot["window"]["search"]["editor_match_count"] == 1,
        5000,
    )
    mutter.wait_for_window_action_enabled(bus, case_dir, "next-match")
    mutter.activate_window_action(bus, "next-match")
    mutter.wait_for_ready(bus, case_dir, "visual-geometry-settled", 5000)
    mutter.wait_for_snapshot_predicate(
        bus,
        "source-view scrolled to middle fixture line",
        lambda snapshot: any(
            row.get("name") == "source-view" and int(row.get("y_value_milli") or 0) > 0
            for row in (snapshot.get("window") or {})
            .get("visual_geometry", {})
            .get("scroll_anchors", [])
        ),
        5000,
    )


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


def optional_surface(snapshot: dict[str, Any], name: str) -> dict[str, Any] | None:
    for row in visual_geometry(snapshot).get("surfaces", []):
        if row.get("name") == name:
            return row
    return None


def scroll_anchor(snapshot: dict[str, Any], name: str) -> dict[str, Any]:
    for row in visual_geometry(snapshot).get("scroll_anchors", []):
        if row.get("name") == name:
            return row
    raise RuntimeError(f"visual scroll anchor not found: {name}")


def pixel_anchor_snapshot(snapshot: dict[str, Any], name: str) -> dict[str, Any]:
    for row in visual_geometry(snapshot).get("pixel_anchors", []):
        if row.get("name") == name:
            return row
    raise RuntimeError(f"visual pixel anchor not found: {name}")


def optional_pixel_anchor(snapshot: dict[str, Any], name: str) -> dict[str, Any] | None:
    for row in visual_geometry(snapshot).get("pixel_anchors", []):
        if row.get("name") == name:
            return row
    return None


def rect_for(snapshot: dict[str, Any], name: str) -> Rect:
    row = surface(snapshot, name)
    if not row.get("visible") or not row.get("rect"):
        raise RuntimeError(f"surface {name} is not visible: {row}")
    return Rect.from_mapping(row["rect"])


def rect_tuple(value: Any) -> tuple[int, int, int, int] | None:
    if not isinstance(value, dict):
        return None
    return (
        int(value.get("x", 0)),
        int(value.get("y", 0)),
        int(value.get("width", 0)),
        int(value.get("height", 0)),
    )


def adjustment_tuple(value: Any) -> tuple[bool, int, int, int, int] | None:
    if not isinstance(value, dict):
        return None
    return (
        bool(value.get("at_lower")),
        int(value.get("value_milli", 0)),
        int(value.get("lower_milli", 0)),
        int(value.get("upper_milli", 0)),
        int(value.get("page_size_milli", 0)),
    )


def selected_surface_rows(snapshot: dict[str, Any], names: tuple[str, ...]) -> list[dict[str, Any]]:
    rows = []
    for name in names:
        row = optional_surface(snapshot, name)
        if row is None:
            rows.append({"name": name, "visible": False, "absence_reason": "missing-from-snapshot"})
            continue
        rows.append(
            {
                "name": row.get("name"),
                "visible": row.get("visible"),
                "rect": row.get("rect"),
                "allocation": row.get("allocation"),
                "absence_reason": row.get("absence_reason"),
            }
        )
    return rows


def rect_for_pixel_anchor(snapshot: dict[str, Any], name: str) -> Rect:
    row = pixel_anchor_snapshot(snapshot, name)
    if not row.get("visible") or not row.get("rect"):
        raise RuntimeError(f"pixel anchor {name} is not visible: {row}")
    return Rect.from_mapping(row["rect"])


def inset_rect(rect: Rect, insets: dict[str, Any]) -> Rect:
    left = int(insets.get("left", 0))
    top = int(insets.get("top", 0))
    right = int(insets.get("right", 0))
    bottom = int(insets.get("bottom", 0))
    return Rect(
        x=rect.x + left,
        y=rect.y + top,
        width=rect.width - left - right,
        height=rect.height - top - bottom,
    )


def rect_for_anchor_search(snapshot: dict[str, Any], spec: dict[str, Any]) -> Rect:
    if spec.get("crop_surface"):
        rect = rect_for(snapshot, str(spec["crop_surface"]))
    else:
        rect = rect_for_pixel_anchor(snapshot, str(spec["name"]))
    if spec.get("crop_insets"):
        rect = inset_rect(rect, spec["crop_insets"])
    return rect


def compare_case(case_dir: Path, case: dict[str, Any], before: dict[str, Any], after: dict[str, Any]) -> None:
    comparison_dir = case_dir / "comparisons"
    comparison_dir.mkdir(exist_ok=True)
    reports = []
    pixel_report = {"status": "not-run", "anchors": [], "relationships": []}
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
                json.dumps(
                    {
                        "status": "failed",
                        "invariant_id": case["manifest"].get("invariant_id"),
                        "regions": reports,
                        "pixel_anchors": pixel_report,
                        "final_geometry": final_geometry_summary(before["snapshot"], after["snapshot"]),
                    },
                    indent=2,
                    sort_keys=True,
                )
                + "\n",
                encoding="utf-8",
            )
            raise RuntimeError(f"visual comparison failed for protected region {region['name']}")

    assert_allowed_region_relationships(case, before["snapshot"], after["snapshot"])
    pixel_report = evaluate_pixel_anchors(case, before, after, comparison_dir)
    status = "passed" if pixel_report["status"] == "passed" else "failed"
    (comparison_dir / "comparison-report.json").write_text(
        json.dumps(
            {
                "status": status,
                "invariant_id": case["manifest"].get("invariant_id"),
                "regions": reports,
                "pixel_anchors": pixel_report,
                "final_geometry": final_geometry_summary(before["snapshot"], after["snapshot"]),
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )
    if status != "passed":
        raise RuntimeError("pixel anchor assertion failed")


def evaluate_pixel_anchors(
    case: dict[str, Any],
    before: dict[str, Any],
    after: dict[str, Any],
    comparison_dir: Path,
) -> dict[str, Any]:
    """Evaluate screenshot-derived pixel anchors for one before/after pair.

    Automation geometry chooses bounded crops; detector rows and relationship
    checks decide pass/fail so app-computed rectangles cannot satisfy
    rendered-effect coverage by themselves.
    """

    anchor_specs = case["manifest"].get("pixel_anchors", [])
    if not anchor_specs:
        return {"status": "passed", "anchors": [], "relationships": []}

    before_image = read_png(before["screenshot"])
    after_image = read_png(after["screenshot"])
    detections: dict[str, dict[str, Any]] = {}
    reports = []
    app_vs_rendered_disagreements = []
    status = "passed"
    for spec in anchor_specs:
        name = spec["name"]
        detector = spec["detector"]
        min_pixels = int(spec.get("min_pixels", 1))
        before_rect = clamp_rect(before_image, rect_for_anchor_search(before["snapshot"], spec))
        after_rect = clamp_rect(after_image, rect_for_anchor_search(after["snapshot"], spec))
        before_detection = detect_pixel_anchor(before_image, name, before_rect, detector, min_pixels)
        after_detection = detect_pixel_anchor(after_image, name, after_rect, detector, min_pixels)
        before_crop = write_anchor_crop(comparison_dir, name, "before", before_image, before_rect)
        after_crop = write_anchor_crop(comparison_dir, name, "after", after_image, after_rect)
        report = {
            "name": name,
            "detector": detector,
            "crop_surface": spec.get("crop_surface"),
            "crop_insets": spec.get("crop_insets", {}),
            "before": before_detection.to_dict(),
            "after": after_detection.to_dict(),
            "artifacts": {
                "before_crop": before_crop.name,
                "after_crop": after_crop.name,
            },
            "status": "passed",
        }
        app_geometry = app_pixel_anchor_geometry(before["snapshot"], after["snapshot"], name)
        if app_geometry is not None:
            report["app_geometry"] = app_geometry
        if before_detection.status != "passed" or after_detection.status != "passed":
            report["status"] = "failed"
            status = "failed"
        # Check both absolute screen drift and in-crop row offset: the first
        # catches a shifted effect, while the second proves the crop has enough
        # surrounding pixels.
        if "max_screen_y_delta" in spec and before_detection.row_y is not None and after_detection.row_y is not None:
            delta = abs(after_detection.row_y - before_detection.row_y)
            report["screen_y_delta"] = delta
            if delta > int(spec["max_screen_y_delta"]):
                report["status"] = "failed"
                status = "failed"
                if app_geometry and app_geometry.get("screen_y_delta") is not None:
                    app_delta = int(app_geometry["screen_y_delta"])
                    if app_delta <= int(spec["max_screen_y_delta"]):
                        diagnostic = {
                            "name": name,
                            "status": "app-vs-rendered-anchor-disagreement",
                            "app_screen_y_delta": app_delta,
                            "rendered_screen_y_delta": delta,
                            "max_screen_y_delta": int(spec["max_screen_y_delta"]),
                        }
                        report.setdefault("diagnostics", []).append(diagnostic)
                        app_vs_rendered_disagreements.append(diagnostic)
        before_offset = row_offset(before_detection)
        after_offset = row_offset(after_detection)
        if before_offset is not None and after_offset is not None:
            report["before_row_offset"] = before_offset
            report["after_row_offset"] = after_offset
        if "min_row_offset" in spec and before_offset is not None and after_offset is not None:
            minimum = int(spec["min_row_offset"])
            if before_offset < minimum or after_offset < minimum:
                report["status"] = "failed"
                status = "failed"
        if "max_row_offset" in spec and before_offset is not None and after_offset is not None:
            maximum = int(spec["max_row_offset"])
            if before_offset > maximum or after_offset > maximum:
                report["status"] = "failed"
                status = "failed"
        detections[name] = {"before": before_detection, "after": after_detection}
        reports.append(report)

    relationship_reports = []
    # Relative anchors catch coupled drift, such as the viewport edge moving
    # against the first content row while both rows remain individually visible.
    for spec in case["manifest"].get("relative_pixel_anchors", []):
        first = spec["from"]
        second = spec["to"]
        before_first = detections[first]["before"].row_y
        before_second = detections[second]["before"].row_y
        after_first = detections[first]["after"].row_y
        after_second = detections[second]["after"].row_y
        report = {"from": first, "to": second, "status": "passed"}
        if None in (before_first, before_second, after_first, after_second):
            report["status"] = "failed"
            status = "failed"
        else:
            before_delta = before_first - before_second
            after_delta = after_first - after_second
            report["before_delta"] = before_delta
            report["after_delta"] = after_delta
            report["delta_change"] = after_delta - before_delta
            if "max_delta_change" in spec and abs(after_delta - before_delta) > int(spec["max_delta_change"]):
                report["status"] = "failed"
                status = "failed"
            if "min_delta" in spec and (before_delta < int(spec["min_delta"]) or after_delta < int(spec["min_delta"])):
                report["status"] = "failed"
                status = "failed"
            if "max_delta" in spec and (before_delta > int(spec["max_delta"]) or after_delta > int(spec["max_delta"])):
                report["status"] = "failed"
                status = "failed"
        relationship_reports.append(report)

    return {
        "status": status,
        "anchors": reports,
        "relationships": relationship_reports,
        "app_vs_rendered_disagreements": app_vs_rendered_disagreements,
    }


def app_pixel_anchor_geometry(
    before_snapshot: dict[str, Any],
    after_snapshot: dict[str, Any],
    pixel_anchor_name: str,
) -> dict[str, Any] | None:
    app_anchor_name = APP_PIXEL_ANCHOR_ALIASES.get(pixel_anchor_name, pixel_anchor_name)
    before_anchor = optional_pixel_anchor(before_snapshot, app_anchor_name)
    after_anchor = optional_pixel_anchor(after_snapshot, app_anchor_name)
    if before_anchor is None and after_anchor is None:
        return None
    before_row = app_anchor_row(before_anchor)
    after_row = app_anchor_row(after_anchor)
    return {
        "snapshot_anchor_name": app_anchor_name,
        "before": app_anchor_summary(before_anchor),
        "after": app_anchor_summary(after_anchor),
        "before_row_y": before_row,
        "after_row_y": after_row,
        "screen_y_delta": abs(after_row - before_row)
        if before_row is not None and after_row is not None
        else None,
    }


def app_anchor_row(row: dict[str, Any] | None) -> int | None:
    if not row or not row.get("visible") or not isinstance(row.get("rect"), dict):
        return None
    return int(row["rect"]["y"])


def app_anchor_summary(row: dict[str, Any] | None) -> dict[str, Any]:
    if row is None:
        return {"visible": False, "absence_reason": "missing-from-snapshot"}
    return {
        "name": row.get("name"),
        "surface": row.get("surface"),
        "visible": row.get("visible"),
        "rect": row.get("rect"),
        "absence_reason": row.get("absence_reason"),
    }


def final_geometry_summary(before_snapshot: dict[str, Any], after_snapshot: dict[str, Any]) -> dict[str, Any]:
    return {
        "before": selected_surface_rows(before_snapshot, FINAL_GEOMETRY_SURFACES),
        "after": selected_surface_rows(after_snapshot, FINAL_GEOMETRY_SURFACES),
        "native_minimap": {
            "before": visual_geometry(before_snapshot).get("native_minimap"),
            "after": visual_geometry(after_snapshot).get("native_minimap"),
        },
    }


def row_offset(detection) -> int | None:
    if detection.row_y is None:
        return None
    return detection.row_y - detection.rect.y


def write_anchor_crop(
    comparison_dir: Path,
    name: str,
    step: str,
    image,
    rect: Rect,
) -> Path:
    safe_name = re.sub(r"[^A-Za-z0-9_.-]+", "-", name).strip("-")
    path = comparison_dir / f"{safe_name}-{step}-anchor.png"
    write_png(path, image, crop_rows(image, rect))
    return path


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
        before_anchor = scroll_anchor(before, "source-view")
        after_anchor = scroll_anchor(after, "source-view")
        if after_anchor.get("at_left") is not True:
            raise RuntimeError(f"source-view should remain left anchored: {after_anchor}")
        if case.get("viewport_position", "top") == "mid":
            if int(before_anchor.get("y_value_milli") or 0) <= 0:
                raise RuntimeError(f"mid-file source-view should scroll before action: {before_anchor}")
            if int(after_anchor.get("y_value_milli") or 0) <= 0:
                raise RuntimeError(f"mid-file source-view should stay scrolled after action: {after_anchor}")
        elif after_anchor.get("at_top") is not True:
            raise RuntimeError(f"source-view should remain top anchored: {after_anchor}")
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
    if "animation-frame" in text:
        return "pixel-anchor-failed"
    if "pixel anchor" in text:
        return "pixel-anchor-failed"
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
    evidence = case_evidence_summary(case_dir, comparison_report)
    animation_evidence = animation_evidence_summary(case_dir)
    pixel_verified_invariant_ids = []
    invariant_id = case["manifest"].get("invariant_id")
    if (
        status == "passed"
        and invariant_id
        and len(case["manifest"].get("pixel_anchors", [])) > 0
        and evidence.get("pixel_anchor_status") == "passed"
    ):
        pixel_verified_invariant_ids.append(invariant_id)
    animation_verified_invariant_ids = []
    animation_invariant_id = animation_evidence.get("invariant_id")
    if status == "passed" and animation_evidence.get("status") == "passed" and animation_invariant_id:
        animation_verified_invariant_ids.append(animation_invariant_id)
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
        "invariant_id": invariant_id,
        "pixel_verified_invariant_ids": pixel_verified_invariant_ids,
        "animation_verified_invariant_ids": animation_verified_invariant_ids,
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
        "pixel_anchors": case["manifest"].get("pixel_anchors", []),
        "relative_pixel_anchors": case["manifest"].get("relative_pixel_anchors", []),
        "pixel_anchor_assertion_count": len(case["manifest"].get("pixel_anchors", [])),
        "allowed_changing_regions": case["manifest"].get("allowed_changing_regions", []),
        "comparison_report": "comparisons/comparison-report.json" if comparison_report.exists() else None,
        "final_geometry": evidence.get("final_geometry"),
        "pixel_anchor_evidence": evidence.get("pixel_anchor_evidence", []),
        "app_vs_rendered_disagreements": evidence.get("app_vs_rendered_disagreements", []),
        "rendered_anchor_stability": rendered_anchor_stability_summary(case_dir),
        "animation_sampling": case["manifest"].get("animation_sampling"),
        "animation_frame_evidence": animation_evidence,
        "animation_frame_sample_count": int(animation_evidence.get("sampled_frame_count") or 0),
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
        "invariant_id": invariant_id,
        "pixel_verified_invariant_ids": pixel_verified_invariant_ids,
        "final_geometry": manifest["final_geometry"],
        "pixel_anchor_evidence": manifest["pixel_anchor_evidence"],
        "app_vs_rendered_disagreements": manifest["app_vs_rendered_disagreements"],
        "rendered_anchor_stability": manifest["rendered_anchor_stability"],
        "animation_verified_invariant_ids": animation_verified_invariant_ids,
        "animation_frame_evidence": animation_evidence,
        "animation_frame_sample_count": manifest["animation_frame_sample_count"],
    }
    (case_dir / "summary.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def case_evidence_summary(case_dir: Path, comparison_report: Path) -> dict[str, Any]:
    if not comparison_report.is_file():
        return {
            "pixel_anchor_status": "not-run",
            "final_geometry": final_geometry_from_snapshot_files(case_dir),
            "pixel_anchor_evidence": [],
            "app_vs_rendered_disagreements": [],
        }
    try:
        report = json.loads(comparison_report.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        return {
            "pixel_anchor_status": "parse-error",
            "detail": bounded(exc),
            "final_geometry": final_geometry_from_snapshot_files(case_dir),
            "pixel_anchor_evidence": [],
            "app_vs_rendered_disagreements": [],
        }

    pixel_report = report.get("pixel_anchors") if isinstance(report, dict) else None
    anchors = pixel_report.get("anchors", []) if isinstance(pixel_report, dict) else []
    evidence_rows = []
    for anchor in anchors:
        if not isinstance(anchor, dict):
            continue
        artifacts = anchor.get("artifacts") if isinstance(anchor.get("artifacts"), dict) else {}
        before_row = row_y_from_detection(anchor.get("before"))
        after_row = row_y_from_detection(anchor.get("after"))
        evidence_rows.append(
            {
                "name": anchor.get("name"),
                "status": anchor.get("status"),
                "before_row_y": before_row,
                "after_row_y": after_row,
                "screen_y_delta": anchor.get("screen_y_delta"),
                "before_crop": relative_comparison_artifact(artifacts.get("before_crop")),
                "after_crop": relative_comparison_artifact(artifacts.get("after_crop")),
                "app_geometry": anchor.get("app_geometry"),
                "diagnostics": anchor.get("diagnostics", []),
            }
        )

    app_vs_rendered = (
        pixel_report.get("app_vs_rendered_disagreements", [])
        if isinstance(pixel_report, dict)
        else []
    )
    return {
        "pixel_anchor_status": pixel_report.get("status") if isinstance(pixel_report, dict) else "not-run",
        "final_geometry": report.get("final_geometry") or final_geometry_from_snapshot_files(case_dir),
        "pixel_anchor_evidence": evidence_rows,
        "app_vs_rendered_disagreements": app_vs_rendered,
    }


def animation_evidence_summary(case_dir: Path) -> dict[str, Any]:
    report_path = case_dir / "animation" / "animation-report.json"
    if not report_path.is_file():
        return {
            "status": "not-run",
            "invariant_id": None,
            "sampled_frame_count": 0,
            "geometry_sample_count": 0,
            "intermediate_geometry_sample_count": 0,
            "mapped_intermediate_frame_count": 0,
            "phase_sequence": [],
            "max_sample_skew_ms": None,
            "max_sample_skew_observed_ms": None,
            "max_row_drift": None,
            "frames": [],
            "failures": [],
            "report": None,
        }
    try:
        report = json.loads(report_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        return {
            "status": "parse-error",
            "invariant_id": None,
            "sampled_frame_count": 0,
            "geometry_sample_count": 0,
            "intermediate_geometry_sample_count": 0,
            "mapped_intermediate_frame_count": 0,
            "phase_sequence": [],
            "max_sample_skew_ms": None,
            "max_sample_skew_observed_ms": None,
            "max_row_drift": None,
            "frames": [],
            "failures": [],
            "report": "animation/animation-report.json",
            "detail": bounded(exc),
        }

    frame_rows = []
    for frame in report.get("frames", []):
        if not isinstance(frame, dict):
            continue
        anchors = []
        for anchor in frame.get("anchors", []):
            if not isinstance(anchor, dict):
                continue
            anchors.append(
                {
                    "name": anchor.get("name"),
                    "status": anchor.get("status"),
                    "baseline_row_y": anchor.get("baseline_row_y"),
                    "frame_row_y": anchor.get("frame_row_y"),
                    "row_delta_from_baseline": anchor.get("row_delta_from_baseline"),
                    "max_screen_y_delta": anchor.get("max_screen_y_delta"),
                    "crop": anchor.get("crop"),
                    "app_geometry": anchor.get("app_geometry"),
                    "diagnostics": anchor.get("diagnostics", []),
                }
            )
        frame_rows.append(
            {
                "frame_index": frame.get("frame_index"),
                "elapsed_ms": frame.get("elapsed_ms"),
                "mapped_sample_elapsed_ms": frame.get("mapped_sample_elapsed_ms"),
                "sample_skew_ms": frame.get("sample_skew_ms"),
                "sidebar_phase": frame.get("sidebar_phase"),
                "status": frame.get("status"),
                "failure_reason": frame.get("failure_reason"),
                "screenshot": frame.get("screenshot"),
                "snapshot": frame.get("snapshot"),
                "max_row_drift": frame.get("max_row_drift"),
                "anchors": anchors,
                "relationships": frame.get("relationships", []),
            }
        )

    return {
        "status": report.get("status"),
        "capture_mode": report.get("capture_mode"),
        "invariant_id": report.get("invariant_id"),
        "sampled_frame_count": report.get("sampled_frame_count", 0),
        "geometry_sample_count": report.get("geometry_sample_count", 0),
        "intermediate_geometry_sample_count": report.get(
            "intermediate_geometry_sample_count", 0
        ),
        "mapped_intermediate_frame_count": report.get("mapped_intermediate_frame_count", 0),
        "phase_sequence": report.get("phase_sequence", []),
        "sample_interval_ms": report.get("sample_interval_ms"),
        "max_sample_skew_ms": report.get("max_sample_skew_ms"),
        "max_sample_skew_observed_ms": report.get("max_sample_skew_observed_ms"),
        "action_started_ms": report.get("action_started_ms"),
        "max_screen_y_delta": report.get("max_screen_y_delta"),
        "max_row_drift": report.get("max_row_drift"),
        "failure_reason": report.get("failure_reason"),
        "failures": report.get("failures", []),
        "frames": frame_rows,
        "report": "animation/animation-report.json",
    }


def rendered_anchor_stability_summary(case_dir: Path) -> list[dict[str, Any]]:
    rows = []
    for step in ("before", "after"):
        path = case_dir / f"{step}-rendered-anchor-stability.json"
        if not path.is_file():
            continue
        try:
            payload = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            rows.append(
                {
                    "name": step,
                    "artifact": path.name,
                    "status": "parse-error",
                    "detail": bounded(exc),
                    "anchors": [],
                }
            )
            continue
        anchors = []
        for anchor in payload.get("anchors", []):
            if not isinstance(anchor, dict):
                continue
            warmup = anchor.get("warmup") if isinstance(anchor.get("warmup"), dict) else {}
            final = anchor.get("final") if isinstance(anchor.get("final"), dict) else {}
            anchors.append(
                {
                    "name": anchor.get("name"),
                    "status": anchor.get("status"),
                    "warmup_row_y": warmup.get("row_y"),
                    "final_row_y": final.get("row_y"),
                    "row_delta": anchor.get("row_delta"),
                    "max_row_delta": anchor.get("max_row_delta"),
                }
            )
        rows.append(
            {
                "name": step,
                "artifact": path.name,
                "status": payload.get("status"),
                "anchors": anchors,
            }
        )
    return rows


def row_y_from_detection(value: Any) -> int | None:
    if not isinstance(value, dict) or value.get("row_y") is None:
        return None
    return int(value["row_y"])


def relative_comparison_artifact(value: Any) -> str | None:
    if not isinstance(value, str):
        return None
    return f"comparisons/{value}"


def final_geometry_from_snapshot_files(case_dir: Path) -> dict[str, Any] | None:
    before_path = case_dir / "before-geometry-snapshot.json"
    after_path = case_dir / "after-geometry-snapshot.json"
    if not before_path.is_file() or not after_path.is_file():
        return None
    try:
        before = json.loads(before_path.read_text(encoding="utf-8"))
        after = json.loads(after_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return None
    return final_geometry_summary(before, after)


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
